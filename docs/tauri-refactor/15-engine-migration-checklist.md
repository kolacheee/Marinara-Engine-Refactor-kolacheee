# Engine Migration Checklist

This file tracks the full Marinara Engine migration into the Tauri refactor. Keep it updated after every migration pass.

Last updated: 2026-05-18.

## Status Legend

- `[ ]` Not started
- `[~]` In progress
- `[x]` Moved/wired
- `[d]` Deferred by scope

## Architecture Anchors

- [x] Confirmed top-level modes are `chat`, `roleplay`, and `game`.
- [x] Confirmed conversation/autonomous is a chat subsystem under `engine/modes/chat`.
- [x] Confirmed sidecar and sync-client are deferred external-service scope.
- [~] Create `src/engine` layers from `docs/tauri-refactor/14-layered-module-architecture.md`.
- [~] Replace server-stub frontend API with typed Tauri/native calls. StorageGateway and LLMGateway now use direct Tauri commands, but the generic `api_request` router remains for many feature hooks and must keep shrinking.
- [x] Build Rust capability commands for storage, assets/imports, LLM transport, and integrations.
- [x] Removed direct browser `fetch('/api/...')` calls for local Fastify routes; local app calls now go through Tauri API adapters or local TypeScript helpers.
- [x] Removed remaining active source `fetch()` calls; non-API blob loading no longer uses browser fetch.
- [x] Removed sidecar from active onboarding, agent, connection, and game scene UI; sidecar and sync remain deferred scope only.

## TypeScript Engine Layers

- [x] Layer 0: `src/engine/contracts` from `packages/shared/src`.
- [x] Layer 0: `src/engine/core` primitives.
- [x] Layer 1: `src/engine/capabilities` TypeScript ports.
- [x] Layer 2: `src/engine/shared` pure helpers.
- [ ] Layer 3: `src/engine/entities` pure entity operations.
- [x] Layer 4: `src/engine/repositories` capability-backed repositories.
- [~] Layer 5: `src/engine/generation-core` prompt, lorebook, regex, LLM DTO helpers.
- [~] Layer 6: `src/engine/agents-runtime` agent executor, pipeline, knowledge, and tools.
- [~] Layer 7: `src/engine/generation` generation orchestration and stream event DTOs.
- [~] Layer 8: `src/engine/modes/chat` chat core, autonomous, awareness, schedules, commands.
- [~] Layer 8: `src/engine/modes/roleplay` scene, sprites, encounter, visual-novel.
- [~] Layer 8: `src/engine/modes/game` turn, prompts, mechanics, state, world, assets.

## Rust Capability Layers

- [ ] `src-tauri/src/app.rs` capability graph.
- [x] `src-tauri/src/state.rs` shared state.
- [x] Tauri command registration lives in `src-tauri/src/lib.rs`; the one-line `commands/mod.rs` shim was removed.
- [x] `src-tauri/src/commands/storage.rs` command facade split into capability modules under `src-tauri/src/commands/storage/`.
- [x] Asset commands are routed through focused storage capability modules and `marinara-assets`; the deleted top-level asset shim is no longer active.
- [x] Import commands: character JSON/PNG/CharX, native `.marinara` packages, lorebooks, presets, personas, JSONL chat imports, native folder picking, ST bulk scan/run, current-format media restore, folder/group remapping, and current Marinara/ST heuristic handling are routed through native storage.
- [~] LLM provider transport commands are wired through typed Tauri commands, `commands/storage/llm.rs`, and `crates/llm`; `openai_chatgpt` and `claude_subscription` parity still needs a real native implementation before they can be restored to the active UI.
- [x] Integration commands are wired through focused native storage modules and `crates/integrations`.
- [x] Browser-era update/PWA commands were removed from the active Tauri app surface.
- [x] Removed unused `src-tauri/src/events/mod.rs`; active streaming uses typed Tauri channels.
- [x] `src-tauri/crates/core` paths, IDs, errors, timestamps.
- [x] `src-tauri/crates/security` path and outbound policy helpers.
- [x] `src-tauri/crates/storage` raw file storage and atomic writes.
- [x] `src-tauri/crates/assets` file/blob/media handling for managed game assets and backgrounds.
- [x] Import parsing and file helpers are active in the native storage/import modules.
- [x] `src-tauri/crates/llm` provider transport.
- [x] `src-tauri/crates/integrations` Spotify, TTS, translation, and haptic transport are wired through native commands.
- [x] Removed stale update-planning crate after the active settings update surface was deleted.
- [d] `src-tauri/crates/sidecar` deferred external-service scope.
- [d] `src-tauri/crates/sync-client` deferred external-service scope.
- [d] `src-tauri/crates/sync-protocol` deferred until sync returns.

