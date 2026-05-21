import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { RefreshCw, Sparkles } from "lucide-react";
import type { InventoryItem, PresentCharacter, QuestProgress } from "../../../engine/contracts/types/game-state";
import type { Persona } from "../../../engine/contracts/types/persona";
import { useUIStore } from "../../../shared/stores/ui.store";
import { useChatStore } from "../../../shared/stores/chat.store";
import { useAgentStore } from "../../../shared/stores/agent.store";
import { useChat, useChatMessages, useUpdateChatMetadata } from "../../chats/hooks/use-chats";
import { useCharacters, usePersonas } from "../../characters/hooks/use-characters";
import { useGenerate } from "../../generation/hooks/use-generate";
import { useTrackerCharacterAvatarActions } from "../../world-state/hooks/use-tracker-character-avatar-actions";
import { useTrackerStateController } from "../../world-state/hooks/use-tracker-state-controller";
import {
  appendTrackerListItem,
  createManualCharacterStat,
  createManualInventoryItem,
  createManualPresentCharacter,
  createManualQuest,
  removeTrackerListItem,
  replaceTrackerListItem,
} from "../../world-state/lib/tracker-state-edits";
import {
  TRACKER_AGENT_TYPE_IDS,
  TRACKER_SECTION_AGENT_TYPES,
  TRACKER_SECTION_RERUN_TITLES,
  type TrackerPanelSection,
} from "../../world-state/lib/tracker-state-display";
import { parseCharacterDisplayData } from "../../../shared/lib/character-display";
import { cn } from "../../../shared/lib/utils";
import { TRACKER_FEATURED_CHARACTER_META_KEY } from "./tracker-data-sidebar.constants";
import {
  getCharacterFeatureKey,
  getCharacterProfileColors,
  getLatestSpriteExpressionsFromMessages,
  isSpriteLookupCharacterId,
  normalizeLookupText,
  normalizeMaybeJsonStringArray,
  normalizeSpriteExpressionMap,
  normalizeStringArray,
  parseMetadataRecord,
  type TrackerProfileColors,
} from "./tracker-data-sidebar.helpers";
import { EmptySection, SectionIconButton } from "./tracker-data-sidebar.controls";
import { WorldStatePanel } from "./WorldStatePanel";
import { PersonaInventoryPanel } from "./PersonaTrackerPanel";
import { CharacterTrackerPanel } from "./CharacterTrackerPanel";
import { QuestTrackerPanel } from "./QuestTrackerPanel";
import { CustomTrackerPanel } from "./CustomTrackerPanel";
import { TrackerSkeleton } from "./TrackerSkeleton";
import { TrackerSidebarHeader } from "./TrackerSidebarHeader";

