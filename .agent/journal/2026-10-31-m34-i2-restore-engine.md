---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-31"
task_id: TASK-0017
---

# Journal Entry: 2026-10-31 — M34 I2 restore engine accepted

## Context

I1 produced verified `.orbitbak` archives but no way back. I2 was the restore
slice: a strictly non-mutating preflight, an isolated-restore drill, an explicit
maintenance replacement path with rollback preservation, and fail-closed behaviour
for active targets, incompatible schemas and connected-replica state. I1 and the
restore engine were developed in parallel, so acceptance is recorded only now, on
the frozen tree.

## Work completed

- `.agent/tasks/TASK-0017.md` moved to `completed`: all acceptance criteria checked
  with the I2 evidence.
- Restore preflight reports target/source/schema/replaced/preserved objects,
  required downtime, connected-client warnings and non-restorable external
  dependencies, and mutates nothing. CLI acceptance proved strict non-mutation with
  a recursive file list/hash comparison and no destination created.
- Isolated restore stages, verifies and opens as a working Orbit instance via the
  CLI with the expected tasks/notes; the active workspace is untouched.
- Replacement requires `--confirm`, stages and verifies before switching, preserves
  the previous workspace and a recovery journal that reopens, refuses an active
  target with exit `4`, and leaves no usable partial target after an interrupted
  restore.
- Connected-mode fail-closed behaviour is covered, and client replica snapshots are
  refused with the actionable server-backup guidance from both local and connected
  deployments.
- EC-005, EC-006 and EC-007 were checked in MILESTONE-0034 and the roadmap with the
  recorded evidence; `restore_preflight` 13, `restore_replace` 7,
  `restore_isolated` 6 and `restore_rollback` 4 tests are part of the 126-test
  `orbit-backup` suite.

## Decisions made

- Restore always starts with an isolated copy; replacement is a maintenance
  operation, never a live overwrite, and always preserves the previous workspace
  plus a journal for rollback.
- Replica snapshots are refused instead of guessed: a replica is not an authority,
  and an older authority archive restored over newer client state is the dangerous
  case the build must fail closed on.
- A failed or interrupted restore must leave no usable partial target; the original
  workspace is the recovery guarantee, not a retried guess.

## Files or components changed

- Records: `.agent/tasks/TASK-0017.md`, `.agent/milestones/MILESTONE-0034.md`,
  `.agent/roadmap.md`, this journal entry.
- Implementation (by the restore slice): `orbit-backup` restore modules, CLI
  restore commands, server restore routes and the shared `RestoreService`.

## Validation performed

- `cargo test -p orbit-backup --offline`: 126 passed / 12 targets, including the
  four restore targets above.
- CLI acceptance and failure injection (`web/evidence/m34-recovery/failure-injection.json`):
  strict non-mutating preflight, isolated restore opens, replacement gating +
  preserved copy + journal, active-target refusal (exit 4), interrupted restore
  with follow-up success and original intact, wrong destination/mixed modes,
  replica-snapshot refusal; 22 PASS / 1 NA / 0 defects overall.
- Rehearsal (`web/evidence/m34-recovery/`, 14/14): isolated restore opened as a
  working instance with earlier data present, later disposable changes absent,
  no automatic external execution, disposable write + restart persistence.
- `cargo test --workspace --locked --offline`: 2032 passed, 0 failed.

## Problems encountered

- The full-suite sandboxed `/tmp` fixture-seeding limitation (recorded separately)
  affected browser acceptance, not the restore engine.
- The pending connected-client outbox case is the one NOT_APPLICABLE failure-
  injection case: the CLI accepts no replica restore target, and the
  server/preflight fail-closed blocker is covered by I2 tests. Recorded honestly
  rather than counted as a pass.

## Remaining work

- None for I2. Documented follow-ups: replica support (TODO-0019), offline server
  replacement activation (TODO-0021), preflight free-space measurement
  (TODO-0022), restored-DB re-enrollment HTTP 500 (TODO-0018).

## New TODOs

- TODO-0018, TODO-0019, TODO-0021, TODO-0022 (registered with the M34 close-out).

## Risks or blockers

- Restoring an older authority snapshot over newer accepted history remains a
  disaster-recovery decision, not an automatic merge.
- External Herdr sessions, processes, git worktrees and credentials are outside the
  restore boundary.
