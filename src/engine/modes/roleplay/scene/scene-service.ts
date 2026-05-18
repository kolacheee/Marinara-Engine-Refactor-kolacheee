import type { LlmGateway, StorageGateway } from "../../../capabilities";
import { parseJsonArray, parseJsonObject } from "../../../core/json";
import { parseGameJsonish } from "../../../shared/parsing-jsonish";
import type {
  SceneAnalysis,
  SceneCreateRequest,
  SceneCreateResponse,
  SceneForkRequest,
  SceneForkResponse,
  SceneFullPlan,
  ScenePlanRequest,
  ScenePlanResponse,
} from "@marinara-engine/shared";

type JsonRecord = Record<string, unknown>;

type RoleplaySceneCapabilities = {
  storage: StorageGateway;
  llm: LlmGateway;
};

type StoredMessage = JsonRecord & {
  id?: string;
  role?: string;
  content?: string;
  characterId?: string | null;
};

export async function planRoleplayScene(
  capabilities: RoleplaySceneCapabilities,
  input: ScenePlanRequest,
): Promise<ScenePlanResponse> {
  const chat = await requireChat(capabilities.storage, input.chatId);
  const prompt = input.prompt.trim();
  const fallback = await fallbackScenePlan(capabilities.storage, input.chatId, prompt);
  const allowedCharacterIds = stringArray(chat.characterIds);

  let connectionId: string;
  try {
    connectionId = await resolveConnectionId(capabilities.storage, chat, input.connectionId ?? null);
  } catch (error) {
    return {
      plan: fallback,
      error: `Used local scene planning because no LLM connection was available: ${errorMessage(error)}`,
    };
  }

  const history = (await messagesForChat(capabilities.storage, input.chatId))
    .slice(-20)
    .map((message) => {
      const role = stringValue(message.role) || "user";
      const content = stringValue(message.content).trim();
      return content ? `${role}: ${content}` : "";
    })
    .filter(Boolean)
    .join("\n\n");

  const requestText = prompt
    ? `Plan a complete roleplay scene based on this request: ${prompt}`
    : "Plan a complete roleplay scene that naturally follows the recent conversation.";

  try {
    const raw = await capabilities.llm.complete({
      connectionId,
      messages: [
        {
          role: "system",
          content: [
            "You are a scene planner for Marinara roleplay.",
            "Return only one JSON object with fields name, description, scenario, firstMessage, background, characterIds, systemPrompt, rating, relationshipHistory, and participationGuide.",
            "The name must start with Scene:. The rating must be sfw or nsfw. Use only character IDs from the provided list.",
          ].join("\n"),
        },
        {
          role: "user",
          content: [
            `Available character IDs: ${allowedCharacterIds.join(", ")}`,
            "",
            "Recent conversation:",
            history,
            "",
            requestText,
          ].join("\n"),
        },
      ],
      parameters: { temperature: 0.9, maxTokens: 4096 },
    });
    const parsed = parseObject(raw);
    if (Object.keys(parsed).length === 0) {
      return {
        plan: fallback,
        error: "The model did not return valid scene-plan JSON, so Marinara used a local fallback plan.",
      };
    }
    return { plan: sanitizeScenePlan(parsed, fallback, allowedCharacterIds) };
  } catch (error) {
    return {
      plan: fallback,
      error: `Scene planning used a local fallback after the LLM request failed: ${errorMessage(error)}`,
    };
  }
}

