---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-22"
task_id: TASK-0006
---

# Journal Entry: 2026-10-22 — Connected parity, server attachments and agent lifecycle

## Context

M28 local usability recovery was complete, but the brief also required connected
attachments (scenario D), connected rich task parity and Agent Development
acceptance (scenario C). Connected mode previously rejected descriptions,
criteria, activity and attachments; the server had no attachment routes; local
agent jobs could not be explicitly completed, so resume was unreachable through
the binary. Oracle reviews of the first implementation found ledger-payload,
retry, authorization, pending-accuracy and evidence-wiring defects.

## Work completed

- Added additive `TaskAction::SetDetails`/`AcceptanceCriteria`, server task
  detail/criteria/activity read routes, snapshot projection of rich fields and
  connected activity for raw task actions.
- Connected client: snapshot-first detail reads, read-first-then-queue mutations,
  per-command pending/conflict state, domain validation before queueing, outbox
  4xx handling that records conflicts and continues, 409 reviewable conflicts,
  replica v1→v2 migration and hostile filename validation.
- Server attachments: authenticated import/list/download/remove/associate,
  owner existence + project derivation, auth before body buffering, metadata-only
  ledger payloads, idempotent removal/duplicate-after-removal, download headers
  and staging cleanup.
- Client attachments: durable replica staging, ordered flush, lost-ack and
  truncated retry, checksum verification, cross-client retrieval, deterministic
  removal identities, conflict reporting.
- CLI `orbit attachment add/list/inspect/export/remove/flush`; TUI connected
  attachment actions, `[pending]` rows, header pending count, blocked-flush
  recovery.
- Agent lifecycle: `orbit agent complete|interrupt` (job-record only),
  local-machine and ambiguous-launch resume guards, scripted Scenario C.
- Evidence wiring: post-build required checks for connected TUI, connected MCP
  and agent Scenario C; docs corrected and expanded.

## Decisions made

- Connected rich task writes use the existing accepted ledger; no second
  authority path.
- 400/404 domain rejections become reviewable conflicts and do not block later
  commands; 409 revision fences are recorded as conflicts because the frozen
  envelope cannot be replayed safely.
- Attachment ledger payloads contain checksum/metadata only; bytes live in the
  content-addressed store.
- Pending state is tracked per command, not inferred from aggregate outbox depth.
- The authority remains single-tenant; per-project ACLs are a tracked follow-up
  rather than an ad-hoc partial implementation.
- Agent completion/interruption changes only the job record; it does not stop
  the external session and never completes the task.

## Files or components changed

- Runtime/storage/server: `domain_commands.rs`, `lib.rs`, `store.rs`,
  `accepted_application.rs`, `attachment_store.rs`, `agent_jobs.rs`,
  `local_operations.rs`, `orbit-server/src/main.rs`.
- Client: `authority.rs`, `authority/http.rs`, `attachments_remote.rs`,
  `unified/rich_client.rs`, `connected.rs`, `contract.rs`, `outbox_audit.rs`.
- TUI/MCP/CLI: connected enablement, pending markers, attachment commands,
  agent commands, connected MCP rich tools and read-only reads.
- Tests/docs: new connected/agent suites, connected MCP test, expanded scenario D,
  documentation pages and doc tests.

## Validation performed

- `cargo test --workspace --offline`: 1,489 passed, 0 failed.
- Clippy `-D warnings` on 12 crates and `cargo fmt --all -- --check`: clean.
- Documentation: 39 passed.
- TUI 39, MCP 2, connected 6, agent 1 acceptance tests.
- `smoke-attachments.py` and `smoke-offline-authority.py`: pass.
- M15 full profile: 45 passed, 2 failed (release-workflow contract only;
  `.github/workflows/release.yml` intentionally withheld).

## Problems encountered

- Parallel lanes temporarily broke compilation (`TaskAction` matches,
  `AuthorityError::Unauthorized`); fixed immediately.
- 409 handling initially kept rows pending, breaking the established conflict
  review contract; restored conflict state and fixed the outbox audit to allow
  explanatory receipts on superseded rows.
- Evidence checks could silently skip when binaries were missing; required-env
  checks now fail instead.

## Remaining work

- Per-device project ACLs for multi-tenant attachment/entity authorization.
- Server-side attachment GC endpoint.
- Connected MCP attachment tools.
- Live Herdr/model acceptance (opt-in; Scenario C uses a scripted CLI).
- Public release verification (M27 external gate).

## New TODOs

- TODO-0013: per-device project authorization for connected attachments (ready).
- TODO-0014: server-side attachment garbage collection (ready).

## Risks or blockers

- Single-tenant device access is broader than per-project ACLs.
- Backups must include server attachment objects and replica staging.
- The withheld release workflow prevents the two M15 release-contract checks.