## Original Source Coverage

- [x] `packages/shared/src/constants`
- [x] `packages/shared/src/schemas`
- [x] `packages/shared/src/types`
- [x] `packages/shared/src/utils`
- [~] `packages/server/src/services/prompt`
- [~] `packages/server/src/services/lorebook`
- [x] `packages/server/src/services/regex`
- [~] `packages/server/src/services/agents`
- [~] `packages/server/src/services/tools`
- [~] `packages/server/src/routes/generate`
- [~] `packages/server/src/services/conversation`
- [~] `packages/server/src/services/game`
- [~] `packages/server/src/services/llm`
- [~] `packages/server/src/services/image`
- [~] `packages/server/src/services/spotify`
- [~] `packages/server/src/services/import`
- [~] `packages/server/src/services/storage`
- [ ] `packages/server/src/db`
- [ ] `packages/server/src/routes`
- [ ] `packages/server/src/utils`
- [ ] `packages/server/src/middleware`
- [d] `packages/server/src/services/sidecar`
- [~] `packages/client/src/components`
- [~] `packages/client/src/hooks`
- [~] `packages/client/src/lib`
- [~] `packages/client/src/stores`
- [~] `packages/client/src/styles`

## 2026-05-17 Migration Pass

- [x] Replaced remaining direct local Fastify browser fetches with Tauri-backed API helpers or local asset URL resolution.
- [x] Added managed local file URL handling for backgrounds and game assets through Tauri asset protocol paths.
- [x] Wired character, persona, preset, lorebook, connection, chat, gallery, profile export/import, import, knowledge-source, bot-browser, GIF, and prompt-review utility frontend paths away from deferred throwing API shims.
- [x] Added Tauri-backed chat gallery and character gallery upload/list/delete storage.
- [x] Added Tauri-backed default Pollinations image test and avatar generation route, plus NPC avatar upload persistence.
- [x] Hid active sidecar model controls from onboarding, agents, connections, and game scene setup/runtime surfaces.
- [x] Added local new-app profile/import routes with ZIP download payloads, profile JSON restore, and current-format exports; old backup/archive compatibility is not present in runtime code.
- [~] Added Chub, Janny, CharacterTavern, Pygmalion, Wyvern, DataCat, and Chartavern bot-browser routes behind Rust transport; authenticated-session behavior now validates stored credentials for Pygmalion and Chartavern, but exact upstream parity still needs live-provider QA.

## 2026-05-18 Migration Pass

