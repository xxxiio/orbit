# I5 — real dogfooding cycle, evidence

This directory is the capture set for **M31 I5**: one real planning-to-completion cycle
on the **Orbit Development** project, driven through the browser UI against an isolated
authoritative server.

## Capture identity

| Item | Value |
| --- | --- |
| Application tree | `fa6d653a6b8a90070b0fc74fc734d5df798faab4` (the clean tree before this lane’s script/evidence were added; no product code changed in I5) |
| Build | `web/dist` built from that tree with `vite build` |
| Server | `target/debug/orbit-server` (authoritative runtime, no mocked API) |
| Port / origin | `127.0.0.1:8790` |
| Database | `/tmp/orbit-i5/vault/db.sqlite3` |
| Knowledge vault | `/tmp/orbit-i5/vault/db.knowledge` (path derived from the DB path by the runtime) |
| Attachment store | `/tmp/orbit-i5/vault/attachments/objects` |
| Enrolled device | `orbit-development` (one active device; see `devices` in the backup) |
| Browser | Chromium 1440×900, light theme, headless |
| Capture window | 2026-10-26 08:41:17–08:42:31 local |
| Script | [`web/tests/e2e/i5/dogfood.mjs`](../../tests/e2e/i5/dogfood.mjs) |
| Machine-readable run | [`run.json`](run.json) |

`run.json` records the project, every task id the script saw, the milestones, the work
item, the note, the planning outcome, the conflict and the board move. The screenshots
are numbered in run order.

## Database backup

The server was stopped cleanly after the cycle. The SQLite file still had a WAL when
the backup was taken, so the **WAL travels with the database**:

| File | Bytes |
| --- | --- |
| [`orbit-i5-db.sqlite3`](orbit-i5-db.sqlite3) | 962,560 |
| [`orbit-i5-db.sqlite3-wal`](orbit-i5-db.sqlite3-wal) | 4,202,432 |

To inspect it, copy both to one directory (keep the names) and open the `.sqlite3` file
read-only; SQLite rebuilds the transient `-shm` index. To get a single-file copy,
checkpoint the copy with `sqlite3 orbit-i5-db.sqlite3 'PRAGMA wal_checkpoint(TRUNCATE)'`
before deleting the WAL. The attachment bytes are **not** in SQLite; the DB row
`attachments.116d5c9b-f1a6-4a4d-bdef-e534c1d4a90f` points at an object in the server’s
object store, and the uploaded source file is committed here as
[`task-a-evidence.md`](task-a-evidence.md).

## What was created through the UI

| Item | Id | Notes |
| --- | --- | --- |
| Project **Orbit Development** | `4939d23d-4f9c-437c-bf21-4aca00321e4f` | Created in the **New project** dialog with the **Development** workflow; status stays `proposed` (see I5-08) |
| 18 project tasks | see `run.json` → `taskIds` | Every one created with the **New task** dialog, then enriched in the drawer |
| 1 triaged task | `22471168-8d11-4060-966b-553bd4e8333e` | Captured in the Inbox, then assigned to the project and set to High priority |
| Milestone **M31 completion** | `41539b10-9828-43d3-a338-47673c311788` | 12 tasks attached; status `planned` |
| Milestone **Reference-client ready** | `d6994d8a-73fd-4218-811c-d2cfaca987db` | 6 tasks attached; status `planned` |
| Feature work item | `1ce5d4bf-d612-48c6-937b-90793d55ab05` | Declared fields only: `priority=medium`; state `backlog`. Its displayed name is “Medium” (I5-03) |
| Knowledge note | `c5e04922-e437-45c3-b72c-d6d6020036c4` | `i5-dogfooding.md`; linked from Task A’s description |
| Attachment | `116d5c9b-f1a6-4a4d-bdef-e534c1d4a90f` | `task-a-evidence.md`, 247 bytes, on Task A |

At the end the database held **19 tasks** (2 done, 1 in progress, 2 ready, 1 planned,
13 inbox), **13 acceptance criteria**, **48 labels**, **5 dependency edges**,
**129 activity entries**, **2 milestones**, **1 work item**, **1 note**, **1 attachment**
and **0 schedule blocks**.

## Task ids that actually moved

| Task | Id | Move |
| --- | --- | --- |
| **Task A** — I5: write the dogfooding findings summary | `dac48ec8-8d23-4e16-aa41-f44747c09ad8` | `inbox → ready` (setup), then **`ready → in_progress → done`** through the drawer. Description edited through the UI (initial, note link and final text), both criteria checked, attachment uploaded, completion visible in Activity; one further description edit came from the deliberate second writer |
| **Task B** — I5: record the board keyboard move evidence | `4e5e93d2-5740-44ff-8a46-13bc2a5465a9` | `inbox → ready` (setup), then **`ready → in_progress → done`** on the Board with `m`, arrow keys and `Enter` (9 `ArrowDown` presses total, no pointer for the move) |
| I5: wire estimate and deadline editing… | `c9965f3a-a7f8-4d74-979d-578637722d4c` | `inbox → ready` |
| I6: refine the surfaces flagged by dogfooding | `3d3bd342-69ac-45b6-9c48-4faa1e74403e` | `inbox → ready` |
| I6: update docs/web-app.md for I3 to I5 | `8248f040-b687-406c-9fc7-e9c5e706318c` | `inbox → planned` |
| I6: review the gate 4 findings… | `cb10cbd3-2c69-458e-bb56-b3dca7999509` | `inbox → planned → in_progress` |
| Triage: capture I5 friction… | `22471168-8d11-4060-966b-553bd4e8333e` | Inbox → project **Orbit Development**, priority **High** (status stayed `inbox`) |

