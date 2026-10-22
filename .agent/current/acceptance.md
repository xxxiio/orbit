---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: passed
task_id: TASK-0005
---

# Current Acceptance Report

## Task

TASK-0005 — M28 usability recovery and core workflow completion.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Basic workflow and default binding | pass | `orbit-blueprint` 22 tests; `basic.json` validates offline; runtime/storage tests prove rich + minimal creation bind `basic` v1 and explicit changes replace it |
| G-0002 | Task details and acceptance criteria | pass | domain/application/storage/client tests; details no-op silence; ordered criteria roundtrip; checking all leaves status `inbox` |
| G-0003 | Local attachments | pass | runtime attachment store tests (dedup, size, symlink, GC), storage metadata tests, client tests, MCP tool tests; real PTY attach/read/export path |
| G-0004 | TUI task experience | pass | `orbit-tui` 136 tests; multiline description, priority/labels, criteria modal, attachment modal, activity modal, action bar, session restore |
| G-0005 | Navigation and discoverability | pass | PTY scenario F: arrows/Enter/Esc/Tab, `/` search, `?` help, `K`/`H`/`i`/`F`/`W`, action bar, narrow terminal 60×18 |
| G-0006 | Workflow terminology | pass | TUI labels, help, action bar, CLI help and `orbit workflow` alias; docs updated with compatibility notes; `blueprint.*`/`orbit blueprint` preserved |
| G-0007 | MCP parity | pass | external stdio MCP scenario: project/create → task → details → criteria → attachment → status → activity; TUI inspection; zero `agent_jobs` rows |
| G-0008 | Focused Rust tests | pass | 1,238 passed, 0 failed (`.agent/evidence/m28-usability-focused-tests.txt`) |
| G-0009 | Workspace regression | pass | 1,423 passed, 0 failed (`cargo test --workspace --offline`) |
| G-0010 | Lint and format | pass | clippy `-D warnings` on 12 affected crates; `cargo fmt --all -- --check` clean |
| G-0011 | Documentation contract | pass | `python3 -m unittest discover -s tests/documentation`: 34 passed |
| G-0012 | Real-terminal acceptance | pass | `python3 -m pytest -q tests/acceptance/tui tests/acceptance/mcp`: 40 passed (39 PTY + 1 MCP stdio) |

## Commands run

```text
cargo test --offline -p orbit-domain -p orbit-application -p orbit-storage \
  -p orbit-runtime -p orbit-blueprint -p orbit-client -p orbit-ui -p orbit-tui \
  -p orbit-mcp -p orbit-sync
cargo test --workspace --offline
cargo clippy --offline -p orbit-domain -p orbit-application -p orbit-storage \
  -p orbit-runtime -p orbit-sync -p orbit-blueprint -p orbit-client -p orbit-ui \
  -p orbit-tui -p orbit-mcp -p orbit-cli -p orbit-server --all-targets -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tests/documentation
python3 -m pytest -q tests/acceptance/tui tests/acceptance/mcp
./scripts/m15-full-automated.sh --profile full
```

### M15 profile observation

`./scripts/m15-full-automated.sh --profile full` ran 42 checks: 40 passed,
including every product, CLI, server, blueprint, offline-authority and agent
contract smoke. The two failures are release-workflow contract tests that read
`.github/workflows/release.yml`, which is intentionally withheld from this
repository pending an OAuth workflow-scope fix; they are not M28 product
failures. Evidence: `.agent/evidence/m28-usability-m15.zip`.

## Checks not run

- Connected attachment upload/retrieval (scenario D): the server has no
  attachment endpoints; all connected attachment operations fail closed with an
  explicit Unavailable message instead of a false success.
- Agent Development scenario C with a live Herdr runtime: no authorized runtime
  is configured in this environment; agent job inspection and disabled review
  reasons remain covered by tests.
- The two M15 release-workflow contract checks cannot pass while
  `.github/workflows/release.yml` is withheld from the repository; all other 40
  M15 checks pass. Broad public release acceptance remains M15/M27 scope.

## Waivers

- None. Unsupported connected attachment behaviour is explicitly refused and
  documented rather than silently accepted.

## Residual risk

- Connected mode remains local-authoritative only for task details, acceptance
  criteria and attachments.
- Attachment object storage is local to the workspace; backups must include the
  `attachments/objects` directory.
- PTY evidence covers representative terminal sizes, not every emulator.
