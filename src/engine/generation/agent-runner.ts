import type { AgentContext, AgentResult } from "@marinara-engine/shared";
import type { LlmGateway, LlmMessage, StorageGateway } from "../capabilities";
import type { BaseLLMProvider, ChatCompleteOptions, ChatCompleteResult, ChatMessage } from "../generation-core/llm/base-provider";
import { createAgentPipeline, type AgentInjection, type ResolvedAgent } from "../agents-runtime/pipeline/agent-pipeline";
import type { GenerationCharacterContext, GenerationPersonaContext } from "./prompt-assembly";
import { boolish, hiddenFromAi, isRecord, parseRecord, readString, type JsonRecord } from "./runtime-records";

export interface GenerationAgentRuntimeInput {
  chat: JsonRecord;
  connection: JsonRecord;
  storedMessages: JsonRecord[];
  characters: GenerationCharacterContext[];
  persona: GenerationPersonaContext | null;
  activatedLorebookEntries: Array<{ id: string; name: string; content: string; tag: string }>;
  chatSummary: string | null;
  signal?: AbortSignal;
}

export interface GenerationAgentRuntime {
  preInjections: AgentInjection[];
  preResults: AgentResult[];
  agentData: Record<string, string>;
  runParallel(): Promise<AgentResult[]>;
  runPost(mainResponse: string): Promise<AgentResult[]>;
}

interface AgentDeps {
  storage: StorageGateway;
  llm: LlmGateway;
}

function llmProvider(llm: LlmGateway, connectionId: string | null): BaseLLMProvider {
  return {
    maxTokensOverrideValue: null,
    async chatComplete(messages: ChatMessage[], options: ChatCompleteOptions): Promise<ChatCompleteResult> {
      let content = "";
      const requestMessages: LlmMessage[] = messages.map((message) => ({
        role:
          message.role === "system" || message.role === "assistant" || message.role === "tool" ? message.role : "user",
        content: message.content,
        name: typeof message.name === "string" ? message.name : undefined,
      }));
      for await (const chunk of llm.stream(
        {
          connectionId,
          model: options.model,
          messages: requestMessages,
          parameters: {
            temperature: options.temperature,
            maxTokens: options.maxTokens,
          },
          tools: options.tools as never,
        },
        options.signal,
      )) {
        if (chunk.type === "token" && chunk.text) {
          content += chunk.text;
          options.onToken?.(chunk.text);
        }
      }
      return { content };
    },
  };
}

function agentSettings(agent: JsonRecord): Record<string, unknown> {
  return parseRecord(agent.settings);
}

function normalizePhase(agent: JsonRecord): string {
  const phase = readString(agent.phase || agentSettings(agent).phase || "pre_generation");
  return phase.replace(/-/g, "_");
}

async function loadConnection(storage: StorageGateway, connectionId: string | null, fallback: JsonRecord) {
  if (!connectionId) return fallback;
  const connection = await storage.get<JsonRecord>("connections", connectionId);
  return isRecord(connection) ? connection : fallback;
}

async function loadAgentMemory(storage: StorageGateway, agentId: string, chatId: string): Promise<Record<string, unknown>> {
  const rows = await storage.list<JsonRecord>("agent-memory");
  const memory: Record<string, unknown> = {};
  for (const row of rows) {
    if (readString(row.agentConfigId) !== agentId || readString(row.chatId) !== chatId) continue;
    const key = readString(row.key);
    if (!key) continue;
    const value = row.value;
    memory[key] = typeof value === "string" ? parseMaybeJson(value) : value;
  }
  return memory;
}

