---
template: evidence-manifest
template_version: 1.0.0
template_digest: "2787d570257f4ffbf8add20f62aa21227a4aea1a6723583e8928b5e71e09f79b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: final
task_id: TASK-0009
---

# Evidence Manifest

## Task

TASK-0009 — M30 I2–I4 and the navigation rail correction.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001: Navigation rail drives the workspace | pass | `crates/orbit-tui/src/input/dispatch.rs` (Enter precedence), `crates/orbit-tui/src/app/navigation.rs` (`activate_sidebar` focus), `crates/orbit-tui/src/tests/m30_sidebar_navigation_tests.rs` (4 tests), `docs/guides/tui-workspace.md`; real-binary PTY: Projects → Home → Inbox → Agents → Alpha Project. |
| AC-002: Connected MCP blueprint writes are outbox-backed | pass | `crates/orbit-mcp/src/blueprint_tools.rs` (`blueprint_write`/`blueprint_disposition`, zero `.post(`), `crates/orbit-mcp/src/main.rs` schemas, `tests/acceptance/mcp/test_m30_blueprint_outbox.py`; `cargo test -p orbit-mcp` 76 passed (8 outbox tests: offline pending, accepted replay, conflicting id, lost-ack with original receipt, no fabricated receipt, reads not queued, local mode unchanged); server receipt counts asserted in the Python case. |
| AC-003: Previewable fenced upgrades | pass | `crates/orbit-runtime/src/blueprint_upgrade.rs` (+13 tests), `crates/orbit-cli/src/blueprint_commands.rs` (`upgrade-preview`/`upgrade`, 2 CLI tests), `crates/orbit-cli/src/connected_blueprint.rs` (refusal); `tests/acceptance/local_cli_workflows.py` 60 passed; documented transcript in `docs/guides/custom-blueprints.md`, `docs/reference/cli.md`, `docs/cli-contract.md`. |
| AC-004: Habit recurrence capability ownership | pass | New `crates/orbit-habit-capability` (5 tests, moved algorithm), `crates/orbit-runtime/src/habit_capability.rs` (registration/executor/gating tests), `crates/orbit-runtime/src/application_services.rs` (install+grant gate), `crates/orbit-application` outcomes, domain 56 tests with no recurrence symbols; `docs/guides/habits-routines-schedules.md` executed against an isolated DB. |
| AC-005: Quality gates | pass | See the gate table below. |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Workspace tests | pass | `cargo test --workspace --offline` | 1,658 passed, 0 failed, 31 binaries |
| TUI tests | pass | `cargo test -p orbit-tui --offline` | 250 passed (incl. 4 rail regression tests) |
| Runtime tests | pass | `cargo test -p orbit-runtime --offline` | 542 passed (incl. 13 upgrade tests) |
| MCP tests | pass | `cargo test -p orbit-mcp --offline` | 76 passed (incl. 8 outbox tests) |
| CLI tests | pass | `cargo test -p orbit-cli --offline` | 128 passed |
| Habit capability | pass | `cargo test -p orbit-habit-capability -p orbit-domain -p orbit-application --offline` | 5 / 56 / 17 passed |
| Lints | pass | `cargo clippy … --all-targets -- -D warnings` | clean (14 crates) |
| Formatting | pass | `cargo fmt --all -- --check` | clean |
| Terminal acceptance | pass | `python3 -m pytest -q tests/acceptance/tui` | 65 passed, 1 skipped (opt-in fixture) |
| MCP acceptance | pass | `python3 -m pytest -q tests/acceptance/mcp` | 4 passed |
| Local CLI acceptance | pass | `python3 tests/acceptance/local_cli_workflows.py --binary target/debug/orbit` | 60 passed |
| Capability acceptance | pass | `python3 -m pytest -q tests/acceptance/test_m30_capability_workflow.py` | 1 passed (capability list assertion updated for the opt-in habit package) |
| Contracts / docs | pass | `python3 -m pytest -q tests/contracts tests/documentation` | 43 / 39 passed |
| Project records | pass | `getdone-validate-project` | passed |

## Checks not run

- Full M15 release profile (release qualification remains M15/M27).
- Connected capability execution and MCP capability tools (not implemented).
- Live Herdr/model acceptance (unchanged external requirement).

## Waivers

- None.

## Residual risk

- Connected capability actions stay local-only; connected blueprint writes are
  outbox-backed.
- Habit streaks/evidence require the opt-in capability installation (and grant
  for project evidence); counts and last-occurrence data remain independent.
- Upgrade blockers refuse incompatible live-data changes rather than remapping.
