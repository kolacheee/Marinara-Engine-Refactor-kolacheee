# Promansis

## Current Work

- Saving a character can persist card data in the wrong shape.
  - Status: In review
  - Next step: Ready for review on the focused bug-fix branch after Rust, TypeScript, and docs checks.
  - Blockers: None.

## Owned Bugs

### Saving a character can persist card data in the wrong shape

- Status: In review
- Owner: Promansis
- Impact area: UI | shared/api | Rust capability | engine
- Reported: 2026-05-19
- Last updated: 2026-05-19

#### Notes

The local-only bug backlog lists this as bug 4. Character create and version restore already serialize card `data`, but generic `storage_update` patches could persist object-shaped `data` from the character editor, agent card updates, roleplay scene memories, chat schedules, and connected character commands.

Generic character update patches now normalize card `data` at the Rust storage command boundary before writing to storage, so all `storage_update` callers keep the persisted JSON-string contract.

## Status Notes

No status notes currently listed.
