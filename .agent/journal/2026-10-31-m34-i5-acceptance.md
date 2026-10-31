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

# Journal Entry: 2026-10-31 — M34 I5 rehearsal, failure injection and documentation accepted

## Context

I1–I4 delivered the mechanism; I5 had to prove it on realistic data, break it
deliberately, document it honestly and accept it. This entry records the I5 slice
evidence; the final M34 acceptance verdict is in
`2026-10-31-m34-completion.md`.

## Work completed

- `.agent/tasks/TASK-0020.md` moved to `completed`: all acceptance criteria checked
  with I5 evidence.
- A real recovery rehearsal (`web/evidence/m34-recovery/`) ran 14/14 steps on a
  deterministic dataset: 3 projects, 3 milestones, 103 tasks, 10 dependency edges,
  206 acceptance criteria, 6 workflow Work Items (Development + capability-
  reference bundles), 6 Markdown notes with links + 3 typed references, 5
  attachments (including an 8 KiB binary with recorded SHAs), 2 reserved-only agent
  jobs (queued, `runtime_session_ref` null, never launched), 1 capability install +
  grant + real execution receipt (`succeeded`, `work_item_note` effect), 1
  MCP-authored task change accepted via `authority.flush` (`Accepted(1)`), 124
  activity rows.
- The backup was taken from a live WAL server (pid, `journal_mode=wal`, HTTP serving
  recorded): archive `orbit-20261031T103657Z-m34-rehearsal.orbitbak`, 1,495,552
  bytes, SHA-256 `953cb9b22b4ca43be1b45115d111bceb8d64b95247416da8fbeece124c62c874`,
  format v1, schema 20, source mode `server`,
  `excluded_client_pending_data: true`, 13 inventory files.
- After isolated restore and starting the isolated instance: earlier tasks,
  milestones, notes, attachment bytes (SHA match), dependencies, criteria,
  activity and Work Items present; later disposable task and note absent; agent
  jobs and capability receipt readable; no automatic external execution; a
  disposable write accepted; restart persisted. All 64 tables equal before/after
  (0 differing), DB snapshot SHA-256 identical, vault tree hash identical,
  attachment tree hash identical.
- CLI acceptance + failure injection: 22 PASS / 1 NOT_APPLICABLE / 0 defects,
  covering create/list/verify, strict non-mutating preflight, isolated restore,
  replacement gating/preservation/journal, active-target refusal,
  schedule/retention protection, interrupted backup (SIGKILL), corrupt/truncated
  archive, missing attachment, unreadable vault file, permission-denied and
  mode-preserving destinations, format v999, wrong destination/mixed modes,
  interrupted restore, replica-snapshot refusal.
- Documentation updated and tested; EC-012, EC-013, EC-014 and EC-015 checked in
  MILESTONE-0034 and the roadmap.

## Decisions made

- The rehearsal proves data round-trip with hashes and table equality, not just
  "the command exited 0"; the failure matrix proves the original workspace remains
  recoverable.
- The one NA case (pending connected-client outbox as a CLI restore target) is
  recorded as not applicable because the CLI accepts no replica restore target and
  the server/preflight fail-closed blocker is covered by I2 — not counted as a pass.
- External boundaries (Herdr sessions, processes, worktrees/repositories,
  credentials) stay outside the backup/restore boundary; live MCP/Herdr integration
  is unproven and belongs to M35, not to M34 acceptance.

## Files or components changed

- Records: `.agent/tasks/TASK-0020.md`, `.agent/milestones/MILESTONE-0034.md`,
  `.agent/roadmap.md`, this journal entry.
- Evidence: `web/evidence/m34-recovery/README.md`, `rehearsal-evidence.json`,
  `failure-injection.json`.
- Documentation: backup/recovery and upgrades guides, local web quickstart,
  self-hosting, CLI reference, Web GUI guide, architecture and format reference.

## Validation performed

- `cargo test --workspace --locked --offline`: 2032 passed, 0 failed; fmt/clippy
  clean.
- `orbit-backup` 126 passed / 12 targets; server 95; CLI 164.
- Web: tsc clean, Vitest 77 files / 490 tests, terminology 286 files, production
  build clean; Playwright `backup-restore` 4/4; full Chromium 156 passed / 6
  environment-gated skips / 0 failed.
- Documentation: 40 tests OK (unittest) and 40 passed (pytest).

## Problems encountered

- Sanboxed `/tmp` fixture seeding broke the first full Chromium attempt; a
  workspace `TMPDIR` rerun passed, and the same failure reproduced on the pre-M34
  baseline. Recorded as an environment limitation.
- The `orbit update` network flow cannot be exercised offline; EC-011 stays
  helper-level evidence.

## Remaining work

- None for I5. Documented follow-ups TODO-0018..TODO-0023; the maintainer
  review/approval gate is NEXT-0023.

## New TODOs

- TODO-0018, TODO-0019, TODO-0020, TODO-0021, TODO-0022, TODO-0023 (registered
  with the M34 close-out).

## Risks or blockers

- M34 acceptance is internal engineering acceptance only; no public-release claim.
- Live MCP/Herdr integration remains unproven until M35 and must be tested against
  actual processes.
