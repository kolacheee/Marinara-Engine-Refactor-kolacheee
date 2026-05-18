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
- [x] Replace server-stub frontend API with Tauri-backed API adapter.
- [~] Build Rust capability commands for storage, assets/imports, LLM transport, integrations, and updates.
- [x] Removed direct browser `fetch('/api/...')` calls for local Fastify routes; local app calls now go through Tauri API adapters or local TypeScript helpers.
- [x] Removed sidecar from active onboarding, agent, connection, and game scene UI while preserving deferred sidecar code for later reintroduction.

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
- [ ] Layer 8: `src/engine/modes/roleplay` scene, sprites, encounter, visual-novel.
- [~] Layer 8: `src/engine/modes/game` turn, prompts, mechanics, state, world, assets.

## Rust Capability Layers

- [ ] `src-tauri/src/app.rs` capability graph.
- [x] `src-tauri/src/state.rs` shared state.
- [x] `src-tauri/src/commands/mod.rs` command registration.
- [x] `src-tauri/src/commands/storage.rs` command facade split into capability modules under `src-tauri/src/commands/storage/`.
- [~] `src-tauri/src/commands/assets.rs` asset commands; currently routed through the storage command shim and `marinara-assets`.
- [~] Import commands: character JSON/PNG/CharX, native `.marinara` packages, lorebooks, presets, personas, JSONL chat imports, native folder picking, and basic ST bulk scan/run are routed through native storage; remaining work is exact importer heuristic/media/archive fidelity.
- [ ] `src-tauri/src/commands/llm.rs` provider transport commands.
- [ ] `src-tauri/src/commands/integrations.rs` integration commands.
- [ ] `src-tauri/src/commands/updates.rs` update commands.
- [ ] `src-tauri/src/events/mod.rs` event names/helpers.
- [x] `src-tauri/crates/core` paths, IDs, errors, timestamps.
- [x] `src-tauri/crates/security` path and outbound policy helpers.
- [x] `src-tauri/crates/storage` raw file storage and atomic writes.
- [x] `src-tauri/crates/assets` file/blob/media handling for managed game assets and backgrounds.
- [~] `src-tauri/crates/import` import file helpers.
- [x] `src-tauri/crates/llm` provider transport.
- [~] `src-tauri/crates/integrations` Spotify, TTS, translation, and haptic transport are wired through native commands; webhook/tool edge cases still need parity review.
- [x] `src-tauri/crates/updates` update check/apply planning.
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
- [ ] `packages/server/src/services/tools`
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
- [x] Wired character, persona, preset, lorebook, connection, chat, gallery, backup, import, knowledge-source, bot-browser, GIF, and prompt-review utility frontend paths away from deferred throwing API shims.
- [x] Added Tauri-backed chat gallery and character gallery upload/list/delete storage.
- [x] Added Tauri-backed default Pollinations image test and avatar generation route, plus NPC avatar upload persistence.
- [x] Hid active sidecar model controls from onboarding, agents, connections, and game scene setup/runtime surfaces.
- [~] Added local backup/import/profile routes with ZIP download payloads, profile JSON restore, and compatible exports; remaining work is exact original media/archive timestamp fidelity.
- [~] Added Chub, Janny, CharacterTavern, Pygmalion, Wyvern, DataCat, and Chartavern bot-browser routes behind Rust transport; authenticated-session behavior now validates stored credentials for Pygmalion and Chartavern, but exact upstream parity still needs live-provider QA.

## 2026-05-18 Migration Pass

