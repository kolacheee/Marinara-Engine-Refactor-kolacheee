# Promansis

## Current Work

- Message translation failures leave no visible error
  - Status: Fixed locally on `fix/bug-19-translation-errors`
  - Impact area: UI, shared/api, Rust capability error surface
  - Next step: Manual smoke in the desktop app with an invalid translation provider/API setup to confirm the toast copy and loading reset.
  - Blockers: None.

## Owned Bugs

## Message translation failures leave no visible error

- Status: Fixed locally on `fix/bug-19-translation-errors`
- Owner: Promansis
- Impact area: UI | shared/api | Rust capability
- Reported: Local backlog item 19
- Last updated: 2026-05-19
- Notes: `useTranslate.translate` now owns visible error reporting for rejected translation calls; message action callers explicitly consume the async request while keeping hide/show behavior unchanged. `pnpm typecheck` passes.
- Bug 16: First-launch tutorial does not trap keyboard focus.
  - Status: In progress
  - Last updated: 2026-05-19
  - Branch: `fix/bug-16-onboarding-focus-trap`
  - Worktree: `../Marinara-Engine-Refactor-bug16`
  - Next step: verify typecheck and tutorial keyboard focus behavior.

## Owned Bugs

## First-launch tutorial does not trap keyboard focus
- Bug 15: Mobile shell panels leave hidden content in the tab order.
  - Status: In progress
  - Last updated: 2026-05-19
  - Branch: `fix/bug-15-mobile-panel-focus`
  - Worktree: `../Marinara-Engine-Refactor-bug15`
  - Next step: verify typecheck and mobile keyboard focus behavior.

## Owned Bugs

## Mobile shell panels leave hidden content in the tab order

- Status: In progress
- Owner: Promansis
- Impact area: UI
### Stored generation replay metadata is not applied on replay/regenerate

- Status: In progress
- Owner: Promansis
- Impact area: Generation | prompts | agents | provider boundary
- Likely root cause: Regenerate requests never reapply stored `message.extra.generationReplay` before `startGeneration` assembles prompt and request state.
- Files likely to change: `src/features/generation/hooks/use-generate.ts`, possibly `src/engine/generation/generation-replay.ts` if request shaping needs a helper adjustment.
- Checks planned: `pnpm typecheck`

## Owned Bugs

## Starting a new game session drops carried inventory and player state

- Status: Done
- Owner: Promansis
- Impact area: UI | engine | storage
- Fixing game checkpoint access through the local-only bug branch workflow.

## Owned Bugs

## Game checkpoint manager is not reachable from the game surface

- Status: Done
- Owner: Promansis
- Impact area: UI | engine
- Reported: 2026-05-19
- Last updated: 2026-05-19

### Notes

- Failing behavior: `gameApi.startSession` creates the next session with only setup/map/NPC metadata, dropping durable inventory, widget state, time/weather, morale, notes, journal, and the stored `chat.gameState`.
- Owner: `src/features/game/api/game-api.ts`; dependent readers are `GameSurface`, `useSyncGameState`, world-state hydration, and game prompt assembly.
- Resolution: new sessions now carry durable game metadata and `chat.gameState` while leaving combat-only session state behind.

## Status Notes

- Bug 6 branch: `fix/game-session-carryover-state`.
- Failing behavior: `GameCheckpoints` and checkpoint hooks/API exist, but `GameSurface` has no visible entry point or restore refresh path.
- Owner: `src/features/game/components/GameSurface.tsx`; dependent restore path is `gameApi.loadCheckpoint`, chat detail/messages queries, and `useGameStateStore`.
- Resolution: the game surface now exposes the checkpoint manager on desktop and mobile, and refreshes chat/game state after a checkpoint restore.
- Stop generation does not cancel the provider stream command.
  - Status: In review
  - Next step: Ready for review on the focused bug-fix branch after TypeScript, Rust, and docs checks.
  - Blockers: None.

## Owned Bugs

## Love Toys Control agent results never reach the haptic integration

- Status: Done
- Owner: Promansis
- Impact area: UI | engine | shared/api | Rust capability
- Reported: 2026-05-19
- Last updated: 2026-05-19

### Steps

1. Launch the app with onboarding incomplete.
2. When the tutorial overlay appears, press `Tab` and `Shift+Tab`.
3. Continue or skip the tutorial.

### Expected

Keyboard focus stays inside the tutorial until it is skipped or completed, then focus returns to the previously focused shell control when possible.

