import type { LorebookEntry } from "../contracts/types/lorebook";
import type { ChatMLMessage, MarkerConfig, WrapFormat } from "../contracts/types/prompt";
import type { StorageGateway } from "../capabilities/storage";
import { injectAtDepth, processActivatedEntries } from "../generation-core/lorebooks/prompt-injector";
import { scanForActivatedEntries, type ActivatedEntry } from "../generation-core/lorebooks/keyword-scanner";
import { wrapContent } from "../generation-core/prompt/format-engine";
import { mergeAdjacentMessages, squashLeadingSystemMessages } from "../generation-core/prompt/merger";
import { applyRegexScriptsToPromptMessages } from "../generation-core/regex/regex-application";
import { resolveMacros, type MacroContext } from "../shared/macros/macro-engine";
import {
  bySortOrder,
  boolish,
  hiddenFromAi,
  isRecord,
  parseRecord,
  readNumber,
  readString,
  stringArray,
  type JsonRecord,
} from "./runtime-records";

export interface GenerationCharacterContext {
  id: string;
  name: string;
  description: string;
  personality?: string;
  scenario?: string;
  creatorNotes?: string;
  systemPrompt?: string;
  backstory?: string;
  appearance?: string;
  mesExample?: string;
  firstMes?: string;
  postHistoryInstructions?: string;
  tags: string[];
}

export interface GenerationPersonaContext {
  name: string;
  description: string;
  personality?: string;
  backstory?: string;
  appearance?: string;
  scenario?: string;
  personaStats?: { enabled: boolean; bars: Array<{ name: string; value: number; max: number; color: string }> };
  rpgStats?: {
    enabled: boolean;
    attributes: Array<{ name: string; value: number }>;
    hp: { value: number; max: number };
  };
}

export interface PromptAssemblyResult {
  messages: ChatMLMessage[];
  characters: GenerationCharacterContext[];
  persona: GenerationPersonaContext | null;
  activatedLorebookEntries: Array<{
    id: string;
    lorebookId: string;
    name: string;
    content: string;
    tag: string;
    matchedKeys: string[];
    order: number;
    constant: boolean;
  }>;
  chatSummary: string | null;
}

export interface PromptAssemblyInput {
  chat: JsonRecord;
  storedMessages: JsonRecord[];
  connection: JsonRecord;
  request: JsonRecord;
  latestUserInput: string;
  agentData?: Record<string, string>;
}

type PromptSectionRecord = JsonRecord & {
  role?: unknown;
  content?: unknown;
  name?: unknown;
  identifier?: unknown;
  markerConfig?: unknown;
};

function dataRecord(record: JsonRecord): JsonRecord {
  const data = parseRecord(record.data);
  return Object.keys(data).length > 0 ? data : record;
}

function field(source: JsonRecord, key: string): string {
  return readString(source[key]).trim();
}

function stringRecord(value: unknown): Record<string, string> {
  const record = parseRecord(value);
  return Object.fromEntries(
    Object.entries(record)
      .filter((entry): entry is [string, string | number | boolean] =>
        ["string", "number", "boolean"].includes(typeof entry[1]),
      )
      .map(([key, entry]) => [key, String(entry)]),
  );
}

function loadCharacterContext(record: JsonRecord): GenerationCharacterContext {
  const data = dataRecord(record);
  const name = field(data, "name") || field(record, "name") || "Character";
  return {
    id: field(record, "id") || field(data, "id") || name,
    name,
    description: field(data, "description") || field(record, "description"),
    personality: field(data, "personality") || undefined,
    scenario: field(data, "scenario") || undefined,
    creatorNotes: field(data, "creator_notes") || field(data, "creatorNotes") || undefined,
    systemPrompt: field(data, "system_prompt") || field(data, "systemPrompt") || undefined,
    backstory: field(data, "backstory") || undefined,
    appearance: field(data, "appearance") || undefined,
    mesExample: field(data, "mes_example") || field(data, "mesExample") || undefined,
    firstMes: field(data, "first_mes") || field(data, "firstMes") || undefined,
    postHistoryInstructions:
      field(data, "post_history_instructions") || field(data, "postHistoryInstructions") || undefined,
    tags: stringArray(data.tags ?? record.tags),
  };
}

