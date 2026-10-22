---
template: evidence-manifest
template_version: 1.0.0
template_digest: "2787d570257f4ffbf8add20f62aa21227a4aea1a6723583e8928b5e71e09f79b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: final
task_id: TASK-0005
---

# Evidence Manifest

## Task

TASK-0005 — M28 usability recovery and core workflow completion.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001: Basic workflow | pass | `blueprints/basic.json`; `orbit-blueprint` 22 tests; no work items + four declared views |
| AC-002: Default binding | pass | `default_workflow.rs`, `authority_project_command`; runtime/storage tests for rich + minimal creation and explicit replacement |
| AC-003: Task details/criteria | pass | `task_detail_commands.rs`, schema 20 tables/columns, client tests, TUI criteria modal; checking all criteria leaves status unchanged |
| AC-004: Attachments | pass | `attachment_commands.rs`, `attachment_store.rs`, storage/client/runtime tests; PTY attach path; object store at `<db dir>/attachments/objects` |
| AC-005: TUI experience | pass | `orbit-tui` 136 tests; `test_m28_usability_recovery.py` PTY scenario A |
| AC-006: Navigation | pass | PTY scenario F; action bar; `?` help; session restore on restart |
| AC-007: Workflow terminology | pass | TUI strings, CLI help + `workflow` alias, docs; `blueprint.*` compatibility preserved |
| AC-008: MCP parity | pass | `tests/acceptance/mcp/test_m28_external_agent.py` external stdio scenario; zero agent jobs |
| AC-009: Validation | pass | focused 1,238; workspace 1,423; clippy clean; fmt clean; docs 34; PTY+MCP 40 |
| AC-010: Honest records | pass | `.agent/current/*`, milestone, roadmap, status, journal and this manifest |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Focused Rust | pass | `cargo test -p orbit-domain -p orbit-application -p orbit-storage -p orbit-runtime -p orbit-blueprint -p orbit-client -p orbit-ui -p orbit-tui -p orbit-mcp -p orbit-sync --offline` | 1,238 passed, 0 failed |
| Workspace regression | pass | `cargo test --workspace --offline` | 1,423 passed, 0 failed |
| Clippy | pass | 12 affected crates, `--all-targets -- -D warnings` | clean (`.agent/evidence/m28-usability-clippy.log`) |
| Format | pass | `cargo fmt --all -- --check` | clean |
| Documentation | pass | `python3 -m unittest discover -s tests/documentation` | 34 passed |
| Real PTY + MCP | pass | `python3 -m pytest -q tests/acceptance/tui tests/acceptance/mcp` | 40 passed |
| External MCP scenario | pass | `test_m28_external_agent.py` | create project/task, details, criteria, attachment read, status → done, TUI inspection, no agent jobs |

### M15 profile observation

The full local profile ran 42 checks with 40 passing; the two failures are
release-workflow contract tests that require `.github/workflows/release.yml`,
which is intentionally withheld in this repository. Evidence:
`.agent/evidence/m28-usability-m15.zip`.

## Checks not run

- Connected attachment upload/retrieval (scenario D) — no server endpoint exists;
  connected operations fail closed with explicit messages.
- Live Herdr Agent Development scenario C — no authorized runtime configured.
- M15 release-workflow contract checks — the workflow file is intentionally
  withheld in this repository; the other 40 M15 checks pass.

## Waivers

- None.

## Residual risk

- Backups must include the database, knowledge vault and attachment object store.
- Connected task-detail/attachment parity remains outstanding.
- Attachment viewing depends on an external viewer; export still works without one.
