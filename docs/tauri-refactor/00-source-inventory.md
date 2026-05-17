# Source Inventory

Use this file as the guardrail for implementation slices. Every file from the original app must be moved, mapped, deferred, or explicitly approved for removal.

Original source root: `E:/Personal Projects/Marinara-Engine`.

## Frontend Sources

Source: `packages/client`.

Track these groups during frontend slices:

- `src/components/layout`: app shell, top bar, sidebars, modal root, theme injector.
- `src/components/panels`: settings and feature panels.
- `src/components/ui`: reusable UI primitives and shared controls.
- `src/components/modals`: modal bodies owned by their feature modules.
- `src/components/chat`: chat, conversation, roleplay, gallery, scene, and prompt preview UI.
- `src/components/game`: game mode UI.
- `src/components/agents`: agents, tools, regex, context, and debug UI.
- `src/components/bot-browser`, `characters`, `connections`, `lorebooks`, `onboarding`, `personas`, `presets`, `spotify`: feature-owned UI.
- `src/hooks`: migrate with the feature that owns the behavior.
- `src/stores`: migrate with the feature or app/shared state boundary that owns the data.
- `src/lib`: keep frontend-only helpers in React; move filesystem, secret, provider, import, and backend behavior to Rust.
- `src/styles`: global styles and themes.
- `public`: copy assets only when the slice that renders them moves.

## Backend Sources

Source: `packages/server` and `packages/shared`.

Track these groups during Rust backend slices:

- `src/routes`: route behavior maps to Tauri commands and Rust services.
- `src/services`: service behavior maps to Rust domain modules.
- `src/services/storage`: repository behavior maps to raw file-backed Rust repositories.
- `src/db`: use only as source metadata for the current file-native shapes; do not port SQL, Drizzle, SQLite, or migrations.
- `src/middleware`: map relevant protections to Tauri capabilities, command validation, filesystem policy, outbound URL policy, and secret handling.
- `src/utils`, `src/config`, `src/lib`: map useful behavior to `core`, `security`, or owning domain modules.
- `packages/shared/src/types`, `schemas`, `constants`, `utils`: map to Rust domain DTOs, generated TypeScript bindings, or frontend-only helpers.
- `assets`: copy defaults only when the owning feature slice needs them.
- `scripts`, platform folders, installer/launcher support: account for them during sidecar, updates, packaging, and sync phases.
- `tests`: use as behavior references for Rust service/repository tests.

## Required Slice Update

Each slice handoff must list source inventory status:

- moved
- mapped to a new module
- deferred with reason
- removed with explicit approval

Do not leave a touched source area unaccounted for.

## Phase 2 Rework Required

The current Phase 2 inventory overstates several completed slices. The intended process is to move and lightly reorganize the existing React UI from `E:/Personal Projects/Marinara-Engine/packages/client/src`, not replace feature surfaces with simplified rewrites.

Before continuing to Phase 2 Slice 10 or Phase 3, run a Phase 2 cleanup/rework slice with these rules:

- Do not implement new product behavior.
- Do not add fake data, mock persistence, fake command success, fixture routes, preview routes, or compatibility shims.
- Move original UI files into `src/features`, `src/shared`, or `src/app` according to ownership.
- Keep existing UI structure, markup, controls, layout, and visual states as intact as practical.
- Replace backend-dependent hooks/actions with explicit unavailable seams only where the behavior belongs to a later Rust/backend/file slice.
- If an original source file is not moved in the cleanup slice, list it as deferred by exact path and reason.
- If a simplified replacement already exists, either replace it with the moved original UI or mark the corresponding slice as partial until it is replaced.

Recommended cleanup order:

1. Audit all Phase 1 and Phase 2 claimed source moves against the original source tree.
2. Correct inventory statuses from "Complete" to "Partial" where the migrated file is a simplified rewrite.
3. Rework chat/conversation files first because later roleplay and game surfaces depend on them.
4. Rework lorebook/preset editors next because they are self-contained editor surfaces.
5. Rework roleplay/conversation UI after chat foundations are faithful.
6. Rework game UI last because it has the largest component graph and should be split into smaller move-only sub-slices.

Known likely partial/simplified areas:

- `components/chat/ConversationView.tsx`, `ConversationInput.tsx`, `ConversationMessage.tsx`, `ChatConversationSurface.tsx`, `ChatMessage.tsx`, `ChatInput.tsx`, and `ChatArea.tsx`.
- `components/chat/ChatRoleplaySurface.tsx`, `SceneBanner.tsx`, `CyoaChoices.tsx`, and related roleplay HUD/panel/overlay files.
- `components/lorebooks/LorebookEditor.tsx` and related lorebook editor subcomponents.
- `components/presets/PresetEditor.tsx` and related prompt/preset modal/editor files.
- `components/game/*`, especially `GameSurface.tsx`, `GameNarration.tsx`, `GameInput.tsx`, setup, map, combat, journal, inventory, QTE, widget, session history, and modal components.

