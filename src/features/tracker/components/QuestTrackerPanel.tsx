import type { ReactNode } from "react";
import { CheckCircle2, Circle, Plus, Target, X } from "lucide-react";
import type { QuestProgress } from "../../../engine/contracts/types/game-state";
import { cn } from "../../../shared/lib/utils";
import {
  addQuestObjective,
  removeQuestObjective,
  toggleQuestObjectiveCompletion,
  updateQuestObjectiveText,
} from "../../world-state/lib/tracker-state-edits";
import { TRACKER_BAR, TRACKER_TEXT_ROW } from "./tracker-data-sidebar.constants";
import { visibleText } from "./tracker-data-sidebar.helpers";
import { AddRowButton, InlineEdit, SectionHeader, TrackerReadabilityVeil } from "./tracker-data-sidebar.controls";

function QuestBoard({
  quests,
  action,
  onAddQuest,
  onUpdateQuest,
  onRemoveQuest,
  deleteMode,
  addMode,
  collapsed = false,
  onToggleCollapsed,
}: {
  quests: QuestProgress[];
  action?: ReactNode;
  onAddQuest: () => void;
  onUpdateQuest: (index: number, quest: QuestProgress) => void;
  onRemoveQuest: (index: number) => void;
  deleteMode: boolean;
  addMode: boolean;
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
}) {
  const completedQuests = quests.filter((quest) => quest.completed).length;
  const activeQuests = quests.length - completedQuests;

  return (
    <div className="relative z-10 overflow-hidden pb-0.5">
      <SectionHeader
        icon={<Target size="0.6875rem" />}
        title="Quest Board"
        badge={`${completedQuests}/${activeQuests}`}
        badgeTitle={`${completedQuests} done, ${activeQuests} active`}
        action={action}
        addAction={
          addMode ? <AddRowButton title="Add quest" onClick={onAddQuest} className="h-4 w-4 rounded-sm" /> : undefined
        }
        collapsed={collapsed}
        onToggle={onToggleCollapsed}
      />

      {!collapsed &&
        (quests.length === 0 ? (
          <div className={cn("relative px-1 py-1 text-[var(--foreground)]/35", TRACKER_TEXT_ROW)}>
            Quest board empty.
          </div>
        ) : (
          <div className={cn("relative grid gap-0.5 pt-0.5", quests.length > 1 && "@min-[380px]:grid-cols-2")}>
            {quests.map((quest, index) => (
              <QuestRow
                key={`${quest.questEntryId}-${index}`}
                quest={quest}
                onUpdate={(updated) => onUpdateQuest(index, updated)}
                onRemove={() => onRemoveQuest(index)}
                deleteMode={deleteMode}
                addMode={addMode}
              />
            ))}
          </div>
        ))}
    </div>
  );
}

export function QuestTrackerPanel({
  quests,
  action,
  onAddQuest,
  onUpdateQuest,
  onRemoveQuest,
  deleteMode,
  addMode,
  collapsed = false,
  onToggleCollapsed,
}: {
  quests: QuestProgress[];
  action?: ReactNode;
  onAddQuest: () => void;
  onUpdateQuest: (index: number, quest: QuestProgress) => void;
  onRemoveQuest: (index: number) => void;
  deleteMode: boolean;
  addMode: boolean;
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
}) {
  return (
    <section className="relative z-10 overflow-hidden border-b border-[var(--border)] bg-[color-mix(in_srgb,var(--card)_6%,transparent)] shadow-[inset_0_1px_0_color-mix(in_srgb,var(--foreground)_4%,transparent)]">
      <TrackerReadabilityVeil />
      {!collapsed && (
        <div className="pointer-events-none absolute inset-x-1 bottom-1 top-6 z-0 opacity-[0.1] [background-image:radial-gradient(circle,color-mix(in_srgb,var(--foreground)_42%,transparent)_1px,transparent_1.25px)] [background-size:5px_5px]" />
      )}
      <QuestBoard
        quests={quests}
        action={action}
        onAddQuest={onAddQuest}
        onUpdateQuest={onUpdateQuest}
        onRemoveQuest={onRemoveQuest}
        deleteMode={deleteMode}
        addMode={addMode}
        collapsed={collapsed}
        onToggleCollapsed={onToggleCollapsed}
      />
    </section>
  );
}

