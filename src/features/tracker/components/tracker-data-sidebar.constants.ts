import { BUILT_IN_AGENTS } from "../../../engine/contracts/types/agent";
import type { TrackerDataPanelSection } from "../../../shared/stores/ui.store";

export type TrackerPanelSection = TrackerDataPanelSection;
export type PersonaPortraitMode = "expression" | "avatar";
export type TrackerStatDensity = "normal" | "compact" | "tight";
export type TrackerStatDisplayScale = "standard" | "roomy" | "spacious";

export const TRACKER_AGENT_TYPE_IDS = new Set(
  BUILT_IN_AGENTS.filter((agent) => agent.category === "tracker").map((agent) => agent.id),
);

export const TRACKER_SECTION_AGENT_TYPES: Partial<Record<TrackerPanelSection, string>> = {
  world: "world-state",
  persona: "persona-stats",
  characters: "character-tracker",
  quests: "quest",
  custom: "custom-tracker",
};

export const TRACKER_SECTION_RERUN_TITLES: Partial<Record<TrackerPanelSection, string>> = {
  world: "Re-run world state tracker",
  persona: "Re-run persona tracker",
  characters: "Re-run character tracker",
  quests: "Re-run quest tracker",
  custom: "Re-run custom tracker",
};

export const TRACKER_FEATURED_CHARACTER_META_KEY = "trackerFeaturedCharacterKeys";
export const TRACKER_TEXT_ROW = "text-[0.6875rem] leading-[0.875rem]";
export const TRACKER_TEXT_MICRO = "text-[0.625rem] leading-[0.75rem]";
export const TRACKER_BAR = "h-[3px] rounded-[1px]";
export const TRACKER_SPLIT_WIDTH = 260;

export const FEATURED_PORTRAIT_DEFAULT_FOCUS_X = 50;
export const FEATURED_PORTRAIT_DEFAULT_FOCUS_Y = 36;
export const FEATURED_PORTRAIT_FOCUS_STEP = 8;
export const FEATURED_CHARACTER_PORTRAIT_STAGE_REM = 7.75;
export const FEATURED_CHARACTER_PORTRAIT_ROOMY_STAGE_REM = 9.25;
export const FEATURED_CHARACTER_ROOMY_WIDTH = 380;

export const PERSONA_STAT_ALLOWANCE_REM: Record<PersonaPortraitMode, number> = {
  expression: 11,
  avatar: 7,
};
export const PERSONA_STAT_DENSITY_HEIGHT_REM: Record<TrackerStatDensity, number> = {
  normal: 1.25,
  compact: 0.95,
  tight: 0.72,
};
export const PERSONA_ADD_STAT_DENSITY_HEIGHT_REM: Record<TrackerStatDensity, number> = {
  normal: 1.25,
  compact: 1,
  tight: 0.82,
};
