import type { AgentResult } from "@marinara-engine/shared";
import type { EventGateway, LlmGateway, LlmMessage, StorageGateway } from "../capabilities";
import { createGenerationAgentRuntime } from "./agent-runner";
import { persistConnectedCommandTags } from "./connected-commands";
import { llmParameters, loadChatMessages, requireRecord, resolveGenerationConnection } from "./context";
import type { GenerationEvent } from "./generation-events";
import { assembleGenerationPrompt } from "./prompt-assembly";
import { applyRuntimeRegexScripts } from "./regex-runtime";
import { hiddenFromAi, isRecord, nowIso, readString, type JsonRecord } from "./runtime-records";

export interface StartGenerationInput extends JsonRecord {
  chatId: string;
  connectionId?: string | null;
  message?: string;
  messages?: Array<{ role: "system" | "user" | "assistant"; content: string }>;
  parameters?: Record<string, unknown>;
  promptPresetId?: string | null;
  generationGuide?: string | null;
  regenerateMessageId?: string | null;
  impersonate?: boolean;
}

export interface GenerationEngineDeps {
  storage: StorageGateway;
  llm: LlmGateway;
  events?: EventGateway;
}

async function saveUserMessage(storage: StorageGateway, input: StartGenerationInput): Promise<string> {
  const raw = readString(input.message).trim();
  if (!raw || input.impersonate === true || readString(input.regenerateMessageId).trim()) return "";
  const content = await applyRuntimeRegexScripts(storage, "user_input", raw);
  if (!content.trim()) return "";
  await storage.call(`/chats/${encodeURIComponent(input.chatId)}/messages`, {
    role: "user",
    content,
  });
  return content;
}

function requestMessages(input: StartGenerationInput): LlmMessage[] | null {
  if (!Array.isArray(input.messages) || input.messages.length === 0) return null;
  return input.messages
    .map((message): LlmMessage => ({
      role: message.role === "system" || message.role === "assistant" ? message.role : "user",
      content: readString(message.content).trim(),
    }))
    .filter((message) => message.content.length > 0);
}

function visibleTranscript(messages: JsonRecord[]): string {
  return messages
    .filter((message) => !hiddenFromAi(message))
    .slice(-24)
    .map((message) => `${readString(message.role, "message")}: ${readString(message.content)}`)
    .join("\n");
}

function resultKey(result: AgentResult): string {
  return `${result.agentId}:${result.agentType}:${result.type}:${JSON.stringify(result.data)}`;
}

async function persistAgentResults(
  storage: StorageGateway,
  chatId: string,
  messageId: string | null,
  results: AgentResult[],
): Promise<void> {
  const seen = new Set<string>();
  for (const result of results) {
    const key = resultKey(result);
    if (seen.has(key)) continue;
    seen.add(key);
    await storage.create("agent-runs", {
      chatId,
      messageId,
      agentId: result.agentId,
      agentType: result.agentType,
      resultType: result.type,
      resultData: result.data as never,
      success: result.success,
      error: result.error,
      tokensUsed: result.tokensUsed,
      durationMs: result.durationMs,
      createdAt: nowIso(),
    });
  }
}

async function saveAssistantMessage(args: {
  storage: StorageGateway;
  chat: JsonRecord;
  input: StartGenerationInput;
  connection: JsonRecord;
  content: string;
  agentResults: AgentResult[];
  noteCount: number;
}): Promise<unknown | null> {
  if (args.input.impersonate === true) return null;

  const regenerateMessageId = readString(args.input.regenerateMessageId).trim();
  if (regenerateMessageId) {
    return args.storage.call(
      `/chats/${encodeURIComponent(args.input.chatId)}/messages/${encodeURIComponent(regenerateMessageId)}/swipes`,
      { content: args.content },
    );
  }

  return args.storage.call(`/chats/${encodeURIComponent(args.input.chatId)}/messages`, {
    role: "assistant",
    content: args.content,
    generationInfo: {
      connectionId: readString(args.connection.id) || null,
      model: readString(args.connection.model) || null,
      agentResults: args.agentResults.length,
      notes: args.noteCount,
    },
  });
}

function messageId(saved: unknown): string | null {
  return isRecord(saved) ? readString(saved.id) || null : null;
}

