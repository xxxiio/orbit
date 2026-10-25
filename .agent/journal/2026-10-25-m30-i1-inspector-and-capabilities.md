---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-25"
task_id: TASK-0008
---

# Journal Entry: 2026-10-25 — Two-region TUI inspector and executable trusted capabilities

## Context

M22–M29 were complete. Two gaps remained in the highest-priority unfinished work:
the TUI permanently reserved a third `Details` column (`show_detail: true`), and
M25 EC-003 was unmet because capability executor primitives existed while every
publication path passed an empty installed-capability set and no surface could
install, authorize, offer, request or execute an action. M27 stays externally
blocked on release authorization, so this was the highest-value locally
verifiable work.

## Work completed

- Replaced the docked Details pane with an explicit inspector: two-region shell,
  `Modal::Inspector(InspectorState)` with `InspectorSubject`, overlay/full-screen
  presentation, focus restoration, selection/scroll preservation, missing-entity
  fallback, edit round trip, legacy-session tolerance, and removal of
  `FocusPane::Detail` and the docked cache.
- Added durable capability installation, project-scoped grants, action receipts
  and reference note effects; extended request planning with schema-declared
  optional inputs and length/range validation.
- Added `TrustedComposition` (descriptor + executor registration, capability
  requirements), a deterministic reference capability
  (`orbit.reference.work-item-note`), offers with availability codes, explicit
  requests, single-claim execution, receipts, listing, status and atomic
  settlement of `claimed`/`uncertain` without re-execution.
- Added CLI `orbit blueprint capability {list,install,uninstall,authorize,revoke}`
  and `orbit blueprint action {offers,request,execute,show,list,reconcile}`;
  connected mode refuses with the tracked EC-004 message; local
  `blueprint.transition` now uses durable grants.
- Added TUI client/backend capability methods, cached offers/history, menu/
  palette/inspector entries with reasons, schema-driven form, pending-request
  confirmation, execution result modal and `uncertain` reconciliation.
- Updated the existing documentation tree (TUI guide, blueprint authoring/views,
  walkthrough, developer capability reference, CLI/MCP reference and contracts)
  and regenerated terminal captures.

## Decisions made

- The inspector is an overlay with a full-screen fallback below 80x20; there is
  no docked mode, because the docked panel was the confirmed layout defect.
- Installation, authorization, request, claim, execution and receipt are separate
  durable facts; a manifest never grants execution, and the request path
  re-checks every fact rather than trusting a previously rendered "available".
- A crash after claim is settled by an explicit operator decision (atomic
  storage settlement), never by automatic retry; `uncertain` follows the same
  rule, and no settled receipt can be overwritten.
- The reference capability's effect is an internal idempotent note row keyed by
  the action command id, so the whole path is demonstrable without Python,
  Herdr, network or external services.
- Capability execution is local-authority only in this iteration; connected mode
  refuses instead of queueing an unexecutable command (M25 EC-004).

## Files or components changed

- `crates/orbit-tui/src/{app,render,input}`, `backend.rs`, `ui_state.rs`, tests
  and `tests/acceptance/tui/`.
- `crates/orbit-storage/src/{capability_installs.rs,blueprint_actions.rs,store_core.rs,lib.rs}`.
- `crates/orbit-runtime/src/{capability_composition.rs,blueprint_authority.rs,lib.rs}`.
- `crates/orbit-client/src/{capability_actions.rs,lib.rs}`.
- `crates/orbit-cli/src/{blueprint_commands.rs,connected_blueprint.rs}`,
  `crates/orbit-mcp/src/blueprint_tools.rs`.
- `blueprints/examples/capability-reference.json`,
  `tests/acceptance/test_m30_capability_workflow.py`.
- `docs/**` (guides, developer reference, CLI/MCP reference, contracts, roadmap),
  `scripts/check-m25-blueprint-contracts.py`, `.agent/**` records.

## Validation performed

- `cargo test --workspace --offline` — 1,623 passed, 0 failed (29 binaries).
- `cargo test -p orbit-tui -p orbit-client` — 348 + 246 passed.
- `cargo test -p orbit-storage -p orbit-runtime` — 105 + 527 passed.
- `cargo clippy … --all-targets -- -D warnings` — clean; `cargo fmt --all -- --check` — clean.
- `python3 -m pytest -q tests/contracts` — 43 passed.
- `python3 -m pytest -q tests/documentation` — 39 passed.
- `python3 -m pytest -q tests/acceptance/tui` — 65 passed, 1 skipped (opt-in fixture).
- `python3 -m pytest -q tests/acceptance/test_m30_capability_workflow.py` — 1 passed.
- Manual real-binary reproduction of install → publish → apply → offers →
  authorize → request → execute → receipt → restart.

## Problems encountered

- The docs lane surfaced two contract inconsistencies: `maxLength` was declared
  but not enforced, and schema-declared optional inputs (`tag`) were rejected by
  storage. Both were fixed in the request path (schema-declared allow-list,
  min/max length and integer bounds) with tests and doc corrections.
- A crash between claim and result left a request unreconcilable. Fixed by
  atomic settlement of `claimed`/`uncertain` with an explicit decision, without
  executing or re-claiming.
- `cargo fmt --all` in one lane reformatted concurrently edited files; no
  semantic change, and the final format check is clean.

## Remaining work

- M25 EC-004: connected blueprint MCP writes through the canonical outbox with
  lost-ack replay (M30 I2).
- M25 EC-005: previewable blueprint upgrades (M30 I3).
- M25 EC-006: habit recurrence/streak extraction into an opt-in capability
  (M30 I4).
- M27 public release verification remains externally blocked.
- Connected capability execution is not implemented; the refusal is explicit.

## New TODOs

- TODO-0015 (proposed): connected capability execution parity once M25 EC-004 lands.
- TODO-0016 (proposed): expose capability status/state APIs in a TUI screen once a
  second provider exists (currently implemented but not consumed).

## Risks or blockers

- Connected users cannot execute capability actions by design; the refusal names
  M25 EC-004.
- `uncertain` remains proven by the runtime matrix, not by a public executor.
- M27 public release verification is externally blocked.