export async function createRoleplayScene(
  storage: StorageGateway,
  input: SceneCreateRequest,
): Promise<SceneCreateResponse> {
  const originChat = await requireChat(storage, input.originChatId);
  const plan = input.plan;
  const originCharacterIds = stringArray(originChat.characterIds);
  const characterIds = plan.characterIds.length ? plan.characterIds : originCharacterIds;
  const sceneName = safeTitle(plan.name, "New Scene");
  const description = plan.description || "A new scene begins.";
  const firstMessage = plan.firstMessage || "The scene begins.";
  const connectionId = input.connectionId || stringValue(originChat.connectionId) || null;

  const metadata: JsonRecord = {
    sceneOriginChatId: input.originChatId,
    sceneInitiatorCharId: input.initiatorCharId ?? null,
    sceneDescription: description,
    sceneScenario: plan.scenario ?? null,
    sceneBackground: plan.background ?? null,
    sceneSystemPrompt: plan.systemPrompt ?? null,
    sceneRelationshipHistory: plan.relationshipHistory ?? null,
    sceneRating: plan.rating === "nsfw" ? "nsfw" : "sfw",
    sceneStatus: "active",
    enableMemoryRecall: true,
    ...(plan.background ? { background: plan.background } : {}),
  };

  const sceneChat = await storage.create<JsonRecord>("chats", {
    name: sceneName,
    mode: "roleplay",
    characterIds,
    groupId: originChat.groupId ?? null,
    personaId: originChat.personaId ?? null,
    promptPresetId: originChat.promptPresetId ?? null,
    connectionId,
    connectedChatId: input.originChatId,
    metadata,
  });
  const sceneChatId = stringValue(sceneChat.id);
  if (!sceneChatId) throw new Error("Created scene chat has no id");

  await patchChatMetadata(storage, input.originChatId, {
    activeSceneChatId: sceneChatId,
    sceneBusyCharIds: characterIds,
  });
  await storage.update("chats", input.originChatId, { connectedChatId: sceneChatId });

  if (plan.participationGuide.trim()) {
    await createChatMessage(storage, sceneChatId, {
      role: "narrator",
      content: plan.participationGuide,
      characterId: null,
    });
  }
  const firstCharacterId = input.initiatorCharId || characterIds[0] || null;
  await createChatMessage(storage, sceneChatId, {
    role: "assistant",
    content: [description, "", firstMessage].join("\n"),
    characterId: firstCharacterId,
  });

  return {
    chatId: sceneChatId,
    chatName: stringValue(sceneChat.name) || sceneName,
    description,
    background: plan.background ?? null,
  };
}

export async function concludeRoleplayScene(
  capabilities: RoleplaySceneCapabilities,
  input: { sceneChatId: string; connectionId?: string | null },
): Promise<{ summary: string; originChatId: string }> {
  const sceneChat = await requireChat(capabilities.storage, input.sceneChatId);
  const sceneMeta = parseJsonObject(sceneChat.metadata);
  const originChatId = stringValue(sceneMeta.sceneOriginChatId);
  if (!originChatId) throw new Error("Not a scene chat");
  const summary = await summarizeScene(capabilities, input.sceneChatId, input.connectionId ?? null);

  await createChatMessage(capabilities.storage, originChatId, {
    role: "narrator",
    content: `The scene concluded.\n\n${summary}`,
  });
  await patchChatMetadata(capabilities.storage, input.sceneChatId, { sceneStatus: "concluded" });
  await cleanOriginScenePointers(capabilities.storage, originChatId);
  await capabilities.storage.update("chats", input.sceneChatId, { connectedChatId: null });
  return { summary, originChatId };
}

export async function abandonRoleplayScene(
  storage: StorageGateway,
  input: { sceneChatId: string },
): Promise<{ originChatId: string }> {
  const sceneChat = await requireChat(storage, input.sceneChatId);
  const sceneMeta = parseJsonObject(sceneChat.metadata);
  const originChatId = stringValue(sceneMeta.sceneOriginChatId);
  if (!originChatId) throw new Error("Not a scene chat");
  await cleanOriginScenePointers(storage, originChatId);
  await deleteChatWithMessages(storage, input.sceneChatId);
  return { originChatId };
}

