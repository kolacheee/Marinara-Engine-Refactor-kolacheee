import { invoke } from "@tauri-apps/api/core";

export const ADMIN_SECRET_STORAGE_KEY = "marinara-admin-secret";

export function getAdminSecretHeader(): Record<string, string> {
  const secret = localStorage.getItem(ADMIN_SECRET_STORAGE_KEY);
  return secret ? { "x-admin-secret": secret } : {};
}

export class ApiError extends Error {
  constructor(
    message: string,
    public status = 0,
    public details?: unknown,
  ) {
    super(message);
    this.name = "ApiError";
  }

  get payload() {
    return this.details;
  }
}

export interface JsonRepairRequest {
  id?: string;
  title?: string;
  endpoint?: string;
  rawJson?: string;
  applyEndpoint: string;
  applyBody?: Record<string, unknown>;
  payload?: unknown;
  error?: string;
  [key: string]: unknown;
}

function normalizeApiError(error: unknown): ApiError {
  if (error instanceof ApiError) return error;
  if (error && typeof error === "object") {
    const record = error as Record<string, unknown>;
    const code = typeof record.code === "string" ? record.code : "";
    const message =
      typeof record.message === "string" ? record.message : typeof record.error === "string" ? record.error : code;
    const status = code === "not_found" ? 404 : code === "invalid_input" ? 400 : 500;
    return new ApiError(message || "Tauri API request failed", status, record);
  }
  return new ApiError(String(error ?? "Tauri API request failed"), 500, error);
}

async function request<T>(method: string, path: string, body?: unknown, init?: RequestInit): Promise<T> {
  if (init?.signal?.aborted) {
    throw new DOMException("The operation was aborted.", "AbortError");
  }
  try {
    return await invoke<T>("api_request", {
      method,
      path,
      body: body ?? null,
    });
  } catch (error) {
    throw normalizeApiError(error);
  }
}

async function formDataToJson(body: FormData): Promise<Record<string, unknown>> {
  const entries: Record<string, unknown> = {};
  const appendEntry = (key: string, value: unknown) => {
    const existing = entries[key];
    if (existing === undefined) {
      entries[key] = value;
    } else if (Array.isArray(existing)) {
      existing.push(value);
    } else {
      entries[key] = [existing, value];
    }
  };
  for (const [key, value] of body.entries()) {
    if (value instanceof File) {
      const buffer = await value.arrayBuffer();
      const bytes = new Uint8Array(buffer);
      let binary = "";
      for (const byte of bytes) binary += String.fromCharCode(byte);
      appendEntry(key, {
        name: value.name,
        type: value.type,
        size: value.size,
        base64: btoa(binary),
      });
    } else {
      appendEntry(key, value);
    }
  }
  return entries;
}

function downloadJson(value: unknown, fallbackFilename: string) {
  const blob = new Blob([JSON.stringify(value, null, 2)], { type: "application/json" });
  downloadBlob(blob, fallbackFilename);
}

type BinaryDownloadPayload = {
  base64?: unknown;
  data?: unknown;
  body?: unknown;
  contentType?: unknown;
  mimeType?: unknown;
  filename?: unknown;
};

function isBinaryDownloadPayload(value: unknown): value is BinaryDownloadPayload {
  if (!value || typeof value !== "object") return false;
  const record = value as BinaryDownloadPayload;
  return (
    typeof record.base64 === "string" ||
    typeof record.data === "string" ||
    typeof record.body === "string"
  );
}

function base64ToBlob(base64: string, contentType: string) {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return new Blob([bytes], { type: contentType });
}

function downloadBlob(blob: Blob, fallbackFilename: string) {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fallbackFilename;
  anchor.click();
  URL.revokeObjectURL(url);
}

function downloadApiValue(value: unknown, fallbackFilename: string) {
  if (isBinaryDownloadPayload(value)) {
    const base64 =
      typeof value.base64 === "string" ? value.base64 : typeof value.data === "string" ? value.data : String(value.body ?? "");
    const contentType =
      typeof value.contentType === "string"
        ? value.contentType
        : typeof value.mimeType === "string"
          ? value.mimeType
          : "application/octet-stream";
    const filename = typeof value.filename === "string" && value.filename.trim() ? value.filename : fallbackFilename;
    downloadBlob(base64ToBlob(base64, contentType), filename);
    return;
  }
  downloadJson(value, fallbackFilename);
}

export function isJsonRepairApiError(error: unknown): boolean {
  return error instanceof ApiError && !!getJsonRepairRequest(error);
}

export function getJsonRepairRequest(error: unknown): JsonRepairRequest | null {
  if (!(error instanceof ApiError)) return null;
  const details = error.details;
  if (!details || typeof details !== "object") return null;
  const request = (details as { jsonRepair?: unknown }).jsonRepair;
  if (!request || typeof request !== "object") return null;
  if (typeof (request as { applyEndpoint?: unknown }).applyEndpoint !== "string") return null;
  return request as JsonRepairRequest;
}

export const api = {
  get: <T = unknown>(path: string, init?: RequestInit): Promise<T> => request<T>("GET", path, undefined, init),
  post: <T = unknown>(path: string, body?: unknown, init?: RequestInit): Promise<T> =>
    request<T>("POST", path, body, init),
  put: <T = unknown>(path: string, body?: unknown, init?: RequestInit): Promise<T> =>
    request<T>("PUT", path, body, init),
  patch: <T = unknown>(path: string, body?: unknown, init?: RequestInit): Promise<T> =>
    request<T>("PATCH", path, body, init),
  delete: <T = unknown>(path: string, init?: RequestInit): Promise<T> => request<T>("DELETE", path, undefined, init),
  upload: async <T = unknown>(path: string, body: FormData, init?: RequestInit): Promise<T> =>
    request<T>("POST", path, await formDataToJson(body), init),
  download: async (path: string, fallbackFilename = "marinara-export.json"): Promise<void> => {
    const value = await request("GET", path);
    downloadApiValue(value, fallbackFilename);
  },
  downloadPost: async (path: string, body: unknown, fallbackFilename = "marinara-export.json"): Promise<void> => {
    const value = await request("POST", path, body);
    downloadApiValue(value, fallbackFilename);
  },
  raw: async (path: string, init?: RequestInit): Promise<Response> => {
    const value = await request("GET", path, undefined, init);
    return new Response(JSON.stringify(value), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  },
  stream: async function* (path: string, body?: unknown, signal?: AbortSignal): AsyncGenerator<string> {
    for await (const event of api.streamEvents(path, body, signal)) {
      if (event.type === "token" && typeof event.data === "string") {
        yield event.data;
      }
    }
  },
  streamEvents: async function* (
    path: string,
    body?: unknown,
    signal?: AbortSignal,
  ): AsyncGenerator<{ type: string; data: unknown }> {
    if (signal?.aborted) {
      throw new DOMException("The operation was aborted.", "AbortError");
    }
    let events: Array<{ type?: unknown; data?: unknown; [key: string]: unknown }>;
    try {
      events = await invoke("api_stream_events", { path, body: body ?? null });
    } catch (error) {
      throw normalizeApiError(error);
    }
    for (const event of events) {
      if (signal?.aborted) {
        throw new DOMException("The operation was aborted.", "AbortError");
      }
      const type = typeof event.type === "string" ? event.type : "message";
      yield { type, data: "data" in event ? event.data : event };
    }
  },
};
