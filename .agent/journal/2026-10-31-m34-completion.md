---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-31"
task_id: TASK-0020
---

# Journal Entry: 2026-10-31 — M34 final acceptance and completion

## Context

MILESTONE-0034 (Reliable Workspace Backup & Restore) is complete on the frozen
tree: commit `f82e3fa13d4f1fe4ca216aeed105e6efdd5ad250` on branch `main`, dirty
with the M34 changes; binaries `target/debug/orbit` SHA-256
`ad31c0360b1dc16ee946caf996ae2f88bcd6a967245d0035f332f267b5a83c80`,
`orbit-server` `9bbf52de263fc1cbcc149bddf8f082df0f54954a89c4d86bfd96f90bb0db92f8`
and `orbit-mcp` `3c805e95202ad2ff84637d8483e5d78646081bcd3c29ec31189eed5c37d15104`
(build mtimes 2026-10-31T10:06:40/41Z). This entry records the final verdict, the
EC-to-evidence map and the honest acceptance boundary.

## Work completed

- MILESTONE-0034: status `completed`; EC-001..EC-015 all checked with evidence;
  remaining work records only documented limitations; next milestone
  MILESTONE-0035.
- TASK-0017 (I2 restore), TASK-0018 (I3 web), TASK-0019 (I4 lifecycle) and
  TASK-0020 (I5 rehearsal/acceptance) are `completed` with their evidence;
  TASK-0016 (I1) gate numbers were refreshed to the final suite counts.
- PLAN-0002 is `completed`: SLICE-001..SLICE-005 completed and all SC criteria
  checked. The first deterministic step (I1) was delivered and accepted.
- `.agent/current/task.md` is TASK-0020 `completed`;
  `.agent/current/acceptance.md` is `passed` with gates G-0001..G-0018 and the
  EC-001..EC-015 rows; `.agent/current/evidence.md` is `final` with the same EC
  map and the quality-gate table; `.agent/current/change-impact.md` is bound to
  TASK-0020/M34.
- `.agent/current/next-step.md` is NEXT-0023 (ready, MILESTONE-0035): provide the
  maintainer a bounded walkthrough and review of the M34 evidence and obtain an
  explicit approval/rejection before any M35 implementation.
- `.agent/status/project-status.md` reflects M34 complete, no active milestone,
  M35 proposed and gated; `.agent/tracking/todos.md` gained TODO-0018..TODO-0023.
- Created `.agent/milestones/MILESTONE-0035.md` and added the M35 section to
  `.agent/roadmap.md` (proposed, `current_milestone: null`); roadmap M34 is
  `completed` with evidence and next milestone MILESTONE-0035.
- Per-iteration journals recorded for I2, I3, I4 and I5; this entry is the final
  M34 acceptance record.

## Decisions made

- M34 is accepted as **internal engineering acceptance on a frozen tree**. No
  public-release certification, accessibility/cross-browser certification or
  general live MCP/Herdr support is claimed.
- The connected-mode/external-agent boundary is explicit: server-mode backups
  exclude disconnected-client pending data, replica snapshots are refused as a
  recovery point, server replacement is an offline maintenance operation, browser
  downloads are memory-buffered and capped, preflight free-space is unknown, and
  external Herdr sessions/processes/git worktrees/repositories/credentials are
  never restored or re-executed.
- The full `orbit update` network flow was not run offline; EC-011 is accepted on
  helper-level evidence and the limitation is recorded rather than hidden.
- The sandboxed default-`/tmp` Chromium fixture-seeding failure is an environment
  limitation (reproduced on the pre-M34 baseline), not an M34 defect; the rerun
  with a workspace `TMPDIR` passed.
- M34's completion does not start M35. MILESTONE-0035 is proposed and begins only
  after the maintainer reviews the evidence and explicitly approves (NEXT-0023).
- No M33 item was reopened; no source, `docs/` or `web/` file was modified by this
  records update.

## Files or components changed

- `.agent/milestones/MILESTONE-0034.md`, `.agent/milestones/MILESTONE-0035.md`.
- `.agent/tasks/TASK-0016.md`, `TASK-0017.md`, `TASK-0018.md`, `TASK-0019.md`,
  `TASK-0020.md`.