function loadPersonaContext(record: JsonRecord): GenerationPersonaContext {
  const data = dataRecord(record);
  return {
    name: field(data, "name") || field(record, "name") || "User",
    description: field(data, "description") || field(record, "description"),
    personality: field(data, "personality") || undefined,
    backstory: field(data, "backstory") || undefined,
    appearance: field(data, "appearance") || undefined,
    scenario: field(data, "scenario") || undefined,
    personaStats: isPersonaStats(data.personaStats),
    rpgStats: isRpgStats(data.rpgStats),
  };
}

function isPersonaStats(value: unknown): GenerationPersonaContext["personaStats"] | undefined {
  if (!isRecord(value) || typeof value.enabled !== "boolean" || !Array.isArray(value.bars)) return undefined;
  const bars = value.bars.filter(
    (bar): bar is { name: string; value: number; max: number; color: string } =>
      isRecord(bar) &&
      typeof bar.name === "string" &&
      typeof bar.value === "number" &&
      typeof bar.max === "number" &&
      typeof bar.color === "string",
  );
  return { enabled: value.enabled, bars };
}

function isRpgStats(value: unknown): GenerationPersonaContext["rpgStats"] | undefined {
  if (!isRecord(value) || typeof value.enabled !== "boolean" || !isRecord(value.hp)) return undefined;
  const attributes = Array.isArray(value.attributes)
    ? value.attributes.filter(
        (attr): attr is { name: string; value: number } =>
          isRecord(attr) && typeof attr.name === "string" && typeof attr.value === "number",
      )
    : [];
  const hp = {
    value: readNumber(value.hp.value, 0),
    max: readNumber(value.hp.max, 0),
  };
  return { enabled: value.enabled, attributes, hp };
}

async function loadCharacters(storage: StorageGateway, chat: JsonRecord): Promise<GenerationCharacterContext[]> {
  const ids = stringArray(chat.characterIds);
  const rows = await Promise.all(ids.map((id) => storage.get<JsonRecord>("characters", id)));
  return rows.filter(isRecord).map(loadCharacterContext);
}

async function loadPersona(storage: StorageGateway, chat: JsonRecord): Promise<GenerationPersonaContext | null> {
  const personaId = readString(chat.personaId).trim();
  if (personaId) {
    const row = await storage.get<JsonRecord>("personas", personaId);
    return isRecord(row) ? loadPersonaContext(row) : null;
  }
  const active = (await storage.list<JsonRecord>("personas")).find(
    (persona) => boolish(persona.isActive, false) || boolish(persona.active, false),
  );
  return active ? loadPersonaContext(active) : null;
}

function promptPresetId(chat: JsonRecord, connection: JsonRecord, request: JsonRecord, defaultPromptId: string | null) {
  return (
    readString(request.promptPresetId).trim() ||
    readString(request.presetId).trim() ||
    readString(chat.promptPresetId).trim() ||
    readString(connection.promptPresetId).trim() ||
    defaultPromptId
  );
}

async function loadDefaultPromptId(storage: StorageGateway): Promise<string | null> {
  const prompts = await storage.list<JsonRecord>("prompts");
  return (
    prompts
      .find((prompt) => boolish(prompt.isDefault ?? prompt.default, false))
      ?.id?.toString()
      .trim() || null
  );
}

async function loadPromptSections(storage: StorageGateway, presetId: string): Promise<PromptSectionRecord[]> {
  const sections = await storage.list<PromptSectionRecord>(`prompts/${encodeURIComponent(presetId)}/sections`);
  return sections.filter(isRecord).sort(bySortOrder);
}

function markerConfig(section: PromptSectionRecord): MarkerConfig | null {
  const raw = section.markerConfig;
  if (isRecord(raw) && typeof raw.type === "string") return raw as unknown as MarkerConfig;
  if (typeof raw === "string" && raw.trim()) {
    const parsed = parseRecord(raw);
    if (typeof parsed.type === "string") return parsed as unknown as MarkerConfig;
  }
  const identifier = readString(section.identifier).toLowerCase();
  if (identifier.includes("chat") && identifier.includes("history")) return { type: "chat_history" };
  if (identifier.includes("dialogue")) return { type: "dialogue_examples" };
  if (identifier.includes("world") && identifier.includes("before")) return { type: "world_info_before" };
  if (identifier.includes("world") && identifier.includes("after")) return { type: "world_info_after" };
  if (identifier.includes("lore")) return { type: "lorebook" };
  if (identifier.includes("persona")) return { type: "persona" };
  if (identifier.includes("char")) return { type: "character" };
  return null;
}