export async function forkRoleplayScene(
  storage: StorageGateway,
  input: SceneForkRequest,
): Promise<SceneForkResponse> {
  if (input.mode !== "clone" && input.mode !== "convert") {
    throw new Error("mode must be clone or convert");
  }
  const sceneChat = await requireChat(storage, input.sceneChatId);
  const sceneMeta = parseJsonObject(sceneChat.metadata);
  const originChatId = stringValue(sceneMeta.sceneOriginChatId) || null;
  const baseName = stringValue(sceneChat.name) || "Scene";
  const forkChat = await storage.create<JsonRecord>("chats", {
    name: `${baseName} ${input.mode === "clone" ? "Clone" : "Converted"}`,
    mode: "roleplay",
    characterIds: stringArray(sceneChat.characterIds),
    groupId: sceneChat.groupId ?? null,
    personaId: sceneChat.personaId ?? null,
    promptPresetId: sceneChat.promptPresetId ?? null,
    connectionId: sceneChat.connectionId ?? null,
    metadata: forkMetadata(sceneMeta),
  });
  const forkChatId = stringValue(forkChat.id);
  if (!forkChatId) throw new Error("Created fork chat has no id");

  let skippedGuide = false;
  for (const message of await messagesForChat(storage, input.sceneChatId)) {
    const stopAfterThis = input.upToMessageId && message.id === input.upToMessageId;
    if (input.includeParticipationGuide === false && !skippedGuide && message.role === "narrator") {
      skippedGuide = true;
      if (stopAfterThis) break;
      continue;
    }
    const copy = { ...message };
    delete copy.id;
    copy.chatId = forkChatId;
    await storage.create("messages", copy);
    if (stopAfterThis) break;
  }

  if (input.mode === "convert") {
    if (originChatId) await cleanOriginScenePointers(storage, originChatId);
    await deleteChatWithMessages(storage, input.sceneChatId);
  }

  return { chatId: forkChatId, originChatId, mode: input.mode };
}

export async function analyzeScene(
  capabilities: RoleplaySceneCapabilities,
  input: { chatId?: string; connectionId?: string | null; narration: string; context?: JsonRecord },
): Promise<SceneAnalysis> {
  let connectionId: string | null = null;
  try {
    const chat = input.chatId ? await capabilities.storage.get<JsonRecord>("chats", input.chatId) : null;
    connectionId = chat
      ? await resolveConnectionId(capabilities.storage, chat, input.connectionId ?? null)
      : await resolveConnectionId(capabilities.storage, {}, input.connectionId ?? null);
  } catch {
    return defaultSceneAnalysis();
  }

  const prompt = [
    "Analyze this roleplay scene narration and return only compact JSON with optional keys background, music, ambient, weather, timeOfDay, musicGenre, musicIntensity, locationKind, spotifyTrack, reputationChanges, segmentEffects, directions, illustration.",
    "Narration:",
    "",
    input.narration,
  ].join("\n");

  try {
    const raw = await capabilities.llm.complete({
      connectionId,
      messages: [{ role: "user", content: prompt }],
      parameters: { maxTokens: 800, temperature: 0.2 },
    });
    return sanitizeSceneAnalysis(parseObject(raw));
  } catch {
    return defaultSceneAnalysis();
  }
}

async function summarizeScene(
  capabilities: RoleplaySceneCapabilities,
  sceneChatId: string,
  connectionOverride?: string | null,
): Promise<string> {
  const messages = await messagesForChat(capabilities.storage, sceneChatId);
  const transcript = messages
    .map((message) => {
      const role = stringValue(message.role) || "user";
      const content = stringValue(message.content).trim();
      return content ? `${role}: ${content}` : "";
    })
    .filter(Boolean)
    .join("\n\n");
  const fallback = transcript
    ? `Scene summary: ${transcript.slice(0, 1200)}`
    : "The scene ended before any substantial roleplay occurred.";

  try {
    const sceneChat = await requireChat(capabilities.storage, sceneChatId);
    const connectionId = await resolveConnectionId(capabilities.storage, sceneChat, connectionOverride ?? null);
    const summary = await capabilities.llm.complete({
      connectionId,
      messages: [
        {
          role: "system",
          content: "Summarize the completed roleplay scene in concise third-person prose. Return only the summary.",
        },
        { role: "user", content: transcript },
      ],
      parameters: { temperature: 0.7, maxTokens: 800 },
    });
    return summary.trim() || fallback;
  } catch {
    return fallback;
  }
}

