import type {
  CharacterStat,
  CustomTrackerField,
  InventoryItem,
  PresentCharacter,
  QuestProgress,
} from "../../../engine/contracts/types/game-state";

export function replaceTrackerListItem<T>(items: readonly T[], index: number, item: T): T[] {
  if (index < 0 || index >= items.length) return [...items];
  return items.map((current, currentIndex) => (currentIndex === index ? item : current));
}

export function removeTrackerListItem<T>(items: readonly T[], index: number): T[] {
  return items.filter((_, currentIndex) => currentIndex !== index);
}

export function appendTrackerListItem<T>(items: readonly T[], item: T): T[] {
  return [...items, item];
}

export function createManualPresentCharacter(options: Partial<PresentCharacter> = {}): PresentCharacter {
  return {
    characterId: options.characterId ?? `manual-${Date.now()}`,
    name: options.name ?? "New Character",
    emoji: options.emoji ?? "?",
    mood: options.mood ?? "",
    appearance: options.appearance ?? null,
    outfit: options.outfit ?? null,
    avatarPath: options.avatarPath,
    portraitFocusX: options.portraitFocusX,
    portraitFocusY: options.portraitFocusY,
    customFields: options.customFields ?? {},
    stats: options.stats ?? [],
    thoughts: options.thoughts ?? null,
  };
}

export function createManualInventoryItem(options: Partial<InventoryItem> = {}): InventoryItem {
  return {
    name: options.name ?? "New Item",
    description: options.description ?? "",
    quantity: options.quantity ?? 1,
    location: options.location ?? "on_person",
  };
}

export function createManualQuestObjective(options: Partial<QuestProgress["objectives"][number]> = {}) {
  return {
    text: options.text ?? "New objective",
    completed: options.completed ?? false,
  };
}

export function createManualQuest(options: Partial<QuestProgress> = {}): QuestProgress {
  return {
    questEntryId: options.questEntryId ?? `manual-${Date.now()}`,
    name: options.name ?? "New Quest",
    currentStage: options.currentStage ?? 0,
    objectives: options.objectives ?? [createManualQuestObjective({ text: "Objective 1" })],
    completed: options.completed ?? false,
  };
}

export function createManualCustomTrackerField(
  options: Partial<CustomTrackerField> = {},
): CustomTrackerField {
  return {
    name: options.name ?? "New Field",
    value: options.value ?? "",
  };
}

export function createManualCharacterStat(options: Partial<CharacterStat> = {}): CharacterStat {
  return {
    name: options.name ?? "New Stat",
    value: options.value ?? 0,
    max: options.max ?? 100,
    color: options.color ?? "var(--primary)",
  };
}

export function addPresentCharacterStat(
  character: PresentCharacter,
  stat = createManualCharacterStat(),
): PresentCharacter {
  return {
    ...character,
    stats: appendTrackerListItem(character.stats ?? [], stat),
  };
}

export function updatePresentCharacterCustomField(
  character: PresentCharacter,
  oldName: string,
  nextName: string,
  nextValue: string,
): PresentCharacter | null {
  const nextFields = { ...(character.customFields ?? {}) };
  const trimmedName = nextName.trim();
  if (trimmedName && trimmedName !== oldName && Object.prototype.hasOwnProperty.call(nextFields, trimmedName)) {
    return null;
  }
  delete nextFields[oldName];
  if (trimmedName) nextFields[trimmedName] = nextValue;
  return { ...character, customFields: nextFields };
}

export function addQuestObjective(
  quest: QuestProgress,
  objective = createManualQuestObjective(),
): QuestProgress {
  return {
    ...quest,
    objectives: appendTrackerListItem(quest.objectives, objective),
  };
}

export function replaceQuestObjective(
  quest: QuestProgress,
  index: number,
  objective: QuestProgress["objectives"][number],
): QuestProgress {
  return {
    ...quest,
    objectives: replaceTrackerListItem(quest.objectives, index, objective),
  };
}

export function removeQuestObjective(quest: QuestProgress, index: number): QuestProgress {
  return {
    ...quest,
    objectives: removeTrackerListItem(quest.objectives, index),
  };
}

export function updateQuestObjectiveText(quest: QuestProgress, index: number, text: string): QuestProgress {
  const objective = quest.objectives[index];
  if (!objective) return quest;
  return replaceQuestObjective(quest, index, { ...objective, text });
}

export function toggleQuestObjectiveCompletion(quest: QuestProgress, index: number): QuestProgress {
  const objective = quest.objectives[index];
  if (!objective) return quest;
  return replaceQuestObjective(quest, index, { ...objective, completed: !objective.completed });
}
