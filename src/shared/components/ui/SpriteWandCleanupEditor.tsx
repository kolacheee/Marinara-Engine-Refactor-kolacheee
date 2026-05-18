import { useCallback, useEffect, useRef, useState, type CSSProperties, type PointerEvent } from "react";
import { Eraser, Loader2, RotateCcw, Undo2, Wand2 } from "lucide-react";
import { loadUrlBlob } from "../../lib/url-blob";
import { Modal } from "./Modal";

interface SpriteWandCleanupEditorProps {
  imageUrl: string;
  label: string;
  applying?: boolean;
  onApply: (cleanedDataUrl: string) => Promise<void> | void;
  onClose: () => void;
}

interface WandResult {
  removed: number;
  target: [number, number, number, number];
}

const DEFAULT_TOLERANCE = 36;
const MAX_HISTORY = 12;

const checkerboardStyle: CSSProperties = {
  backgroundColor: "var(--secondary)",
  backgroundImage:
    "linear-gradient(45deg, var(--border) 25%, transparent 25%), linear-gradient(-45deg, var(--border) 25%, transparent 25%), linear-gradient(45deg, transparent 75%, var(--border) 75%), linear-gradient(-45deg, transparent 75%, var(--border) 75%)",
  backgroundPosition: "0 0, 0 10px, 10px -10px, -10px 0",
  backgroundSize: "20px 20px",
};

function colorDistanceSquared(data: Uint8ClampedArray, offset: number, target: [number, number, number]): number {
  const red = data[offset] - target[0];
  const green = data[offset + 1] - target[1];
  const blue = data[offset + 2] - target[2];
  return red * red + green * green + blue * blue;
}

function removeConnectedColor(imageData: ImageData, startX: number, startY: number, tolerance: number): WandResult {
  const { data, width, height } = imageData;
  const startIndex = startY * width + startX;
  const startOffset = startIndex * 4;
  const targetAlpha = data[startOffset + 3];

  if (targetAlpha <= 8) {
    return {
      removed: 0,
      target: [data[startOffset], data[startOffset + 1], data[startOffset + 2], targetAlpha],
    };
  }

  const target: [number, number, number] = [data[startOffset], data[startOffset + 1], data[startOffset + 2]];
  const totalPixels = width * height;
  const threshold = tolerance * tolerance;
  const visited = new Uint8Array(totalPixels);
  const stack = new Int32Array(totalPixels);
  let stackLength = 0;
  let removed = 0;

  const pushPixel = (index: number) => {
    if (!visited[index]) {
      visited[index] = 1;
      stack[stackLength++] = index;
    }
  };

  pushPixel(startIndex);

  while (stackLength > 0) {
    const index = stack[--stackLength];
    const offset = index * 4;
    if (data[offset + 3] <= 8 || colorDistanceSquared(data, offset, target) > threshold) {
      continue;
    }

    data[offset + 3] = 0;
    removed += 1;

    const x = index % width;
    if (x > 0) pushPixel(index - 1);
    if (x < width - 1) pushPixel(index + 1);
    if (index >= width) pushPixel(index - width);
    if (index < totalPixels - width) pushPixel(index + width);
  }

  return {
    removed,
    target: [target[0], target[1], target[2], targetAlpha],
  };
}

async function loadImageToCanvas(imageUrl: string, canvas: HTMLCanvasElement): Promise<ImageData> {
  const blob = await loadUrlBlob(imageUrl, {
    init: { cache: "no-store" },
    errorMessage: "Sprite image could not be loaded",
  });
  const objectUrl = URL.createObjectURL(blob);

  try {
    const image = await new Promise<HTMLImageElement>((resolve, reject) => {
      const img = new Image();
      img.onload = () => resolve(img);
      img.onerror = () => reject(new Error("Sprite image could not be decoded"));
      img.src = objectUrl;
    });

    canvas.width = image.naturalWidth;
    canvas.height = image.naturalHeight;
    if (canvas.width <= 0 || canvas.height <= 0) throw new Error("Sprite image has no usable size");

    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    if (!ctx) throw new Error("Canvas is unavailable");

    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(image, 0, 0);
    return ctx.getImageData(0, 0, canvas.width, canvas.height);
  } finally {
    URL.revokeObjectURL(objectUrl);
  }
}