async function fallbackScenePlan(storage: StorageGateway, chatId: string, prompt: string): Promise<SceneFullPlan> {
  const chat = await requireChat(storage, chatId);
  const characterIds = stringArray(chat.characterIds);
  const history = (await messagesForChat(storage, chatId))
    .slice(-8)
    .map((message) => {
      const role = stringValue(message.role) || "user";
      const content = stringValue(message.content).trim();
      return content ? `${role}: ${content}` : "";
    })
    .filter(Boolean)
    .join("\n");
  const premise =
    prompt ||
    history.split(/\r?\n/).filter(Boolean).at(-1) ||
    "A focused roleplay scene continues from the current conversation.";
  return {
    name: safeTitle(premise, "New Scene"),
    description: `The scene opens around this premise: ${premise}`,
    scenario: history
      ? `Use the recent conversation as continuity and develop this premise: ${premise}\n\nRecent context:\n${history}`
      : premise,
    firstMessage: `The moment settles into focus. ${premise}`,
    background: null,
    characterIds,
    systemPrompt:
      "Write immersive roleplay prose with consistent point of view, clear character agency, and continuity from the originating conversation.",
    rating: "sfw",
    relationshipHistory: history,
    participationGuide: "Play the scene naturally and respond as your character would.",
  };
}

function sanitizeScenePlan(parsed: JsonRecord, fallback: SceneFullPlan, allowedCharacterIds: string[]): SceneFullPlan {
  const requestedIds = stringArray(parsed.characterIds);
  const characterIds =
    requestedIds.length === 0
      ? fallback.characterIds
      : allowedCharacterIds.length === 0
        ? requestedIds
        : requestedIds.filter((id) => allowedCharacterIds.includes(id));
  const background = stringValue(parsed.background);
  return {
    name: safeTitle(stringValue(parsed.name) || fallback.name, "New Scene"),
    description: stringValue(parsed.description) || fallback.description,
    scenario: stringValue(parsed.scenario) || fallback.scenario,
    firstMessage: stringValue(parsed.firstMessage) || fallback.firstMessage,
    background: background && background !== "null" ? background : null,
    characterIds,
    systemPrompt: stringValue(parsed.systemPrompt) || fallback.systemPrompt,
    rating: parsed.rating === "nsfw" ? "nsfw" : "sfw",
    relationshipHistory: stringValue(parsed.relationshipHistory) || fallback.relationshipHistory,
    participationGuide: stringValue(parsed.participationGuide) || fallback.participationGuide,
  };
}

function safeTitle(value: string, fallback: string): string {
  const title = (value.trim() || fallback)
    .replace(/[\r\n\t]/g, " ")
    .split(/\s+/)
    .join(" ")
    .slice(0, 60);
  return title.startsWith("Scene:") ? title : `Scene: ${title}`;
}

async function requireChat(storage: StorageGateway, chatId: string): Promise<JsonRecord> {
  const chat = await storage.get<JsonRecord>("chats", chatId);
  if (!chat) throw new Error("Chat not found");
  return chat;
}

async function messagesForChat(storage: StorageGateway, chatId: string): Promise<StoredMessage[]> {
  const rows = await storage.request<unknown>("GET", `/chats/${encodeURIComponent(chatId)}/messages`);
  return Array.isArray(rows) ? rows.filter(isRecord) : [];
}

async function createChatMessage(storage: StorageGateway, chatId: string, message: JsonRecord): Promise<void> {
  await storage.request("POST", `/chats/${encodeURIComponent(chatId)}/messages`, message);
}

async function patchChatMetadata(storage: StorageGateway, chatId: string, patch: JsonRecord): Promise<void> {
  await storage.request("PATCH", `/chats/${encodeURIComponent(chatId)}/metadata`, patch);
}