- `.agent/plans/PLAN-0002.md`.
- `.agent/current/task.md`, `acceptance.md`, `evidence.md`, `next-step.md`,
  `change-impact.md`.
- `.agent/status/project-status.md`, `.agent/roadmap.md`,
  `.agent/tracking/todos.md`.
- `.agent/journal/2026-10-31-m34-i2-restore-engine.md`,
  `2026-10-31-m34-i3-web-backup.md`, `2026-10-31-m34-i4-lifecycle.md`,
  `2026-10-31-m34-i5-acceptance.md`, this completion entry.

## Validation performed

- `getdone-validate-records --project-root /home/xliu/dev/orbit --skills-root
  /home/xliu/dev/getdone`: `record validation passed`.
- `getdone-validate-project --project-root /home/xliu/dev/orbit --skills-root
  /home/xliu/dev/getdone`: `validation passed: 27 managed file(s), 14 warning(s);
  composition=498a1f868912; overlays=none`. The warnings are the expected
  project-owned template-digest divergences.
- Record-contract checks confirmed: roadmap `current_milestone` is `null` with no
  active milestone; M34 is `completed` with all EC-001..EC-015 checked and an
  evidence row each; M35 is `proposed` with its ECs; TASK-0017..TASK-0020 are
  completed with all criteria checked; TASK-0020 is the current completed task with
  a passed acceptance and final evidence manifest; NEXT-0023 references
  MILESTONE-0035 EC-001.
- M34 gate evidence (executed, frozen tree): `cargo fmt --all -- --check` clean;
  `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`
  clean; `cargo test --workspace --locked --offline` 2032 passed / 0 failed;
  `cargo test -p orbit-backup --offline` 126 passed / 12 targets; server 95; CLI
  164; documentation 40/40 (both runners); web tsc clean, Vitest 77 files / 490
  tests, terminology 286 files, production build clean; Playwright
  `backup-restore` 4/4 and full Chromium 156 passed / 6 environment-gated skips /
  0 failed.
- I5 evidence: rehearsal 14/14 PASS with all 64 tables equal before/after and
  identical DB/vault/attachment hashes; CLI failure injection 22 PASS / 1
  NOT_APPLICABLE / 0 defects.

## Problems encountered

- The sandboxed default `/tmp` breaks child-process SQLite fixture seeding in the
  full Chromium suite; a workspace `TMPDIR` run passed and the same failure
  reproduced on the pre-M34 baseline. Recorded as an environment limitation.
- EC-011 cannot include the real network update flow in this offline environment;
  helper-level evidence is recorded with an explicit limitation.
- One failure-injection case is NOT_APPLICABLE (pending connected-client outbox as
  a CLI restore target); the server/preflight fail-closed blocker is covered by I2
  and the matrix records the reason instead of counting a pass.
- Post-restore device re-enrollment returns HTTP 500 instead of 409/idempotent; a
  P3 follow-up (TODO-0018), recovery unaffected.

## Remaining work

- Maintainer walkthrough and review of the M34 evidence, then explicit
  approval/rejection of MILESTONE-0035 (NEXT-0023). No M35 implementation before
  that decision.
- Documented follow-ups TODO-0018..TODO-0023; M35 (self-dogfooding with MCP,
  Workflows and Herdr) only after approval; M27 remains blocked externally.

## New TODOs

- TODO-0018 — Restored-DB re-enrollment returns HTTP 500 (P3).
- TODO-0019 — Connected client replica snapshot support (P2).
- TODO-0020 — Streaming backup download instead of 512 MiB memory buffer (P3).
- TODO-0021 — Optional live-server replacement activation on restart (P2).
- TODO-0022 — Preflight free-space measurement (P3).
- TODO-0023 — `knowledge_folders` includes internal `.orbit-operations` dirs (P4).

## Risks or blockers

- M34 acceptance is internal engineering acceptance only; not a public-release
  certification.
- Connected-mode and external-agent limitations are documented and deliberate; live
  MCP/Herdr integration is unproven until M35 and must be tested against actual
  processes.
- A same-disk backup is not disk-failure protection; no zero-data-loss or
  zero-downtime claim is made.
