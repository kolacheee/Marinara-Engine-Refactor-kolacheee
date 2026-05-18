import { useMutation } from "@tanstack/react-query";
import type { SceneAnalysis } from "../../../engine/contracts/types/scene";
import { analyzeScene } from "../../../engine/modes/roleplay/scene/scene-service";
import { llmApi } from "../../../shared/api/llm-api";
import { storageApi } from "../../../shared/api/storage-api";

type SceneAnalysisRequest = {
  chatId?: string;
  connectionId?: string;
  narration: string;
  context?: Record<string, unknown>;
};

export function useSceneAnalysis() {
  return useMutation({
    mutationFn: (request: SceneAnalysisRequest): Promise<SceneAnalysis> =>
      analyzeScene({ storage: storageApi, llm: llmApi }, request),
  });
}
