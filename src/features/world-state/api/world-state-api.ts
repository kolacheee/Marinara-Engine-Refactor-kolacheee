import type { GameState } from "../../../engine/contracts/types/game-state";
import { api } from "../../../shared/api/api-client";

export type WorldState = GameState;
export type WorldStatePatch = Record<string, unknown>;

export const worldStateApi = {
  get: (chatId: string, init?: RequestInit) =>
    api.get<WorldState | null>(`/world-state/${encodeURIComponent(chatId)}`, init),
  patch: (chatId: string, patch: WorldStatePatch, init?: RequestInit) =>
    api.patch<WorldState>(`/world-state/${encodeURIComponent(chatId)}`, patch, init),
};
