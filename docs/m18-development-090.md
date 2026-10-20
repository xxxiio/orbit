# M18 development 090 — Authoritative project/task relationships

## Changes

- Connected CLI project listing/creation to the existing authenticated canonical server APIs.
- Optional project-scoped task creation using the shared Rust task scope model.
- Server-side existing-project validation and SQLite foreign key with additive migration.
- Project membership exposed through canonical task read results and immutable task events.
- Transactional retry semantics remain inherited from the existing authority transaction.
- Preview command path rejects project-scoped tasks rather than discarding the scope.
- Regression test added for project membership, idempotency, and missing project rejection.

## Status

M18 estimate: 60% (up from 55%). This is source progress, not release acceptance. Neither Rust compilation nor the two-device connected workflow was executable in this environment. Canonical habit/metric commands, offline sync, browser/worker cutover, and migration/import remain to be implemented.