- [x] Split the previous monolithic native storage route shim into focused modules (`router`, `shared`, `chats`, `imports`, `bulk_imports`, `llm`, assets, integrations, and related capability slices). Product-owned game/encounter/conversation orchestration has been moved out of Rust routes.
- [x] Added visible error toasts to create connection, character, persona, and preset modals so failed native calls no longer look like dead sidebar buttons.
- [x] Reworked import routes for the sidebar/settings import flows: ST character inspect/batch now returns the frontend shape, PNG character-card metadata is parsed from `chara`/`ccv3` chunks, CharX reads `card.json` and embedded icons, and native `.marinara` packages read `data.json` plus avatar assets.
- [x] Added native JSONL chat import and branch import routes used by the chat/settings import UI.
- [x] Added local SillyTavern folder browsing response shape plus basic ST bulk scan/run for characters, chats, group chats, presets, lorebooks, backgrounds, and personas.
- [x] Replaced game/encounter route placeholders with local Tauri mechanics for dice, skill checks, time/weather, random encounters, combat rounds, loot, journal entries, checkpoints, party cards, map movement, encounter init/action/summary, LLM-backed setup/session carryover/conclusion, and generated game image/audio orchestration.
- [x] Replaced chat/agent no-op route responses for autonomous unread state, LLM-backed summaries/consolidation, memory chunks, agent memory, echo-message cleanup, retry bookkeeping, cadence status, generated schedules, autonomous timing, and conversation schedule/status checks with migrated TypeScript orchestration over Tauri storage/LLM capabilities.
- [x] Normalized lorebook imports into lorebook rows plus `lorebook-entries`, including current-format category/tag handling and new-app media/profile packaging.
- [x] Replaced haptic placeholders with the open-source Rust `buttplug` client package, Intiface websocket connect/disconnect, scan, device listing, output commands, auto-stop, and stop-all routes.
- [~] Replaced TTS placeholders with native provider transport for OpenAI-compatible, ElevenLabs, NanoGPT ElevenLabs, and PocketTTS voice/audio routes; browser playback remains frontend-owned.
- [~] Replaced Spotify placeholders with OAuth, token refresh, status, access-token, player, devices, playlists, playback controls, DJ playlist, and game scene candidate/play routes over native Spotify Web API transport.
- [x] Added native font listing/file serving/folder opening and Google font download routes; App startup injects local font faces through Tauri asset URLs.
- [x] Added native translation route for Google, DeepL, DeepLX, and AI-backed translation.
- [x] Added lorebook vectorization route using configured embedding connections and persisted entry embeddings.
- [x] Replaced bulk character/persona/preset/lorebook export JSON responses with ZIP binary download payloads, and replaced character PNG export placeholder with an embedded `chara` PNG card.
- [x] Replaced avatar upload echo routes with local avatar persistence for characters, personas, and NPC portraits while keeping frontend-compatible avatar paths.
- [x] Replaced persona activation, prompt default, chat-preset active, regex reorder, game-assets rescan/open-folder/folder-description, scoped admin expunge, and character-version restore placeholders with storage-backed behavior.
- [x] Replaced game setup, session lorebook regeneration, session recap/carryover/conclusion, and campaign progression stubs with LLM-backed native flows; malformed JSON now raises a repair request that applies through TypeScript game APIs instead of missing server endpoints.
- [x] Removed active `/game`, `/encounter`, `/conversation`, `/agents/retry`, and `/sidecar` frontend product-route calls; normal mode workflows now use TypeScript mode APIs over storage/assets/LLM/integration capabilities.
- [x] Split shared chat/message repository hooks, common chat components, and chat UI types into neutral `src/features/chats`, so conversation, roleplay, and game mode code no longer depend on `features/conversation`.
- [x] Wired custom static/webhook tools into the migrated agent runtime through native `/custom-tools/execute`; script execution remains explicitly disabled in the native runtime.
- [x] Expanded native image-provider parity: OpenAI/GPT image edits, NanoGPT, Together, Stability v1/v2, Horde, ComfyUI reference uploads, RunPod Serverless ComfyUI, Draw Things/A1111 img2img, NovelAI raw references/ZIP/image responses, OpenRouter/Gemini chat image parsing, xAI aspect-ratio payloads, transparent-background options, and selected image connection use for sprite generation are wired.
- [x] Removed active settings copy that presented theme/extension data as server-synced runtime behavior; sync remains deferred scope.
- [x] Removed runtime legacy asset URL compatibility and added the no-legacy-runtime rule to `AGENTS.md`; old data conversion will be a separate migration script.
- [x] Removed browser-era custom theme/extension migration hooks, stale settings-sync/CSRF files, and sessionStorage draft migration fallbacks.
- [x] Replaced the historical UI-store `migrate` chain with a fresh Tauri persistence key, and split `ui.store.ts` into focused store, model/helper, and persistence modules.
- [x] Deleted sidecar-only frontend store/modal/contracts from the active TypeScript app and moved active scene-analysis contracts into neutral scene types.
- [x] Moved bot-browser provider parity behind Rust transport for Janny token refresh, CharacterTavern cookies/filters, Pygmalion token/session cleanup, Wyvern creator routes, DataCat session minting, and provider asset validation.
- [x] Rebuilt current-format imports/exports for Marinara character/persona/lorebook/preset envelopes, SillyTavern-compatible exports, embedded character media, sprite/gallery restore, prompt group/section remapping, and embedded lorebook pointer cleanup.
- [x] Replaced prompt-review fallback assembly with full preset/section/group/choice-block orchestration over migrated storage and LLM streaming.
- [x] Removed duplicate API re-export shims, deleted conversation-mode re-export wrappers, removed active sidecar/sync runtime branches, removed active legacy runtime compatibility paths, and kept old-data conversion out of normal app code.

## 2026-05-18 Follow-up Parity Pass

- [x] Re-ran old-vs-refactor source audits after the previous migration pass and patched concrete misses instead of documenting them only.
- [x] Fixed game setup persistence so the full `GameSetupConfig`, party character IDs, persona, lorebooks, and selected connections survive `createGame` and `setupGame`.
- [x] Made game start idempotent by detecting existing active GM turns before generating the first turn again.
- [x] Replaced missing game JSON repair endpoints with local TypeScript apply handling for setup, session conclusion, session lorebook, and campaign progression JSON.
- [x] Added game morale and element preset API methods and passed the selected element preset into combat resolution.
- [x] Restored original sprite cleanup route names (`cleanup-saved`, `cleanup-restore`) while keeping fresh-app restore point data, and added cleanup status plus exact sprite file lookup routes.
- [x] Persisted manual translations back to message `extra.translation`, and clear persisted translations when the user hides them.
- [x] Tightened Memory Recall import to the new `marinara_memory_recall`/`data.chunks` schema only; old `memories` fallback imports were removed from runtime code.
- [x] Expanded fresh-app profile export/import to include current collections and managed asset directories, with no old backup/archive/profile compatibility fallback.
- [x] Removed browser XHR binary loading; URL binary reads now go through a native Tauri command.
- [x] Moved engine storage and LLM gateways off the generic local route string and onto typed Tauri commands (`storage_*`, `llm_*`).
- [x] Removed empty/dead crate placeholders, including deferred sidecar/sync placeholders.

