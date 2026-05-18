import { useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { retryGenerationAgents, startGeneration } from "../../../engine/generation/start-generation";
import { backfillConversationSummaries } from "../../../engine/modes/chat/core/summaries/auto-summary.service";
import {
  EDITABLE_CHARACTER_CARD_FIELDS,
  type AgentResult,
  type CharacterCardFieldUpdate,
  type EditableCharacterCardField,
} from "../../../engine/contracts/types/agent";
import type { Chat } from "../../../engine/contracts/types/chat";
import { chatBackgroundMetadataToUrl } from "../../../shared/lib/backgrounds";
import { llmApi } from "../../../shared/api/llm-api";
import { storageApi } from "../../../shared/api/storage-api";
import { ApiError } from "../../../shared/api/api-client";
import { useAgentStore, type PendingCardUpdate } from "../../../shared/stores/agent.store";
import { useChatStore } from "../../../shared/stores/chat.store";
import { useUIStore } from "../../../shared/stores/ui.store";
import { useGameStateStore } from "../../world-state/stores/world-state.store";
import { chatKeys } from "../../chats/hooks/use-chats";
import { characterKeys } from "../../characters/hooks/use-characters";

type GenerateArgs = {
  chatId: string;
  connectionId?: string | null;
  message?: string;
  [key: string]: unknown;
};

type StreamEvent = { type: string; data?: unknown };

function errorMessage(error: unknown): string {
  if (error instanceof ApiError) return error.message;
  if (error instanceof Error) return error.message;
  return String(error ?? "Generation failed");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function readString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function parseMaybeRecord(value: unknown): Record<string, unknown> {
  if (typeof value === "string") {
    try {
      const parsed = JSON.parse(value);
      return isRecord(parsed) ? parsed : {};
    } catch {
      return {};
    }
  }
  return isRecord(value) ? value : {};
}

const editableCharacterCardFieldSet = new Set<string>(EDITABLE_CHARACTER_CARD_FIELDS);

function parseCardFieldUpdate(raw: unknown): CharacterCardFieldUpdate | null {
  if (!isRecord(raw)) return null;
  if (raw.action !== "update") return null;
  const characterId = readString(raw.characterId).trim();
  const field = readString(raw.field);
  const oldText = readString(raw.oldText);
  const newText = readString(raw.newText);
  if (!characterId || !editableCharacterCardFieldSet.has(field) || oldText === newText) return null;
  return {
    characterId,
    action: "update",
    field: field as EditableCharacterCardField,
    oldText,
    newText,
    reason: readString(raw.reason),
  };
}

function normalizeIdList(value: unknown): string[] {
  if (Array.isArray(value)) return value.filter((item): item is string => typeof item === "string" && item.length > 0);
  if (typeof value !== "string") return [];
  try {
    const parsed = JSON.parse(value);
    return normalizeIdList(parsed);
  } catch {
    return [];
  }
}

function parseAgentResult(raw: unknown): AgentResult | null {
  if (!isRecord(raw)) return null;
  const agentType = readString(raw.agentType) || readString(raw.agentId) || "agent";
  const type = (readString(raw.type) || readString(raw.resultType) || agentType) as AgentResult["type"];
  return {
    agentId: readString(raw.agentId) || agentType,
    agentType,
    type,
    data: raw.data,
    tokensUsed: typeof raw.tokensUsed === "number" ? raw.tokensUsed : 0,
    durationMs: typeof raw.durationMs === "number" ? raw.durationMs : 0,
    success: raw.success !== false,
    error: typeof raw.error === "string" ? raw.error : null,
  };
}

function characterNameFromRow(row: Record<string, unknown> | undefined, fallback = "Character"): string {
  const data = parseMaybeRecord(row?.data);
  return readString(data.name).trim() || readString(row?.name).trim() || fallback;
}

async function buildPendingCardUpdates(
  queryClient: ReturnType<typeof useQueryClient>,
  chatId: string,
  agentName: string,
  rawData: unknown,
): Promise<PendingCardUpdate[]> {
  const data = parseMaybeRecord(rawData);
  const rawUpdates = Array.isArray(data.updates) ? data.updates : [];
  const updates = rawUpdates.map(parseCardFieldUpdate).filter((update): update is CharacterCardFieldUpdate => !!update);
  if (updates.length === 0) return [];

  let chat = queryClient.getQueryData<Chat>(chatKeys.detail(chatId));
  if (!chat) {
    try {
      chat = (await storageApi.get("chats", chatId)) as Chat;
    } catch {
      return [];
    }
  }

  const chatCharacterIds = normalizeIdList((chat as unknown as Record<string, unknown>).characterIds);
  if (chatCharacterIds.length === 0) return [];
  const chatCharacterIdSet = new Set(chatCharacterIds);

  let characters = queryClient.getQueryData<Record<string, unknown>[]>(characterKeys.list());
  if (!characters) {
    try {
      characters = (await storageApi.list("characters")) as Record<string, unknown>[];
      queryClient.setQueryData(characterKeys.list(), characters);
    } catch {
      characters = [];
    }
  }

  const groupedUpdates = new Map<string, CharacterCardFieldUpdate[]>();
  for (const update of updates) {
    if (!chatCharacterIdSet.has(update.characterId)) continue;
    groupedUpdates.set(update.characterId, [...(groupedUpdates.get(update.characterId) ?? []), update]);
  }
  if (groupedUpdates.size === 0) return [];

  const timestamp = Date.now();
  return chatCharacterIds.flatMap((characterId, index) => {
    const grouped = groupedUpdates.get(characterId);
    if (!grouped?.length) return [];
    const row = characters.find((character) => readString(character.id) === characterId);
    return [
      {
        id: `card-update-${characterId}-${timestamp}-${index}`,
        characterId,
        characterName: characterNameFromRow(row),
        updates: grouped,
        agentName,
        timestamp: timestamp + index,
      },
    ];
  });
}

function formatAgentBubble(result: AgentResult, agentName: string): string | null {
  const data = parseMaybeRecord(result.data);
  if (!Object.keys(data).length) return null;

  switch (result.agentType) {
    case "continuity": {
      const issues = Array.isArray(data.issues) ? data.issues : [];
      return issues
        .map((issue) => parseMaybeRecord(issue).description)
        .filter((description): description is string => typeof description === "string" && description.trim().length > 0)
        .join("\n") || null;
    }
    case "prompt-reviewer": {
      const issues = Array.isArray(data.issues) ? data.issues : [];
      if (issues.length === 0) return readString(data.summary, "Prompt looks good");
      return issues
        .map((issue) => parseMaybeRecord(issue).description)
        .filter((description): description is string => typeof description === "string" && description.trim().length > 0)
        .join("\n") || null;
    }
    case "director":
    case "prose-guardian":
    case "chat-summary":
    case "secret-plot-driver":
      return readString(data.text).trim() || (result.agentType === "secret-plot-driver" ? "Secret plotline active." : null);
    case "quest": {
      const updates = Array.isArray(data.updates) ? data.updates : [];
      return updates
        .map((update) => readString(parseMaybeRecord(update).questName).trim())
        .filter(Boolean)
        .join("\n") || null;
    }
    case "expression": {
      const expressions = Array.isArray(data.expressions) ? data.expressions : [];
      return expressions
        .map((entry) => {
          const record = parseMaybeRecord(entry);
          const name = readString(record.characterName).trim();
          const expression = readString(record.expression).trim();
          return name && expression ? `${name}: ${expression}` : "";
        })
        .filter(Boolean)
        .join("\n") || null;
    }
    case "world-state": {
      const parts = [data.location, data.time, data.weather]
        .map((part) => readString(part).trim())
        .filter(Boolean);
      return parts.length ? parts.join(" - ") : null;
    }
    case "character-tracker": {
      const present = Array.isArray(data.presentCharacters) ? data.presentCharacters : [];
      return present
        .map((entry) => readString(parseMaybeRecord(entry).name).trim())
        .filter(Boolean)
        .join(", ") || null;
    }
    case "background": {
      const chosen = readString(data.chosen).trim();
      return chosen ? `Background: ${chosen}` : null;
    }
    case "echo-chamber": {
      const reactions = Array.isArray(data.reactions) ? data.reactions : [];
      return reactions
        .map((entry) => {
          const record = parseMaybeRecord(entry);
          const name = readString(record.characterName).trim();
          const reaction = readString(record.reaction).trim();
          return name && reaction ? `${name}: ${reaction}` : "";
        })
        .filter(Boolean)
        .join("\n") || null;
    }
    case "spotify": {
      const action = readString(data.action);
      if (action === "none") return readString(data.mood, "Keeping current track");
      if (action === "volume") return `Volume: ${data.volume ?? ""}`.trim();
      const trackNames = Array.isArray(data.trackNames)
        ? data.trackNames.map((track) => readString(track).trim()).filter(Boolean)
        : [readString(data.trackName).trim()].filter(Boolean);
      return trackNames.length ? trackNames.join("\n") : readString(data.mood).trim() || null;
    }
    case "persona-stats": {
      const status = readString(data.status).trim();
      const stats = Array.isArray(data.stats) ? data.stats : [];
      const statLines = stats
        .map((entry) => {
          const record = parseMaybeRecord(entry);
          const name = readString(record.name).trim();
          return name ? `${name}: ${record.value ?? ""}/${record.max ?? 100}` : "";
        })
        .filter(Boolean);
      return [status, ...statLines].filter(Boolean).join(" - ") || null;
    }
    case "illustrator":
      return data.shouldGenerate === true ? readString(data.reason, "Generating scene illustration") : null;
    case "lorebook-keeper": {
      const updates = Array.isArray(data.updates) ? data.updates : [];
      return updates
        .map((entry) => readString(parseMaybeRecord(entry).entryName).trim())
        .filter(Boolean)
        .join("\n") || null;
    }
    case "editor": {
      const changes = Array.isArray(data.changes) ? data.changes : [];
      if (changes.length === 0) return "No edits needed";
      return changes
        .map((entry) => readString(parseMaybeRecord(entry).description).trim())
        .filter(Boolean)
        .join("\n") || null;
    }
    case "html":
      return readString(data.text, "HTML formatting active");
    default:
      return agentName ? null : null;
  }
}

function applyBackgroundChoice(chosen: unknown) {
  const url = chatBackgroundMetadataToUrl(chosen);
  if (url) useUIStore.getState().setChatBackground(url);
}

function applyQuestUpdates(rawData: unknown) {
  const data = parseMaybeRecord(rawData);
  const updates = Array.isArray(data.updates) ? data.updates.map(parseMaybeRecord) : [];
  if (updates.length === 0) return;

  const current = useGameStateStore.getState().current;
  const existingPlayerStats = parseMaybeRecord(current?.playerStats);
  const quests = Array.isArray(existingPlayerStats.activeQuests)
    ? [...existingPlayerStats.activeQuests.map(parseMaybeRecord)]
    : [];

  for (const update of updates) {
    const questName = readString(update.questName).trim();
    if (!questName) continue;
    const action = readString(update.action, "update");
    const index = quests.findIndex((quest) => readString(quest.name) === questName);
    if (action === "create" && index === -1) {
      quests.push({
        questEntryId: questName,
        name: questName,
        currentStage: 0,
        objectives: Array.isArray(update.objectives) ? update.objectives : [],
        completed: false,
      });
    } else if (index !== -1) {
      if (action === "fail") {
        quests.splice(index, 1);
      } else {
        quests[index] = {
          ...quests[index],
          ...(Array.isArray(update.objectives) ? { objectives: update.objectives } : {}),
          ...(action === "complete" ? { completed: true } : {}),
        };
      }
    }
  }

  useGameStateStore.getState().setGameState({
    ...(current ?? ({} as never)),
    playerStats: { ...existingPlayerStats, activeQuests: quests },
  } as never);
}

async function applyAgentResultEffects(
  queryClient: ReturnType<typeof useQueryClient>,
  chatId: string,
  rawResult: unknown,
) {
  const result = parseAgentResult(rawResult);
  if (!result) return;
  const agentName =
    readString((rawResult as Record<string, unknown>).agentName).trim() ||
    readString((rawResult as Record<string, unknown>).name).trim() ||
    result.agentType;
  const agentStore = useAgentStore.getState();
  agentStore.addResult(result.agentId || result.agentType, result);

  if (!result.success) return;
  const bubble = formatAgentBubble(result, agentName);
  if (bubble) agentStore.addThoughtBubble(result.agentType, agentName, bubble);

  const data = parseMaybeRecord(result.data);
  if (result.agentType === "echo-chamber") {
    const reactions = Array.isArray(data.reactions) ? data.reactions : [];
    for (const reaction of reactions) {
      const record = parseMaybeRecord(reaction);
      const characterName = readString(record.characterName).trim();
      const text = readString(record.reaction).trim();
      if (characterName && text) agentStore.addEchoMessage(characterName, text);
    }
  }

  if (result.agentType === "cyoa" || result.type === "cyoa_choices") {
    const rawChoices = Array.isArray(data.choices) ? data.choices : [];
    const choices = rawChoices
      .map((choice) => {
        const record = parseMaybeRecord(choice);
        const label = readString(record.label).trim();
        const text = readString(record.text).trim();
        return label && text ? { label, text } : null;
      })
      .filter((choice): choice is { label: string; text: string } => !!choice);
    if (choices.length) agentStore.setCyoaChoices(choices, chatId);
  }

  if (result.type === "character_card_update") {
    const pending = await buildPendingCardUpdates(queryClient, chatId, agentName, result.data);
    for (const entry of pending) agentStore.enqueuePendingCardUpdate(entry);
    if (pending.length) useUIStore.getState().openModal("character-card-update");
  }

  if (result.type === "background_change") applyBackgroundChoice(data.chosen);
  if (result.agentType === "quest") applyQuestUpdates(result.data);
}

export function useGenerate() {
  const queryClient = useQueryClient();

  const generate = useCallback(
    async (args: GenerateArgs): Promise<boolean> => {
      const chatId = args.chatId;
      const controller = new AbortController();
      const chatStore = useChatStore.getState();
      chatStore.setAbortController(chatId, controller);
      chatStore.setStreaming(true, chatId);
      chatStore.setGenerationPhase("Starting generation...");
      chatStore.setStreamBuffer("", chatId);
      chatStore.setThinkingBuffer("", chatId);
      useAgentStore.getState().setProcessing(true);

      let received = "";
      try {
        await backfillConversationSummaries(
          { storage: storageApi, llm: llmApi },
          { chatId, connectionId: typeof args.connectionId === "string" ? args.connectionId : null, maxMissingDays: 2 },
        ).catch(() => {
          // Summary refresh should never block an otherwise valid generation.
        });
        for await (const event of startGeneration(
          { storage: storageApi, llm: llmApi },
          args,
          controller.signal,
        ) as AsyncGenerator<StreamEvent>) {
          switch (event.type) {
            case "phase":
              if (typeof event.data === "string") {
                useChatStore.getState().setGenerationPhase(event.data);
              }
              break;
            case "thinking":
              if (typeof event.data === "string") {
                useChatStore.getState().appendThinkingBuffer(event.data, chatId);
              }
              break;
            case "token":
            case "delta":
              if (typeof event.data === "string") {
                received += event.data;
                useChatStore.getState().appendStreamBuffer(event.data, chatId);
                useChatStore.getState().setMariPhase(chatId, "thinking");
              }
              break;
            case "message":
            case "assistant_message":
              if (event.data && typeof event.data === "object") {
                await queryClient.invalidateQueries({ queryKey: ["chats"] });
              }
              break;
            case "agent_result":
              await applyAgentResultEffects(queryClient, chatId, event.data);
              break;
            case "done":
              break;
          }
        }
        await queryClient.invalidateQueries({ queryKey: ["chats"] });
        return received.length > 0;
      } catch (error) {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          toast.error(errorMessage(error));
        }
        throw error;
      } finally {
        useChatStore.getState().setAbortController(chatId, null);
        useChatStore.getState().setStreaming(false, chatId);
        useChatStore.getState().setMariPhase(chatId, "idle");
        useChatStore.getState().setGenerationPhase(null);
        useChatStore.getState().setTypingCharacterName(null);
        useChatStore.getState().setStreamingCharacterId(null);
        useAgentStore.getState().setProcessing(false);
        await queryClient.invalidateQueries({ queryKey: ["chats"] });
      }
    },
    [queryClient],
  );

  const retryAgents = useCallback(
    async (chatId: string, agentTypes?: string[], options?: Record<string, unknown>) => {
      useAgentStore.getState().setProcessing(true);
      try {
        const results = await retryGenerationAgents(
          { storage: storageApi, llm: llmApi },
          { chatId, agentTypes, options },
        );
        for (const result of results) {
          await applyAgentResultEffects(queryClient, chatId, result);
        }
        useAgentStore.getState().clearFailedAgentTypes();
        await queryClient.invalidateQueries({ queryKey: ["agents"] });
        await queryClient.invalidateQueries({ queryKey: ["chats"] });
      } catch (error) {
        toast.error(errorMessage(error));
        throw error;
      } finally {
        useAgentStore.getState().setProcessing(false);
      }
    },
    [queryClient],
  );

  return { generate, retryAgents };
}
