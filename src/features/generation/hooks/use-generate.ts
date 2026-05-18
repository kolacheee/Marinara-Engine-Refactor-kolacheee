import { useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { startGeneration } from "../../../engine/generation";
import { llmApi } from "../../../shared/api/llm-api";
import { storageApi } from "../../../shared/api/storage-api";
import { api, ApiError } from "../../../shared/lib/api-client";
import { useAgentStore } from "../../../shared/stores/agent.store";
import { useChatStore } from "../../../shared/stores/chat.store";

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
              if (event.data && typeof event.data === "object") {
                const result = event.data as { agentId?: string; type?: string };
                useAgentStore.getState().addResult(result.agentId ?? result.type ?? "agent", event.data as never);
              }
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
        await api.post("/agents/retry", { chatId, agentTypes, options });
        useAgentStore.getState().clearFailedAgentTypes();
        await queryClient.invalidateQueries({ queryKey: ["agents"] });
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
