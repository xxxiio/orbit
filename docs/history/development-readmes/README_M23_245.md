# Development 245 — Local agent evidence monitoring

## Delivered source scope

- `orbit agent sweep`: inspect all claimed nonterminal local Herdr sessions through `AuthoritativeHost`, never launch a session or mutate task acceptance; per-job failure isolation.
- `orbit agent inspect`: on reviewable states, automatically persist a read-only Git snapshot as a structured handoff. Collects no shell text from a session, and explicitly makes no attribution claim.
- Duplicate Git snapshots are suppressed across repeated polls; deterministic handoff IDs protect conflicting replays.
- Poll candidates are persisted launch claims only, with unit regression test and SQLite query validation.
- Existing `orbit agent capture`, `review`, `attention`, `overview`, and `start` remain under the unified `orbit` CLI.

## Verification and limitations

- SQLite schema/poll query ran under Python sqlite3; no Rust executable or Herdr installation is available in this environment, so **Rust compilation, Rust unit tests and live Herdr smoke tests are unverified**.
- Git evidence is collected on inspection or sweep (no background monitor). The worktree path is still the recorded repository path; after Herdr creates an isolated worktree, automatic capture may inspect the original repository rather than the isolated worktree. It is therefore not proof of changes made by that agent.
- `idle` maps to reviewable `waiting_input` and does not imply job or task acceptance; no automatic approval is performed.
- Connected-mode agent dispatch and authenticated worker orchestration remain outstanding.

## Progress accounting

M23 **38% -> 43% (+5 points provisional implemented source-scope)**, attributed to automatic evidence handoffs and batch monitoring together; not executable acceptance. M22 source scope 100%; M15 executable acceptance pending. Internal-testing readiness not established.
