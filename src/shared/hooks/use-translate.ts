import { useCallback } from "react";
import { api } from "../api/api-client";
import { useTranslationStore } from "../stores/translation.store";

export function useTranslate() {
  const config = useTranslationStore((s) => s.config);
  const translations = useTranslationStore((s) => s.translations);
  const translating = useTranslationStore((s) => s.translating);
  const setTranslation = useTranslationStore((s) => s.setTranslation);
  const removeTranslation = useTranslationStore((s) => s.removeTranslation);
  const setTranslating = useTranslationStore((s) => s.setTranslating);

  const translate = useCallback(
    async (messageId?: string, content?: string, chatId?: string) => {
      if (!messageId || !content?.trim()) return;
      if (translations[messageId]) {
        removeTranslation(messageId);
        if (chatId) {
          api
            .patch(`/chats/${encodeURIComponent(chatId)}/messages/${encodeURIComponent(messageId)}/extra`, {
              translation: null,
            })
            .catch((error) => console.warn("[translation] Failed to clear persisted translation", error));
        }
        return;
      }
      setTranslating(messageId, true);
      try {
        const result = await api.post<{ translatedText: string }>("/translate", {
          text: content,
          provider: config.provider,
          targetLanguage: config.targetLanguage,
          connectionId: config.connectionId,
          deeplApiKey: config.deeplApiKey,
          deeplxUrl: config.deeplxUrl,
        });
        setTranslation(messageId, result.translatedText);
        if (chatId) {
          await api
            .patch(`/chats/${encodeURIComponent(chatId)}/messages/${encodeURIComponent(messageId)}/extra`, {
              translation: result.translatedText,
            })
            .catch((error) => console.warn("[translation] Failed to persist translation", error));
        }
      } finally {
        setTranslating(messageId, false);
      }
    },
    [config, removeTranslation, setTranslating, setTranslation, translations],
  );

  return {
    translations,
    translating,
    translateMessage: translate,
    translate,
  };
}