### Actual

The tutorial appears as an overlay, but `Tab` can move focus to shell or window controls behind it.

### Notes

- Owner: `src/features/onboarding/components/OnboardingTutorial.tsx`.
- Branch: `fix/bug-16-onboarding-focus-trap`.
- Local-only worktree: `../Marinara-Engine-Refactor-bug16`.

## Status Notes

- Bug 16 keeps the fix in onboarding UI; no engine, shared API, storage, or Rust capability behavior is expected to change.
1. Open the app at mobile width.
2. Open the left chat sidebar, tracker drawer, or right settings/tools drawer.
3. Press `Tab` or `Shift+Tab`, then press `Escape`.

### Expected

Keyboard focus stays inside the active mobile panel, closed panels and underlying page controls are not reachable, and `Escape` dismisses the active panel.

### Actual

Closed sidebars and underlying page controls can remain tabbable while another drawer is open.

### Notes

- Owner: `src/app/shell/AppShell.tsx`.
- Branch: `fix/bug-15-mobile-panel-focus`.
- Local-only worktree: `../Marinara-Engine-Refactor-bug15`.

## Status Notes

- Bug 15 keeps the fix in the shell UI layer; no engine, shared API, storage, or Rust capability behavior is expected to change.
1. Enable or trigger the Love Toys Control agent during generation.
2. Observe a successful agent result with haptic commands.
3. Check whether the connected haptic device receives the command.

### Expected

Agent-emitted haptic commands should be sent through the native haptic integration.

### Actual

The agent result is recorded, but nothing reaches `integrationGateway.haptic.command`.

### Notes

- Likely owner: `src/features/generation/hooks/use-generate.ts`.
- Keep the fix at the agent-result bridge, not the agent executor.
- Fixed by dispatching successful `haptic_command` agent results to `integrationGateway.haptic.command`.
- Follow-up fixed multi-command patterns by serializing agent haptic commands above the native 200ms rate limit.
- Verification: `pnpm typecheck`; `cargo check --manifest-path src-tauri/Cargo.toml`.

## Haptic inflate actions are advertised but execute as vibrate or fail

- Status: Done
- Owner: Promansis
- Impact area: engine | shared/api | Rust capability
- Reported: 2026-05-19
- Last updated: 2026-05-19

### Steps

1. Configure a connected haptic device that supports inflation.
2. Send a haptic command with `action: "inflate"`.
3. Observe the native command result.

### Expected

Inflate-capable devices should receive an inflate-compatible native command.

### Actual

`inflate` is advertised in prompts and types, but the Rust command path normalizes it to a vibrate fallback or rejects it.

### Notes

- Likely owner: `src-tauri/src/commands/storage/integrations/haptic.rs`.
- The native layer should explicitly recognize `inflate` instead of hiding it behind a generic fallback.
- Current Buttplug dependency does not expose an `inflate` output command, so the fix removes `inflate` from the advertised TypeScript and prompt contract instead of faking support.
- Verification: `pnpm typecheck`; `cargo check --manifest-path src-tauri/Cargo.toml`.
### Stop generation does not cancel the provider stream command

- Status: In review
- Owner: Promansis
- Impact area: Generation | prompts | agents | provider boundary
- Reported: 2026-05-19
- Last updated: 2026-05-19

#### Notes

The local-only bug backlog lists this as bug 5. The frontend abort signal stopped the local async stream iterator, but `llm_stream_channel` had no request id or cancellation token, so Rust provider streaming could continue until completion or channel send failure.

The fix should add an explicit stream cancellation contract between `src/shared/api/llm-api.ts` and the Rust LLM command boundary without moving provider transport behavior out of `marinara_llm`.

`llmApi.stream` now assigns each stream a native cancellation id and calls `llm_stream_cancel` when the abort signal fires. The Rust command registers active stream ids in app state and uses `tokio::select!` to drop the provider stream future when cancellation is requested.
The local-only bug backlog lists this as bug 4. Character create and version restore already serialize card `data`, but generic `storage_update` patches could persist object-shaped `data` from the character editor, agent card updates, roleplay scene memories, chat schedules, and connected character commands.

Generic character update patches now normalize card `data` at the Rust storage command boundary before writing to storage, so all `storage_update` callers keep the persisted JSON-string contract.

## Status Notes

- Bug 11 branch: `fix/game-checkpoint-manager-surface`.
