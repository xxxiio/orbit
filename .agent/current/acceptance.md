---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: passed
task_id: TASK-0009
---

# Current Acceptance Report

## Task

TASK-0009 — M30 I2–I4 and the navigation rail correction.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Navigation rail drives the workspace | pass | `input/dispatch.rs` activates the focused rail row on every screen; `activate_sidebar` keeps the rail focused for Home/Inbox/Agents/Projects; project activation hands focus to the workspace. 4 regression tests (`m30_sidebar_navigation_tests`); real-binary PTY repro: Projects → Home → Inbox → Agents → Alpha Project when driven from the rail. |
| G-0002 | Connected MCP blueprint writes are outbox-backed | pass | `crates/orbit-mcp/src/blueprint_tools.rs` has no `.post(`; every write queues via `queue_blueprint_transport` then flushes and reports `accepted`/`pending`/`conflict` from the outbox row. 8 MCP tests; real server/MCP acceptance covers offline pending, accepted receipt, identical replay, same-id different-body conflict, lost-ack replay with one applied effect, duplicate command refusal, read paths never queued, policy refusal. |
| G-0003 | Previewable, fenced blueprint upgrades | pass | `preview_blueprint_upgrade` diffs kinds/states/fields/transitions/actions/capabilities/views and classifies warnings/blockers with a SHA-256 fingerprint over accepted state; `apply_blueprint_upgrade` requires `--confirm` plus the fingerprint, recomputes inside a savepoint, refuses blockers/mismatch, preserves items, idempotent. 13 runtime tests, 2 CLI tests, 60 local CLI acceptance checks (preview → refusals → apply → restart). No auto-upgrade on publish/install. |
| G-0004 | Habit recurrence owned by an opt-in capability | pass | New `orbit-habit-capability` crate owns `evaluate_habit_periods` and the summary types; `orbit-domain` keeps only occurrence/period types. Runtime registers `orbit.habit.recurrence` without installing/authorizing it; application/evidence reads return explicit `Unavailable` until installed (and granted for project evidence). Package 5 tests, domain 56, application 17, runtime 542; moved tests preserve streak numbers; task-only flow needs no habit capability. |
| G-0005 | Quality gates | pass | `cargo test --workspace --offline` 1,658 passed, 0 failed (31 binaries); clippy `-D warnings` clean across 14 crates; fmt clean; PTY 65 passed/1 skipped; MCP 4; contracts 43; documentation 39; capability workflow 1; local CLI 60; `getdone-validate-project` passed. |

## Commands run

```text
cargo fmt --all -- --check                                   # clean
cargo test --workspace --offline                             # 1,658 passed, 0 failed (31 binaries)
cargo clippy -p orbit-domain -p orbit-application -p orbit-storage -p orbit-runtime \
  -p orbit-sync -p orbit-blueprint -p orbit-client -p orbit-ui -p orbit-tui \
  -p orbit-mcp -p orbit-cli -p orbit-server -p orbit-progress \
  -p orbit-habit-capability --all-targets --offline -- -D warnings   # clean
cargo test -p orbit-runtime --offline blueprint_upgrade       # 13 passed
cargo test -p orbit-mcp --offline                             # 76 passed
cargo test -p orbit-cli --offline                             # 128 passed
cargo test -p orbit-habit-capability -p orbit-domain -p orbit-application --offline
python3 -m pytest -q tests/acceptance/tui tests/acceptance/mcp \
  tests/contracts tests/documentation                          # 151 passed, 1 skipped (plus capability test)
python3 -m pytest -q tests/acceptance/test_m30_capability_workflow.py   # 1 passed
python3 tests/acceptance/local_cli_workflows.py --binary target/debug/orbit  # 60 passed
getdone-validate-project                                      # passed
```

## Checks not run

- Full `./scripts/m15-full-automated.sh --profile full` release profile (unchanged
  release qualification remains M15/M27; the focused suites above were run).
- Connected capability execution and MCP capability tools (not implemented; the
  refusal is explicit).
- Live Herdr/model acceptance (unchanged external requirement).

## Waivers

- None.

## Residual risk

- Connected capability actions remain local-only by design; connected blueprint
  writes are now outbox-backed.
- Habit streaks/evidence are unavailable until the capability is installed and
  (for project evidence) granted; this is intended opt-in behaviour.
- Upgrade blockers intentionally refuse incompatible live-data changes.
