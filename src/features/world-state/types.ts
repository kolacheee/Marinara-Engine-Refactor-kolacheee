import type {
  CharacterStat,
  CustomTrackerField,
  GameState,
  InventoryItem,
  PlayerStats,
  PresentCharacter,
  QuestProgress,
} from "../../engine/contracts/types/game-state";

export type GameStatePatchField =
  | "date"
  | "time"
  | "location"
  | "weather"
  | "temperature"
  | "presentCharacters"
  | "playerStats"
  | "personaStats";

export type WorldTemperatureUnit = "celsius" | "fahrenheit";

export interface GameStatePatchValue {
  date: GameState["date"];
  time: GameState["time"];
  location: GameState["location"];
  weather: GameState["weather"];
  temperature: GameState["temperature"];
  presentCharacters: GameState["presentCharacters"];
  playerStats: GameState["playerStats"];
  personaStats: GameState["personaStats"];
}

export interface TrackerStateController {
  gameState: GameState | null;
  playerStats: PlayerStats | null;
  personaStats: CharacterStat[];
  presentCharacters: PresentCharacter[];
  inventory: InventoryItem[];
  quests: QuestProgress[];
  customTrackerFields: CustomTrackerField[];
  loadingGameState: boolean;
  gameStateRefreshing: boolean;
  isLoadingGameState: boolean;
  patchField: <K extends GameStatePatchField>(field: K, value: GameStatePatchValue[K]) => void;
  patchPlayerStats: <K extends keyof PlayerStats>(field: K, value: PlayerStats[K]) => void;
  flushPatch: () => Promise<void>;
}