export async function* startGeneration(
  deps: GenerationEngineDeps,
  input: StartGenerationInput,
  signal?: AbortSignal,
): AsyncGenerator<GenerationEvent> {
  const chatId = readString(input.chatId).trim();
  if (!chatId) throw new Error("chatId is required");

  yield { type: "phase", data: "Saving message..." };
  const latestUserInput = await saveUserMessage(deps.storage, input);
  const chat = requireRecord(await deps.storage.get("chats", chatId), "Chat");
  const connection = await resolveGenerationConnection(deps.storage, chat, input);
  const storedMessages = await loadChatMessages(deps.storage, chatId);
  const directMessages = requestMessages(input);
  const agentEvents: AgentResult[] = [];

  yield { type: "phase", data: "Assembling prompt..." };
  let prompt = directMessages;
  let assembly = await assembleGenerationPrompt(deps.storage, {
    chat,
    storedMessages,
    connection,
    request: input,
    latestUserInput: latestUserInput || readString(input.message),
  });

  if (!directMessages) {
    yield { type: "phase", data: "Running pre-generation agents..." };
    const runtime = await createGenerationAgentRuntime(
      { storage: deps.storage, llm: deps.llm },
      {
        chat,
        connection,
        storedMessages,
        characters: assembly.characters,
        persona: assembly.persona,
        activatedLorebookEntries: assembly.activatedLorebookEntries,
        chatSummary: assembly.chatSummary,
        signal,
      },
      (result) => agentEvents.push(result),
    );
    for (const result of agentEvents) {
      yield { type: "agent_result", data: result };
    }
    agentEvents.length = 0;

    assembly = await assembleGenerationPrompt(deps.storage, {
      chat,
      storedMessages,
      connection,
      request: input,
      latestUserInput: latestUserInput || readString(input.message),
      agentData: runtime.agentData,
    });
    prompt = assembly.messages;

    const parallelAgents = runtime.runParallel();
    yield { type: "phase", data: "Calling model..." };
    let content = "";
    for await (const chunk of deps.llm.stream(
      {
        connectionId: readString(connection.id) || input.connectionId,
        model: readString(connection.model) || undefined,
        messages: [...prompt, generationGuide(input)].filter((message): message is LlmMessage => !!message),
        parameters: llmParameters(connection, input),
      },
      signal,
    )) {
      if (chunk.type === "token" && chunk.text) {
        content += chunk.text;
        yield { type: "token", data: chunk.text };
      }
    }

    const parallelResults = await parallelAgents;
    const postResults = await runtime.runPost(content);
    for (const result of [...parallelResults, ...postResults, ...agentEvents]) {
      yield { type: "agent_result", data: result };
    }
    const allAgentResults = [...runtime.preResults, ...parallelResults, ...postResults, ...agentEvents];
    content = await applyRuntimeRegexScripts(deps.storage, "ai_output", content);
    const connected = await persistConnectedCommandTags(deps.storage, chat, content);
    const saved = await saveAssistantMessage({
      storage: deps.storage,
      chat,
      input,
      connection,
      content: connected.displayContent,
      agentResults: allAgentResults,
      noteCount: connected.createdNotes.length,
    });
    await persistAgentResults(deps.storage, chatId, messageId(saved), allAgentResults);
    if (saved) yield { type: "assistant_message", data: saved };
    yield { type: "done", data: { transcript: visibleTranscript(storedMessages) } };
    return;
  }

  yield { type: "phase", data: "Calling model..." };
  let content = "";
  for await (const chunk of deps.llm.stream(
    {
      connectionId: readString(connection.id) || input.connectionId,
      model: readString(connection.model) || undefined,
      messages: [...(prompt ?? []), generationGuide(input)].filter((message): message is LlmMessage => !!message),
      parameters: llmParameters(connection, input),
    },
    signal,
  )) {
    if (chunk.type === "token" && chunk.text) {
      content += chunk.text;
      yield { type: "token", data: chunk.text };
    }
  }
  content = await applyRuntimeRegexScripts(deps.storage, "ai_output", content);
  const connected = await persistConnectedCommandTags(deps.storage, chat, content);
  const saved = await saveAssistantMessage({
    storage: deps.storage,
    chat,
    input,
    connection,
    content: connected.displayContent,
    agentResults: [],
    noteCount: connected.createdNotes.length,
  });
  if (saved) yield { type: "assistant_message", data: saved };
  yield { type: "done" };
}

function generationGuide(input: StartGenerationInput): LlmMessage | null {
  const guide = readString(input.generationGuide).trim();
  return guide ? { role: "user", content: guide } : null;
}
