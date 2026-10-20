# Orbit 177 — M20 explicit remote conflict recovery

## Priority decision

Core connected daily write workflows remain incomplete and outrank standalone hardening, packaging, automation, and plugin distribution. This iteration focuses on the user-facing blocked-write recovery path. The larger rich-domain write transport gap remains open.

## Delivered

- `AuthorityConnection::unresolved_commands()` exposes globally ordered pending/conflicted outbox intentions to clients without confusing them with accepted state.
- `AuthorityConnection::resolve_conflict()` lets a user-reviewed replacement enter the remote outbox while preserving the rejected intention in the audit log; synchronous embedded mode deliberately rejects this remote-only operation.
- Conflict replacement now refuses cross-family substitution and already-used idempotency IDs before it updates the audit row. It retains the rejected command's position ahead of later queued work.
- Rust behavior tests added for pending state separation, rejected replacement of a nonconflicted command, replacement order, audit state, family/ID constraints. The tests are NOT executed because Cargo is unavailable.

## Validation and limitations

- `python -m unittest discover -s deploy/server -p 'test_*.py' -q`: **12 passed**.
- Rust compilation and behavioral tests: **not run**; availability of runnable end-to-end server and two-client workflow unverified.
- No rich planning/routine/scheduling/project strategy/milestone transport added this iteration. The next implementation priority remains rich-domain write parity and server receipts.

## Estimated progress

M20 **72% provisional source implementation** (71% in Orbit 176). Not an acceptance metric. Other milestone estimates unchanged: M15 8%, M18 99%, M19 99%, M21 65%, M22 10%. No overall percentage inferred.

## Internal testing readiness

**Not verified:** runnable Rust build and core authoritative server/client smoke tests are missing. No claim of absence of blocking data loss issues is possible.

## Completion priority review

Core daily connected write workflows are still the highest unfinished category. Structured knowledge/automation, installation and internal deployment, and plugin execution remain below them. Flutter is deferred to Atomic App. No standalone hardening or public infrastructure was prioritized.
