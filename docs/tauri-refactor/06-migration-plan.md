# Migration Plan

Each phase is a review checkpoint, not a release milestone. Keep slices small, update source inventory status, and stop for human review after every slice.

## Global Rules

- No temporary functionality, fake commands, mock persistence, or compatibility shims.
- Preserve existing React UI/UX; move and lightly reorganize.
- Use raw file storage only for local desktop data.
- Keep Rust minimal and explicit.
- Prefer complete backend vertical slices when feasible.
- Sync is the final major module.

## Phase 0: Baseline Structure

Status: Complete.

1. Add `src/app`, `src/shared`, and `src/features`.
2. Move only starter app bootstrap into `src/app`.
3. Add Rust workspace crate folders with concise names.
4. Add TypeScript binding generation setup.
5. Add basic check commands for Rust, frontend typecheck, and docs.

Exit criteria:

- Project remains buildable.
- Starter app still opens.
- No old Marinara UI is copied yet.

## Phase 1: Frontend Shell And Shared Foundations

Status: Complete.

1. Move app shell: bootstrap, providers, top-level layout, navigation surfaces, modal/drawer roots.
2. Move global styles and theme loading.
3. Move shared UI primitives and backend-free utilities as needed by the shell.
4. Update frontend inventory status.

Exit criteria:

- Shell renders.
- No feature screens are bulk-copied.
- No temporary backend behavior was added.

## Phase 2: Small Frontend Feature Slices

Move copied UI in this order, one small reviewed surface at a time:

1. Settings shell and settings sections. Complete.
2. Theme/preferences UI. Complete.
3. Character/persona library read surfaces. Complete.
4. Chat shell/navigation without generation.
5. Chat message display/input UI.
6. Lorebooks/prompts/presets editors.
7. Connections/provider settings UI.
8. Roleplay/conversation UI.
9. Game UI.
10. Agents/tools UI.
11. Imports/assets/gallery/media UI.
12. Sidecar/integrations/update UI.

### Phase 2 Rework Checkpoint

Status: Complete for the known simplified Phase 2 UI targets. Settings, chat/conversation, lorebook/preset panels and editors, connections, roleplay, and game surfaces have been reworked from simplified or reduced replacements into moved original UI trees with backend-dependent behavior routed to unavailable seams.

The first pass through Phase 2 drifted from the project rule to "move and lightly reorganize the existing React UI." Several slices introduced simplified replacement surfaces instead of moving the original component files and preserving their UI structure with backend-dependent behavior explicitly deferred.

The next slice must be a cleanup/rework slice, not a new feature slice. Its goal is to bring Phase 2 back in line with the migration strategy:

1. Audit every Phase 1 and Phase 2 inventory claim against `E:/Personal Projects/Marinara-Engine/packages/client/src`.
2. For each claimed moved/mapped file, classify it as:
   - faithful move/light reorganization
   - partial move with acceptable backend deferral
   - simplified rewrite that must be replaced by moved original UI
   - intentionally deferred source
   - explicitly removed source, only if approved by the human
3. Replace simplified rewrites with moved/reorganized original UI wherever the original UI is in scope for the completed slice.
4. Keep backend behavior non-functional only through explicit unavailable hooks, unavailable command seams, disabled controls, or clear error states.
5. Do not create fake data, mock persistence, fake command success, preview-only routes, or compatibility shims.
6. Do not redesign the UI or substitute compact replacement components unless the human explicitly approves that specific deviation.
7. Update `00-source-inventory.md` with accurate status for every touched original source file.
8. Leave future-slice UI files deferred by name and reason, rather than implying they were moved.

Known rework targets from the audit:

