---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-31"
task_id: TASK-0016
---

# Journal Entry: 2026-10-31 — M34 I1 backup core accepted

## Context

MILESTONE-0034 became active after M33 was accepted for internal dogfooding. I1
(TASK-0016) was the first slice: one shared Rust backup service, the versioned
`.orbitbak` archive and manifest, a fenced consistent snapshot, failure-safe
creation and the CLI `orbit backup create|list|inspect|verify` surface. I2–I5 are
landing in parallel, so this entry records only I1 and keeps M34 active.

## Work completed

- `.agent/tasks/TASK-0016.md` moved to `completed`: all eight acceptance criteria
  checked, with the exact test targets, CLI end-to-end runs, verification-failure
  semantics and the resolved destination-permission defect recorded as validation
  evidence, plus an Outcome section.
- `.agent/current/task.md` is TASK-0016 `completed` (all criteria checked) with the
  implemented current behaviour and the I1 validation commands.
- `.agent/current/acceptance.md` is `passed` for TASK-0016: M33 historical rows are
  retained, and new gates G-0015..G-0020 record the backup crate tests, CLI
  create/list/inspect/verify, failure-safe creation and verification failures, the
  format documentation, the destination-permission fix and the independent
  live-server smoke.
- `.agent/current/evidence.md` is `final` for TASK-0016: every M33 and TASK-0016
  criterion row passes with evidence, and the quality-gate table records the
  backup crate, CLI build/check/format, CLI E2E, failure paths, verification
  semantics, live-server smoke and the regression test.
- `.agent/milestones/MILESTONE-0034.md`: EC-001, EC-002, EC-003, EC-004 and EC-008
  are checked with concrete evidence; EC-010 records partial scheduler smoke
  evidence and stays unchecked; remaining work now names exactly which criteria are
  met and which are still owned by I2–I5. Status remains `active`.
- `.agent/roadmap.md` M34 section: the same five criteria are checked with compact
  evidence rows, EC-010 records the partial scheduler smoke, and the milestone
  stays `active` as the current milestone.
- `.agent/plans/PLAN-0002.md`: SLICE-001 is `completed`; SLICE-002 (I2 restore) is
  `active`.
- `.agent/current/next-step.md` is NEXT-0022 (I2 restore preflight and isolated
  restore acceptance; advances EC-005 and EC-006).
- `.agent/status/project-status.md`: M34 I1 recorded as recently completed, I2–I5
  as in progress, project health updated with the I1 test/CLI gates, restore risk
  narrowed to the not-yet-accepted I2, and the next deterministic step updated to
  NEXT-0022.

## Decisions made

- Only I1 is marked accepted. Restore, web management, automatic lifecycle and the
  recovery rehearsal remain unclaimed until their own evidence arrives; EC-010 is
  explicitly partial because only a same-day scheduler smoke was recorded, not the
  retention/pre-upgrade evidence.
- The current task/acceptance/evidence records stay bound to TASK-0016 because I1
  is the only bounded task with accepted evidence; the next step (NEXT-0022)
  carries the continuation into I2 without claiming it.
- Verification semantics are recorded as part of the contract: `verify_archive`
  returns `Ok` with `ok == false` for content/manifest/hash/SQLite/schema failures
  (including bad magic and truncation), `Err` only when the file cannot be opened,
  and `file_hashes` is `skipped` (not `pass`) when the structure check fails first.
- The destination-permission defect is recorded as a resolved acceptance defect,
  not hidden: the fix tightens permissions only when it creates a directory and the
  regression test plus the `0500` end-to-end run cover it.

## Files or components changed

- `.agent/tasks/TASK-0016.md`, `.agent/current/task.md`,
  `.agent/current/acceptance.md`, `.agent/current/evidence.md`,
  `.agent/current/next-step.md`.
- `.agent/milestones/MILESTONE-0034.md`, `.agent/roadmap.md`,
  `.agent/plans/PLAN-0002.md`, `.agent/status/project-status.md`.
- This journal entry.
- No source, `docs/` or `web/` file was changed by this records update.

## Validation performed

- Independently re-ran `cargo test -p orbit-backup --offline`: the suite exited 0
  and the tail showed `verify_integrity` 9 passed / 0 failed; the Lead-recorded
  totals are 118 passed / 0 failed across 11 test targets (80 backup-core tests: 49
  lib + `create_verify` 13 + `verify_integrity` 9 + `inspect_list` 9).
- Lead-recorded acceptance: `cargo check -p orbit-cli --offline` pass, `cargo build
  --offline -p orbit-cli` pass, `cargo fmt --all -- --check` clean; CLI E2E create
  (default/`--output`/`--label`/`--json`) producing a verified archive and sidecar;
  `list` on default/external directories with `unknown` for a copied archive;
  `inspect` manifest/totals/counts/inventory; `verify` deep 8/8 and
  `--structural-only`; tampered archive exit `1`; permission-denied destination
  exit `4` with the directory mode unchanged; live-server WAL smoke and
  restore/replace smoke.
- `getdone-validate-records --project-root /home/xliu/dev/orbit --skills-root
  /home/xliu/dev/getdone` and `getdone-validate-project --project-root
  /home/xliu/dev/orbit --skills-root /home/xliu/dev/getdone` were run after this
  record update; result recorded in the final validation note.

## Problems encountered

- A real acceptance run found that `util::create_private_dir` chmod-ed an existing
  destination directory (`0500` → `0700`), which let a backup that should have been
  denied succeed. Fixed so permissions are tightened only when the directory is
  created; regression test `existing_directory_permissions_are_never_rewritten`
  and the `0500` destination E2E check cover it.
- `verify_archive` needed explicit semantics: content failures must stay
  `Ok(report.ok = false)` and only an unopenable file is `Err`; `file_hashes` must
  report `skipped` when structure fails first. Both are now recorded so later
  slices cannot reinterpret them.
- `cargo test -p orbit-backup` includes restore/retention targets that are being
  written in parallel; the totals are reported with the backup-core breakdown so
  I1 does not claim teammates' in-progress work.

## Remaining work

- I2–I5 (TASK-0017–TASK-0020): restore acceptance (preflight/isolated accepted
  next; replacement/rollback later), Web GUI Backup & Restore, automatic schedules
  and the complete pre-upgrade recovery point (EC-010/EC-011), MCP/Herdr coverage,
  failure injection, recovery rehearsal and the final documentation pass.
- EC-005..EC-007, EC-009, EC-011..EC-015 remain unmet; EC-010 is partial.

## New TODOs

- None.

## Risks or blockers

- Verification is not restoration; until I2 is accepted the manual offline
  procedure remains the only recovery path.
- The full failure-injection matrix (disk-full, interrupted restore, missing
  external dependency, pending connected-client outbox) is still I5 work.
- Connected-client outbox/staged uploads and external Herdr/git/credential
  boundaries are not part of a workspace archive, and a same-disk backup does not
  protect against disk failure.
