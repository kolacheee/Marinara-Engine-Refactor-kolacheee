import { Check, X } from "lucide-react";
import { Modal } from "../../../../shared/components/ui/Modal";
import type { AgentInjectionReviewRequest } from "../hooks/use-agent-injection-review";

type AgentInjectionReviewModalProps = {
  request: AgentInjectionReviewRequest;
  drafts: Record<string, string>;
  onDraftChange: (agentType: string, text: string) => void;
  onContinue: () => void;
  onClose: () => void;
};

export function AgentInjectionReviewModal({
  request,
  drafts,
  onDraftChange,
  onContinue,
  onClose,
}: AgentInjectionReviewModalProps) {
  return (
    <Modal open onClose={onClose} title="Writer Agent Review" width="max-w-3xl">
      <div className="flex flex-col gap-3">
        <p className="text-xs leading-relaxed text-[var(--muted-foreground)]">
          Edit the writer guidance before the main reply starts.
        </p>
        <div className="flex max-h-[55dvh] flex-col gap-2 overflow-y-auto pr-1">
          {request.injections.map((injection) => (
            <div key={injection.agentType} className="rounded-lg border border-[var(--border)] bg-[var(--card)]/60">
              <div className="flex items-center justify-between gap-2 border-b border-[var(--border)] px-3 py-2">
                <div className="min-w-0">
                  <div className="truncate text-xs font-semibold text-[var(--foreground)]">{injection.agentName}</div>
                  <div className="truncate text-[0.625rem] text-[var(--muted-foreground)]">{injection.agentType}</div>
                </div>
              </div>
              <textarea
                value={drafts[injection.agentType] ?? injection.text}
                onChange={(event) => onDraftChange(injection.agentType, event.target.value)}
                rows={6}
                className="min-h-32 w-full resize-y rounded-b-lg border-0 bg-[var(--secondary)]/35 px-3 py-2 font-mono text-xs leading-relaxed text-[var(--foreground)] outline-none focus:ring-1 focus:ring-[var(--ring)]"
                spellCheck={false}
              />
            </div>
          ))}
        </div>
        <div className="flex justify-end gap-2 border-t border-[var(--border)] pt-3">
          <button
            type="button"
            onClick={onClose}
            className="inline-flex items-center gap-1.5 rounded-lg border border-[var(--border)] px-3 py-2 text-xs text-[var(--foreground)] transition-colors hover:bg-[var(--accent)]"
          >
            <X size="0.875rem" />
            Close
          </button>
          <button
            type="button"
            onClick={onContinue}
            className="inline-flex items-center gap-1.5 rounded-lg bg-[var(--primary)] px-3 py-2 text-xs font-medium text-[var(--primary-foreground)] transition-opacity hover:opacity-90"
          >
            <Check size="0.875rem" />
            Continue
          </button>
        </div>
      </div>
    </Modal>
  );
}