async function cleanOriginScenePointers(storage: StorageGateway, originChatId: string): Promise<void> {
  await patchChatMetadata(storage, originChatId, {
    activeSceneChatId: null,
    sceneBusyCharIds: null,
  });
  await storage.update("chats", originChatId, { connectedChatId: null });
}

async function deleteChatWithMessages(storage: StorageGateway, chatId: string): Promise<void> {
  for (const message of await messagesForChat(storage, chatId)) {
    if (message.id) {
      await storage.request("DELETE", `/chats/${encodeURIComponent(chatId)}/messages/${encodeURIComponent(message.id)}`);
    }
  }
  await storage.delete("chats", chatId);
}

function forkMetadata(sceneMeta: JsonRecord): JsonRecord {
  const excluded = new Set([
    "sceneOriginChatId",
    "sceneInitiatorCharId",
    "sceneDescription",
    "sceneScenario",
    "sceneSystemPrompt",
    "sceneRating",
    "sceneStatus",
    "sceneConversationContext",
    "sceneRelationshipHistory",
    "sceneBackground",
    "activeSceneChatId",
    "sceneBusyCharIds",
  ]);
  return Object.fromEntries(
    Object.entries(sceneMeta).filter(([key]) => !excluded.has(key) && !key.startsWith("scene")),
  );
}

async function resolveConnectionId(
  storage: StorageGateway,
  chat: JsonRecord,
  override?: string | null,
): Promise<string> {
  if (override?.trim()) return override.trim();
  const chatConnectionId = stringValue(chat.connectionId).trim();
  const connections = await storage.list<JsonRecord>("connections");
  if (chatConnectionId === "random") {
    const pool = connections.filter((connection) => connection.useForRandom === true);
    const selected = pool[Math.floor(Math.random() * pool.length)];
    if (!selected?.id) throw new Error("No connections marked for the random pool");
    return stringValue(selected.id);
  }
  if (chatConnectionId) return chatConnectionId;
  const selected =
    connections.find((connection) => connection.isDefault === true || connection.default === true) ?? connections[0];
  const id = stringValue(selected?.id);
  if (!id) throw new Error("No connection configured");
  return id;
}

function defaultSceneAnalysis(): SceneAnalysis {
  return {
    background: null,
    music: null,
    ambient: null,
    weather: null,
    timeOfDay: null,
    musicGenre: null,
    musicIntensity: null,
    locationKind: null,
    spotifyTrack: null,
    reputationChanges: [],
    segmentEffects: [],
    directions: [],
    illustration: null,
    generatedIllustration: null,
    generatedNpcAvatars: [],
  } as SceneAnalysis;
}

function sanitizeSceneAnalysis(parsed: JsonRecord): SceneAnalysis {
  return {
    ...defaultSceneAnalysis(),
    ...copyOptional(parsed, [
      "background",
      "music",
      "ambient",
      "weather",
      "timeOfDay",
      "musicGenre",
      "musicIntensity",
      "locationKind",
      "spotifyTrack",
      "illustration",
    ]),
    reputationChanges: Array.isArray(parsed.reputationChanges) ? parsed.reputationChanges : [],
    segmentEffects: Array.isArray(parsed.segmentEffects) ? parsed.segmentEffects : [],
    directions: Array.isArray(parsed.directions) ? parsed.directions : [],
  } as SceneAnalysis;
}

function copyOptional(source: JsonRecord, keys: string[]): JsonRecord {
  return Object.fromEntries(keys.filter((key) => key in source).map((key) => [key, source[key]]));
}

function parseObject(raw: string): JsonRecord {
  try {
    const parsed = parseGameJsonish(raw);
    return isRecord(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function stringArray(value: unknown): string[] {
  return parseJsonArray<string>(value).filter((item) => typeof item === "string" && item.trim().length > 0);
}

function stringValue(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function isRecord(value: unknown): value is JsonRecord {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error ?? "Unknown error");
}