export function TrackerDataSidebar({ fillHeight = false }: { fillHeight?: boolean } = {}) {
  const activeChatId = useChatStore((s) => s.activeChatId);
  const streamingChatId = useChatStore((s) => s.streamingChatId);
  const isStreamingGlobal = useChatStore((s) => s.isStreaming);
  const {
    gameState: currentGameState,
    playerStats,
    personaStats,
    presentCharacters,
    inventory,
    quests,
    customTrackerFields: customFields,
    gameStateRefreshing,
    isLoadingGameState,
    patchField,
    patchPlayerStats,
    flushPatch,
  } = useTrackerStateController(activeChatId, "tracker-data-sidebar");
  const trackerPanelSide = useUIStore((s) => s.trackerPanelSide);
  const trackerPanelCollapsedSections = useUIStore((s) => s.trackerPanelCollapsedSections);
  const trackerPanelSectionOrder = useUIStore((s) => s.trackerPanelSectionOrder);
  const trackerPanelUseExpressionSprites = useUIStore((s) => s.trackerPanelUseExpressionSprites);
  const toggleTrackerPanelSectionCollapsed = useUIStore((s) => s.toggleTrackerPanelSectionCollapsed);
  const setTrackerPanelOpen = useUIStore((s) => s.setTrackerPanelOpen);
  const setTrackerPanelSide = useUIStore((s) => s.setTrackerPanelSide);
  const isAgentProcessing = useAgentStore((s) => s.isProcessing);
  const { data: chat } = useChat(activeChatId);
  const updateChatMetadata = useUpdateChatMetadata();
  const { retryAgents } = useGenerate();
  const [deleteMode, setDeleteMode] = useState(false);
  const [addMode, setAddMode] = useState(false);
  const [featuredCharacterCards, setFeaturedCharacterCards] = useState<Set<string>>(() => new Set());
  const [avatarUploadIndex, setAvatarUploadIndex] = useState<number | null>(null);
  const avatarFileInputRef = useRef<HTMLInputElement>(null);
  const featuredCharacterCardsRef = useRef<Set<string>>(new Set());
  const isStreaming = isStreamingGlobal && streamingChatId === activeChatId;
  const trackerRetryBusy = isAgentProcessing || isStreaming || gameStateRefreshing;

  const chatMeta = useMemo(() => {
    const raw = (chat as unknown as { metadata?: string | Record<string, unknown> } | undefined)?.metadata;
    return parseMetadataRecord(raw);
  }, [chat]);
  const chatCharacterIds = useMemo(
    () => normalizeMaybeJsonStringArray((chat as unknown as { characterIds?: unknown } | undefined)?.characterIds),
    [chat],
  );
  const enabledAgentTypes = useMemo(() => {
    const set = new Set<string>();
    if (!chatMeta.enableAgents) return set;
    const activeAgentIds = Array.isArray(chatMeta.activeAgentIds) ? chatMeta.activeAgentIds : [];
    for (const id of activeAgentIds) {
      if (typeof id === "string") set.add(id);
    }
    return set;
  }, [chatMeta]);
  const expressionAgentEnabled = enabledAgentTypes.has("expression");
  const isSectionEnabled = useCallback(
    (section: TrackerPanelSection) => {
      const agentType = TRACKER_SECTION_AGENT_TYPES[section];
      return !!agentType && enabledAgentTypes.has(agentType);
    },
    [enabledAgentTypes],
  );
  const personaTrackerEnabled = isSectionEnabled("persona");
  const characterTrackerEnabled = isSectionEnabled("characters");
  const orderedTrackerSections = useMemo(
    () => trackerPanelSectionOrder.filter(isSectionEnabled),
    [isSectionEnabled, trackerPanelSectionOrder],
  );
  const spriteExpressionLookupEnabled =
    !!activeChatId &&
    trackerPanelUseExpressionSprites &&
    expressionAgentEnabled &&
    (personaTrackerEnabled || characterTrackerEnabled);
  const characterDataLookupEnabled = !!activeChatId && characterTrackerEnabled;
  const personaDataLookupEnabled = !!activeChatId && personaTrackerEnabled;
  const agentConfigLookupEnabled = !!activeChatId && characterTrackerEnabled;
  const { data: messageData } = useChatMessages(activeChatId, 20, spriteExpressionLookupEnabled);
  const { data: charactersData } = useCharacters(characterDataLookupEnabled);
  const { data: personasData } = usePersonas(personaDataLookupEnabled);
  const updatePresentCharacters = useCallback(
    (characters: PresentCharacter[]) => patchField("presentCharacters", characters),
    [patchField],
  );
  const {
    autoGenerateCharacterAvatars,
    canToggleAutoGenerateCharacterAvatars,
    isUpdatingAutoGenerateCharacterAvatars,
    toggleAutoGenerateCharacterAvatars,
    uploadCharacterAvatar,
  } = useTrackerCharacterAvatarActions({
    chatId: activeChatId,
    characters: presentCharacters,
    onUpdateCharacters: updatePresentCharacters,
    agentConfigLookupEnabled,
  });
  const characterSpriteLookup = useMemo(() => {
    const rows = (
      Array.isArray(charactersData)
        ? (charactersData as Array<{ id: string; data: unknown; comment?: string | null; avatarPath?: string | null }>)
        : []
    ).filter((character) => typeof character.id === "string" && character.id.length > 0);
    const chatIdSet = new Set(chatCharacterIds);
    const orderedRows = [
      ...rows.filter((character) => chatIdSet.has(character.id)),
      ...rows.filter((character) => !chatIdSet.has(character.id)),
    ];
    const knownIds = new Set(rows.map((character) => character.id));
    const idByName = new Map<string, string>();
    const pictureById: Record<string, string> = {};
    const profileColorsById: Record<string, TrackerProfileColors> = {};
    for (const character of orderedRows) {
      if (character.avatarPath) pictureById[character.id] = character.avatarPath;
      const profileColors = getCharacterProfileColors(character.data);
      if (profileColors) profileColorsById[character.id] = profileColors;
      const display = parseCharacterDisplayData(character);
      const nameKey = normalizeLookupText(display.name);
      if (nameKey && !idByName.has(nameKey)) idByName.set(nameKey, character.id);
      const commentKey = normalizeLookupText(display.comment);
      if (commentKey && !idByName.has(commentKey)) idByName.set(commentKey, character.id);
    }
    return { knownIds, idByName, pictureById, profileColorsById };
  }, [charactersData, chatCharacterIds]);
  const resolveSpriteCharacterId = useCallback(
    (character: PresentCharacter) => {
      const rawId = character.characterId?.trim() ?? "";
      if (rawId && characterSpriteLookup.knownIds.has(rawId)) return rawId;
      const idNameMatch = characterSpriteLookup.idByName.get(normalizeLookupText(rawId));
      if (idNameMatch) return idNameMatch;
      const nameMatch = characterSpriteLookup.idByName.get(normalizeLookupText(character.name));
      if (nameMatch) return nameMatch;
      return isSpriteLookupCharacterId(rawId) ? rawId : null;
    },
    [characterSpriteLookup],
  );
  const cachedMessages = useMemo(() => messageData?.pages.flat() ?? [], [messageData]);
  const spriteExpressions = useMemo(
    () =>
      getLatestSpriteExpressionsFromMessages(cachedMessages as Array<{ role?: string; extra?: unknown }>) ??
      normalizeSpriteExpressionMap(chatMeta.spriteExpressions),
    [cachedMessages, chatMeta.spriteExpressions],
  );
  useEffect(() => {
    const next = new Set(normalizeStringArray(chatMeta[TRACKER_FEATURED_CHARACTER_META_KEY]));
    featuredCharacterCardsRef.current = next;
    setFeaturedCharacterCards(next);
  }, [activeChatId, chatMeta]);
  const personas = useMemo(() => (Array.isArray(personasData) ? (personasData as Persona[]) : []), [personasData]);
  const activePersona = useMemo(() => {
    const chatPersonaId = (chat as unknown as { personaId?: unknown } | undefined)?.personaId;
    const selectedPersonaId = typeof chatPersonaId === "string" ? chatPersonaId : null;
    return (
      (selectedPersonaId ? personas.find((persona) => persona.id === selectedPersonaId) : null) ??
      personas.find((persona) => persona.isActive) ??
      null
    );
  }, [chat, personas]);
  const expressionSpritesEnabled = trackerPanelUseExpressionSprites && expressionAgentEnabled;

  const isPanelCollapsed = (section: TrackerPanelSection) => trackerPanelCollapsedSections[section] === true;
  const hasFixedTrackerPanel = orderedTrackerSections.length > 0;
  const showTrackerSections = !!activeChatId && !isLoadingGameState && !!currentGameState && hasFixedTrackerPanel;
  const persistFeaturedCharacterCards = useCallback(
    (next: Set<string>) => {
      featuredCharacterCardsRef.current = next;
      setFeaturedCharacterCards(next);
      if (!activeChatId) return;
      updateChatMetadata.mutate({
        id: activeChatId,
        [TRACKER_FEATURED_CHARACTER_META_KEY]: Array.from(next),
      });
    },
    [activeChatId, updateChatMetadata],
  );
  const toggleFeaturedCharacterCard = useCallback(
    (key: string) => {
      const next = new Set(featuredCharacterCardsRef.current);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      persistFeaturedCharacterCards(next);
    },
    [persistFeaturedCharacterCards],
  );
  const openAvatarUpload = useCallback((index: number) => {
    setAvatarUploadIndex(index);
    avatarFileInputRef.current?.click();
  }, []);
  const handleAvatarUpload = useCallback(
    (index: number, file: File) => void uploadCharacterAvatar(index, file),
    [uploadCharacterAvatar],
  );
  const updateCharacter = (index: number, character: PresentCharacter) => {
    updatePresentCharacters(replaceTrackerListItem(presentCharacters, index, character));
  };
  const removeCharacter = (index: number) => {
    const removed = presentCharacters[index];
    if (removed) {
      const removedKey = getCharacterFeatureKey(removed, index);
      if (featuredCharacterCardsRef.current.has(removedKey)) {
        const nextFeatured = new Set(featuredCharacterCardsRef.current);
        nextFeatured.delete(removedKey);
        persistFeaturedCharacterCards(nextFeatured);
      }
    }
    updatePresentCharacters(removeTrackerListItem(presentCharacters, index));
  };
  const addCharacter = () => {
    updatePresentCharacters(appendTrackerListItem(presentCharacters, createManualPresentCharacter()));
  };
  const updateInventory = (items: InventoryItem[]) => patchPlayerStats("inventory", items);
  const updateInventoryItem = (index: number, item: InventoryItem) => {
    updateInventory(replaceTrackerListItem(inventory, index, item));
  };
  const removeInventoryItem = (index: number) => {
    updateInventory(removeTrackerListItem(inventory, index));
  };
  const addInventoryItem = () => {
    updateInventory(appendTrackerListItem(inventory, createManualInventoryItem()));
  };
  const updateQuests = (nextQuests: QuestProgress[]) => patchPlayerStats("activeQuests", nextQuests);
  const updateQuest = (index: number, quest: QuestProgress) => {
    updateQuests(replaceTrackerListItem(quests, index, quest));
  };
  const removeQuest = (index: number) => {
    updateQuests(removeTrackerListItem(quests, index));
  };
  const addQuest = () => {
    updateQuests(appendTrackerListItem(quests, createManualQuest()));
  };
  const rerunTracker = useCallback(
    async (agentType: string) => {
      if (
        !activeChatId ||
        trackerRetryBusy ||
        !TRACKER_AGENT_TYPE_IDS.has(agentType) ||
        !enabledAgentTypes.has(agentType)
      ) {
        return;
      }
      try {
        await flushPatch();
      } catch {
        return;
      }
      await retryAgents(activeChatId, [agentType]);
    },
    [activeChatId, enabledAgentTypes, flushPatch, retryAgents, trackerRetryBusy],
  );
  const renderRerunAction = (section: TrackerPanelSection) => {
    const agentType = TRACKER_SECTION_AGENT_TYPES[section];
    if (!agentType || !TRACKER_AGENT_TYPE_IDS.has(agentType) || !enabledAgentTypes.has(agentType)) return null;
    const title = trackerRetryBusy
      ? "A tracker or reply is already running"
      : (TRACKER_SECTION_RERUN_TITLES[section] ?? `Re-run ${agentType} tracker`);
    return (
      <SectionIconButton onClick={() => void rerunTracker(agentType)} disabled={trackerRetryBusy} title={title}>
        <RefreshCw size="0.75rem" className={trackerRetryBusy ? "animate-spin" : ""} />
      </SectionIconButton>
    );
  };
  const renderCharacterHeaderAction = () => {
    const autoAvatarTitle = autoGenerateCharacterAvatars
      ? "Auto-generate character avatars: ON"
      : "Auto-generate character avatars: OFF";
    return (
      <>
        {canToggleAutoGenerateCharacterAvatars && (
          <SectionIconButton
            onClick={toggleAutoGenerateCharacterAvatars}
            disabled={isUpdatingAutoGenerateCharacterAvatars}
            title={autoAvatarTitle}
            pressed={autoGenerateCharacterAvatars}
            tone="feature"
          >
            <Sparkles size="0.6875rem" />
          </SectionIconButton>
        )}
        {renderRerunAction("characters")}
      </>
    );
  };
  const renderTrackerSection = (section: TrackerPanelSection) => {
    if (!activeChatId || !currentGameState) return null;

    switch (section) {
      case "world":
        return (
          <WorldStatePanel
            key="world"
            state={currentGameState}
            action={renderRerunAction("world")}
            onSaveField={patchField}
            collapsed={isPanelCollapsed("world")}
            onToggleCollapsed={() => toggleTrackerPanelSectionCollapsed("world")}
          />
        );
      case "persona":
        return (
          <PersonaInventoryPanel
            key="persona"
            persona={activePersona}
            status={playerStats?.status ?? ""}
            spriteExpression={
              expressionSpritesEnabled && activePersona
                ? (spriteExpressions[activePersona.id] ?? spriteExpressions[activePersona.name] ?? "neutral")
                : undefined
            }
            personaStats={personaStats}
            inventory={inventory}
            action={renderRerunAction("persona")}
            onSaveStatus={(status) => patchPlayerStats("status", status)}
            onUpdatePersonaStats={(stats) => patchField("personaStats", stats)}
            onAddPersonaStat={() =>
              patchField("personaStats", appendTrackerListItem(personaStats, createManualCharacterStat()))
            }
            onAddInventoryItem={addInventoryItem}
            onUpdateInventoryItem={updateInventoryItem}
            onRemoveInventoryItem={removeInventoryItem}
            deleteMode={deleteMode}
            addMode={addMode}
            collapsed={isPanelCollapsed("persona")}
            onToggleCollapsed={() => toggleTrackerPanelSectionCollapsed("persona")}
          />
        );
      case "characters":
        return (
          <CharacterTrackerPanel
            key="characters"
            activeChatId={activeChatId}
            characters={presentCharacters}
            featuredCharacterCards={featuredCharacterCards}
            spriteExpressions={spriteExpressions}
            expressionSpritesEnabled={expressionSpritesEnabled}
            characterPictures={characterSpriteLookup.pictureById}
            characterProfileColors={characterSpriteLookup.profileColorsById}
            resolveSpriteCharacterId={resolveSpriteCharacterId}
            trackerPanelSide={trackerPanelSide}
            action={renderCharacterHeaderAction()}
            onUpdateCharacter={updateCharacter}
            onRemoveCharacter={removeCharacter}
            onAddCharacter={addCharacter}
            onUploadAvatar={openAvatarUpload}
            onToggleFeatured={toggleFeaturedCharacterCard}
            deleteMode={deleteMode}
            addMode={addMode}
            collapsed={isPanelCollapsed("characters")}
            onToggleCollapsed={() => toggleTrackerPanelSectionCollapsed("characters")}
          />
        );
      case "quests":
        return (
          <QuestTrackerPanel
            key="quests"
            quests={quests}
            action={renderRerunAction("quests")}
            onAddQuest={addQuest}
            onUpdateQuest={updateQuest}
            onRemoveQuest={removeQuest}
            deleteMode={deleteMode}
            addMode={addMode}
            collapsed={isPanelCollapsed("quests")}
            onToggleCollapsed={() => toggleTrackerPanelSectionCollapsed("quests")}
          />
        );
      case "custom":
        return (
          <CustomTrackerPanel
            key="custom"
            fields={customFields}
            action={renderRerunAction("custom")}
            onUpdateFields={(fields) => patchPlayerStats("customTrackerFields", fields)}
            deleteMode={deleteMode}
            addMode={addMode}
            collapsed={isPanelCollapsed("custom")}
            onToggleCollapsed={() => toggleTrackerPanelSectionCollapsed("custom")}
          />
        );
      default:
        return null;
    }
  };

  return (
    <section
      data-component="TrackerDataSidebar"
      className={cn(
        "@container relative flex flex-col overflow-hidden bg-[color-mix(in_srgb,var(--background)_8%,transparent)] backdrop-blur-sm",
        fillHeight ? "h-full" : "min-h-0",
      )}
    >
      <div className="pointer-events-none absolute inset-0 z-0 opacity-[0.08] [background-image:linear-gradient(color-mix(in_srgb,var(--foreground)_12%,transparent)_1px,transparent_1px),linear-gradient(90deg,color-mix(in_srgb,var(--foreground)_9%,transparent)_1px,transparent_1px)] [background-size:8px_8px]" />
      <input
        ref={avatarFileInputRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={(event) => {
          const file = event.target.files?.[0];
          const index = avatarUploadIndex;
          setAvatarUploadIndex(null);
          if (file && index !== null) handleAvatarUpload(index, file);
          event.target.value = "";
        }}
      />

      <TrackerSidebarHeader
        trackerPanelSide={trackerPanelSide}
        addMode={addMode}
        deleteMode={deleteMode}
        onSetAddMode={setAddMode}
        onSetDeleteMode={setDeleteMode}
        onSetSide={setTrackerPanelSide}
        onClose={() => setTrackerPanelOpen(false)}
      />

      <div className={cn("relative z-10", fillHeight && "min-h-0 flex-1 overflow-y-auto")}>
        {showTrackerSections ? orderedTrackerSections.map((section) => renderTrackerSection(section)) : null}

        {!activeChatId ? (
          <EmptySection>Select a chat to view tracker data.</EmptySection>
        ) : isLoadingGameState ? (
          <TrackerSkeleton />
        ) : !currentGameState ? (
          <EmptySection>No tracker data yet.</EmptySection>
        ) : !hasFixedTrackerPanel ? (
          <EmptySection>No enabled tracker panels.</EmptySection>
        ) : null}
      </div>
    </section>
  );
}
