# M32 I5 — dogfooding evidence

Recorded from an **automated interactive dogfooding** session — a scripted
Playwright session driving the real browser UI, not a person. That label is
deliberate and appears in the deliverable.

## What was run

- Persistent workspace: isolated `orbit-server` on `127.0.0.1:8781`,
  `ORBIT_SERVER_DB=/tmp/opencode/orbit-dogfood/data/server.db`, credentials in
  `workspace.json`. **Nine** sessions ran against that one database; run 9 resumed
  run 8 (run 7's completed task still Done, its note still in the vault, each run
  appending its own work — 140 tasks, 8 saved views, `df-run-2..8` notes, 5
  capability requests).
- Seeding split stated honestly: the 122-task fixture plus capability
  install/grant/announce through the public/operator API **once**; every session's
  real work through the browser.
- 23 steps per session: quick capture, full New-task dialog, unified search,
  priority/status plus a saved view with reload/restore, day planning, description
  and acceptance criteria, a blocker cycle, note creation with a note link and
  project/task references, agent job queue → claim → handoff → browser review, a
  capability request executed by the real `orbit capability serve`, milestone
  assignment and completion, project switch, and a mid-session reload.
- Artefacts: `.slim/deepwork/m32-web-completeness/i5-dogfood/` — `session.md`
  (environment, step log, defect table, persistence proof, human-operator gaps),
  23 screenshots with `captions.md`, `steps-run-1..9.json`, and a trace.

## Defects and disposition

| id | symptom | layer | severity | state |
|---|---|---|---|---|
| D-01 | **Review refused the plan the server itself proposed** (`approved schedule failed domain validation`), so Apply was unreachable — reproduced in five sessions | planner/runtime | high | **fixed** |
| D-02 | Typing the next acceptance criterion while the previous save was in flight silently lost the text | web client | medium | **fixed** |
| D-03 | A settled capability request still read available; a second request was an unexplained 409 | web client (copy/control) | medium | **fixed** |
| D-07 | Planning failure printed an internal sentence, naming neither task nor next step | web client | medium | **fixed** |
| D-04 | Enter swallowed while a criterion save is pending (draft survives) | web client | low | open, recorded |
| D-05 | Capacity + Enter does not build the day plan; a second click is needed | web client | low | open, recorded |
| D-06/06b | Row middle hits the inline project select; Blocked is not offered from In progress with no explanation | web client | low | open, recorded |

### D-01, the blocker

The planner proposed blocks with deadlines already in the past, while approval
validation rejected the resulting `AfterDeadline`. Fixed by one shared rule
(`orbit_planner::deadline_bound_at`) used by the planner's own proposer and
validator, `schedule_planning`, `portfolio_schedule` and `planning_approval`: a
deadline at or after the window start still binds; a deadline before it cannot be
met by any block the window can hold, so it stops constraining the block end and
the slot says why. A deadline still ahead that the window cannot satisfy is now
reported as unplaced **with the task and the reason** at proposal time instead of
being proposed and then refused. `AfterDeadline` was not weakened for any deadline
that is still ahead.

Proof the fix lands where it matters: the dogfooding session that found D-01 was
re-run on the same persistent workspace and **step 06b now passes** — "1 blocks
reviewed and applied in 170 ms" — with step 06c confirming the same flow on a
project without overdue tasks.

### D-02 (silent data loss)

The acceptance-criterion input was cleared unconditionally when a save completed,
wiping a newer draft. It now clears only if it still holds the submitted text.
Regression: `web/tests/unit/features/criteria-section.spec.tsx` (deferred first
add + second draft), verified red on the old line.

### D-03 and D-07

The capability panel renders the recorded request for `(action, item revision)` —
state and a link to the history — instead of a request control the server would
refuse with 409, and explains that one action is requested once per revision.
Planning failures now name the task when the server's sentence names one, keep the
server's sentence as a secondary line, and always state what to change; when the
server does not name a task the copy says so rather than guessing.

## What still needs a human operator

Visual and copy judgement; screen-reader and keyboard-only walking; a real
Codex/Herdr runtime (claim/launch/handoff were posted through the documented machine
routes); operator install/grant decisions; whether an overdue task *should* be
planned; real network latency (nothing crossed 1 s on loopback); destructive and
bulk operations.
