---
template: project-status
template_version: 2.0.0
template_digest: "bae979b223043d02aa7be0cacebed8dd05241d5cc9f0d330796a7c919fbbe019"
project_owned: true
record_contract: project-status
record_schema_version: 1
status: current
---

# Project Status

## Current state

Orbit is usable as a local project manager. A new project is pinned to the Basic
workflow offline and opens Overview/Tasks/Board/Timeline without setup. Tasks
carry Markdown descriptions, priority, labels, planning metadata and ordered
acceptance criteria, and can own content-addressed file attachments managed by
the workspace. The TUI works with arrows/Enter/Esc/Tab plus a discoverable action
bar, and restores the active project/view after restart. An external agent can
create a Basic project, create and edit tasks, add criteria, attach evidence,
update progress and complete tasks through MCP without launching an agent job.
M28 is complete for the verified local scope; connected attachment parity is the
next deterministic step.

## Current milestone

MILESTONE-0028 — M28 Project-Centric TUI and Blueprint UI Composition, completed
with usability recovery. EC-001..EC-009 are satisfied; connected attachments,
live Herdr acceptance and public release verification remain documented gaps.

## Recently completed

- Basic v1 bundled workflow and authority-owned default binding (rich + minimal
  creation paths), plus explicit change/unpin operations.
- Task description, priority, labels and ordered acceptance criteria with stable
  ids and completion timestamps (schema 20).
- First-class local attachments: metadata, object store, limits, symlink
  rejection, dedup, integrity verification, GC, TUI attach/list/preview/export
  and MCP metadata/content tools.
- TUI multiline editor, criteria/attachment/activity modals, action bar, session
  restore, first-run guidance and workflow terminology; `orbit workflow` alias.
- External stdio MCP acceptance scenario and 40 real-terminal/MCP acceptance
  items; 1,421 workspace tests; clippy/fmt clean; 34 documentation tests.

## In progress

- None. NEXT-0009 defines connected attachment authority as the next step.

## Blocked

- M27 real GitHub Release installation/update verification: the local account is
  read-only, no release is published, and publication needs maintainer approval.

## Key decisions

- `App`/renderer is authority-agnostic; local and connected differences live in
  `backend.rs`, never in view rendering.
- Connected mutations keep the accepted-vs-pending distinction and use the
  durable outbox; unsupported connected edit surfaces fail closed with a reason.
- The original connected command palette remains opt-in through
  `ORBIT_TUI_LEGACY_CONNECTED=1` while connected blueprint/note/habit parity is
  completed.
- Legacy blueprints receive generated views; no migration.
- Blueprint declarations are presentation metadata, not authority.
- Agent job completion never completes an Orbit task.

## Project health

- Build: `cargo check --workspace --all-targets --offline` pass.
- Tests: `cargo test --workspace --offline` 1,350 passed, 0 failed.
- Focused: `orbit-tui` 125, `orbit-ui` 14, `orbit-client` 305.
- PTY: `python3 -m pytest -q tests/acceptance/tui` 37 passed (local + connected).
- Format/lint: fmt clean; clippy `-D warnings` on affected crates.
- Documentation: 29 tests pass.
- Acceptance: `./scripts/m15-full-automated.sh --profile full` 42 passed, 0 failed,
  0 not-run; `.agent/evidence/m28-m15-final.zip`.
- Data integrity: no schema change; storage/regression suites pass.
- Interactive evidence: `.agent/evidence/m28-terminal-smoke.txt` and PTY evidence
  under `target/tui-evidence/`.

## Risks

- Connected multi-command edits are non-atomic; a later rejection can leave earlier
  intents queued. The UI never presents them as accepted and documents recovery.
- Connected blueprint/work-item/habit/metric/note editing remains on the legacy
  command surface.
- Agent review/resume requires an explicitly configured runtime or connected
  review service.
- Public release remains unverified; M27 is blocked externally.
- Terminal tests cover representative emulators, not every terminal.

## Remaining milestones

1. MILESTONE-0027 — blocked on authorized external release verification.
2. MILESTONE-0025 — deferred (EC-003 through EC-006).

## Next deterministic step

NEXT-0008 (blocked) — authorized real GitHub Release publication and installation
verification for M27.