The cleanup slice should end with a new status section below that records exact files moved, exact files deferred, and which old simplified files were replaced.

## Slice Status

### Phase 0 Baseline Structure

Status: Complete.

- Starter Tauri frontend bootstrap moved from `src/main.tsx`, `src/App.tsx`, and `src/App.css` to `src/app`.
- Starter sample assets in `src/assets` and `public` are deferred until sample Tauri code is removed during hardening.
- Rust backend source remains starter Tauri code in `src-tauri/src`; planned domain crate homes are mapped under `src-tauri/crates`.
- No original Marinara source from `E:/Personal Projects/Marinara-Engine` has been moved yet.

### Phase 1 Frontend Shell And Shared Foundations

Status: Complete.

- Moved original `packages/client/src/App.tsx` and `main.tsx` into `src/app`, then adapted them for the Tauri refactor shell without old HTTP health checks, PWA registration, keep-alive, CSRF fetch shims, or server font preloading.
- Moved original `components/layout/AppShell.tsx`, `TopBar.tsx`, `RightPanel.tsx`, `ModalRenderer.tsx`, and `CustomThemeInjector.tsx` into `src/app/shell` and `src/app/providers`.
- Mapped original shell-owned range slider setup into `src/app/startup/range-slider-sync.ts`.
- Moved original shared UI dialog/modal primitives from `components/ui/AppDialogRenderer.tsx` and `components/ui/Modal.tsx` into `src/shared/components/ui`.
- Moved original frontend-only helpers `lib/utils.ts` and `lib/app-dialogs.ts` into `src/shared/lib`.
- Moved original browser UI state stores `stores/ui.store.ts` and `stores/dialog.store.ts` into `src/shared/stores`.
- Global styles are present under `src/styles/globals.css` and `src/styles/themes/sillytavern.css`; `src/app/main.tsx` imports them directly.
- Deferred feature screens and backend-backed surfaces from `components/chat`, `components/panels`, `components/modals`, `components/spotify`, `components/onboarding`, feature hooks, feature stores, and backend API clients to Phase 2+ slices. Current shell placeholders preserve navigation locations without adding fake backend behavior.

### Phase 2 Slice 1 Settings Shell And Settings Sections

Status: Complete.

- Moved original `components/panels/SettingsPanel.tsx` into `src/features/settings/components/SettingsPanel.tsx` with layout and markup preserved.
- Moved original `components/panels/settings/SettingControls.tsx` into `src/features/settings/components/settings/SettingControls.tsx`.
- Moved original shared UI primitives used by settings, `components/ui/HelpTooltip.tsx`, `DraftNumberInput.tsx`, and `ExportFormatDialog.tsx`, into `src/shared/components/ui`.
- Reopened the settings slice during cleanup and replaced the reduced `SettingsPanel` with the moved original `components/panels/SettingsPanel.tsx`, including tracker-panel appearance controls, custom font/background UI, import/export flows, advanced/admin controls, and the original synced-theme hook contract.
- Moved original settings support that the panel expects, including `components/panels/settings/TTSConfigCard.tsx` and the original `hooks/use-themes.ts`; backend/file actions intentionally route through unavailable API seams.
- Wired the existing right panel settings route to render the migrated `SettingsPanel`.
- Deferred real persistence, imports, backups, updates, background/font file operations, extension execution, and destructive data actions until their owning Rust backend slices.

### Phase 2 Slice 2 Theme/Preferences UI

Status: Complete.

- Mapped theme and appearance preferences from original `components/panels/SettingsPanel.tsx` to the migrated settings feature, preserving the Appearance and Themes tab UI.
- Mapped original theme preference state from `stores/ui.store.ts` to `src/shared/stores/ui.store.ts`, including color scheme, visual theme, font sizing, conversation gradient, text appearance, avatar style, and local custom theme fields.
- Wired app-level preference effects in `src/app/App.tsx` and `src/app/providers/CustomThemeInjector.tsx` so color scheme, visual theme, font size, font family, and active custom CSS apply to the document shell.
- Replaced the earlier local-theme implementation with the moved original synced-theme hook contract. Rust-backed theme storage/sync, custom font folder operations, Google Fonts download, background file operations, and chat metadata persistence are deferred through failing API/file seams.

