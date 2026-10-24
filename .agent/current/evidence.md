---
template: evidence-manifest
template_version: 1.0.0
template_digest: "2787d570257f4ffbf8add20f62aa21227a4aea1a6723583e8928b5e71e09f79b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: final
task_id: TASK-0006
---

# Evidence Manifest

## Task

TASK-0006 — M28 connected completion and agent lifecycle acceptance.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001: Connected rich commands/reads | pass | `domain_commands.rs` additive variants; runtime idempotency/replay tests; server detail/criteria/activity routes; client snapshot-first detail |
| AC-002: Per-command pending/conflict | pass | client `command_state`; TUI/MCP fail-closed tests; no aggregate-depth success |
| AC-003: Server attachment authority | pass | routes + `smoke-attachments.py`; 401/404/409/413; metadata-only ledger test; auth before `to_bytes` |
| AC-004: Offline staging/retry/cross-client | pass | scenario D (6 tests) + client staging tests; duplicate/truncated retry; checksum verification |
| AC-005: CLI/TUI attachment surface | pass | `orbit attachment` family; TUI `[pending]`, header count, blocked flush; unit tests |
| AC-006: Agent lifecycle | pass | `agent complete|interrupt`; resume guards; scripted Scenario C (1 passed) |
| AC-007: Evidence wiring/docs | pass | post-build required checks; 39 documentation tests |
| AC-008: Full validation | pass | workspace 1,489; clippy/fmt clean; TUI 39, MCP 2, connected 6, agent 1; M15 45/47 |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Workspace regression | pass | `cargo test --workspace --offline` | 1,489 passed, 0 failed |
| Clippy | pass | 12 crates, `--all-targets -- -D warnings` | clean |
| Format | pass | `cargo fmt --all -- --check` | clean |
| Documentation | pass | `python3 -m unittest discover -s tests/documentation` | 39 passed |
| PTY | pass | `python3 -m pytest -q tests/acceptance/tui` | 39 passed |
| MCP | pass | `python3 -m pytest -q tests/acceptance/mcp` | 2 passed (local + connected server) |
| Connected attachments | pass | `python3 -m pytest -q tests/acceptance/connected` | 6 passed |
| Agent scenario C | pass | `ORBIT_REQUIRE_AGENT_EVIDENCE=1 pytest tests/acceptance/agent` | 1 passed (scripted Herdr) |
| Server attachment smoke | pass | `python3 deploy/server/smoke-attachments.py` | PASS (auth, import, duplicate, download, removal) |
| Offline authority smoke | pass | `python3 deploy/server/smoke-offline-authority.py` | PASS (stale conflict, reviewed resolution) |

### M15 profile observation

The full local profile ran 47 checks: 45 passed, including all product, server,
blueprint, offline-authority, connected, MCP and agent checks. The two failures
are release-workflow contract tests that read `.github/workflows/release.yml`,
which is intentionally withheld. Evidence:
`.agent/evidence/m28-connected-final-m15.zip`.

## Checks not run

- Live Herdr/model execution (token spend/explicit authorization required).
- Per-device project authorization (single-tenant by design; TODO-0013).
- Server-side attachment GC endpoint (local purge only).
- Connected MCP attachment tools (not exposed).
- Public release installation/update (M27 external gate).

## Waivers

- None.

## Residual risk

- Single-tenant device access to attachments is broader than per-project ACLs.
- Backups must include server attachment objects and replica staging.
- Criteria/activity reads require a reachable server; detail reads use the snapshot.
