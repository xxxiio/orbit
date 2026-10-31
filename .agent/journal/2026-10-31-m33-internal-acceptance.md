---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-31"
task_id: TASK-0015
---

# Journal Entry: 2026-10-31 — M33 internal-dogfooding acceptance and the M34 records

## Context

M33 (Professional Web UX, Milestone-Based Planning & Unified Project Work) had
every automated gate green and the corrected Milestone model, unified Tasks
workspace, compact Board, Milestone Roadmap and opt-in local-trusted authentication
in place. The only open items were the human review (EC-010) and its dependent
reporting state. The project maintainer reviewed the latest build and said it is
"generally good enough to begin internal use".

That statement closes M33 as **internal dogfooding acceptance**. It is explicitly
not public-release certification. Because Orbit is about to become the authoritative
workspace for its own development — with MCP and Herdr initiating real agent work —
the next milestone must make backup and restore reliable, so this iteration also
records MILESTONE-0034 (Reliable Workspace Backup & Restore) and its five iteration
slices. This was a records-only iteration: no source code, `docs/` or `web/` file
was modified.

## Work completed

- Recorded the maintainer's review as the human acceptance for M33 EC-010 on
  2026-10-31, with the explicit caveat that it is internal-use acceptance, not
  public-release certification. EC-010 and EC-011 now pass and MILESTONE-0033 is
  `completed`; its corrective-history notice and all automated evidence are
  preserved.
- Archived the completed TASK-0015 record at `.agent/tasks/TASK-0015.md` with every
  acceptance criterion checked, the recorded validation evidence and an Outcome
  section describing the internal-use acceptance. The next milestone pointer is
  MILESTONE-0034.
- Closed out M33 in `.agent/current/acceptance.md` and `.agent/current/evidence.md`:
  G-0010 and EC-010 are now `pass` with the internal-use note; all automated M33
  evidence is retained unchanged; the records also state honestly that no
  accessibility, cross-browser, public security or release-readiness review was
  performed (report status `incomplete` because the new M34 gates are `not-run`).
- Created MILESTONE-0034 with the milestone-plan contract: intended outcome, why
  now, scope, non-goals, deliverables, dependencies, risks, the I1–I5 sequence and
  the exact EC-001..EC-015 exit criteria, all unchecked with planned evidence.
- Created iteration task records TASK-0016 (I1 backup core, active), TASK-0017 (I2
  restore engine, proposed), TASK-0018 (I3 web management, proposed), TASK-0019 (I4
  automatic backups and lifecycle, proposed) and TASK-0020 (I5 coverage, rehearsal,
  documentation and acceptance, proposed). Each has its own specification, ordered
  plan, unchecked criteria and planned evidence; none claims implementation.
- Created PLAN-0002 with SLICE-001..SLICE-005 mapped to I1–I5, explicit
  dependencies, validation tiers, must-not-change lists, SC criteria and the first
  deterministic step.
- Created NEXT-0021 for the I1 first step and switched `.agent/current/task.md` to
  TASK-0016; refreshed `.agent/current/change-impact.md` for the M34 dimensions
  (public API, configuration, security boundary, concurrency, performance, UI and
  deployment).
- Updated `.agent/roadmap.md`: MILESTONE-0033 `completed` with EC-010 checked and
  the internal-acceptance note, the M34 milestone section added with all fifteen
  unchecked criteria and planned evidence, `current_milestone: MILESTONE-0034`.
- Updated `.agent/status/project-status.md` (M33 complete, M34 current, I1 in
  progress, honest recovery risk) and added TODO-0015, TODO-0016 and TODO-0017 for
  the minor M33 follow-ups without reopening any accepted feature.

## Decisions made

- The human review is recorded as **internal dogfooding acceptance**, not as
  release approval. The project remains pre-public-release; no accessibility,
  cross-browser, security-review or release-qualification claim is made from a
  maintainer's judgement that the app is good enough for internal use.
- M33 completion does not reopen the accepted typography, Milestone, Epic-removal,
  unified Tasks, compact Board or authentication work. The residual items are
  ordinary backlog TODOs with explicit do-not-reopen acceptance conditions.
- M34 uses one shared Rust backup/restore service for both CLI and server, and the
  Web GUI reuses it; no second archive implementation and no arbitrary
  browser-supplied server paths.
