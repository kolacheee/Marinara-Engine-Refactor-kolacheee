import { useChatStore } from "../../../shared/stores/chat.store";
import { useUIStore } from "../../../shared/stores/ui.store";
import { useChat, useChatMessages, useDeleteMessage } from "../../chats/hooks/use-chats";
import type { Chat, Message } from "../../../engine/contracts/types/chat";
import { GameSurface } from "./GameSurface";

interface GameConversationViewProps {
  activeChatId: string;
}

function fallbackChat(activeChatId: string, snapshot: Record<string, unknown> | null): Chat {
  return {
    id: activeChatId,
    name: typeof snapshot?.name === "string" ? snapshot.name : "Game",
    mode: "game",
    characterIds: Array.isArray(snapshot?.characterIds) ? snapshot.characterIds : [],
    groupId: null,
    personaId: null,
    promptPresetId: null,
    connectionId: null,
    folderId: null,
    metadata:
      snapshot?.metadata && typeof snapshot.metadata === "object" && !Array.isArray(snapshot.metadata)
        ? (snapshot.metadata as Record<string, unknown>)
        : {},
    createdAt: new Date(0).toISOString(),
    updatedAt: new Date(0).toISOString(),
  } as Chat;
}

export function GameConversationView({ activeChatId }: GameConversationViewProps) {
  const chatQuery = useChat(activeChatId);
  const messagesQuery = useChatMessages(activeChatId);
  const deleteMessage = useDeleteMessage(activeChatId);
  const activeChat = useChatStore((s) => s.activeChat);
  const isStreaming = useChatStore((s) => s.isStreaming && s.streamingChatId === activeChatId);
  const chatBackground = useUIStore((s) => s.chatBackground);
  const chat = (chatQuery.data as unknown as Chat | undefined) ?? fallbackChat(activeChatId, activeChat as Record<string, unknown> | null);
  const chatMeta =
    chat.metadata && typeof chat.metadata === "object" && !Array.isArray(chat.metadata)
      ? (chat.metadata as Record<string, unknown>)
      : {};

  return (
    <GameSurface
      activeChatId={activeChatId}
      chat={chat}
      chatMeta={chatMeta}
      messages={(messagesQuery.data as unknown as Message[] | undefined) ?? []}
      isStreaming={isStreaming}
      isMessagesLoading={messagesQuery.isLoading}
      characterMap={new Map()}
      characters={[]}
      chatBackground={chatBackground}
      onOpenSettings={() => {}}
      onDeleteMessage={(messageId) => deleteMessage.mutate(messageId)}
    />
  );
}