### Phase 2 Slice 3 Character/Persona Library Read Surfaces

Status: Complete.

- Moved original `components/panels/CharactersPanel.tsx` into `src/features/characters/components/CharactersPanel.tsx` and wired the right-panel `characters` route to render it.
- Moved original `components/panels/PersonasPanel.tsx` into `src/features/personas/components/PersonasPanel.tsx` and wired the right-panel `personas` route to render it.
- Moved original `components/characters/CharacterLibraryView.tsx` into `src/features/characters/components/CharacterLibraryView.tsx` and wired the existing character-library UI state to render it in the center shell.
- Moved original shared `components/ui/ContextMenu.tsx` into `src/shared/components/ui/ContextMenu.tsx`.
- Mapped original `lib/character-display.ts` to `src/features/characters/lib/character-display.ts`.
- Added feature-owned frontend API seams in `src/features/characters/api/characters-api.ts` and `src/features/personas/api/personas-api.ts`; these intentionally fail with explicit Rust-backend-slice errors instead of fake persistence.
- Added Phase 2-safe character/persona query and mutation hooks under `src/features/characters/hooks` and `src/features/personas/hooks`, preserving click paths while deferring real storage.
- Deferred full character editor, persona editor, create/import/maker modals, PNG import/export, avatar upload, group persistence, duplicate/delete, active persona persistence, and start-chat behavior until their owning frontend/modal and Rust backend slices.

### Phase 2 Slice 4 Chat Shell And Navigation

Status: Complete.
Review: Approved by human.

- Mapped original `components/layout/ChatSidebar.tsx` chat navigation behavior into the migrated shell sidebar, preserving mode tabs, search, sort cycling, tag filters, chat row selection, grouped branch badges, folder read display, active-row highlighting, mobile close-on-select, and the user status footer.
- Added feature-owned chat DTOs in `src/features/chats/types.ts` for the frontend read surface until Rust-owned DTO bindings replace them in the domain DTO phase.
- Added Phase 2-safe chat API seams and hooks under `src/features/chats/api` and `src/features/chats/hooks`; these intentionally fail with explicit Rust chats backend errors instead of fake chat data or fake persistence.
- Mapped active chat selection and unread-count clearing into `src/shared/stores/chat.store.ts`, including persisted active chat ID restoration for the navigation shell.
- Wired the center shell placeholder to acknowledge selected chats while deferring message display, input, setup wizard, chat settings drawer, gallery/files drawers, branch mutation actions, folder mutations, delete actions, chat creation, connection/preset application, autonomous notifications, and all generation behavior to later reviewed slices.

### Phase 2 Slice 5 Chat Message Display/Input UI

Status: Complete, reworked from simplified surface to moved original UI tree.

- Replaced the simplified `ChatConversationView` implementation with a thin adapter into moved original `components/chat/ChatArea.tsx`.
- Moved the original chat surface tree into `src/features/chats/components`, including `ChatArea.tsx`, `ConversationView.tsx`, `ConversationMessage.tsx`, `ConversationInput.tsx`, `ChatConversationSurface.tsx`, `ChatMessage.tsx`, `ChatInput.tsx`, `ChatBranchSelector.tsx`, `ChatCommonOverlays.tsx`, `ChatFilesDrawer.tsx`, `ChatSettingsDrawer.tsx`, `ChatSetupWizard.tsx`, `ChatNotificationBubbles.tsx`, `ConversationAutonomousEffects.tsx`, `SummariesEditorModal.tsx`, `SummaryPopover.tsx`, prompt peek/replay/quick-switcher helpers, and related chat UI files.
- Replaced simplified chat hook/store contracts with moved original `hooks/use-chats.ts` and `stores/chat.store.ts`, wired through unavailable Tauri/backend seams rather than fake persistence.
- Moved original supporting chat helpers and shared UI used by the moved tree: `chat-display.ts`, `backgrounds.ts`, `slash-commands.ts`, `translate-text.ts`, `GifPicker.tsx`, `ExpandedTextarea.tsx`, and `TrackerPanelIcon.tsx`.
- Backend-backed actions remain unavailable: message persistence, generation, regeneration, cancellation, branch mutations, setup/settings persistence, gallery/files storage, prompt preview, translation, autonomous messaging, exports/imports, and provider/file actions all route to deferred seams or failing API calls.

### Phase 2 Slice 6 Lorebooks/Prompts/Presets Editors

Status: Complete, reworked from simplified editor shells to moved original UI.