- [x] Split the previous monolithic native storage route shim into focused modules (`router`, `shared`, `chats`, `imports`, `bulk_imports`, `llm`, `scene`, `game`, assets, integrations, and related capability slices).
- [x] Added visible error toasts to create connection, character, persona, and preset modals so failed native calls no longer look like dead sidebar buttons.
- [x] Reworked import routes for the sidebar/settings import flows: ST character inspect/batch now returns the frontend shape, PNG character-card metadata is parsed from `chara`/`ccv3` chunks, CharX reads `card.json` and embedded icons, and native `.marinara` packages read `data.json` plus avatar assets.
- [x] Added native JSONL chat import and branch import routes used by the chat/settings import UI.
- [x] Added local SillyTavern folder browsing response shape plus basic ST bulk scan/run for characters, chats, group chats, presets, lorebooks, backgrounds, and personas.
- [~] Replaced game/encounter route placeholders with local Tauri mechanics for dice, skill checks, time/weather, random encounters, combat rounds, loot, journal entries, checkpoints, party cards, map movement, and encounter init/action/summary fallbacks. LLM-authored GM prose and generated image/audio assets still depend on broader provider/orchestration migration.
- [~] Replaced chat/agent no-op route responses for autonomous unread state, local summaries, memory chunks, agent memory, echo-message cleanup, retry bookkeeping, cadence status, and conversation schedule/status checks with durable Tauri storage-backed behavior. LLM-backed summary/schedule parity remains pending in the broader generation/provider migration.
- [~] Normalized lorebook imports into lorebook rows plus `lorebook-entries`; deeper original importer parity, timestamp fidelity, category/tag heuristics, media bundling, and profile archive restore still need follow-up.
- [x] Replaced haptic placeholders with the open-source Rust `buttplug` client package, Intiface websocket connect/disconnect, scan, device listing, output commands, auto-stop, and stop-all routes.
- [~] Replaced TTS placeholders with native provider transport for OpenAI-compatible, ElevenLabs, NanoGPT ElevenLabs, and PocketTTS voice/audio routes; browser playback remains frontend-owned.
- [~] Replaced Spotify placeholders with OAuth, token refresh, status, access-token, player, devices, playlists, playback controls, DJ playlist, and game scene candidate/play routes over native Spotify Web API transport.
- [x] Added native font listing/file serving/folder opening and Google font download routes; App startup injects local font faces through Tauri asset URLs.
- [x] Added native translation route for Google, DeepL, DeepLX, and AI-backed translation.
- [x] Added lorebook vectorization route using configured embedding connections and persisted entry embeddings.
- [x] Replaced bulk character/persona/preset/lorebook export JSON responses with ZIP binary download payloads, and replaced character PNG export placeholder with an embedded `chara` PNG card.
- [x] Replaced avatar upload echo routes with local avatar persistence for characters, personas, and NPC portraits while keeping frontend-compatible avatar paths.
- [x] Replaced persona activation, prompt default, chat-preset active, regex reorder, game-assets rescan/open-folder/folder-description, scoped admin expunge, and character-version restore placeholders with storage-backed behavior.
- [~] Replaced game setup, session lorebook regeneration, and campaign progression stubs with LLM-backed native flows plus deterministic local fallbacks.

## Current Blockers Before Migration Can Be Called Complete

- [ ] Finish full image-generation provider parity beyond the default Pollinations path: OpenAI-compatible image APIs, NovelAI, Stability, Horde, ComfyUI, Automatic1111, RunPod ComfyUI, Draw Things, and NanoGPT.
- [~] Finish TTS edge parity for provider-specific voice metadata, streaming/playback timing, and live-device QA.
- [~] Finish Spotify and haptic live-provider/device QA. Native Spotify Web API transport and Rust Buttplug/Intiface transport are wired, but hardware/account-dependent paths still need manual verification.
- [~] Finish sprite sheet generation, cleanup, and restore parity; native routes now do file-backed sprite upload/list/delete, Pollinations-backed sprite generation, image crate sheet slicing, built-in white-matte cleanup, and cleanup backup/restore, but provider-specific image generation parity still remains.
- [~] Finish bot-browser parity for all non-Chub source edge cases, upstream schema drift, and authenticated source session recovery.
- [~] Finish import/export parity for exact SillyTavern/Marinara media bundling, timestamp fidelity, and original importer heuristics.
- [~] Finish game route parity for deeper mechanics and long-running campaign state behaviors beyond the native setup/lorebook/progression pass.
- [~] Finish prompt reviewer, character maker, persona maker, lorebook maker, and generation-agent workflows so they use migrated orchestration rather than minimal deterministic fallbacks. Character/persona/lorebook maker now call the migrated Rust LLM provider path; prompt reviewer and generation-agent parity still need follow-up.
- [ ] Finish LLM-backed conversation summaries, automatic daily/weekly consolidation, generated schedules, and autonomous timing parity on top of the local storage-backed chat/agent routes.

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
