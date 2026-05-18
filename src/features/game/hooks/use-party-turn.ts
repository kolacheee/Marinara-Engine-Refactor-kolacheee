// ──────────────────────────────────────────────
// Hook: usePartyTurn
//
// Generates party member reactions to the GM narration.
// ──────────────────────────────────────────────

import { useMutation } from "@tanstack/react-query";
import { parsePartyDialogue } from "../lib/party-dialogue-parser";
import { useUIStore } from "../../../shared/stores/ui.store";
import { gameApi } from "../api/game-api";
import type { PartyDialogueLine } from "../../../engine/contracts/types/game";

interface PartyTurnInput {
  chatId: string;
  narration: string;
  playerAction?: string;
  connectionId?: string;
  debugMode?: boolean;
}

export interface PartyTurnResult {
  raw: string;
  lines: PartyDialogueLine[];
}

async function generatePartyTurn(input: PartyTurnInput): Promise<PartyTurnResult> {
  const debugMode = useUIStore.getState().debugMode;
  const res = await gameApi.partyTurn({ ...input, debugMode });
  const lines = parsePartyDialogue(res.raw);
  return { raw: res.raw, lines };
}

export function usePartyTurn() {
  return useMutation({
    mutationFn: generatePartyTurn,
  });
}
