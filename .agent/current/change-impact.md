---
template: change-impact
template_version: 1.0.0
template_digest: "8618a859250c7346e16cb5e20990e6c6d496e6b238ab8f44824028eba527e134"
project_owned: true
record_contract: change-impact
record_schema_version: 1
status: current
task_id: TASK-0006
---

# Change Impact Declaration

## Task

TASK-0006 — M28 connected completion and agent lifecycle acceptance.

## Impact classification

| Impact | Value | Reason | Activated gate |
|---|---|---|---|
| public_api | yes | Additive `TaskAction::SetDetails`/`AcceptanceCriteria`, new server task/attachment routes, `orbit attachment` commands, `agent complete`/`interrupt`, and client APIs for per-command state and staging. | workspace compile, wire-compatibility and acceptance suites |
| persisted_data | yes | Replica schema v2 adds `attachment_uploads`, `replica_metadata` and accepted-attachment metadata; server ledger remains metadata-only for imports; no change to authoritative entity tables. | replica migration, idempotency and retry tests |
| configuration | yes | `ORBIT_REQUIRE_CONNECTED_TUI_EVIDENCE`, `ORBIT_REQUIRE_MCP_EVIDENCE`, `ORBIT_REQUIRE_AGENT_EVIDENCE` gate required evidence checks. | configuration and evidence-flag tests |
| dependencies | yes | No new external crates; `serde`/`sha2`/`reqwest` were already present. | dependency review, locked offline build and clippy |
| security_boundary | yes | Attachment routes authenticate before reading the body, validate owner existence, enforce limits, reject path-like filenames, verify checksums and never execute content; agent resume refuses ambiguous launch claims. | security path/scope, auth-before-body and no-execution tests |
| concurrency | yes | Ordered outbox flush continues after permanent domain rejection, records revision fences as reviewable conflicts, and uploads attachments only after the owning intents are accepted. | concurrency/ordering outbox audit and staging-order tests |
| performance_sensitive | yes | Import ledger payloads are checksum/metadata-only; JSON reads keep a short timeout while binary transfers use a longer one; no new per-frame queries. | performance-sensitive ledger-size, bounded-query and transfer tests |
| user_interface | yes | Connected details/criteria/activity, attachment pending markers, header pending count, blocked-flush recovery and agent job commands. | Ratatui buffer tests and PTY/connected acceptance |
| deployment | yes | Connected server-authoritative rich writes and attachments, with explicit single-tenant authorization semantics and offline staging. | deployment-mode connected server, MCP and scenario D acceptance tests |

## Assumptions and unknowns

- The authority is a single-tenant workspace: all enrolled devices share access to
  every project/entity, consistent with existing commands. Per-device project
  ACLs are a tracked follow-up (TODO-0013).
- Live Herdr/model execution requires explicit authorization and is not part of
  automated evidence; Scenario C uses a scripted Herdr CLI.
- The release workflow file is intentionally withheld; M15 release-contract
  checks cannot pass until it is restored.

## Required outputs

- Additive task/attachment wire and server routes with tests.
- Replica staging and ordered flush with idempotency/integrity tests.
- CLI/TUI surface with honest pending/conflict state.
- Agent lifecycle commands and scripted acceptance.
- Documentation, evidence wiring and records.

## Test tier

- Tier 3: user-facing UI, server-authoritative writes, file content handling and
  agent lifecycle. Workspace tests, clippy, fmt, documentation, real PTY,
  connected-server, MCP and agent suites were run, plus the M15 product profile.
