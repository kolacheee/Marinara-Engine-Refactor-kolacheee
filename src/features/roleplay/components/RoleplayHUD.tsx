// ──────────────────────────────────────────────
// Chat: Roleplay HUD — immersive world-state widgets
// Each tracker category gets its own mini widget with
// a compact preview and expandable editable popover.
// Supports top (horizontal) and left/right (vertical) layout.
// ──────────────────────────────────────────────
import { useCallback, useMemo, useState } from "react";
import { RefreshCw } from "lucide-react";
import { cn } from "../../../shared/lib/utils";
import { invokeTauri } from "../../../shared/api/tauri-client";
import { worldStateApi } from "../../world-state/api/world-state-api";
import { useGameStateStore } from "../../world-state/stores/world-state.store";
import { useAgentStore } from "../../../shared/stores/agent.store";
import { useAgentConfigs } from "../../agents/hooks/use-agents";
import { useChat } from "../../chats/hooks/use-chats";
import { useTrackerStateController } from "../../world-state/hooks/use-tracker-state-controller";
import { discardPendingGameStatePatch } from "../../world-state/hooks/use-world-state-patcher";
import { TRACKER_SECTION_AGENT_TYPES, type TrackerPanelSection } from "../../world-state/lib/tracker-state-display";
import { useUIStore } from "../../../shared/stores/ui.store";
import type { Message } from "../../../engine/contracts/types/chat";
import type { GameState } from "../../../engine/contracts/types/game-state";
import type { HudPosition } from "../../../shared/stores/ui.store";
import { ActionsGroup } from "./RoleplayHUDActionsGroup";
import { CombinedPlayerWidget } from "./RoleplayHUDPlayerWidget";
import {
  CharactersWidget,
  CustomTrackerWidget,
  InventoryWidget,
  PersonaStatsWidget,
  QuestsWidget,
} from "./RoleplayHUDTrackerWidgets";
import { MOBILE_HUD_BTN, TrackerPanelToggleButton, WIDGET } from "./RoleplayHUDWidgetShell";
import { CombinedWorldWidget } from "./RoleplayHUDWorldWidget";