- Phase 2 Slice 1 settings was reopened and replaced with moved original `components/panels/SettingsPanel.tsx`, original theme hook contracts, and original TTS config UI support where the moved panels require it.
- Phase 2 Slice 4/5 chat surfaces were reopened and replaced with moved original `components/chat` UI and original chat hook/store contracts where needed for compilation.
- Phase 2 Slice 6 panels and editors were reopened and replaced with moved original `components/panels/LorebooksPanel.tsx`, `components/panels/PresetsPanel.tsx`, `components/lorebooks`, and `components/presets` trees plus original lorebook/preset hook contracts.
- Phase 2 Slice 7 connections was reopened and replaced with moved original `components/panels/ConnectionsPanel.tsx` plus original connection-folder hook contracts and panel support UI.
- Phase 2 Slice 8 roleplay/conversation UI was reopened and replaced with moved original `ChatRoleplaySurface`, roleplay HUD/panels, scene, sprite, encounter, and related chat support UI.
- Phase 2 Slice 9 Game UI was reopened and replaced with the moved original `components/game` tree plus required support files. Remaining work is backend wiring and later-slice behavior, not a simplified UI replacement.
- Phase 1/2 shared utility seams that intentionally became unavailable placeholders should remain placeholders only when the original behavior belongs to Rust or a later backend/file slice.

Exit criteria for the rework checkpoint:

- The docs accurately distinguish moved UI from deferred UI.
- Completed Phase 2 slices contain moved original UI for their stated scope.
- Any simplified replacements are removed or clearly marked as temporary mistakes to replace before the slice can be complete.
- `pnpm check` passes.
- The handoff includes an explicit list of original files moved, still deferred, and intentionally not moved.

Exit criteria for each slice:

- Existing UI flow is preserved where practical.
- Source inventory is updated.
- Assets are moved only when the slice uses them.
- Backend-dependent actions are not hidden behind fake behavior.

## Phase 3: Domain DTOs And Tauri API Shell

1. Put shared primitives such as IDs, pagination, timestamps, and app errors in `core`.
2. Add domain-owned DTOs beside the Rust domain behavior that owns them.
3. Generate TypeScript bindings from Rust DTOs.
4. Create final-shape `src/shared/api` wrappers only for real commands.

Exit criteria:

- Frontend imports generated bindings where available.
- No central contracts crate exists.
- No duplicate long-term TypeScript DTOs are hand-maintained.

## Phase 4: Storage Foundation

1. Implement `core` config and app data paths.
2. Implement raw file-backed `storage`.
3. Preserve `storage/manifest.json` and `storage/tables/*.json` compatibility where practical.
4. Implement atomic JSON writes.
5. Implement table manifest and per-table snapshots.
6. Implement first repositories: app settings, chats, messages, characters, connections.
7. Implement backup/export/import skeleton.

Exit criteria:

- Basic CRUD persists to raw files.
- Existing file-native snapshots remain readable where practical.
- No SQLite, database, or legacy SQLite importer exists.
- Atomic write and repository tests exist.

## Phase 5: Secrets And Security

1. Implement Rust-owned secret storage.
2. Keep non-secret connection metadata in file storage.
3. Return redacted saved-secret status to the frontend.
4. Implement path safety, filename safety, content-type checks, outbound URL validation, and safe fetch.
5. Preserve existing key-entry and connection-management UI flows.

Exit criteria:

- API keys and OAuth tokens are not stored in frontend state or normal file snapshots.
- Saved raw secrets are not returned to React.
- Path traversal and outbound URL tests exist.

## Phase 6: Core Entity Backend Slices

Implement as vertical slices where feasible:

1. Chats, messages, folders, swipes, branches.
2. Characters and personas.
3. Lorebooks.
4. Prompts, presets, prompt overrides.
5. Regex scripts and custom tool metadata.

Each slice should include storage, DTOs, commands, services, frontend hook adaptation, source inventory updates, and tests.

Exit criteria:

- Existing library/editor screens work through real Tauri commands.
- No old `/api` fetch remains for migrated domains.

## Phase 7: Providers And Generation

1. Implement `llm` provider registry.
2. Implement provider calls, model discovery, and connection tests in Rust.
3. Move image generation provider calls to Rust.
4. Implement prompt builder, context builder, lorebook injection, regex pipeline, retry, dry run, and prompt preview.
5. Implement generation streaming with Tauri events and cancellation run IDs.

Exit criteria:

- React does not perform provider/authenticated network calls directly.
- Generation streams through Tauri events, not HTTP/SSE.
- Prompt preview, cancellation, retry, and dry run work.

