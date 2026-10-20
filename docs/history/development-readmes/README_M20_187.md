# Orbit M20 Development 187 — Routine and Milestone Authority

## Functional scope

Development 187 introduces two complete *source-level* command families into
Orbit's shared authoritative Rust runtime. It is not another convenience API.
Both families use the same immediate SQLite transaction, global revision fence,
immutable idempotency receipt, semantic history and server-accepted projection
that existing task, habit and scheduling commands use.

### Routines

- Create named routine with ordered habit membership.
- Rename, replace all membership in a single transaction, or delete.
- Validate habit existence, unique IDs, membership limit, routine identity and name.
- Preserve explicit habit order and reject missing or duplicated members.
- Return a revision-scoped accepted snapshot, including ordered membership.
- Safely retry the identical command using a persisted receipt.
- Reject a conflicting command ID, stale expected revision or different actor.
- Keep habit records intact when their containing routine is deleted.

### Project milestones

- Create a checkpoint associated with an existing project.
- Rename, transition status, move to another project, or delete.
- Implement existing Orbit milestone state machine: planned -> in_progress /
  blocked / cancelled; in_progress -> blocked / completed / cancelled;
  blocked -> in_progress / cancelled. Completed and cancelled are terminal.
- Reject dangling projects, invalid statuses and illegal transitions.
- Refuse milestone move/deletion while tasks reference the milestone.
- Report revision-scoped milestone projections, command receipts and semantic
  changes without bypassing the central authority ledger.

### Transport and client integration

- `GET /v1/authoritative/routines`
- `POST /v1/authoritative/routine-commands`
- `GET /v1/authoritative/milestones`
- `POST /v1/authoritative/milestone-commands`
- Both collections are included in `/v1/authoritative/snapshot` with a
  consistency check against its global revision.
- `AuthoritativeHost`, `EmbeddedAuthorityClient` and `AuthorityConnection`
  expose the same two command families and accepted reads.
- Remote clients durably enqueue routine/milestone commands in the existing
  outbox and reject mismatched authoritative acknowledgement identities.
- Connected replicas cache new collections as *accepted* data, not optimistic
  offline changes. All older replica snapshots must be reinitialized because
  this is a greenfield project with a fail-closed accepted snapshot schema.

### Tests added (Rust, not yet executed)

- Complete create/edit/delete routine flow, membership reorder, empty routine,
  failed membership replacement, stale revisions, cross-actor retries and
  persistence after reopening a file-backed database.
- Milestone create/edit/move/status/delete, legal and illegal transitions,
  stale revisions, invalid project, linked-task constraints and idempotency.
- Unified-client accepted-view equivalence between embedded and remote modes.
- Offline queued writes remain separate from server-accepted views.
- Mixed routine/milestone commands use a single global revision sequence.

## Validation and limits

- `python scripts/legacy-contracts/check_m20_187.py`: 14 structural checks pass.
- `python -m compileall -q scripts`: passed.
- Cargo and rustc are **not available** in this environment. No compilation
  or executable Rust test pass is asserted. The new command families remain
  subject to compiled verification and fixes in Development 188.
- This does **not** complete M20: project strategy command integration,
  CLI/TUI/MCP rich-command wiring and two-client executable acceptance still
  require work. M20 closure is conditional on those gates, not a version number.

## Required executable acceptance

```bash
cargo fmt --all -- --check
cargo test -p orbit-runtime authority_routines
cargo test -p orbit-runtime authority_milestones
cargo test -p orbit-client
cargo test -p orbit-server
cargo test --workspace
```

Then exercise two clients against a self-hosted server: create habits and a
project, create/rename/reorder a routine, create/transition/move a milestone,
force a stale revision conflict, retry an accepted command, refresh the
second client, and verify that rejected/pending commands never affect accepted
state. Keep Flutter work deferred to Atomic App.
