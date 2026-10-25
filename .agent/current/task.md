---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0009
status: completed
milestone_id: MILESTONE-0030
---

# Task: M30 I2–I4 and the navigation rail correction

## Objective

Fix the reported navigation defect — whatever is selected in the sidebar must
drive the workspace — and finish the remaining instruction scope: connected MCP
blueprint outbox parity (M25 EC-004), previewable blueprint upgrades (EC-005) and
habit recurrence ownership by an opt-in capability package (EC-006).

## Current behaviour

After M30 I1 the rail looked unresponsive: on the Projects screen `Enter` ignored
the focused sidebar row and always opened the workspace's selected project row,
and activating a top-level destination dropped focus into the workspace so
further `j`/`k` did nothing. M25 EC-004/005/006 were open: connected MCP blueprint
writes posted directly to the authority and bypassed the replica outbox; binding
was first-binding-wins with no upgrade preview; habit recurrence/streak math
lived in `orbit-domain`.

## Desired behaviour

`Enter` activates the focused rail row on every screen; top-level rail
destinations keep the rail focused while project activation hands the keyboard to
the workspace. Connected blueprint writes are queued durably before any flush and
report accepted/pending/conflict with immutable receipts and lost-ack replay.
Blueprint upgrades are previewed with a typed diff, warnings/blockers and a
fingerprint, and only applied with explicit confirmation; nothing upgrades
automatically. Habit recurrence is computed by the opt-in
`orbit.habit.recurrence` capability, not core, with explicit unavailable status
when it is not installed.

## Scope

- Rail `Enter` precedence and focus behaviour, regression tests, guide wording.
- Queue-first connected MCP blueprint writes, dispositions, receipts, offline
  replay and lost-ack recovery; no direct connected write bypass.
- `preview_blueprint_upgrade`/`apply_blueprint_upgrade` with fingerprint fencing,
  CLI `upgrade-preview|upgrade`, connected refusal, tests and docs.
- `orbit-habit-capability` package, runtime registration/executor, install and
  project-grant gating, domain/application extraction, docs.
- Records, evidence, documentation and full acceptance.

## Out of scope

- Connected capability execution and MCP capability tools.
- New top-level TUI screens or a second persistence model.
- M27 public release publication and deployment verification.
- Per-device project ACLs and server attachment GC.

## Dependencies

- M30 I1 capability composition, storage grants/receipts and TUI shell.
- Existing replica outbox (`AuthorityConnection::queue_blueprint_transport`,
  `flush`) and blueprint command receipts.
- Existing habit occurrence storage and bundled habits workflow.

## Risks

- A queue-first write that reports accepted without confirmation would violate
  the pending-vs-accepted rule; dispositions are read from the outbox row and an
  unobservable outcome is pending.
- Upgrade previews can be stale; apply recomputes inside a savepoint and refuses
  fingerprint mismatches or blockers.
- Moving recurrence out of core could silently change streak semantics; the
  algorithm is moved verbatim and covered by the moved tests plus gating tests.

## Acceptance criteria

- [x] AC-001: `Enter` activates the focused rail row on every screen; top-level
  destinations keep rail focus; project activation moves to the workspace.
- [x] AC-002: Connected MCP blueprint writes queue before flushing and report
  `accepted`/`pending`/`conflict`; offline replay and lost-ack replay return the
  original receipt exactly once.
- [x] AC-003: Blueprint upgrades are previewable with typed diffs, warnings and
  blockers plus a fingerprint; apply requires `--confirm` and the exact
  fingerprint, preserves items and is idempotent.
- [x] AC-004: Habit recurrence/streak math lives in `orbit-habit-capability` and
  is gated by installation (and project grant for evidence); core domain no
  longer computes it; existing habit data and non-recurrence flows are unchanged.
- [x] AC-005: Workspace, clippy, fmt, PTY, MCP, docs, contracts and CLI acceptance
  pass; M25 EC-001..EC-007 are satisfied with evidence.

## Validation

```sh
cargo fmt --all -- --check
cargo test --workspace --offline          # 1,658 passed, 31 binaries
cargo clippy -p orbit-domain -p orbit-application -p orbit-storage \
  -p orbit-runtime -p orbit-sync -p orbit-blueprint -p orbit-client -p orbit-ui \
  -p orbit-tui -p orbit-mcp -p orbit-cli -p orbit-server -p orbit-progress \
  -p orbit-habit-capability --all-targets -- -D warnings
cargo test -p orbit-habit-capability -p orbit-runtime -p orbit-mcp -p orbit-cli --offline
python3 -m pytest -q tests/acceptance/tui tests/acceptance/mcp tests/contracts tests/documentation
python3 tests/acceptance/local_cli_workflows.py --binary target/debug/orbit
python3 -m pytest -q tests/acceptance/test_m30_capability_workflow.py
getdone-validate-project
```
