---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0006
status: completed
milestone_id: MILESTONE-0028
---

# Task: M28 connected completion and agent lifecycle acceptance

## Objective

Finish the remaining M28 acceptance scope: server-authoritative task details,
acceptance criteria, activity and attachments in connected mode; an explicit
agent job completion/interruption lifecycle with resume guards; and required
scenario evidence, without weakening local authority or claiming unverified
live-agent behaviour.

## Current behaviour

Local Basic project management, rich task details, acceptance criteria and
attachments were complete. Connected mode could queue project/task commands but
rejected descriptions, criteria, activity and attachments; server routes did not
exist for attachments; agent jobs could be launched/reviewed but not explicitly
completed locally, so `agent resume` could not be exercised end-to-end. Oracle
reviews then found ledger-payload, retry, pending-accuracy and evidence-wiring
defects in the new code.

## Desired behaviour

Connected clients can read and edit rich task fields through the accepted
command ledger, stage attachment uploads offline and synchronise them exactly
once, retrieve attachments from another authorized client, and see pending vs
accepted state honestly. Operators can complete or interrupt a local agent job,
resume from a terminal predecessor with a durable handoff, and exercise the
documented lifecycle with a scripted Herdr CLI. Evidence for connected TUI, MCP
and agent scenarios is required, not silently skipped.

## Scope

- Additive `TaskAction::SetDetails`/`AcceptanceCriteria` and server task
  detail/criteria/activity read routes.
- Connected client rich reads/writes, per-command pending/conflict state,
  read-first-then-queue, snapshot-first detail reads.
- Server attachment import/list/download/remove/associate routes with owner
  validation, auth-before-body, metadata-only ledger payloads and idempotent
  retry/removal.
- Replica attachment staging, ordered flush, checksum verification, CLI
  `orbit attachment` commands, TUI pending markers.
- Local `agent complete`/`agent interrupt`, resume launch-claim guard and
  scripted Scenario C acceptance.
- Oracle remediation (outbox 4xx handling, error propagation, docs accuracy),
  evidence wiring and records.

## Out of scope

- Live Herdr/model execution (requires explicit authorization/token spend).
- Per-device project ACLs; the authority remains intentionally single-tenant.
- Server-side attachment garbage collection.
- Connected MCP attachment tools.
- Public release verification (M27 external gate).

## Dependencies

- MILESTONE-0015 Linux executable baseline and the existing authority ledger.
- MILESTONE-0022/0023 planning and agent read contracts.
- MILESTONE-0025 implemented blueprint/work-item surfaces.
- M28 local task-detail/attachment work (TASK-0005).

## Risks

- Connected writes could be reported as accepted while queued; mitigated with
  per-command state and fail-closed pending.
- Lost acknowledgements could duplicate attachments/criteria; mitigated with
  deterministic identities and metadata-only command payloads.
- Attachment content could bloat the ledger; mitigated by checksum metadata.
- Unauthenticated uploads could exhaust memory; authenticated before buffering.
- Agent completion could resume an ambiguous launch; guarded with a launch-claim
  check.
- Single-tenant authorization could be mistaken for per-project scoping;
  documented explicitly and tracked as TODO-0013.

## Acceptance criteria

- [x] AC-001: Connected clients can read task detail, ordered acceptance
  criteria and activity, and queue details/criteria changes through the accepted
  ledger with exactly-once receipts.
- [x] AC-002: Queued connected mutations are never reported as accepted; TUI/MCP
  use per-command pending/conflict state and fail closed when it is unavailable.
- [x] AC-003: Server attachment import/list/download/remove/associate are
  device-authenticated, validate owners, enforce limits before buffering, store
  metadata-only ledger payloads and return actionable 400/401/404/409/413.
- [x] AC-004: Attachments can be staged offline, flushed exactly once after
  reconnect, retrieved by another client with checksum verification, retried
  after lost acknowledgements/truncated uploads, and removed idempotently.
- [x] AC-005: CLI and TUI expose attachment add/list/inspect/export/remove/flush
  with explicit pending and conflict markers; TUI header counts staged uploads.
- [x] AC-006: `orbit agent complete|interrupt` record job-only terminal states;
  resume requires a terminal predecessor with a durable handoff and refuses
  ambiguous/claimed launches; scripted Scenario C passes.
- [x] AC-007: Connected TUI, connected MCP and agent scenario checks run after
  the build with required-evidence envs; docs match connected behaviour.
- [x] AC-008: Oracle findings are remediated; workspace tests, clippy, fmt,
  documentation, PTY/MCP/connected/agent suites and the M15 product profile pass
  (apart from the withheld release-workflow contract).

## Validation

```sh
cargo test --workspace --offline
cargo clippy --offline -p orbit-domain -p orbit-application -p orbit-storage \
  -p orbit-runtime -p orbit-sync -p orbit-blueprint -p orbit-client -p orbit-ui \
  -p orbit-tui -p orbit-mcp -p orbit-cli -p orbit-server --all-targets -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tests/documentation
python3 -m pytest -q tests/acceptance/tui tests/acceptance/mcp \
  tests/acceptance/connected
ORBIT_REQUIRE_AGENT_EVIDENCE=1 python3 -m pytest -q tests/acceptance/agent
./scripts/m15-full-automated.sh --profile full
```

### Evidence

- Workspace 1,489 passed, 0 failed.
- TUI 39, MCP 2, connected 6, agent 1.
- Documentation 39; clippy/fmt clean.
- M15 45 passed, 2 failed (release-workflow contract only; workflow file
  intentionally withheld). `.agent/evidence/m28-connected-final-m15.zip`.
- Oracle gate reports and remediation are recorded in the journal.