- Replaced the reduced lorebook list panel with moved original `components/panels/LorebooksPanel.tsx`, preserving active-chat filtering, tag controls, bulk export/delete flows, character/persona labels, selection mode, row actions, and click-to-edit path.
- Replaced the simplified `LorebookEditor` with moved original `components/lorebooks/LorebookEditor.tsx`, plus `LorebookEntryRow.tsx`, `LorebookFolderRow.tsx`, and `LorebookFormFields.tsx`.
- Replaced the simplified lorebook hook contract with moved original `hooks/use-lorebooks.ts`; its API calls intentionally hit the unavailable backend seam instead of fake storage.
- Replaced the reduced preset list panel with moved original `components/panels/PresetsPanel.tsx`, preserving assignment behavior, choice selection modal path, bulk export/delete, selection mode, default/duplicate/delete row actions, and click-to-edit path.
- Replaced the simplified `PresetEditor` with moved original `components/presets/PresetEditor.tsx`, plus `ChoiceSelectionModal.tsx`.
- Replaced the simplified preset hook contract with moved original `hooks/use-presets.ts`; its API calls intentionally hit the unavailable backend seam instead of fake storage.
- Wired the right panel `lorebooks` and `presets` routes and center shell detail rendering for `lorebookDetailId` and `presetDetailId`.
- Deferred only backend/file/provider behavior: persistence, semantic vectorization, prompt review generation, chat preset assignment persistence, exports/imports, and provider-backed actions remain unavailable.

### Phase 2 Slice 7 Connections Read Surface

Status: Complete.

- Replaced the reduced connections read surface with moved original `components/panels/ConnectionsPanel.tsx`, including folders/reorder UI, local model card, TTS config card, agent assignment controls, active/random connection controls, and row actions.
- Mapped original `hooks/use-connections.ts` and `hooks/use-connection-folders.ts` to `src/features/connections/hooks`, preserving list/detail/folder/mutation hook contracts for downstream backend slices.
- Added frontend-owned Phase 2 connection DTOs in `src/features/connections/types.ts` until Rust-owned DTO bindings replace them.
- Added a feature-owned API seam in `src/features/connections/api/connections-api.ts`; it intentionally fails with explicit Rust connections backend errors instead of fake connection data or fake persistence.
- Wired connection row clicks to the existing center-shell detail placeholder so the navigation path is accounted for without moving the full editor.
- Deferred `components/connections/ConnectionEditor.tsx`, `components/modals/CreateConnectionModal.tsx`, provider tests, model discovery, secret persistence, image test generation, and active connection persistence until their owning frontend and Rust backend slices. UI controls now exist where they were part of the moved original panel, but backend/file/model actions still fail through unavailable seams.

### Phase 2 Slice 8 Roleplay/Conversation UI

Status: Complete, reworked from partial recreation to moved original roleplay UI tree.

- Removed the simplified `RoleplayConversationView.tsx`; roleplay now renders through moved original `ChatArea.tsx` and `ChatRoleplaySurface.tsx`.
- Moved original roleplay/chat support files including `RoleplayHUD.tsx`, `RoleplayHUDPanels.tsx`, `RoleplayHUDActionsMenu.tsx`, `ChatRoleplayPanels.tsx`, `ChatCommonOverlays.tsx`, `SceneBanner.tsx`, `CyoaChoices.tsx`, `WeatherEffects.tsx`, `SpriteOverlay.tsx`, `SpriteSidebar.tsx`, `ExpressionPanel.tsx`, `EchoChamberPanel.tsx`, `EncounterModal.tsx`, `SummaryPopover.tsx`, and autonomous/notification helpers.
- Moved compile-time support seams for roleplay dependencies: original chat preset hooks, agent hooks for roleplay HUD dependencies, encounter hooks/store, scene hooks, game-state patcher hook, and translation/autonomous placeholders.
- Mapped active chat mode selection into `src/shared/stores/chat.store.ts` and `src/app/shell/ChatSidebar.tsx` so selected roleplay chats render the roleplay surface even before the detail query resolves.
- Deferred backend behavior only: roleplay agents, scene forking/conclusion persistence, sprite persistence, autonomous sends, lorebook activation scans, encounter persistence, generation/provider behavior, and file-backed actions remain unavailable.

### Phase 2 Slice 9 Game UI

Status: Complete, reworked from simplified shell to moved original UI tree.