function macroContext(input: {
  chat: JsonRecord;
  connection: JsonRecord;
  characters: GenerationCharacterContext[];
  persona: GenerationPersonaContext | null;
  latestUserInput: string;
  agentData?: Record<string, string>;
}): MacroContext {
  const first = input.characters[0];
  return {
    user: input.persona?.name || "User",
    char: first?.name || "Character",
    characters: input.characters.map((character) => character.name),
    characterProfiles: input.characters.map((character) => ({
      name: character.name,
      description: character.description,
      personality: character.personality,
      backstory: character.backstory,
      appearance: character.appearance,
      scenario: character.scenario,
      example: character.mesExample,
    })),
    variables: stringRecord(input.chat.promptVariables ?? input.chat.variableValues),
    lastInput: input.latestUserInput,
    chatId: readString(input.chat.id),
    model: readString(input.connection.model),
    agentData: input.agentData,
    characterFields: first
      ? {
          description: first.description,
          personality: first.personality,
          backstory: first.backstory,
          appearance: first.appearance,
          scenario: first.scenario,
          example: first.mesExample,
        }
      : undefined,
    personaFields: input.persona
      ? {
          description: input.persona.description,
          personality: input.persona.personality,
          backstory: input.persona.backstory,
          appearance: input.persona.appearance,
          scenario: input.persona.scenario,
        }
      : undefined,
  };
}

function renderCharacters(characters: GenerationCharacterContext[]): string {
  return characters
    .map((character) =>
      [
        ["Name", character.name],
        ["Description", character.description],
        ["Personality", character.personality],
        ["Scenario", character.scenario],
        ["Backstory", character.backstory],
        ["Appearance", character.appearance],
        ["First Message", character.firstMes],
        ["System Prompt", character.systemPrompt],
        ["Post History Instructions", character.postHistoryInstructions],
      ]
        .filter(([, value]) => typeof value === "string" && value.trim().length > 0)
        .map(([label, value]) => `${label}: ${value}`)
        .join("\n"),
    )
    .filter(Boolean)
    .join("\n\n");
}

function renderDialogueExamples(characters: GenerationCharacterContext[]): string {
  return characters
    .map((character) => character.mesExample)
    .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
    .join("\n\n");
}

function renderPersona(persona: GenerationPersonaContext | null): string {
  if (!persona) return "";
  return [
    ["Name", persona.name],
    ["Description", persona.description],
    ["Personality", persona.personality],
    ["Backstory", persona.backstory],
    ["Appearance", persona.appearance],
    ["Scenario", persona.scenario],
  ]
    .filter(([, value]) => typeof value === "string" && value.trim().length > 0)
    .map(([label, value]) => `${label}: ${value}`)
    .join("\n");
}

function chatSummary(chat: JsonRecord): string | null {
  const meta = parseRecord(chat.metadata);
  const parts = [meta.conversationSummary, meta.summary, meta.daySummaries, meta.weekSummaries]
    .map((value) => (typeof value === "string" ? value : isRecord(value) || Array.isArray(value) ? JSON.stringify(value) : ""))
    .filter((value) => value.trim().length > 0);
  return parts.length > 0 ? parts.join("\n\n") : null;
}

const MEMORY_EMBEDDING_DIMS = 256;
const DEFAULT_MEMORY_RECALL_BUDGET_TOKENS = 1024;
const MIN_MEMORY_RECALL_BUDGET_TOKENS = 256;
const MAX_MEMORY_RECALL_BUDGET_TOKENS = 2048;
const MAX_RECALLED_MEMORY_TOKENS = 384;
const MIN_RECALLED_MEMORY_TOKENS = 96;
const MEMORY_RECALL_CONTEXT_SHARE = 0.15;
const RECALL_TRUNCATION_MARKER = "\n...[recalled memory truncated]...\n";

function estimateTextTokens(text: string): number {
  const trimmed = text.trim();
  return trimmed ? Math.max(1, Math.ceil(trimmed.length / 4)) : 0;
}

