# Development 235 — M23 runtime reconciliation slice

## Delivered

- Durable launch-claim lookup for a single claimed attempt, so uncertain dispatches are not restarted automatically.
- Runtime evidence reconciliation with revision checks, claim token verification and session identity protection.
- Separate handling for reachable sessions, unreachable machines, and confirmed-absent runtime sessions.
- Conservative ambiguous-launch handling: absence after uncertain dispatch retains job ownership in `unreachable` until manual reconciliation.
- Agent attention query (waiting for input, blocked, unreachable, launching) for future monitoring UI.
- Regression coverage for ambiguous launch recovery, stale revisions, forged session identities, confirmed absence, and job/task separation.
- Corrected a pre-existing test setup in the accepted-command suite which had left the task title changed before expecting a successful preparation.

## Still required

Authority command router wiring and role/host authorization; Herdr adapter; CLI/MCP; UI; packaged end-to-end tests. The reconciliation API is currently an authority-local storage primitive; callers must be authenticated at the future application boundary.

## Verification

SQLite schema parsed using Python sqlite3. Rust cargo/rustc unavailable, so this development slice has **not** passed Rust compile or unit tests; do not treat it as accepted runtime behavior.

## Progress

M23 remains **21% verified/provisionally credited** from the previous checkpoint; do not inflate milestone progress without executable acceptance. M23-I1 is not closed. This checkpoint contributes additional source implementation but is not yet an accepted 5-percentage-point milestone iteration. M15 remains pending, and M22 remains 100% source scope.
