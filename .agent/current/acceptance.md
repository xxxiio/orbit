---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: passed
task_id: TASK-0006
---

# Current Acceptance Report

## Task

TASK-0006 — M28 connected completion and agent lifecycle acceptance.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Connected rich task commands | pass | Additive `TaskAction::SetDetails`/`AcceptanceCriteria`; runtime tests for idempotency, invalid priority, ordered criteria and mixed raw/rich replay |
| G-0002 | Connected task reads | pass | Server detail/criteria/activity routes; client snapshot-first detail; connected MCP reads tested against a disposable server |
| G-0003 | Per-command pending/conflict | pass | Client `command_state`; TUI/MCP fail-closed tests; no aggregate-depth success path |
| G-0004 | Server attachment authority | pass | Import/list/download/remove/associate; 401 before body, 404/409/413, metadata-only ledger, idempotent removal/duplicate-after-remove; `smoke-attachments.py` |
| G-0005 | Offline staging and retry | pass | Replica `attachment_uploads`, ordered flush, lost-ack duplicate, truncated-body retry, checksum verification; scenario D |
| G-0006 | CLI/TUI attachment surface | pass | `orbit attachment add/list/inspect/export/remove/flush`; TUI `[pending]` rows, header count, blocked flush guidance |
| G-0007 | Agent lifecycle | pass | `agent complete`/`interrupt`, resume guards, scripted Scenario C with a PATH-shimmed Herdr CLI |
| G-0008 | Evidence wiring | pass | Post-build required checks for connected TUI, connected MCP and agent scenario C |
| G-0009 | Workspace regression | pass | 1,489 passed, 0 failed |
| G-0010 | Lint and format | pass | clippy `-D warnings` on 12 crates; `cargo fmt --all -- --check` clean |
| G-0011 | Documentation | pass | 39 doc tests; connected behaviour, pending markers, MCP contract and backup updated |
| G-0012 | Real acceptance suites | pass | TUI 39, MCP 2, connected 6, agent 1 |

## Commands run

```text
cargo test --workspace --offline
cargo clippy --offline -p orbit-domain -p orbit-application -p orbit-storage \
  -p orbit-runtime -p orbit-sync -p orbit-blueprint -p orbit-client -p orbit-ui \
  -p orbit-tui -p orbit-mcp -p orbit-cli -p orbit-server --all-targets -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tests/documentation
python3 -m pytest -q tests/acceptance/tui tests/acceptance/mcp tests/acceptance/connected
ORBIT_REQUIRE_AGENT_EVIDENCE=1 python3 -m pytest -q tests/acceptance/agent
./scripts/m15-full-automated.sh --profile full
python3 deploy/server/smoke-attachments.py
python3 deploy/server/smoke-offline-authority.py
```

### M15 profile observation

45 of 47 checks passed. The two failures are `release-workflow-contract` and
`python-contract-tests`, both reading `.github/workflows/release.yml`, which is
intentionally withheld pending an OAuth workflow-scope fix. Every product,
server, blueprint, offline-authority and agent smoke passed. Evidence:
`.agent/evidence/m28-connected-final-m15.zip`.

## Checks not run

- Live Herdr/model acceptance (Scenario C runs with a scripted Herdr CLI;
  Orbit-side lifecycle only). Requires explicit authorization/token spend.
- Per-device project authorization: the authority is single-tenant and has no
  per-project ACL for any entity. Documented and tracked as TODO-0013.
- Server-side attachment garbage collection (no endpoint; local purge only).
- Connected MCP attachment tools (intentionally not exposed).
- Public release installation/update (M27 external gate).

## Waivers

- None. Single-tenant authorization and the scripted-agent limitation are
  explicitly documented, not silently accepted.

## Residual risk

- Any enrolled device can access any existing attachment owner in the
  single-tenant authority. Mitigate with deployment-level device control until
  per-device scoping lands.
- Attachment content and replica staging must be included in backups.
- Connected dependency on server availability for criteria/activity reads.