## Remaining Migration Gaps

- [~] Generic local API router cleanup: `api_request`, `api_stream_events`, `api_stream_channel`, `shared/api/api-client.ts`, and many feature hooks still use method/path strings. StorageGateway and LLMGateway are direct now; assets, integrations, import/export helpers, and feature CRUD hooks still need typed command/client replacements.
- [~] LLM provider parity: `openai_chatgpt` and `claude_subscription` are still not active because they need native Tauri implementations rather than the old Node SDK/server path.
- [~] Prompt/command workflow parity: prompt reviewer and preset preview are migrated, but embedded command execution still needs a full current-app pass for the old command families that are still exposed in settings.
- [~] Conversation autonomy parity: core local orchestration exists, but schedule inheritance, scene-busy filtering, talkativeness fallback, and follow-up limit parity still need deeper source-level migration.
- [~] Spotify DJ playlist parity: native Spotify transport exists, but exact DJ Mari playlist construction/playback behavior still needs original-vs-refactor parity work.
- [~] Bot browser and paid provider parity: native routes exist, but auth/session recovery for non-Chub providers still needs live-provider verification and fixes as upstreams change.
- [~] Large-file cleanup: `imports.rs`, `bot_browser.rs`, and `sprites.rs` are split out of `storage.rs` but are still large enough to merit further provider/workflow submodule cleanup.

## Remaining External QA

- [ ] Run live-account QA for upstream bot-browser providers that require credentials or are subject to Cloudflare/session policy changes.
- [ ] Run live-provider QA across paid image providers, TTS providers, Spotify accounts/devices, and physical Buttplug/Intiface hardware.
- [ ] Run full manual Tauri UI smoke testing for imports/exports, sprite cleanup/restore, long-running game sessions, and autonomous scheduling with real user data after the remaining source-level migration gaps above are closed.

## Verification

- [x] `pnpm typecheck` passed on 2026-05-17.
- [x] `cargo check --manifest-path src-tauri/Cargo.toml` passed on 2026-05-17.
- [x] `pnpm check:docs` passed on 2026-05-17.
- [x] `pnpm build` passed on 2026-05-17 with Vite large-chunk warnings only.
- [x] `cargo check --manifest-path src-tauri/Cargo.toml` passed on 2026-05-18 after storage/import split.
- [x] `pnpm typecheck` passed on 2026-05-18.
- [x] `pnpm build` passed on 2026-05-18 with Vite large-chunk warnings only.
- [x] `pnpm check:docs` passed on 2026-05-18.
- [x] `cargo check --manifest-path src-tauri/Cargo.toml` passed on 2026-05-18 after haptic, export, avatar, vectorization, game-assets, admin, and game workflow patches.
- [x] `pnpm typecheck` passed on 2026-05-18 after deferred sidecar UI cleanup.
- [x] `cargo check --manifest-path src-tauri/Cargo.toml` passed on 2026-05-18 after image-provider parity expansion.
- [x] `pnpm typecheck` passed on 2026-05-18 after conversation-data split.
- [x] `pnpm build` passed on 2026-05-18 after conversation-data split and active sync-copy cleanup.
- [x] `pnpm typecheck` passed on 2026-05-18 after no-legacy-runtime/UI-store cleanup.
- [x] `cargo check --manifest-path src-tauri/Cargo.toml` passed on 2026-05-18 after no-legacy-runtime/UI-store cleanup.
- [x] `pnpm check:docs` passed on 2026-05-18 after no-legacy-runtime/UI-store cleanup.
- [x] `pnpm build` passed on 2026-05-18 after no-legacy-runtime/UI-store cleanup, with Vite large-chunk warnings only.
- [x] `pnpm typecheck` passed on 2026-05-18 after full source migration parity and mode-separation cleanup.
- [x] `cargo check --manifest-path src-tauri/Cargo.toml` passed on 2026-05-18 after full source migration parity and mode-separation cleanup.
- [x] `pnpm check:docs` passed on 2026-05-18 after full source migration parity checklist update.
- [x] `pnpm build` passed on 2026-05-18 after full source migration parity and mode-separation cleanup, with Vite large-chunk warnings only.
- [x] `pnpm typecheck` passed on 2026-05-18 after game JSON repair, translation persistence, typed storage/LLM command, and sprite cleanup-route fixes.
- [x] `cargo check --manifest-path src-tauri/Cargo.toml` passed on 2026-05-18 after typed storage/LLM command additions.