function lexicalMemoryEmbedding(text: string): number[] {
  const vector = Array.from({ length: MEMORY_EMBEDDING_DIMS }, () => 0);
  for (const match of text.toLowerCase().matchAll(/[a-z0-9]{2,}/g)) {
    let hash = 2166136261;
    for (let index = 0; index < match[0].length; index += 1) {
      hash ^= match[0].charCodeAt(index);
      hash = Math.imul(hash, 16777619) >>> 0;
    }
    vector[hash % MEMORY_EMBEDDING_DIMS] += 1;
  }
  const magnitude = Math.sqrt(vector.reduce((sum, value) => sum + value * value, 0));
  return magnitude > 0 ? vector.map((value) => value / magnitude) : vector;
}

function cosineSimilarity(a: number[], b: number[]): number {
  if (a.length !== b.length || a.length === 0) return 0;
  let dot = 0;
  let magA = 0;
  let magB = 0;
  for (let index = 0; index < a.length; index += 1) {
    const left = a[index] ?? 0;
    const right = b[index] ?? 0;
    dot += left * right;
    magA += left * left;
    magB += right * right;
  }
  const denominator = Math.sqrt(magA) * Math.sqrt(magB);
  return denominator > 0 ? dot / denominator : 0;
}

function memoryVector(memory: JsonRecord): number[] | null {
  if (!Array.isArray(memory.embedding)) return null;
  const vector = memory.embedding.filter((value): value is number => typeof value === "number" && Number.isFinite(value));
  return vector.length === MEMORY_EMBEDDING_DIMS ? vector : null;
}

function truncateRecalledMemory(content: string, tokenBudget: number): string {
  const maxChars = Math.max(32, tokenBudget * 4);
  if (content.length <= maxChars) return content;
  const availableChars = maxChars - RECALL_TRUNCATION_MARKER.length;
  if (availableChars <= 0) return content.slice(0, maxChars);
  const headChars = Math.max(16, Math.ceil(availableChars * 0.7));
  const tailChars = Math.max(16, availableChars - headChars);
  return `${content.slice(0, headChars).trimEnd()}${RECALL_TRUNCATION_MARKER}${content.slice(-tailChars).trimStart()}`;
}

function packRecalledMemories(recalled: Array<{ content: string }>, maxContext?: number) {
  const targetBudget = maxContext
    ? Math.floor(maxContext * MEMORY_RECALL_CONTEXT_SHARE)
    : DEFAULT_MEMORY_RECALL_BUDGET_TOKENS;
  const budgetTokens = Math.max(
    MIN_MEMORY_RECALL_BUDGET_TOKENS,
    Math.min(MAX_MEMORY_RECALL_BUDGET_TOKENS, targetBudget),
  );
  const lines: string[] = [];
  let estimatedTokens = 0;
  for (const memory of recalled) {
    const remainingTokens = budgetTokens - estimatedTokens;
    if (remainingTokens < MIN_RECALLED_MEMORY_TOKENS) break;
    const packed = truncateRecalledMemory(memory.content, Math.min(MAX_RECALLED_MEMORY_TOKENS, remainingTokens));
    const packedTokens = estimateTextTokens(packed);
    if (packedTokens <= 0 || packedTokens > remainingTokens) break;
    lines.push(packed);
    estimatedTokens += packedTokens;
  }
  return { lines, estimatedTokens, budgetTokens };
}

function memoryRecallEnabled(chat: JsonRecord): boolean {
  const meta = parseRecord(chat.metadata);
  if (typeof meta.enableMemoryRecall === "boolean") return meta.enableMemoryRecall;
  const mode = readString(chat.mode || chat.chatMode);
  return mode === "conversation" || meta.sceneStatus === "active";
}