interface RoleplayHUDProps {
  chatId: string;
  characterCount: number;
  layout?: HudPosition;
  isStreaming: boolean;
  onRetriggerTrackers?: () => void;
  /** Re-run one tracker agent only (same pipeline as full tracker run). */
  onRerunSingleTracker?: (agentType: string) => void;
  onRetryFailedAgents?: () => void;
  /** When true, tracker agents are manual — show a trigger button in the widget strip */
  manualTrackers?: boolean;
  /** When provided, overrides the globally-computed set so that only per-chat agents show widgets. */
  enabledAgentTypes?: Set<string>;
  /** Chat messages (chronological) — used to resolve cached prompt injections on the latest assistant reply */
  injectionSourceMessages?: Message[];
}
export function RoleplayHUD({
  chatId,
  characterCount: _characterCount,
  layout = "top",
  isStreaming,
  onRetriggerTrackers,
  onRerunSingleTracker,
  onRetryFailedAgents,
  manualTrackers,
  mobileCompact,
  enabledAgentTypes: enabledAgentTypesProp,
  injectionSourceMessages,
}: RoleplayHUDProps & { mobileCompact?: boolean }) {
  const [agentsOpen, setAgentsOpen] = useState(false);
  const {
    gameState,
    playerStats,
    personaStats: personaStatBars,
    presentCharacters,
    inventory,
    quests: activeQuests,
    customTrackerFields,
    gameStateRefreshing,
    patchField,
    patchPlayerStats,
  } = useTrackerStateController(chatId, "roleplay-hud");
  const setGameState = useGameStateStore((s) => s.setGameState);

  const { data: agentConfigs } = useAgentConfigs();
  const globalEnabledAgentTypes = useMemo(() => {
    const set = new Set<string>();
    if (agentConfigs) {
      for (const a of agentConfigs as Array<{ type: string; enabled: string }>) {
        if (a.enabled === "true") set.add(a.type);
      }
    }
    return set;
  }, [agentConfigs]);
  const enabledAgentTypes = enabledAgentTypesProp ?? globalEnabledAgentTypes;

  const { data: chatForAgentsMenu } = useChat(chatId);
  const agentsMenuMetadata = useMemo(() => {
    const raw = chatForAgentsMenu?.metadata;
    let m: Record<string, unknown> = {};
    if (typeof raw === "string") {
      try {
        m = JSON.parse(raw) as Record<string, unknown>;
      } catch {
        m = {};
      }
    } else if (raw && typeof raw === "object") {
      m = raw as Record<string, unknown>;
    }
    return m;
  }, [chatForAgentsMenu?.metadata]);
  const showInjectionsTab = agentsMenuMetadata.showInjectionsPanel === true;
  const showSecretPlotTab =
    agentsMenuMetadata.showSecretPlotPanel === true && enabledAgentTypes.has("secret-plot-driver");

  const thoughtBubbles = useAgentStore((s) => s.thoughtBubbles);
  const isAgentProcessing = useAgentStore((s) => s.isProcessing);
  const failedAgentTypes = useAgentStore((s) => s.failedAgentTypes);
  const failedAgentFailures = useAgentStore((s) => s.failedAgentFailures);
  const dismissThoughtBubble = useAgentStore((s) => s.dismissThoughtBubble);
  const clearThoughtBubbles = useAgentStore((s) => s.clearThoughtBubbles);
  const resetAgentStore = useAgentStore((s) => s.reset);
  const trackerPanelEnabled = useUIStore((s) => s.trackerPanelEnabled);
  const trackerPanelOpen = useUIStore((s) => s.trackerPanelOpen);
  const trackerPanelHideHudWidgets = useUIStore((s) => s.trackerPanelHideHudWidgets);
  const toggleTrackerPanel = useUIStore((s) => s.toggleTrackerPanel);

  const isTrackerBusy = isAgentProcessing || isStreaming || gameStateRefreshing;
  const showHudTrackerWidgets = !gameStateRefreshing && !(trackerPanelEnabled && trackerPanelHideHudWidgets);

  const clearGameState = useCallback(() => {
    const cleared = {
      date: null,
      time: null,
      location: null,
      weather: null,
      temperature: null,
      presentCharacters: [],
      recentEvents: [],
      playerStats: {
        stats: [],
        attributes: null,
        skills: {},
        inventory: [],
        activeQuests: [],
        status: "",
      },
      personaStats: [],
    };
    const prev = useGameStateStore.getState().current;
    if (prev?.chatId === chatId) {
      setGameState({ ...prev, ...cleared } as GameState);
    } else {
      setGameState({
        id: "",
        chatId,
        messageId: "",
        swipeIndex: 0,
        createdAt: "",
        ...cleared,
      } as GameState);
    }
    void discardPendingGameStatePatch(chatId)
      .then(() => worldStateApi.patch(chatId, { ...cleared, manual: true, clearOverrides: true }))
      .catch(() => {});
    // Clear committed agent runs & memory from DB + reset client state
    invokeTauri("agent_runs_clear_for_chat", { chatId }).catch(() => {});
    resetAgentStore();
  }, [chatId, setGameState, resetAgentStore]);

  const date = gameState?.date ?? null;
  const time = gameState?.time ?? null;
  const location = gameState?.location ?? null;
  const weather = gameState?.weather ?? null;
  const temperature = gameState?.temperature ?? null;
  const personaStatus = playerStats?.status ?? "";
  const playerTrackerSections: TrackerPanelSection[] = ["persona", "characters", "quests", "custom"];
  const hasPlayerTrackerSections = playerTrackerSections.some((section) =>
    enabledAgentTypes.has(TRACKER_SECTION_AGENT_TYPES[section]),
  );

  const isVertical = layout === "left" || layout === "right";
  // If mobileCompact, widgets are even narrower and action buttons are not cut off

  return (
    <div
      className={cn(
        "rpg-hud",
        isVertical ? "flex flex-col items-center gap-1.5" : "flex items-center gap-1.5",
        mobileCompact && "flex-1 min-w-0",
      )}
    >
      {trackerPanelEnabled && !trackerPanelOpen && <TrackerPanelToggleButton onToggle={toggleTrackerPanel} />}

      {/* Actions (Agents + Clear) */}
      <ActionsGroup
        chatId={chatId}
        injectionSourceMessages={injectionSourceMessages}
        agentConfigs={agentConfigs}
        isVertical={isVertical}
        agentsOpen={agentsOpen}
        setAgentsOpen={setAgentsOpen}
        isAgentProcessing={isAgentProcessing}
        isGenerationBusy={isTrackerBusy}
        thoughtBubbles={thoughtBubbles}
        clearThoughtBubbles={clearThoughtBubbles}
        dismissThoughtBubble={dismissThoughtBubble}
        enabledAgentTypes={enabledAgentTypes}
        clearGameState={clearGameState}
        onRetriggerTrackers={onRetriggerTrackers}
        onRetryFailedAgents={onRetryFailedAgents}
        failedAgentTypes={failedAgentTypes}
        failedAgentFailures={failedAgentFailures}
        showInjectionsTab={showInjectionsTab}
        showSecretPlotTab={showSecretPlotTab}
      />

      {/* ── Mobile: combined widgets, centered ── */}
      {showHudTrackerWidgets && (
        <div className={cn("flex items-center gap-0.5 md:hidden", mobileCompact && "flex-1 justify-center")}>
          {enabledAgentTypes.has(TRACKER_SECTION_AGENT_TYPES.world) && (
            <CombinedWorldWidget
              location={location ?? ""}
              date={date ?? ""}
              time={time ?? ""}
              weather={weather ?? ""}
              temperature={temperature ?? ""}
              onSaveLocation={(v) => patchField("location", v)}
              onSaveDate={(v) => patchField("date", v)}
              onSaveTime={(v) => patchField("time", v)}
              onSaveWeather={(v) => patchField("weather", v)}
              onSaveTemperature={(v) => patchField("temperature", v)}
              layout={layout}
              onRerunSingleTracker={onRerunSingleTracker}
              isTrackerRetryBusy={isTrackerBusy}
            />
          )}

          {hasPlayerTrackerSections && (
            <CombinedPlayerWidget
              layout={layout}
              showPersona={enabledAgentTypes.has(TRACKER_SECTION_AGENT_TYPES.persona)}
              showCharacters={enabledAgentTypes.has(TRACKER_SECTION_AGENT_TYPES.characters)}
              showQuests={enabledAgentTypes.has(TRACKER_SECTION_AGENT_TYPES.quests)}
              showCustomTracker={enabledAgentTypes.has(TRACKER_SECTION_AGENT_TYPES.custom)}
              personaStats={personaStatBars}
              onUpdatePersonaStats={(bars) => patchField("personaStats", bars)}
              personaStatus={personaStatus}
              onUpdatePersonaStatus={(status) => patchPlayerStats("status", status)}
              characters={presentCharacters}
              onUpdateCharacters={(chars) => patchField("presentCharacters", chars)}
              inventory={inventory}
              onUpdateInventory={(items) => patchPlayerStats("inventory", items)}
              quests={activeQuests}
              onUpdateQuests={(q) => patchPlayerStats("activeQuests", q)}
              customTrackerFields={customTrackerFields}
              onUpdateCustomTracker={(fields) => patchPlayerStats("customTrackerFields", fields)}
              onRerunSingleTracker={onRerunSingleTracker}
              isTrackerRetryBusy={isTrackerBusy}
            />
          )}

          {/* Manual tracker trigger button (mobile) */}
          {manualTrackers && onRetriggerTrackers && (
            <button
              onClick={(e) => {
                e.preventDefault();
                onRetriggerTrackers();
              }}
              disabled={isTrackerBusy}
              className={cn(
                MOBILE_HUD_BTN,
                "justify-center text-[0.5625rem] font-medium",
                isTrackerBusy ? "text-purple-600 dark:text-purple-300" : "text-[var(--muted-foreground)]",
              )}
            >
              <RefreshCw size="0.875rem" className={cn("shrink-0 h-4 w-4", isTrackerBusy && "animate-spin")} />
            </button>
          )}
        </div>
      )}

      {/* ── Desktop: separate individual widgets ── */}
      {showHudTrackerWidgets && (
        <div className="hidden md:flex items-center gap-1.5">
          {enabledAgentTypes.has(TRACKER_SECTION_AGENT_TYPES.world) && (
            <CombinedWorldWidget
              location={location ?? ""}
              date={date ?? ""}
              time={time ?? ""}
              weather={weather ?? ""}
              temperature={temperature ?? ""}
              onSaveLocation={(v) => patchField("location", v)}
              onSaveDate={(v) => patchField("date", v)}
              onSaveTime={(v) => patchField("time", v)}
              onSaveWeather={(v) => patchField("weather", v)}
              onSaveTemperature={(v) => patchField("temperature", v)}
              layout={layout}
              onRerunSingleTracker={onRerunSingleTracker}
              isTrackerRetryBusy={isTrackerBusy}
            />
          )}

          {enabledAgentTypes.has(TRACKER_SECTION_AGENT_TYPES.persona) && (
            <PersonaStatsWidget
              bars={personaStatBars}
              onUpdate={(bars) => patchField("personaStats", bars)}
              status={personaStatus}
              onUpdateStatus={(status) => patchPlayerStats("status", status)}
              layout={layout}
              onRerunSingleTracker={onRerunSingleTracker}
              isTrackerRetryBusy={isTrackerBusy}
            />
          )}

          {enabledAgentTypes.has(TRACKER_SECTION_AGENT_TYPES.characters) && (
            <CharactersWidget
              characters={presentCharacters}
              onUpdate={(chars) => patchField("presentCharacters", chars)}
              chatId={chatId}
              layout={layout}
              onRerunSingleTracker={onRerunSingleTracker}
              isTrackerRetryBusy={isTrackerBusy}
            />
          )}

          {hasPlayerTrackerSections && (
            <InventoryWidget
              items={inventory}
              onUpdate={(items) => patchPlayerStats("inventory", items)}
              layout={layout}
            />
          )}

          {enabledAgentTypes.has(TRACKER_SECTION_AGENT_TYPES.quests) && (
            <QuestsWidget
              quests={activeQuests}
              onUpdate={(q) => patchPlayerStats("activeQuests", q)}
              layout={layout}
              onRerunSingleTracker={onRerunSingleTracker}
              isTrackerRetryBusy={isTrackerBusy}
            />
          )}

          {enabledAgentTypes.has(TRACKER_SECTION_AGENT_TYPES.custom) && (
            <CustomTrackerWidget
              fields={customTrackerFields}
              onUpdate={(fields) => patchPlayerStats("customTrackerFields", fields)}
              layout={layout}
              onRerunSingleTracker={onRerunSingleTracker}
              isTrackerRetryBusy={isTrackerBusy}
            />
          )}

          {/* Manual tracker trigger button (desktop) */}
          {manualTrackers && onRetriggerTrackers && (
            <button
              onClick={(e) => {
                e.preventDefault();
                onRetriggerTrackers();
              }}
              disabled={isTrackerBusy}
              className={cn(WIDGET, isTrackerBusy ? "text-purple-300" : "text-[var(--muted-foreground)]")}
              title={isTrackerBusy ? "Trackers running…" : "Run Trackers"}
            >
              <RefreshCw size="0.875rem" className={cn(isTrackerBusy && "animate-spin")} />
            </button>
          )}
        </div>
      )}
    </div>
  );
}