## Phase 8: Agents, Tools, And Permissions

1. Port agent config storage.
2. Implement tool permission model.
3. Port agent executor and pipeline.
4. Port knowledge routing/retrieval and memory.
5. Port custom tool execution with permissions in the same slice.
6. Port agent debug events.

Exit criteria:

- Tool execution is not ported without permission checks.
- Agent runs and memory persist.
- Debug output is evented and reviewable.

## Phase 9: Conversation And Roleplay

1. Port autonomous messaging, schedules, awareness, summaries.
2. Port scene analysis/postprocess.
3. Port sprites, sprite placement, sprite uploads.
4. Port encounters and visual-novel choices.

Exit criteria:

- Existing conversation/roleplay behavior is preserved.
- File and sprite handling is Rust-owned where it touches filesystem or validation.

## Phase 10: Game Mode

1. Port pure mechanics first: dice, skill checks, time, weather.
2. Port combat, loot, morale, reputation, perception, elements.
3. Port snapshots, checkpoints, map, travel, journal, inventory.
4. Port GM/party prompt builders and turn orchestrator.
5. Port game asset generation.

Exit criteria:

- Game mode can start, advance turns, checkpoint, restore, and generate assets.
- Large game modules are split by ownership.

## Phase 11: Assets, Media, Imports, Backups, Updates

1. Port avatars, backgrounds, default backgrounds, gallery, fonts, generated image storage, and thumbnails if needed.
2. Port SillyTavern scanners/importers.
3. Port character card PNG import/export.
4. Port profile backup export/import.
5. Port update check/apply flow.
6. Add progress/cancel events for long-running jobs.

Exit criteria:

- Existing upload, preview, crop, picker, drag/drop, review, and progress UI remains recognizable.
- Rust owns filesystem access, validation, parsing, blob placement, and persistence.
- Update apply is permission-gated.

## Phase 12: Sidecar

1. Inventory current sidecar runtime/package requirements from source and scripts.
2. Evaluate existing runtimes/packages such as Crane, llama.cpp, and MLX.
3. Rewrite sidecar internals around the chosen runtime/package where practical.
4. Port model catalog, downloads, runtime install, process manager, logs, health checks, local inference provider, and scene analysis.

Exit criteria:

- Current sidecar UI flow remains recognizable.
- Runtime choice is documented with packaging, platform, model coverage, binary size, and maintenance tradeoffs.
- Old custom sidecar internals are not ported line-for-line where a package covers the behavior.

## Phase 13: Integrations

Implement under `integrations`:

1. Spotify OAuth/playback/tools.
2. Haptic device control.
3. TTS providers/cache.
4. Translation providers.
5. GIF search.
6. Bot-browser providers and authenticated sessions.
7. Discord webhook.
8. Home Assistant bridge if retained.

Exit criteria:

- Existing integration UI flows remain recognizable.
- Tokens, cookies, provider auth, and authenticated calls are Rust-owned.

## Phase 14: Hardening Before Sync

1. Remove unused HTTP compatibility code.
2. Remove sample Tauri code.
3. Enable strict CSP.
4. Audit Tauri capabilities.
5. Add integration tests for commands.
6. Add frontend smoke tests for primary flows.
7. Document release packaging.

Exit criteria:

- No feature depends on old `/api` routes.
- No direct secrets are in frontend storage.
- Capabilities and command permissions are reviewed.

## Phase 15: Optional Sync Server

Sync is last. Earlier phases should only preserve stable IDs, timestamps, manifest versions, and clear blob references.

1. Add `sync-protocol`.
2. Add Tauri-side `sync-client`.
3. Add sync settings/status UI.
4. Implement single-user sync server with raw metadata files and local filesystem blobs.
5. Add metadata push/pull for core domains.
6. Add blob upload/download.
7. Add WebSocket heads broadcast.
8. Add conflict review UI.
9. Add PostgreSQL and S3/MinIO production mode.
10. Add OpenAPI docs and deployment guide.

Exit criteria:

- Desktop app remains usable without sync.
- Sync can be enabled per install.
- Conflicts are not silently overwritten.
