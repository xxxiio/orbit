# Orbit 096 — Full authoritative evidence snapshot

## Delivered

- Consolidated server snapshot of projects, tasks, habits, occurrences, metric targets and observations into one SQLite transaction with one relay revision/history fingerprint.
- Legacy `/v1/authoritative/evidence` reads now delegate to the same verified snapshot path.
- The opt-in offline Python client caches accepted evidence separately from queued actions, exposes cached evidence read commands, and preserves existing SQLite caches.
- New checks verify atomic six-family caching, additive cache compatibility, and fail-closed incomplete snapshot handling.

## Boundaries

This is not a native Rust desktop/mobile replica. An advancing history fingerprint alone does not establish prefix continuity. Canonical browser/agent cutover, offline conflict reconciliation, import from legacy history, and Git removal remain. Rust compile/unit/runtime acceptance was not performed in the current environment.
