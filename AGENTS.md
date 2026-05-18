# Agent Instructions

The Marinara Engine Tauri migration is not complete until the entire application works locally in Tauri, except for sidecar and sync. Sidecar and sync are the only deferred scopes.

Original source reference:

- The original Marinara Engine repo is at `E:\Personal Projects\Marinara-Engine`.
- During migration, constantly compare against that original repo to verify behavior, files, routes, services, types, hooks, stores, and UI workflows have been ported correctly.
- Treat the original repo as the source of truth for what must exist in the Tauri refactor, except for sidecar and sync, which should be removed from the active app surface.
- Use new rust packages where you see fit, even for things like LLM generation, if you think we can support MANY LLM providers easily with a package do so, find other places
- Update docs to keep track of the FULL MIGRATION

Important migration rule:

- Move or rebuild every non-sidecar, non-sync backend feature into the new layered Tauri architecture so the UI works end to end.
- Do not leave placeholder routes, fake-success stubs, or "not configured yet" responses for normal app features such as chat, roleplay, game mode, agents, generation, LLM providers, storage, imports, assets, integrations, lorebooks, presets, characters, personas, themes, TTS, translation, backgrounds, avatars, and game mechanics.
- Remove sidecar and sync from the active app surface instead of trying to keep their old server endpoints alive. Any UI, stores, docs, routes, or capability modules that only exist for sidecar or sync should be deleted or cleanly hidden until those services are intentionally reintroduced.
- Do not add runtime compatibility for legacy installs, old server-shaped `/api/...` asset URLs, old profile backups, or old backup/archive formats. This refactor is a fresh Tauri application; any old-data conversion will be handled later by an explicit migration script, not by broad compatibility branches in normal app code.
- Keep the architecture layered as described in `docs/tauri-refactor/13-typescript-rust-organization-plan.md` and `docs/tauri-refactor/14-layered-module-architecture.md`: TypeScript owns expressive engine logic; Rust owns local capabilities, storage, security, provider transport, filesystem assets, and Tauri commands.
- Keep `docs/tauri-refactor/15-engine-migration-checklist.md` updated while moving code so future compaction or new sessions do not lose track of remaining work.Also, keep things separated as much as possible.

Verification expectation before calling the migration done:

- `pnpm typecheck`
- `pnpm build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `pnpm check:docs`

Do not mark the migration complete while any normal non-sidecar/non-sync application workflow still depends on an old server endpoint or placeholder behavior.

## IMPORTANT

## MIGRATE THE ENTIRE APPLICATION. THE ENTIRE THING SHOULD WORK, ALL BUTTONS, UI COMPONENTS, ETC. SPIN UP SUBAGENTS FOR TESTING OR MOVING PIECES. MAKE SURE TO ONLY RE-WRITE THINGS THAT MUST BE RE-WRITTEN FOR MUCH BETTER CODE ORGANIZATION. THIS IS THE MAIN IMPORTANT FOCUS. MOVING TO TAURI AND MUCH MORE READABLE ORGANIZED CODE (i see a massive storage.rs file kreeping that should heavily be split, stuff like that is horrible).

### KEEP THINGS SEPARATED AS MUCH AS POSSIBLE. DO NOT DUPLICATE CODE OVER AND OVER AGAIN. FOR EXAMPLE, RUST SHOULD JUST DIRECTLY BE CALLED IN TAURI ITS OKAY TO DO THAT. AVOID GIANT FILES.

## API CALLS NEED TO FULLY BE REFACTORED. NO MORE API CALLS, ALL IS JUST CALLING RUST CODE DIRECTLY OR CALLING TYPESCRIPT CODE LIKE AGENTS THAT CALL RUST CODE.

# AGAIN, YOUR TASK IS COMPLETE WHEN THE ENTIRE APP IS PORTED AND WORKING EXCEPT FOR SYNC AND SIDECAR.
