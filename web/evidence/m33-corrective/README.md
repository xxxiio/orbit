# M33 corrective visual evidence

Captured from the built SPA (`web/dist`) served by the real `target/debug/orbit-server`
against a copy of the persistent dogfood database. The copy still contained two
pre-release Epics, so this set is also the real legacy-conversion check.

| Item | Value |
| --- | --- |
| Reviewed HEAD | `ea059f0` plus the corrective working tree |
| Build | `node node_modules/vite/bin/vite.js build` |
| Server | `target/debug/orbit-server` on `127.0.0.1:8791`, `ORBIT_WEB_DIST=web/dist` |
| Database | copy of `/tmp/opencode/orbit-dogfood/data/server.db` at `/tmp/opencode/orbit-m33-corrective/data/server.db` |
| Browser | Playwright Chromium, `LD_LIBRARY_PATH=/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu` |
| Theme | `orbit.theme` set before first paint; light and dark |
| Sizes | 1440×900 desktop, 390×844 phone |
| Result | 28 PNGs, no page errors and no console errors in the capture run |

## Legacy conversion observed on the real database

Before opening the corrective build the copy held 2 Epics, 142 Tasks, 8 Milestones
and 37 Tasks with a milestone. Opening it with the corrective server:

- converted both Epics into Milestones (`epic-web-ux`, `epic-agent-tools`) with
  their description, status, priority, owner, dates, progress and activity;
- preserved all 142 Tasks and every Task identity;
- kept tasks that already had a milestone on that milestone (the existing
  assignment wins); 41 Tasks now carry a milestone in total;
- dropped the Epic tables and left `EntityKind::Epic` only as a decode-only tag
  for historical wires;
- survived a full server restart with revision `889`, 10 Milestones, 142 Tasks
  and 41 milestone links unchanged.

## Screens

Every screen exists in light and dark at both sizes.

| Screen | Files |
| --- | --- |
| Tasks, List | `tasks-list-{light,dark}-{1440x900,390x844}.png` |
| Tasks, Board (compact default) | `tasks-board-compact-{light,dark}-{1440x900,390x844}.png` |
| Tasks, Backlog | `tasks-backlog-{light,dark}-{1440x900,390x844}.png` |
| Full task creation dialog | `create-task-{light,dark}-{1440x900,390x844}.png` |
| Milestones (one card list) | `milestones-{light,dark}-{1440x900,390x844}.png` |
| Milestone details | `milestone-detail-{light,dark}-{1440x900,390x844}.png` |
| Roadmap (Milestone hierarchy) | `roadmap-{light,dark}-{1440x900,390x844}.png` |

## What the screenshots show

- **Tasks** has one tab with a List | Board | Backlog switcher; the URL carries the
  view and the shared filters. List defaults to title, status, priority, milestone
  and due date; the board is compact by default with title, priority, deadline,
  blocked cue and a compact action menu.
- **Create task** uses primary → properties → advanced progressive disclosure. The
  primary section is title, project and Markdown description; status, priority,
  milestone and deadline sit under Properties; estimate, dependencies and
  acceptance criteria are collapsed.
- **Milestones** is one card list with status, date summary, progress, owner,
  blocked count and named Open details/Edit actions. Milestone-source panels from
  the declared view are replaced by the cards; other workflow panels on the same
  view (the Development schedule) remain below them, so no declared panel is
  stranded.
- **Roadmap** is one Milestone hierarchy: duration bars where start and target
  exist, checkpoint diamonds where only a target exists, child task windows and an
  Unscheduled section. Unassigned tasks are their own group.
- At 390px the project header wraps, the Tasks controls stack, and the board keeps
  readable columns behind horizontal scrolling. The automated responsive suite
  reports no page-level horizontal overflow.

## Flows exercised in the real browser

The Playwright suite runs the real `orbit-server`, the built SPA and a seeded
workspace; it does not mock the API. Final Chromium result: **150 passed, 5
env-gated skips, 0 failed**. The corrective suite covers:

- one Tasks tab with URL-backed List/Board/Backlog switching and shared filters;
- a fully configured task created in one accepted command (title, description,
  status, priority, milestone, deadline, estimate, two acceptance criteria, one
  dependency), verified in the request, the stored record, the inspector, the
  board and the roadmap;
- an uncertain create retried with the same command/task identity;
- milestone create/edit, progress, children, activity, legal status change and
  the non-empty delete guard;
- roadmap duration/checkpoint/unscheduled rendering and milestone details;
- compact board status movement, inspector access, density persistence and a
  reload;
- the legacy `/list`, `/board` and `/backlog` links;
- a workflow-defined mixed Board remaining at its own route when it carries a
  Work Item surface.

## Review notes

These notes come from inspecting the captures, not from a person approving the
product. A human review is still required.

- The canvas is calm and readable; the board is materially less crowded than the
  first M33 pass and no raw ids appear on cards.
- The Roadmap communicates milestone scope without inventing dates; the
  checkpoint and duration cases are visually distinct.
- The Milestones section is a single coherent list; the earlier duplicated
  declared panels are gone.
- The Development project keeps its workflow-declared mixed Board because that
  view also owns a Work Item board. It is intentionally not merged into Tasks and
  is the only remaining task-shaped top-level entry besides Tasks. This is the
  workflow exception described in `docs/web-app.md`.
- On a phone the board requires horizontal scrolling by design; columns stay at a
  readable width rather than being squeezed.

## Reproduce

```bash
cargo build -p orbit-server --offline
cd web && node node_modules/vite/bin/vite.js build
cp -a /tmp/opencode/orbit-dogfood /tmp/opencode/orbit-m33-corrective
ORBIT_SERVER_DB=/tmp/opencode/orbit-m33-corrective/data/server.db \
ORBIT_SERVER_BIND=127.0.0.1:8791 \
ORBIT_SERVER_ADMIN_KEY=<admin key> \
ORBIT_WEB_DIST=$PWD/web/dist \
target/debug/orbit-server
LD_LIBRARY_PATH=/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu \
  node /tmp/opencode/m33-capture.mjs
```

The capture script is scratch tooling and is not part of the repository. The
authoritative acceptance tests live in `web/tests/e2e/`.
