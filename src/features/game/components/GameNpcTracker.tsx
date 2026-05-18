import type { GameNpc } from "../../../engine/contracts/types/game";
import { Users, ChevronDown, ChevronUp } from "lucide-react";
import { useState } from "react";
import { cn } from "../../../shared/lib/utils";

interface GameNpcTrackerProps {
  npcs: GameNpc[];
}

function reputationLabel(rep: number): { text: string; color: string } {
  if (rep >= 50) return { text: "Allied", color: "text-emerald-400" };
  if (rep >= 20) return { text: "Friendly", color: "text-green-400" };
  if (rep >= -20) return { text: "Neutral", color: "text-gray-400" };
  if (rep >= -50) return { text: "Hostile", color: "text-orange-400" };
  return { text: "Enemy", color: "text-red-400" };
}

export function GameNpcTracker({ npcs }: GameNpcTrackerProps) {
  const [expanded, setExpanded] = useState(false);

  if (npcs.length === 0) return null;

  const metNpcs = npcs.filter((n) => n.met);
  if (metNpcs.length === 0) return null;

  const display = expanded ? metNpcs : metNpcs.slice(0, 5);

  return (
    <div className="w-44 rounded-lg border border-[var(--border)] bg-[var(--card)]/92 p-2 shadow-lg backdrop-blur-sm">
      <button
        onClick={() => setExpanded(!expanded)}
        className="mb-1.5 flex w-full items-center gap-1.5 text-xs font-medium text-[var(--muted-foreground)] hover:text-[var(--foreground)]"
      >
        <Users size={12} />
        <span className="flex-1 text-left">NPCs ({metNpcs.length})</span>
        {metNpcs.length > 5 && (expanded ? <ChevronUp size={12} /> : <ChevronDown size={12} />)}
      </button>

      <div className="flex flex-col gap-1">
        {display.map((npc) => {
          const rep = reputationLabel(npc.reputation);
          return (
            <div
              key={npc.id}
              className="flex items-center gap-2 rounded-md bg-white/5 px-2 py-1.5 transition-colors hover:bg-white/10"
              title={`${npc.description}\nLocation: ${npc.location}\nReputation: ${npc.reputation}`}
            >
              {npc.avatarUrl ? (
                <img src={npc.avatarUrl} alt={npc.name} className="h-5 w-5 shrink-0 rounded-full object-cover" />
              ) : (
                <img src="/npc-silhouette.svg" alt={npc.name} className="h-5 w-5 shrink-0 rounded-full object-cover" />
              )}
              <span className="flex-1 truncate text-xs text-[var(--foreground)]">{npc.name}</span>
              <span className={cn("shrink-0 text-[10px]", rep.color)}>{rep.text}</span>
            </div>
          );
        })}
        {!expanded && metNpcs.length > 5 && (
          <span className="text-center text-[0.625rem] text-[var(--muted-foreground)]">+{metNpcs.length - 5} more</span>
        )}
      </div>
    </div>
  );
}
