---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0012
status: completed
milestone_id: MILESTONE-0030
advances_exit_criteria: [EC-001, EC-002, EC-003, EC-004, EC-005, EC-006, EC-007, EC-008]
---

# Next Deterministic Step

## Objective

Fix the navigation rail so the selected sidebar entry drives the workspace, then
finish the remaining instruction scope: connected MCP blueprint outbox parity
(M25 EC-004), previewable blueprint upgrades (EC-005) and habit recurrence
ownership by an opt-in capability package (EC-006).

## Why this is next

The rail defect blocked everyday navigation, and the three open M25 criteria were
the only remaining blueprint work that could be completed and verified locally;
M27 is externally blocked.

## Preconditions

- M30 I1 capability composition, receipts and TUI shell.
- Existing replica outbox (`queue_blueprint_transport`, `flush`) and blueprint
  command receipts.
- Existing habit occurrence storage and bundled habits workflow.

## Inputs

- `crates/orbit-tui/src/{input,app}`, `crates/orbit-mcp/src`, `crates/orbit-client/src`,
  `crates/orbit-storage/src/blueprints*`, `crates/orbit-runtime/src`,
  `crates/orbit-cli/src/blueprint_commands.rs`, `crates/orbit-domain/src/habit_tracking.rs`.
- `tests/acceptance/{tui,mcp,local_cli_workflows.py}` and `docs/**`.

## Ordered actions

1. Fix `Enter` precedence and rail focus; add regression and PTY coverage.
2. Route connected MCP blueprint writes through the outbox with honest
   dispositions, receipts, offline replay and lost-ack recovery.
3. Add preview/apply blueprint upgrades with typed diffs, warnings, blockers and
   fingerprint fencing.
4. Extract habit recurrence into `orbit-habit-capability`, register the
   capability and gate streak/evidence reads on installation.
5. Update docs, records and evidence; re-run workspace/PTY/MCP/CLI acceptance.

## Expected outputs

- A rail that drives the workspace on every screen.
- Outbox-backed connected blueprint writes with immutable receipts.
- Explicit, previewable, fenced blueprint upgrades.
- Habit recurrence owned by an opt-in trusted capability package.
- Passing focused and workspace acceptance, docs and records.

## Acceptance criteria

- [x] NS-001: `Enter` activates the focused rail row on every screen; top-level
  destinations keep rail focus; project activation moves into the workspace.
- [x] NS-002: Connected blueprint writes queue first and report
  accepted/pending/conflict with offline and lost-ack replay.
- [x] NS-003: Upgrades preview typed diffs with warnings/blockers and a
  fingerprint; apply requires `--confirm` plus the exact fingerprint and never
  happens automatically.
- [x] NS-004: Habit recurrence lives in the opt-in capability package and reports
  an explicit unavailable status when not installed/granted.
- [x] NS-005: Workspace, clippy, fmt, PTY, MCP, local CLI, docs and contracts
  suites pass and the records reflect only verified behaviour.

## Validation

```sh
cargo fmt --all -- --check
cargo test --workspace --offline
cargo clippy -p orbit-domain -p orbit-application -p orbit-storage -p orbit-runtime \
  -p orbit-sync -p orbit-blueprint -p orbit-client -p orbit-ui -p orbit-tui \
  -p orbit-mcp -p orbit-cli -p orbit-server -p orbit-progress \
  -p orbit-habit-capability --all-targets -- -D warnings
python3 -m pytest -q tests/acceptance/tui tests/acceptance/mcp tests/contracts tests/documentation
python3 tests/acceptance/local_cli_workflows.py --binary target/debug/orbit
python3 -m pytest -q tests/acceptance/test_m30_capability_workflow.py
getdone-validate-project
```

## Stop conditions

- A blocking authority, data-loss or unauthorized-execution defect remains.
- A criterion cannot be evidenced end to end.

None applied at completion.

## Out of scope

- Connected capability execution and MCP capability tools.
- M27 public release verification.
- Per-device project ACLs and server attachment GC.