async function buildMemoryRecallBlock(
  storage: StorageGateway,
  chat: JsonRecord,
  latestUserInput: string,
  maxContext?: number,
): Promise<string | null> {
  if (!memoryRecallEnabled(chat) || !latestUserInput.trim()) return null;
  const chatId = readString(chat.id).trim();
  if (!chatId) return null;
  let memories: JsonRecord[] = [];
  try {
    const rows = await storage.request<unknown>("GET", `/chats/${encodeURIComponent(chatId)}/memories`);
    memories = Array.isArray(rows) ? rows.filter(isRecord) : [];
  } catch {
    memories = Array.isArray(chat.memories) ? chat.memories.filter(isRecord) : [];
  }
  if (memories.length === 0) return null;

  const queryVector = lexicalMemoryEmbedding(latestUserInput);
  const queryTokens = new Set(latestUserInput.toLowerCase().match(/[a-z0-9]{2,}/g) ?? []);
  const recalled = memories
    .map((memory) => {
      const content = readString(memory.content).trim();
      if (!content) return null;
      const vector = memoryVector(memory) ?? lexicalMemoryEmbedding(content);
      const haystack = content.toLowerCase();
      const lexicalScore = Array.from(queryTokens).reduce((score, token) => score + (haystack.includes(token) ? 1 : 0), 0);
      const similarity = cosineSimilarity(queryVector, vector) + Math.min(0.2, lexicalScore * 0.025);
      return { content, similarity };
    })
    .filter((memory): memory is { content: string; similarity: number } => !!memory && memory.similarity > 0)
    .sort((a, b) => b.similarity - a.similarity)
    .slice(0, 8);
  if (recalled.length === 0) return null;

  const packed = packRecalledMemories(recalled, maxContext);
  if (packed.lines.length === 0) return null;
  return [
    "<memories>",
    "The following are recalled fragments from earlier in this chat. Use them to maintain continuity, remember past events, and stay in character. Do not explicitly reference memory recall unless it is natural.",
    ...packed.lines.map((line, index) => `--- Memory ${index + 1} ---\n${line}`),
    "</memories>",
  ].join("\n");
}

function normalizeRole(value: unknown): "system" | "user" | "assistant" {
  return value === "system" || value === "assistant" ? value : "user";
}

function historyMessages(storedMessages: JsonRecord[], limit: number): ChatMLMessage[] {
  return storedMessages
    .filter((message) => !hiddenFromAi(message))
    .slice(-limit)
    .map((message) => ({
      role: normalizeRole(message.role),
      content: readString(message.content).trim(),
      contextKind: "history" as const,
    }))
    .filter((message) => message.content.length > 0);
}

function normalizeLorebookEntry(entry: JsonRecord): LorebookEntry {
  return {
    id: readString(entry.id),
    lorebookId: readString(entry.lorebookId),
    name: readString(entry.name) || "Entry",
    content: readString(entry.content),
    description: readString(entry.description),
    keys: stringArray(entry.keys),
    secondaryKeys: stringArray(entry.secondaryKeys),
    selective: boolish(entry.selective, false),
    selectiveLogic: readString(entry.selectiveLogic, "and") as LorebookEntry["selectiveLogic"],
    constant: boolish(entry.constant, false),
    enabled: boolish(entry.enabled, true),
    position: readNumber(entry.position, 0),
    role: normalizeRole(entry.role) as LorebookEntry["role"],
    depth: readNumber(entry.depth, 0),
    order: readNumber(entry.order ?? entry.sortOrder, 0),
    probability: entry.probability == null ? null : readNumber(entry.probability, 100),
    useRegex: boolish(entry.useRegex, false),
    matchWholeWords: boolish(entry.matchWholeWords, false),
    caseSensitive: boolish(entry.caseSensitive, false),
    ephemeral: entry.ephemeral == null ? null : readNumber(entry.ephemeral, 0),
    group: readString(entry.group),
    groupWeight: entry.groupWeight == null ? null : readNumber(entry.groupWeight, 100),
    folderId: readString(entry.folderId) || null,
    locked: boolish(entry.locked, false),
    preventRecursion: boolish(entry.preventRecursion, false),
    tag: readString(entry.tag),
    relationships: stringRecord(entry.relationships),
    dynamicState: parseRecord(entry.dynamicState),
    scanDepth: readNumber(entry.scanDepth, 0),
    sticky: entry.sticky == null ? null : readNumber(entry.sticky, 0),
    cooldown: entry.cooldown == null ? null : readNumber(entry.cooldown, 0),
    delay: entry.delay == null ? null : readNumber(entry.delay, 0),
    activationConditions: Array.isArray(entry.activationConditions) ? entry.activationConditions : [],
    schedule: isRecord(entry.schedule) ? (entry.schedule as unknown as LorebookEntry["schedule"]) : null,
    excludeFromVectorization: boolish(entry.excludeFromVectorization, false),
    embedding: Array.isArray(entry.embedding) ? entry.embedding.filter((item): item is number => typeof item === "number") : null,
    additionalMatchingSources: stringArray(entry.additionalMatchingSources) as LorebookEntry["additionalMatchingSources"],
    characterFilterMode: readString(entry.characterFilterMode, "any") as LorebookEntry["characterFilterMode"],
    characterFilterIds: stringArray(entry.characterFilterIds),
    characterTagFilterMode: readString(entry.characterTagFilterMode, "any") as LorebookEntry["characterTagFilterMode"],
    characterTagFilters: stringArray(entry.characterTagFilters),
    generationTriggerFilterMode: readString(entry.generationTriggerFilterMode, "any") as LorebookEntry["generationTriggerFilterMode"],
    generationTriggerFilters: stringArray(entry.generationTriggerFilters),
    createdAt: readString(entry.createdAt),
    updatedAt: readString(entry.updatedAt),
  };
}

