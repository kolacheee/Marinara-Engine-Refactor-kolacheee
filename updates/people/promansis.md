# Promansis

## Current Work

- Game asset operations can follow symlinks outside the managed asset root.
  - Status: Done
  - Next step: Ready for review on the focused bug-fix branch.
  - Blockers: None.




## Owned Bugs

### Game asset operations can follow symlinks outside the managed asset root

- Status: Done
- Owner: Promansis
- Impact area: shared/api | Rust capability
- Reported: 2026-05-19
- Last updated: 2026-05-19

#### Notes

The local-only bug backlog lists this as bug 1. The fix belongs to the Rust asset capability and shared path safety helper so reads, writes, moves, copies, deletes, file info, tree, and manifest paths stay inside the managed game asset root after symlink resolution.

Resolved by canonicalizing managed asset roots, validating resolved asset paths against the root, skipping symlinked entries during asset scans/copies, and adding focused Rust regression coverage for symlink escapes.

## Status Notes

No status notes currently listed.
