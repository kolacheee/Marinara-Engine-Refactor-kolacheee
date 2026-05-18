import type { StorageGateway } from "../capabilities";
import { newId, nowIso, parseArray, readString, type JsonRecord } from "./runtime-records";

export interface ConnectedCommandResult {
  displayContent: string;
  createdNotes: JsonRecord[];
}

function extractTaggedContent(content: string, tag: "note" | "influence"): string[] {
  const regex = new RegExp(`<${tag}[^>]*>([\\s\\S]*?)<\\/${tag}>`, "gi");
  const results: string[] = [];
  for (const match of content.matchAll(regex)) {
    const text = match[1]?.trim();
    if (text) results.push(text);
  }
  return results;
}

function stripCommandTags(content: string): string {
  return content.replace(/<(?:note|influence)[^>]*>[\s\S]*?<\/(?:note|influence)>/gi, "").trim();
}

export async function persistConnectedCommandTags(
  storage: StorageGateway,
  chat: JsonRecord,
  content: string,
): Promise<ConnectedCommandResult> {
  const chatId = readString(chat.id);
  const existingNotes = parseArray(chat.notes).filter((entry): entry is JsonRecord => !!entry && typeof entry === "object");
  const createdNotes: JsonRecord[] = [];

  for (const [type, tag] of [
    ["note", "note"],
    ["influence", "influence"],
  ] as const) {
    for (const text of extractTaggedContent(content, tag)) {
      createdNotes.push({
        id: newId(type),
        type,
        content: text,
        sourceChatId: chatId,
        targetChatId: null,
        createdAt: nowIso(),
      });
    }
  }

  if (createdNotes.length > 0 && chatId) {
    await storage.update("chats", chatId, { notes: [...existingNotes, ...createdNotes] });
  }

  return {
    displayContent: stripCommandTags(content),
    createdNotes,
  };
}
