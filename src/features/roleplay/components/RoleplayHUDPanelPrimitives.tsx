import { useEffect, useRef, useState, type ReactNode } from "react";
import { Pencil, RefreshCw, Sparkles } from "lucide-react";
import { cn } from "../../../shared/lib/utils";
import { InlineEdit } from "./RoleplayHUDInlineEdit";

export const EMPTY_STATE = "text-[0.625rem] text-[var(--muted-foreground)]/60 text-center py-1";

export function TrackerSectionRefresh({
  agentType,
  onRerunSingleTracker,
  busy,
  title,
}: {
  agentType: string;
  onRerunSingleTracker?: (agentType: string) => void;
  busy?: boolean;
  title?: string;
}) {
  if (!onRerunSingleTracker) return null;
  return (
    <button
      type="button"
      onClick={(event) => {
        event.preventDefault();
        onRerunSingleTracker(agentType);
      }}
      disabled={busy}
      title={title ?? `Re-run ${agentType} only`}
      aria-label={title ?? `Re-run ${agentType} only`}
      className="rounded p-0.5 text-[var(--muted-foreground)]/50 transition-colors hover:bg-[var(--accent)] hover:text-purple-300 disabled:opacity-40"
    >
      <RefreshCw size="0.625rem" className={busy ? "animate-spin" : ""} />
    </button>
  );
}

export function PersonaStatusField({ value, onSave }: { value: string; onSave?: (value: string) => void }) {
  return (
    <div className="mb-2 rounded-lg border border-violet-400/15 bg-violet-500/5 px-2 py-1.5">
      <div className="mb-0.5 flex items-center gap-1.5">
        <Sparkles size="0.5625rem" className="text-violet-300/60" />
        <span className="text-[0.5625rem] font-semibold uppercase tracking-wide text-violet-200/65">
          Current Status
        </span>
      </div>
      <InlineEdit
        value={value}
        onSave={onSave ?? (() => {})}
        className="w-full text-[0.6875rem]! text-[var(--foreground)]/85!"
        placeholder="Status not tracked"
        scrollOnHover
      />
    </div>
  );
}

export function LabeledEdit({
  label,
  value,
  onSave,
}: {
  label: string;
  value: string;
  onSave: (value: string) => void;
}) {
  return (
    <div className="flex items-center gap-1">
      <span className="text-[0.5625rem] text-[var(--muted-foreground)]/60 w-10 shrink-0">{label}</span>
      <InlineEdit value={value} onSave={onSave} className="flex-1 min-w-0" placeholder="—" scrollOnHover />
    </div>
  );
}

export function WorldFieldRow({
  icon,
  label,
  value,
  onSave,
  accent,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  onSave: (value: string) => void;
  accent: string;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing) {
      setDraft(value);
      inputRef.current?.focus();
    }
  }, [editing, value]);

  const commit = () => {
    const trimmed = draft.trim();
    if (trimmed && trimmed !== value) onSave(trimmed);
    setEditing(false);
  };

  return (
    <div className="flex items-center gap-2.5 px-3 py-2 group/row hover:bg-[var(--muted)]/20 transition-colors">
      <div className="shrink-0 w-5 flex items-center justify-center">{icon}</div>
      <div className="flex-1 min-w-0">
        <div className="text-[0.5625rem] font-semibold uppercase tracking-wider text-[var(--muted-foreground)]/60 mb-0.5">
          {label}
        </div>
        {editing ? (
          <input
            ref={inputRef}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") commit();
              if (event.key === "Escape") setEditing(false);
            }}
            onBlur={commit}
            className={cn(
              "w-full bg-transparent text-[0.6875rem] font-medium outline-none placeholder:text-[var(--muted-foreground)]/40",
              accent,
            )}
            placeholder={label}
          />
        ) : (
          <button
            type="button"
            onClick={() => setEditing(true)}
            className={cn(
              "w-full text-left text-[0.6875rem] font-medium truncate",
              value ? "text-[var(--foreground)]/80" : "text-[var(--muted-foreground)]/50 italic",
            )}
          >
            {value || `Set ${label.toLowerCase()}…`}
          </button>
        )}
      </div>
      {!editing && (
        <button
          type="button"
          onClick={() => setEditing(true)}
          className="shrink-0 text-[var(--muted-foreground)]/30 opacity-0 group-hover/row:opacity-100 transition-opacity"
          title={`Edit ${label.toLowerCase()}`}
          aria-label={`Edit ${label.toLowerCase()}`}
        >
          <Pencil size="0.625rem" />
        </button>
      )}
    </div>
  );
}