- Removed the simplified game DTO/API files `src/features/game/types.ts` and `src/features/game/api/game-api.ts`; the migrated game feature now uses the moved original contracts/hooks instead of the temporary simplified shell.
- Moved original `components/game/AnimatedText.tsx`, `DirectionEngine.tsx`, `DraggablePanel.tsx`, `game-asset-generation-payload.ts`, `GameCharacterSheet.tsx`, `GameCheckpoints.tsx`, `GameChoiceCards.tsx`, `GameCombatUI.tsx`, `GameDialogueOverlay.tsx`, `GameDiceResult.tsx`, `GameElementReaction.tsx`, `GameGridMap.tsx`, `GameImagePromptReviewModal.tsx`, `GameInput.tsx`, `GameInventory.tsx`, `GameJournal.tsx`, `GameJsonRepairModal.tsx`, `GameMap.tsx`, `GameNarration.tsx`, `GameNodeMap.tsx`, `GameNpcTracker.tsx`, `GamePartyBar.tsx`, `GamePartySidebar.tsx`, `GameQteOverlay.tsx`, `GameReadableDisplay.tsx`, `GameSessionBanner.tsx`, `GameSessionHistory.tsx`, `GameSetupWizard.tsx`, `GameSkillCheckResult.tsx`, `GameStateIndicator.tsx`, `GameSurface.tsx`, `GameTransitionManager.tsx`, `GameTravelView.tsx`, `GameTutorial.tsx`, and `GameWidgetPanel.tsx` into `src/features/game/components`.
- Moved original game support files `hooks/use-game.ts`, `hooks/use-party-turn.ts`, `stores/game-mode.store.ts`, `stores/game-state.store.ts`, `stores/game-asset.store.ts`, and game-specific libs `asset-fuzzy-match.ts`, `game-asset-selection.ts`, `game-audio.ts`, `game-character-name-match.ts`, `game-full-body-pose.ts`, `game-segment-edits.ts`, `game-tag-parser.ts`, and `party-dialogue-parser.ts` into `src/features/game`.
- Added a thin `GameConversationView` adapter that wires selected `game` chats into the moved original `GameSurface`; it supplies existing chat/message query results and leaves backend-dependent actions routed to failing seams.
- Moved supporting original UI used directly by the game surface: `ActiveWorldInfoButton.tsx`, `ChatGallery.tsx`, `ChatGalleryDrawer.tsx`, `ChatRoleplayPanels.tsx`, `ImagePromptPanel.tsx`, `PinnedImageOverlay.tsx`, `SpriteOverlay.tsx`, `WeatherEffects.tsx`, `chat-area.types.ts`, `sprite-display-modes.ts`, and `sprite-placement.ts`.
- Moved original shared UI/helpers needed by the game tree: `EmojiPicker.tsx`, `GenerationParametersEditor.tsx`, `ImagePromptReviewModal.tsx`, `SpeechToTextButton.tsx`, `utils.ts`, `character-display.ts`, `chat-macros.ts`, `connection-filters.ts`, `dialogue-quotes.ts`, `draft-translation.ts`, `markdown.tsx`, `spotify-playback-events.ts`, `tts-audio-cache.ts`, `tts-dialogue.ts`, `tts-service.ts`, `ui.store.ts`, and `sidecar.store.ts`.
- Copied original `packages/shared/src` to `src/shared/legacy-shared` and aliased `@marinara-engine/shared` to it so moved UI retains its original type imports until Rust-generated DTO bindings replace it in Phase 3.
- Added explicit unavailable/inert seams for later backend or future frontend slices: generation (`useGenerate`), scene analysis (`useSceneAnalysis`), regex application, translation, TTS config, gallery persistence, and agent expression results. These do not fake success or persistence.
- Added original client dependencies required by moved UI: `@dnd-kit/core` and `zod`.
- Deferred real backend behavior to later Rust slices: turn orchestration, generation streaming, scene analysis, map/combat/checkpoint/journal/inventory mutation persistence, gallery file storage, asset generation, TTS/translation/provider calls, Spotify playback, and filesystem-backed game assets.

### Phase 2 Cleanup/Rework Slice

Status: Complete for the known simplified Phase 2 UI targets.

- Reworked Phase 2 Slice 9 first because it had the clearest simplified replacement and the original game UI tree could be moved as a coherent component graph.
- Reworked Phase 2 Slices 1, 5, 6, 7, and 8 by replacing simplified or reduced settings, chat/conversation, lorebook/preset panels and editors, connections panel, and roleplay surfaces with moved original UI code and matching original hook/store contracts where needed for compilation.
- Added only compile/deferred seams for later backend or future frontend ownership: generation, scene analysis, scene actions, autonomous messaging, agents, encounter, haptics, custom tools, translation, TTS, gallery/file storage, sidecar/model operations, exports/imports, and API calls remain unavailable or inert.
- Do not continue to agents/tools UI until this cleanup is reviewed.
