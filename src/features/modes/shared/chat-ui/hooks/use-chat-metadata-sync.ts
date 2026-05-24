import { useEffect, useRef } from "react";
import { useUpdateChatMetadata, type Chat } from "../../../../catalog/chats/index";
import { chatBackgroundMetadataToUrl, chatBackgroundUrlToMetadata } from "../../../../../shared/lib/backgrounds";
import { useTranslationStore } from "../../../../../shared/stores/translation.store";
import { useUIStore } from "../../../../../shared/stores/ui.store";
import type { MessageWithSwipes } from "../types";

type UseChatMetadataSyncOptions = {
  chat: Chat | null | undefined;
  chatMeta: Record<string, any>;
  messages: MessageWithSwipes[] | undefined;
  messagePageCount: number;
};

export function useChatMetadataSync({ chat, chatMeta, messages, messagePageCount }: UseChatMetadataSyncOptions) {
  const chatBackground = useUIStore((state) => state.chatBackground);
  const updateMeta = useUpdateChatMetadata();

  useEffect(() => {
    if (!chat?.id) return;
    useTranslationStore.getState().setConfig({
      provider: chatMeta.translationProvider ?? "google",
      targetLanguage: chatMeta.translationTargetLang ?? "en",
      connectionId: chatMeta.translationConnectionId,
      deeplApiKey: chatMeta.translationDeeplApiKey,
      deeplxUrl: chatMeta.translationDeeplxUrl,
    });
  }, [
    chat?.id,
    chatMeta.translationProvider,
    chatMeta.translationTargetLang,
    chatMeta.translationConnectionId,
    chatMeta.translationDeeplApiKey,
    chatMeta.translationDeeplxUrl,
  ]);

  const prevChatIdRef = useRef(chat?.id);
  useEffect(() => {
    if (!messages) return;
    if (prevChatIdRef.current !== chat?.id) {
      useTranslationStore.getState().clearAll();
      prevChatIdRef.current = chat?.id;
    }
    useTranslationStore
      .getState()
      .seedFromMessages(messages as unknown as Array<{ id: string; extra?: string | Record<string, unknown> | null }>);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chat?.id, messagePageCount]);

  const restoredChatBackgroundRef = useRef<{ chatId: string | null; url: string | null; isSyncing: boolean }>({
    chatId: null,
    url: null,
    isSyncing: false,
  });
  useEffect(() => {
    if (!chat?.id) return;
    const restoredUrl = chatBackgroundMetadataToUrl(chatMeta.background);
    restoredChatBackgroundRef.current = { chatId: chat.id, url: restoredUrl, isSyncing: true };
    useUIStore.getState().setChatBackground(restoredUrl);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chat?.id]);

  const bgPersistTimer = useRef<ReturnType<typeof setTimeout>>(null);
  useEffect(() => {
    if (!chat?.id) return;
    const chatId = chat.id;
    const savedBackground = chatBackgroundUrlToMetadata(chatBackgroundMetadataToUrl(chatMeta.background));
    const restoredBackground = restoredChatBackgroundRef.current;

    if (
      restoredBackground.isSyncing &&
      (restoredBackground.chatId !== chatId || chatBackground !== restoredBackground.url)
    ) {
      return;
    }
    if (restoredBackground.isSyncing) {
      restoredBackground.isSyncing = false;
    }

    if (!chatBackground) {
      if (savedBackground === null) return;
      if (bgPersistTimer.current) clearTimeout(bgPersistTimer.current);
      bgPersistTimer.current = setTimeout(() => {
        updateMeta.mutate({ id: chatId, background: null });
      }, 500);
      return;
    }

    const nextBackground = chatBackgroundUrlToMetadata(chatBackground);
    if (nextBackground === savedBackground) return;
    if (bgPersistTimer.current) clearTimeout(bgPersistTimer.current);
    bgPersistTimer.current = setTimeout(() => {
      updateMeta.mutate({ id: chatId, background: nextBackground });
    }, 500);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chatBackground, chat?.id]);

  useEffect(() => {
    return () => {
      if (bgPersistTimer.current) clearTimeout(bgPersistTimer.current);
    };
  }, []);

  return { chatBackground, updateMeta };
}