export function SpriteWandCleanupEditor({
  imageUrl,
  label,
  applying = false,
  onApply,
  onClose,
}: SpriteWandCleanupEditorProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const originalImageRef = useRef<ImageData | null>(null);
  const [tolerance, setTolerance] = useState(DEFAULT_TOLERANCE);
  const [history, setHistory] = useState<ImageData[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [hasChanges, setHasChanges] = useState(false);

  const restoreImageData = useCallback((imageData: ImageData) => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    canvas.width = imageData.width;
    canvas.height = imageData.height;
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    if (!ctx) return;

    ctx.putImageData(imageData, 0, 0);
  }, []);

  useEffect(() => {
    let cancelled = false;
    let frameId: number | null = null;

    setLoading(true);
    setError(null);
    setStatus(null);
    setHasChanges(false);
    setHistory([]);
    originalImageRef.current = null;

    const loadWhenCanvasMounts = () => {
      const canvas = canvasRef.current;
      if (!canvas) {
        frameId = requestAnimationFrame(loadWhenCanvasMounts);
        return;
      }

      loadImageToCanvas(imageUrl, canvas)
        .then((imageData) => {
          if (cancelled) return;
          originalImageRef.current = new ImageData(
            new Uint8ClampedArray(imageData.data),
            imageData.width,
            imageData.height,
          );
          setLoading(false);
        })
        .catch((err: any) => {
          if (cancelled) return;
          setError(err?.message || "Sprite image could not be loaded");
          setLoading(false);
        });
    };

    frameId = requestAnimationFrame(loadWhenCanvasMounts);

    return () => {
      cancelled = true;
      if (frameId !== null) cancelAnimationFrame(frameId);
    };
  }, [imageUrl]);

  const handleCanvasPointerDown = useCallback(
    (event: PointerEvent<HTMLCanvasElement>) => {
      if (loading || applying) return;

      const canvas = canvasRef.current;
      if (!canvas) return;

      const rect = canvas.getBoundingClientRect();
      const x = Math.floor(((event.clientX - rect.left) / rect.width) * canvas.width);
      const y = Math.floor(((event.clientY - rect.top) / rect.height) * canvas.height);

      if (x < 0 || y < 0 || x >= canvas.width || y >= canvas.height) return;

      const ctx = canvas.getContext("2d", { willReadFrequently: true });
      if (!ctx) {
        setError("Canvas is unavailable");
        return;
      }

      const before = ctx.getImageData(0, 0, canvas.width, canvas.height);
      const next = new ImageData(new Uint8ClampedArray(before.data), before.width, before.height);
      const result = removeConnectedColor(next, x, y, tolerance);

      if (result.removed === 0) {
        setStatus("No opaque pixels selected");
        return;
      }

      setHistory((prev) => [...prev.slice(Math.max(0, prev.length - MAX_HISTORY + 1)), before]);
      ctx.putImageData(next, 0, 0);
      setHasChanges(true);
      setStatus(`${result.removed.toLocaleString()} px removed`);
      setError(null);
    },
    [applying, loading, tolerance],
  );

  const handleUndo = useCallback(() => {
    setHistory((prev) => {
      const previous = prev[prev.length - 1];
      if (!previous) return prev;

      restoreImageData(previous);
      const nextHistory = prev.slice(0, -1);
      setHasChanges(nextHistory.length > 0);
      setStatus("Undo applied");
      setError(null);
      return nextHistory;
    });
  }, [restoreImageData]);

  const handleReset = useCallback(() => {
    if (!originalImageRef.current) return;
    restoreImageData(originalImageRef.current);
    setHistory([]);
    setHasChanges(false);
    setStatus("Reset");
    setError(null);
  }, [restoreImageData]);

  const handleApply = useCallback(async () => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    try {
      setError(null);
      await onApply(canvas.toDataURL("image/png"));
    } catch (err: any) {
      setError(err?.message || "Failed to save sprite cleanup");
    }
  }, [onApply]);

  return (
    <Modal open onClose={onClose} title={`Clean ${label}`} width="max-w-5xl">
      <div className="flex min-h-0 flex-col gap-4">
        <div className="flex flex-wrap items-center gap-2">
          <span className="inline-flex items-center gap-1.5 rounded-lg bg-[var(--primary)] px-3 py-2 text-xs font-semibold text-[var(--primary-foreground)]">
            <Wand2 size="0.875rem" />
            Wand
          </span>
          <label className="flex min-w-0 flex-1 items-center gap-2 rounded-lg bg-[var(--secondary)] px-3 py-2 text-xs">
            <span className="shrink-0 font-medium text-[var(--foreground)]">Tolerance</span>
            <input
              type="range"
              min={4}
              max={96}
              step={1}
              value={tolerance}
              onChange={(event) => setTolerance(Number(event.target.value))}
              disabled={loading || applying}
              className="min-w-28 flex-1 accent-[var(--primary)] disabled:opacity-50"
            />
            <span className="w-8 shrink-0 text-right tabular-nums text-[var(--muted-foreground)]">{tolerance}</span>
          </label>
          <button
            type="button"
            onClick={handleUndo}
            disabled={loading || applying || history.length === 0}
            className="inline-flex items-center gap-1.5 rounded-lg px-3 py-2 text-xs font-medium text-[var(--muted-foreground)] ring-1 ring-[var(--border)] transition-colors hover:text-[var(--foreground)] disabled:opacity-45"
          >
            <Undo2 size="0.875rem" />
            Undo
          </button>
          <button
            type="button"
            onClick={handleReset}
            disabled={loading || applying || !hasChanges}
            className="inline-flex items-center gap-1.5 rounded-lg px-3 py-2 text-xs font-medium text-[var(--muted-foreground)] ring-1 ring-[var(--border)] transition-colors hover:text-[var(--foreground)] disabled:opacity-45"
          >
            <RotateCcw size="0.875rem" />
            Reset
          </button>
        </div>

        <div
          className="relative flex min-h-[22rem] items-center justify-center overflow-auto rounded-xl border border-[var(--border)] p-3 max-sm:min-h-[18rem]"
          style={checkerboardStyle}
        >
          {loading && (
            <div className="absolute inset-0 z-10 flex items-center justify-center bg-[var(--background)]/60">
              <Loader2 size="1.5rem" className="animate-spin text-[var(--primary)]" />
            </div>
          )}
          <canvas
            ref={canvasRef}
            onPointerDown={handleCanvasPointerDown}
            className="max-h-[62dvh] max-w-full cursor-crosshair rounded-lg shadow-xl shadow-black/30 [touch-action:manipulation]"
            aria-label={`Wand cleanup canvas for ${label}`}
            title="Click an unwanted background patch"
          />
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <div className="min-w-0 flex-1 text-xs text-[var(--muted-foreground)]">
            {error ? <span className="text-[var(--destructive)]">{error}</span> : (status ?? "Wand ready")}
          </div>
          <button
            type="button"
            onClick={onClose}
            disabled={applying}
            className="rounded-lg px-3 py-2 text-sm font-medium text-[var(--muted-foreground)] transition-colors hover:bg-[var(--accent)] hover:text-[var(--foreground)] disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => void handleApply()}
            disabled={loading || applying || !hasChanges}
            className="inline-flex items-center gap-1.5 rounded-lg bg-[var(--primary)] px-3 py-2 text-sm font-medium text-[var(--primary-foreground)] transition-colors hover:opacity-90 disabled:opacity-50"
          >
            {applying ? <Loader2 size="0.875rem" className="animate-spin" /> : <Eraser size="0.875rem" />}
            Apply Cleanup
          </button>
        </div>
      </div>
    </Modal>
  );
}