function QuestRow({
  quest,
  onUpdate,
  onRemove,
  deleteMode = false,
  addMode = false,
}: {
  quest: QuestProgress;
  onUpdate?: (quest: QuestProgress) => void;
  onRemove?: () => void;
  deleteMode?: boolean;
  addMode?: boolean;
}) {
  const completed = quest.objectives.filter((objective) => objective.completed).length;
  const totalObjectives = quest.objectives.length;
  const completionPercent = quest.completed ? 100 : totalObjectives > 0 ? (completed / totalObjectives) * 100 : 0;
  const completionLabel = totalObjectives > 0 ? `${completed}/${totalObjectives}` : quest.completed ? "done" : "open";
  const objectiveGridColumns = deleteMode
    ? "grid-cols-[0.875rem_minmax(0,1fr)_1rem]"
    : "grid-cols-[0.875rem_minmax(0,1fr)]";
  const questTitle = visibleText(quest.name, "Quest");
  const updateObjective = (index: number, nextText: string) => {
    if (!onUpdate) return;
    onUpdate(updateQuestObjectiveText(quest, index, nextText));
  };
  const toggleObjective = (index: number) => {
    if (!onUpdate) return;
    onUpdate(toggleQuestObjectiveCompletion(quest, index));
  };
  const removeObjective = (index: number) => {
    if (!onUpdate) return;
    onUpdate(removeQuestObjective(quest, index));
  };
  const addObjective = () => {
    if (!onUpdate) return;
    onUpdate(addQuestObjective(quest));
  };
  return (
    <article
      className={cn(
        "group/quest relative mx-1 overflow-hidden rounded-sm border border-[var(--border)]/30 bg-[color-mix(in_srgb,var(--background)_22%,transparent)] shadow-[inset_0_1px_0_color-mix(in_srgb,var(--foreground)_5%,transparent)]",
        quest.completed && "opacity-75",
      )}
    >
      <div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-[var(--primary)]/16" />
      <div
        className={cn(
          "relative grid min-h-5 grid-cols-[1rem_minmax(0,1fr)_auto] items-center gap-1 px-1 py-0.5",
          deleteMode && "grid-cols-[1rem_minmax(0,1fr)_auto_1rem]",
        )}
      >
        {onUpdate && (
          <button
            type="button"
            onClick={() => onUpdate({ ...quest, completed: !quest.completed })}
            className={cn(
              "flex h-4 w-4 shrink-0 items-center justify-center rounded-sm text-[var(--muted-foreground)] transition-colors hover:bg-[var(--primary)]/10 hover:text-emerald-300",
              quest.completed && "text-emerald-300",
            )}
            title={quest.completed ? "Mark incomplete" : "Mark complete"}
            aria-label={quest.completed ? "Mark quest incomplete" : "Mark quest complete"}
          >
            {quest.completed ? <CheckCircle2 size="0.75rem" /> : <Target size="0.75rem" />}
          </button>
        )}
        {!onUpdate && (
          <span className="flex h-4 w-4 shrink-0 items-center justify-center text-[var(--muted-foreground)]">
            {quest.completed ? <CheckCircle2 size="0.75rem" /> : <Target size="0.75rem" />}
          </span>
        )}
        {onUpdate ? (
          <InlineEdit
            value={quest.name}
            onSave={(name) => onUpdate({ ...quest, name: name || "Quest" })}
            placeholder="Quest"
            title={`Quest: ${questTitle}`}
            showEditHint={false}
            className={cn(
              "h-5 w-full min-w-0 overflow-hidden px-0.5 py-0 text-[0.75rem] font-semibold leading-5 text-[var(--foreground)]/92 hover:bg-[var(--accent)]/20",
              quest.completed && "line-through opacity-60",
            )}
          />
        ) : (
          <div
            className={cn(
              "min-w-0 truncate text-[0.75rem] font-semibold",
              quest.completed && "text-[var(--muted-foreground)] line-through",
            )}
          >
            {questTitle}
          </div>
        )}
        <span className="shrink-0 rounded-sm border border-[var(--border)]/32 bg-[var(--background)]/18 px-1 py-0.5 text-[0.5625rem] font-semibold uppercase leading-none tabular-nums text-[var(--foreground)]/68">
          {completionLabel}
        </span>
        {onRemove && deleteMode && (
          <button
            type="button"
            onClick={onRemove}
            className="flex h-4 w-4 items-center justify-center rounded-sm text-[var(--destructive)] transition-all hover:bg-[var(--accent)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--primary)] active:scale-90"
            title="Remove quest"
          >
            <X size="0.625rem" />
          </button>
        )}
      </div>

      <div className={cn("relative mx-1 overflow-hidden bg-[var(--border)]/28", TRACKER_BAR)}>
        <div
          className={cn(
            "h-full rounded-[1px] transition-[width] duration-200",
            quest.completed ? "bg-emerald-300/85" : "bg-[var(--primary)]/85",
          )}
          style={{ width: `${completionPercent}%` }}
        />
      </div>

      {(quest.objectives.length > 0 || (onUpdate && addMode)) && (
        <div className="relative mx-1 mb-0.5 mt-0.5 grid gap-px pl-4">
          <span
            className={cn(
              "pointer-events-none absolute left-[0.4375rem] top-1 w-px bg-[var(--border)]/28",
              addMode ? "bottom-4" : "bottom-1",
            )}
          />
          {quest.objectives.map((objective, index) => (
            <div
              key={`${objective.text}-${index}`}
              className={cn(
                "relative grid min-h-4 items-center gap-1 rounded-[2px] px-0.5 text-[0.6875rem] leading-4 transition-colors hover:bg-[var(--accent)]/14",
                objectiveGridColumns,
              )}
            >
              {onUpdate ? (
                <button
                  type="button"
                  onClick={() => toggleObjective(index)}
                  className={cn(
                    "flex h-4 w-4 shrink-0 items-center justify-center rounded-sm text-[var(--muted-foreground)] transition-colors hover:bg-[var(--primary)]/10 hover:text-emerald-300",
                    objective.completed && "text-emerald-300",
                  )}
                  title={objective.completed ? "Mark incomplete" : "Mark complete"}
                  aria-label={objective.completed ? "Mark objective incomplete" : "Mark objective complete"}
                >
                  {objective.completed ? <CheckCircle2 size="0.6875rem" /> : <Circle size="0.6875rem" />}
                </button>
              ) : objective.completed ? (
                <CheckCircle2 size="0.6875rem" className="shrink-0 text-emerald-300" />
              ) : (
                <Circle size="0.6875rem" className="shrink-0 text-[var(--muted-foreground)]" />
              )}
              {onUpdate ? (
                <InlineEdit
                  value={objective.text}
                  onSave={(text) => updateObjective(index, text || "Objective")}
                  placeholder="Objective"
                  title={`Objective: ${visibleText(objective.text, "Objective")}`}
                  showEditHint={false}
                  className={cn(
                    "h-4 w-full min-w-0 overflow-hidden px-0.5 py-0 text-[0.6875rem] leading-4 hover:bg-[var(--accent)]/20",
                    objective.completed && "line-through opacity-60",
                  )}
                />
              ) : (
                <span
                  className={cn(
                    "min-w-0 truncate",
                    objective.completed ? "text-[var(--muted-foreground)] line-through" : "text-[var(--foreground)]",
                  )}
                >
                  {visibleText(objective.text, "Objective")}
                </span>
              )}
              {onUpdate && deleteMode && (
                <button
                  type="button"
                  onClick={() => removeObjective(index)}
                  className="flex h-4 w-4 items-center justify-center rounded-sm text-[var(--destructive)] transition-all hover:bg-[var(--accent)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--primary)] active:scale-90"
                  title="Remove objective"
                >
                  <X size="0.5rem" />
                </button>
              )}
            </div>
          ))}
          {onUpdate && addMode && (
            <button
              type="button"
              onClick={addObjective}
              className="relative grid h-4 w-full grid-cols-[0.875rem_minmax(0,1fr)] items-center gap-1 rounded-[2px] px-0.5 text-left text-[0.6875rem] leading-4 text-[var(--foreground)]/35 transition-colors hover:bg-[var(--primary)]/10 hover:text-[var(--primary)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--primary)]"
              title="Add objective"
              aria-label="Add objective"
            >
              <Plus size="0.625rem" className="justify-self-center" />
              <span className="truncate font-medium">Objective</span>
            </button>
          )}
        </div>
      )}
    </article>
  );
}