function lorebookAppliesToContext(
  lorebook: JsonRecord,
  chat: JsonRecord,
  characters: GenerationCharacterContext[],
  persona: GenerationPersonaContext | null,
): boolean {
  if (!boolish(lorebook.enabled, true)) return false;
  if (boolish(lorebook.isGlobal ?? lorebook.global, false)) return true;
  const activeIds = new Set(characters.map((character) => character.id));
  const lorebookCharacterIds = stringArray(lorebook.characterIds);
  if (lorebookCharacterIds.some((id) => activeIds.has(id)) || activeIds.has(readString(lorebook.characterId))) {
    return true;
  }
  const personaId = readString(chat.personaId);
  if (persona && personaId) {
    const personaIds = stringArray(lorebook.personaIds);
    if (personaIds.includes(personaId) || readString(lorebook.personaId) === personaId) return true;
  }
  const meta = parseRecord(chat.metadata);
  return stringArray(meta.activeLorebookIds ?? chat.activeLorebookIds).includes(readString(lorebook.id));
}

async function loadActivatedLore(
  storage: StorageGateway,
  chat: JsonRecord,
  characters: GenerationCharacterContext[],
  persona: GenerationPersonaContext | null,
  storedMessages: JsonRecord[],
): Promise<ActivatedEntry[]> {
  const lorebooks = (await storage.list<JsonRecord>("lorebooks")).filter((book) =>
    lorebookAppliesToContext(book, chat, characters, persona),
  );
  const rows = (
    await Promise.all(
      lorebooks.map(async (book) => {
        const id = readString(book.id);
        return id ? storage.list<JsonRecord>(`lorebooks/${encodeURIComponent(id)}/entries`) : [];
      }),
    )
  ).flat();
  const entries = rows.map(normalizeLorebookEntry).filter((entry) => entry.enabled && entry.content.trim());
  const activeCharacterIds = characters.map((character) => character.id);
  const activeCharacterTags = characters.flatMap((character) => character.tags);
  return scanForActivatedEntries(
    storedMessages.filter((message) => !hiddenFromAi(message)).map((message) => ({
      role: readString(message.role, "user"),
      content: readString(message.content),
    })),
    entries,
    { activeCharacterIds, activeCharacterTags, generationTriggers: ["chat", readString(chat.mode)] },
  );
}

function loreForEvent(entry: ActivatedEntry) {
  return {
    id: entry.entry.id,
    lorebookId: entry.entry.lorebookId,
    name: entry.entry.name,
    content: entry.entry.content,
    tag: entry.matchedKeys.join(", "),
    matchedKeys: entry.matchedKeys,
    order: entry.entry.order,
    constant: entry.entry.constant,
  };
}

function sectionContent(args: {
  section: PromptSectionRecord;
  marker: MarkerConfig | null;
  characters: GenerationCharacterContext[];
  persona: GenerationPersonaContext | null;
  worldBefore: string;
  worldAfter: string;
  summary: string | null;
  agentData: Record<string, string>;
}) {
  switch (args.marker?.type) {
    case "character":
      return renderCharacters(args.characters);
    case "persona":
      return renderPersona(args.persona);
    case "dialogue_examples":
      return renderDialogueExamples(args.characters);
    case "chat_summary":
      return args.summary ?? "";
    case "world_info_before":
      return args.worldBefore;
    case "world_info_after":
      return args.worldAfter;
    case "lorebook":
      return [args.worldBefore, args.worldAfter].filter(Boolean).join("\n\n");
    case "agent_data":
      return args.marker.agentType
        ? args.agentData[args.marker.agentType] ?? ""
        : Object.entries(args.agentData)
            .map(([type, text]) => `${type}: ${text}`)
            .join("\n\n");
    case "chat_history":
      return "";
    default:
      return readString(args.section.content);
  }
}

