// ──────────────────────────────────────────────
// Game: Skill Check Result Display
// ──────────────────────────────────────────────
import { useEffect, useState } from "react";
import { X } from "lucide-react";
import type { SkillCheckResult } from "../../../engine/contracts/types/game";
import { cn } from "../../../shared/lib/utils";

interface GameSkillCheckResultProps {
  result: SkillCheckResult;
  onDismiss: () => void;
}

export function GameSkillCheckResult({ result, onDismiss }: GameSkillCheckResultProps) {
  const [animate, setAnimate] = useState(false);

  useEffect(() => {
    setAnimate(false);
    const raf = requestAnimationFrame(() => setAnimate(true));
    return () => {
      cancelAnimationFrame(raf);
    };
  }, [result]);

  const label = result.criticalSuccess
    ? "CRITICAL SUCCESS!"
    : result.criticalFailure
      ? "CRITICAL FAILURE!"
      : result.success
        ? "SUCCESS"
        : "FAILURE";

  const color = result.criticalSuccess
    ? "text-yellow-300"
    : result.criticalFailure
      ? "text-red-400"
      : result.success
        ? "text-emerald-400"
        : "text-red-300";

  return (
    <div
      className={cn(
        "pointer-events-auto mx-auto mb-2 flex w-full max-w-md justify-center transition-all duration-300",
        animate ? "translate-y-0 opacity-100" : "translate-y-4 opacity-0",
      )}
    >
      <div className="relative flex w-full items-center gap-3 rounded-xl bg-black/80 px-5 py-3 pr-10 shadow-lg shadow-black/30 backdrop-blur-sm ring-1 ring-white/10">
        <span className="game-dice-animate text-2xl">🎲</span>
        <div>
          <div className="text-xs font-mono text-white/60">
            {result.skill} Check (DC {result.dc})
          </div>
          <div className="flex items-center gap-2">
            <span className="text-xs text-white/40">
              [{result.rolls.join(", ")}]
              {result.modifier !== 0 && ` ${result.modifier > 0 ? "+" : ""}${result.modifier}`}
              {result.rollMode !== "normal" && ` (${result.rollMode})`}
            </span>
            <span className="text-lg font-bold text-white">= {result.total}</span>
          </div>
          <div className={cn("text-xs font-bold", color)}>{label}</div>
        </div>
        <button
          type="button"
          onClick={onDismiss}
          className="absolute right-2 top-2 rounded p-1 text-white/40 transition hover:bg-white/10 hover:text-white"
          aria-label="Dismiss skill check result"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