function parseMaybeJson(value: string): unknown {
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

async function resolveAgents(deps: AgentDeps, input: GenerationAgentRuntimeInput): Promise<ResolvedAgent[]> {
  const rows = (await deps.storage.list<JsonRecord>("agents"))
    .filter((agent) => boolish(agent.enabled, false))
    .filter((agent) => !readString(agent.connectionId).startsWith("sidecar"));
  const resolved: ResolvedAgent[] = [];
  for (const agent of rows) {
    const settings = agentSettings(agent);
    const connectionId = readString(agent.connectionId).trim() || readString(input.connection.id).trim() || null;
    const connection = await loadConnection(deps.storage, connectionId, input.connection);
    const model = readString(agent.model).trim() || readString(connection.model).trim();
    if (!model) continue;
    resolved.push({
      id: readString(agent.id) || readString(agent.type) || "agent",
      type: readString(agent.type || agent.agentType) || "agent",
      name: readString(agent.name) || readString(agent.type) || "Agent",
      phase: normalizePhase(agent),
      promptTemplate: readString(agent.promptTemplate),
      connectionId,
      settings,
      provider: llmProvider(deps.llm, connectionId),
      model,
      maxParallelJobs: typeof settings.maxParallelJobs === "number" ? settings.maxParallelJobs : undefined,
      toolContext: undefined,
    });
  }
  return resolved;
}

async function buildAgentContext(deps: AgentDeps, input: GenerationAgentRuntimeInput): Promise<AgentContext> {
  const chatId = readString(input.chat.id);
  const memoryRows = await Promise.all(
    (await deps.storage.list<JsonRecord>("agents"))
      .filter((agent) => readString(agent.id).trim())
      .map((agent) => loadAgentMemory(deps.storage, readString(agent.id), chatId)),
  );
  const memory = Object.assign({}, ...memoryRows);
  return {
    chatId,
    chatMode: readString(input.chat.mode || input.chat.chatMode, "roleplay"),
    recentMessages: input.storedMessages
      .filter((message) => !hiddenFromAi(message))
      .slice(-60)
      .map((message) => ({
        role: readString(message.role, "user"),
        content: readString(message.content),
      })),
    mainResponse: null,
    gameState: isRecord(input.chat.gameState) ? (input.chat.gameState as unknown as AgentContext["gameState"]) : null,
    characters: input.characters.map((character) => ({
      id: character.id,
      name: character.name,
      description: character.description,
      personality: character.personality,
      scenario: character.scenario,
      creatorNotes: character.creatorNotes,
      systemPrompt: character.systemPrompt,
      backstory: character.backstory,
      appearance: character.appearance,
      mesExample: character.mesExample,
      firstMes: character.firstMes,
      postHistoryInstructions: character.postHistoryInstructions,
    })),
    persona: input.persona,
    memory,
    activatedLorebookEntries: input.activatedLorebookEntries,
    writableLorebookIds: null,
    chatSummary: input.chatSummary,
    streaming: true,
    signal: input.signal,
  };
}

function resultText(result: AgentResult): string | null {
  if (!result.success) return null;
  if (typeof result.data === "string") return result.data;
  if (!isRecord(result.data)) return null;
  const text = result.data.text ?? result.data.direction ?? result.data.summary ?? result.data.raw;
  return typeof text === "string" && text.trim() ? text.trim() : null;
}

function resultEventData(result: AgentResult): AgentResult {
  return result;
}

export async function createGenerationAgentRuntime(
  deps: AgentDeps,
  input: GenerationAgentRuntimeInput,
  onResult?: (result: AgentResult) => void,
): Promise<GenerationAgentRuntime> {
  const agents = await resolveAgents(deps, input);
  const context = await buildAgentContext(deps, input);
  const preResults: AgentResult[] = [];
  const agentData: Record<string, string> = {};
  const pipeline = createAgentPipeline(agents, context, (result) => {
    const text = resultText(result);
    if (text) agentData[result.agentType] = text;
    onResult?.(resultEventData(result));
  });

  const preInjections = await pipeline.preGenerate((type) => type !== "prompt-reviewer");
  for (const result of pipeline.results) {
    if (result.agentType && !preResults.includes(result)) preResults.push(result);
  }
  for (const injection of preInjections) {
    if (injection.text.trim()) agentData[injection.agentType] = injection.text.trim();
  }

  return {
    preInjections,
    preResults,
    agentData,
    runParallel: async () => pipeline.runParallel(),
    runPost: async (mainResponse) => pipeline.postGenerate(mainResponse, { preGenInjections: preInjections }),
  };
}
