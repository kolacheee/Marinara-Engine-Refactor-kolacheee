import { useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { storageApi } from "../../../shared/api/storage-api";
import { useChatStore } from "../../../shared/stores/chat.store";
import { chatKeys, useCreateChat } from "../../chats/hooks/use-chats";
import { useApplyChatPreset, useChatPresets } from "../../chat-presets/hooks/use-chat-presets";

type ChatMode = "roleplay" | "conversation";

interface StartChatFromCharacterOptions {
  characterId: string;
  characterName: string;
  mode: ChatMode;
  firstMessage?: string;
  alternateGreetings?: string[];
}

export function useStartChatFromCharacter() {
  const createChat = useCreateChat();
  const queryClient = useQueryClient();
  const { data: chatPresetsData } = useChatPresets();
  const applyChatPreset = useApplyChatPreset();

  const startChatFromCharacter = useCallback(
    ({ characterId, characterName, mode, firstMessage, alternateGreetings }: StartChatFromCharacterOptions) => {
      const label = mode === "conversation" ? "Conversation" : "Roleplay";
      const presetMode = mode === "conversation" ? "conversation" : "roleplay";
      const starred = (chatPresetsData ?? []).find(
        (preset) => preset.mode === presetMode && preset.isActive && !preset.isDefault,
      );

      createChat.mutate(
        {
          name: characterName ? `${characterName} - ${label}` : `New ${label}`,
          mode,
          characterIds: [characterId],
        },
        {
          onSuccess: async (chat) => {
            useChatStore.getState().setActiveChatId(chat.id);

            if (starred) {
              try {
                await applyChatPreset.mutateAsync({ presetId: starred.id, chatId: chat.id });
              } catch {
                /* non-fatal: chat still opens with system defaults */
              }
            }

            if (mode === "roleplay" && firstMessage?.trim()) {
              try {
                const msg = await storageApi.createChatMessage<{ id: string }>(chat.id, {
                  role: "assistant",
                  content: firstMessage,
                  characterId,
                });

                if (msg?.id && alternateGreetings?.length) {
                  for (const greeting of alternateGreetings) {
                    if (greeting.trim()) {
                      await storageApi.addChatMessageSwipe(chat.id, msg.id, greeting);
                    }
                  }
                }

                queryClient.invalidateQueries({ queryKey: chatKeys.messages(chat.id) });
              } catch {
                /* non-fatal: do not block the new chat if greeting injection fails */
              }
            }

            useChatStore.getState().setShouldOpenSettings(true, chat.id);
            useChatStore.getState().setShouldOpenWizard(true, chat.id);
            useChatStore.getState().setShouldOpenWizardInShortcutMode(true, chat.id);
          },
        },
      );
    },
    [applyChatPreset, chatPresetsData, createChat, queryClient],
  );

  return {
    startChatFromCharacter,
    isStartingChat: createChat.isPending,
  };
}
