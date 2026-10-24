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

Orbit is usable as a local and connected project manager. New projects are
pinned to the Basic workflow offline and open Overview/Tasks/Board/Timeline.
Tasks carry Markdown descriptions, priority, labels, planning metadata, ordered
acceptance criteria and content-addressed attachments. The TUI works without Vim
knowledge, restores the active project/view, and shows pending vs accepted state.
Connected clients support rich task details/criteria/activity, durable offline
attachment staging with exactly-once flush, cross-client retrieval and
idempotent removal. External agents use MCP without Orbit launching a managed
agent, and operators can explicitly complete/interrupt local agent jobs before
resuming from a durable handoff. Live Herdr/model acceptance remains opt-in.
M28 is complete for the verified scope; per-device project ACLs, a server-side
attachment GC endpoint and live-agent acceptance remain documented follow-ups.

## Current milestone

MILESTONE-0028 — M28 Project-Centric TUI and Blueprint UI Composition, completed
with usability recovery and connected completion. EC-001..EC-010 are satisfied;
the remaining gaps are per-device project authorization, server attachment GC,
live Herdr acceptance and public release verification.

## Recently completed

- Connected rich task parity: additive TaskAction variants, server detail/
  criteria/activity routes, snapshot-first detail reads, per-command pending.
- Server-authoritative attachments: import/list/download/remove/associate,
  auth-before-body, metadata-only ledger payloads, idempotent retry/removal.
- Replica attachment staging, ordered flush, checksum verification, CLI
  `orbit attachment`, TUI pending markers.
- Local `agent complete`/`interrupt`, resume launch-claim guards, scripted
  Scenario C; required connected TUI/MCP/agent evidence checks.
- Oracle remediation for outbox 4xx handling, error propagation, pending
  accuracy, label consistency and documentation.

## In progress

- None. The next-step record NEXT-0009 is completed.

## Blocked

- M27 real GitHub Release installation/update verification: the local account is
  read-only, no release is published, and publication needs maintainer approval.
  The release workflow file is also intentionally withheld pending an OAuth
  workflow-scope fix, so two M15 release-contract checks cannot run.

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
- Tests: `cargo test --workspace --offline` 1,489 passed, 0 failed.
- Acceptance suites: TUI 39, MCP 2, connected 6, agent 1.
- Format/lint: fmt clean; clippy `-D warnings` on 12 crates.
- Documentation: 39 tests pass.
- Acceptance: `./scripts/m15-full-automated.sh --profile full` 45 passed, 2 failed
  (release-workflow contract only; workflow file withheld);
  `.agent/evidence/m28-connected-final-m15.zip`.
- Data integrity: replica schema v2 migration; server ledger metadata-only for
  attachment imports; storage/regression suites pass.
- Interactive evidence: `.agent/evidence/m28-terminal-smoke.txt` and PTY evidence
  under `target/tui-evidence/`.

## Risks

- The authority is single-tenant: any enrolled device may access any existing
  attachment owner. Per-device project ACLs are TODO-0013.
- No server-side attachment GC endpoint (TODO-0014); local purge only.
- Connected MCP does not expose attachment tools.
- Live Herdr/model acceptance remains opt-in; Scenario C uses a scripted CLI.
- Public release remains unverified; M27 is blocked externally.
- Terminal tests cover representative emulators, not every terminal.

## Remaining milestones

1. MILESTONE-0027 — blocked on authorized external release verification.
2. MILESTONE-0025 — deferred (EC-003 through EC-006).

## Next deterministic step

None. NEXT-0009 (connected attachment authority) is completed; the remaining
items are tracked as TODOs and the blocked M27 release gate.