export async function assembleGenerationPrompt(
  storage: StorageGateway,
  input: PromptAssemblyInput,
): Promise<PromptAssemblyResult> {
  const characters = await loadCharacters(storage, input.chat);
  const persona = await loadPersona(storage, input.chat);
  const activated = await loadActivatedLore(storage, input.chat, characters, persona, input.storedMessages);
  const processedLore = processActivatedEntries(activated, readNumber(input.request.lorebookTokenBudget, 0));
  const summary = chatSummary(input.chat);
  const memoryRecallBlock = await buildMemoryRecallBlock(
    storage,
    input.chat,
    input.latestUserInput,
    readNumber(input.connection.maxContext, 0) || undefined,
  );
  const defaultPrompt = await loadDefaultPromptId(storage);
  const presetId = promptPresetId(input.chat, input.connection, input.request, defaultPrompt);
  const wrapFormat = (readString(input.chat.wrapFormat) || readString(input.connection.wrapFormat) || "xml") as WrapFormat;
  const historyLimit = Math.max(1, Math.min(300, readNumber(input.request.historyLimit, 80)));
  const history = historyMessages(input.storedMessages, historyLimit);
  const macros = macroContext({
    chat: input.chat,
    connection: input.connection,
    characters,
    persona,
    latestUserInput: input.latestUserInput,
    agentData: input.agentData,
  });
  const agentData = input.agentData ?? {};
  let messages: ChatMLMessage[] = [];
  let insertedHistory = false;

  if (presetId) {
    const sections = await loadPromptSections(storage, presetId);
    for (const section of sections) {
      if (!boolish(section.enabled, true)) continue;
      const marker = markerConfig(section);
      if (marker?.type === "chat_history") {
        messages.push(...history);
        insertedHistory = true;
        continue;
      }
      const rawContent = sectionContent({
        section,
        marker,
        characters,
        persona,
        worldBefore: processedLore.worldInfoBefore,
        worldAfter: processedLore.worldInfoAfter,
        summary,
        agentData,
      });
      const resolved = resolveMacros(rawContent, macros);
      if (!resolved.trim()) continue;
      const name = readString(section.name) || readString(section.identifier) || marker?.type || "Prompt";
      messages.push({
        role: normalizeRole(section.role),
        content: wrapContent(resolved, name, wrapFormat),
        contextKind: "prompt",
      });
    }
  }

  if (messages.length === 0) {
    messages.push({
      role: "system",
      content: [
        "You are roleplaying in Marinara Engine. Stay in character, respect the scenario, and continue the scene naturally.",
        renderCharacters(characters),
        renderPersona(persona),
        processedLore.worldInfoBefore,
        processedLore.worldInfoAfter,
        summary ? `Summary:\n${summary}` : "",
      ]
        .filter((part) => part.trim().length > 0)
        .join("\n\n"),
      contextKind: "prompt",
    });
  }

  if (!insertedHistory) {
    messages.push(...history);
  }

  if (memoryRecallBlock) {
    const insertAt = messages.findIndex((message) => message.role === "user" || message.role === "assistant");
    messages.splice(insertAt >= 0 ? insertAt : messages.length, 0, {
      role: "system",
      content: memoryRecallBlock,
      contextKind: "prompt",
    });
  }

  messages = injectAtDepth(messages, processedLore.depthEntries);
  const regexScripts = await storage.list<JsonRecord>("regex-scripts");
  applyRegexScriptsToPromptMessages(messages, regexScripts, {
    resolveMacros: (value) => resolveMacros(value, macros, { trimResult: false }),
  });
  messages = mergeAdjacentMessages(messages);
  if (boolish(input.request.squashSystemMessages, false)) {
    messages = squashLeadingSystemMessages(messages);
  }

  return {
    messages,
    characters,
    persona,
    activatedLorebookEntries: activated.map(loreForEvent),
    chatSummary: summary,
  };
}