The two completed tasks are the two that survive the server restart in
[`20-after-restart-board-done.png`](20-after-restart-board-done.png).

## The cycle, frame by frame

| Frame | What it proves |
| --- | --- |
| `01-home-empty.png` | Signed in through the real form; Home before any project existed |
| `01b-new-project-no-workflows.png` | On a fresh server the New project dialog offers only the default workflow and says no other workflow is installed. There is no install control (I5-02) |
| `02-new-project-development.png` | Development v2 selected in the dialog; the project is named Orbit Development |
| `03-project-overview.png` | Project created with the Development workflow; status reads “Proposed” (I5-08) |
| `04-new-task-dialog-copy.png` | The dialog promises “a deadline … from the task itself”, which the task surface does not offer (I5-01/I5-09) |
| `05-milestones-created.png` | Both milestones created through the milestones view |
| `06-new-work-item-fields.png` | The Development **Feature** kind declares only Priority. There is no title field (I5-03) |
| `07-work-item-created.png` | The created Feature renders as “Medium”, because a choice field becomes the fallback title (I5-03) |
| `08-knowledge-note.png` | Note created in the vault; the tree header says “1 notes” (I5-14) |
| `09-task-with-note-link.png` | The note is linked from Task A in a rendered description. The link is absolute and opens a new tab; a relative `/knowledge/...` link is not rendered (I5-04) |
| `10-home-with-project.png` | Home after the project has ready work |
| `11-inbox-before-triage.png` | One unassigned task in the Inbox |
| `11b-invalid-status-refused.png` | The drawer’s status control offers `In progress` from `Inbox`; the server refuses it (`invalid task status transition`) (I5-11) |
| `12-inbox-after-triage.png` | The task left the Inbox after project + priority were set inline |
| `13-planning-day-plan.png` | Authored capacity 240 min; the server returns `missing_estimate` dispositions and warns that actionable tasks have no duration estimate |
| `14-planning-window.png` | A custom 13:00–16:00 window is chosen; the server proposes 0 slots and Review is disabled. The time inputs also clip the hour in this locale (I5-07/I5-10) |
| `15-task-a-opened.png` | Task A opened from the day plan. Details has status, priority, project, milestone, labels — **no estimate, earliest start or deadline** (I5-01) |
| `16-task-a-description-criteria.png` | Description edited and both acceptance criteria checked |
| `17-task-a-stale-revision.png` | A second writer moved the revision; the drawer’s next edit is refused with the server’s stale-revision reason and offers no reload action (I5-05) |
| `18-task-a-attachment.png` | `task-a-evidence.md` uploaded through the drawer; the accepted row and Activity entry are visible |
| `19-task-a-complete-activity.png` | Task A is Done; Activity records statuses, description updates, criteria, milestone and attachment. Every origin says “Another device” (I5-06), and summaries expose task/milestone ULIDs (I5-07) |
| `20-after-restart-board-done.png` | The server was stopped and restarted between tasks; the session survived, and Task A is still in the Done column |
| `21-board-task-b-before.png` | Task B focused on the Board with the keyboard |
| `22-board-task-b-done.png` | Task B completed with `m → arrows → Enter`; Done now holds two tasks |
| `23-milestone-progress.png` | Milestone progress reads 2/12 and 0/6 tasks done; Schedule is empty because no plan was applied |
| `24-settings-no-upgrade.png` | Settings reports that Development v2 is the newest published version — no upgrade was available (honest none) |
| `25-agents-no-jobs.png` | No agent job can be created without an executor/CLI; the browser says so instead of offering a launch (honest gap) |
| `26-home-final.png` | Home at the end: 17 open, 2 done, 0 unassigned, readiness-driven sections |

## What this run does **not** prove

- **Planning review/approve/apply.** It could not be reached. The browser has no control
  for `set_planning`, so no task in this project can carry an estimate; the day plan
  reports `missing_estimate`, the schedule plan has zero slots, and “Review this plan”
  is disabled. No calendar block exists (`schedule_blocks = 0`). This is recorded as
  I5-01 in `../../../.slim/deepwork/m31-web-app/i5-issues.md`; the script completes and
  reports the blocked outcome rather than inventing an estimate behind the UI.
- **An upgrade.** None was published beyond Development v2.
- **An agent job.** Creating one needs an executor or CLI; the browser deliberately has
  no launch path. The gap is recorded rather than simulated.

## How to reproduce

```bash
cd /home/xliu/dev/orbit
cargo build -p orbit-server
cd web
node node_modules/typescript/bin/tsc --noEmit
node node_modules/vite/bin/vite.js build
node tests/e2e/i5/dogfood.mjs
```

The script deletes `/tmp/orbit-i5`, enrols `orbit-development`, publishes the bundled
Development workflow through the setup API (the UI has no install control), and then
does every content change through the browser. It refuses to start if port 8790 is in
use. It writes this directory and the database backup on success. A failure writes
`failure.png`, `failure-page.html` and `failure-state.json` here for diagnosis; a green
run deletes those at the start and leaves the numbered frames above.

Green means “the cycle completed and recorded its outcomes”, including the blocked
planning gate — not that every gate passed.