- Restore is destructive by nature: the records require explicit authorization, a
  maintenance boundary, staged verification, rollback preservation and fail-closed
  connected-replica behaviour. Recovery never automatically re-executes external
  agent or capability actions.
- Current-record binding: `.agent/current/task.md` now points at TASK-0016, so the
  current acceptance/evidence singleton records were rebound to TASK-0016
  (`incomplete`) and keep the completed M33 acceptance inside their retained gate
  tables. TASK-0015's durable record was archived under `.agent/tasks/` so the
  completed task is not lost when the singleton moves on. This avoids a
  task-bound-record validation error while keeping the M33 evidence visible.

## Files or components changed

- `.agent/milestones/MILESTONE-0033.md` (completed, EC-010/EC-011 checked, next
  milestone MILESTONE-0034).
- `.agent/milestones/MILESTONE-0034.md` (new).
- `.agent/tasks/TASK-0015.md` (new archived completion record), `TASK-0016.md`,
  `TASK-0017.md`, `TASK-0018.md`, `TASK-0019.md`, `TASK-0020.md` (new).
- `.agent/plans/PLAN-0002.md` (new).
- `.agent/current/task.md`, `next-step.md`, `acceptance.md`, `evidence.md`,
  `change-impact.md`.
- `.agent/roadmap.md`, `.agent/status/project-status.md`,
  `.agent/tracking/todos.md`.
- This journal entry.

No source code, `docs/` or `web/` file was changed.

## Validation performed

- `getdone-validate-records --project-root /home/xliu/dev/orbit --skills-root
  /home/xliu/dev/getdone`: `record validation passed` (exit 0).
- `getdone-validate-project --project-root /home/xliu/dev/orbit --skills-root
  /home/xliu/dev/getdone`: `validation passed: 27 managed file(s), 14 warning(s);
  composition=498a1f868912; overlays=none` (exit 0). The 14 warnings are the
  expected project-owned template-digest divergences.
- Cross-record checks confirmed: the active roadmap milestone equals
  `current_milestone` (MILESTONE-0034), TASK-0016's milestone matches it, NEXT-0021
  advances criteria defined by MILESTONE-0034, and the current acceptance/evidence
  records are bound to TASK-0016.
- Not run: no Rust, web or documentation gates were needed because no source or
  documentation file changed. M34 gates remain `not-run`.

## Problems encountered

- The M33 milestone plan and roadmap still had EC-010 and EC-011 unchecked, even
  though the authentication work and its evidence were recorded and only the human
  gate remained. Both were checked as part of the acceptance close-out; the
  corrective-history notice was preserved.
- The `.agent/current/` acceptance and evidence records are task-bound, while the
  iteration also had to move the current task to TASK-0016. Rebinding them to
  TASK-0016 and archiving TASK-0015 was the only way to keep both the M33 evidence
  visible and the records structurally valid; the trade-off is stated in the
  decision above.
- No human review note existed in the repository beyond the maintainer's statement;
  the acceptance is recorded from the direct statement, with its limits stated.

## Remaining work

- MILESTONE-0034 I1–I5 (TASK-0016–TASK-0020) are unstarted or in progress; EC-001
  through EC-015 are all unmet and no backup/restore implementation is claimed.
- M34 documentation, failure injection and real recovery rehearsal are I5 work.
- M27 release publication remains blocked externally; TUI redesign and the recorded
  web follow-ups remain post-M34 candidates.

## New TODOs

- TODO-0015 — Local-trusted per-device agent-job visibility (P2, ready,
  MILESTONE-0033).
- TODO-0016 — Explicit read-only `/mcp` boundary in local-trusted mode (P2, ready,
  MILESTONE-0033).
- TODO-0017 — Residual M33 visual/comfort polish (P3, ready, MILESTONE-0033).

## Risks or blockers

- M33 is accepted for internal use only; public-release certification remains
  absent and must not be inferred from this acceptance.
- Until M34 lands, the only recovery procedure is the manual SQLite snapshot plus
  vault/attachment/config copy, and a same-disk backup is not protection against
  disk failure.
- Connected-client outbox commands and staged uploads are a documented server-backup
  exclusion; restore must reconcile and fail closed rather than re-execute external
  actions.
