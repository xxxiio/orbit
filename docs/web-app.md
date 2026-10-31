# The Orbit web app

The Orbit web app is an optional browser client for a self-hosted `orbit-server`. It reads and
writes the same authoritative state as the CLI and the terminal workspace, over the server's
HTTP API, and adds no second data model and no local database. This page documents the build
shipped through M32 I4: the app shell and sign-in, Home, Inbox, Projects, one search across projects,
tasks and knowledge, workflow-driven project workspaces (task list, board, milestones, timeline,
and the panels a workflow declares), saved views, Work Items, planning with an explicit approval
gate, a knowledge vault with folders, note links and project/task references, attachments,
browser-authorized agent jobs with a real executor read and review, editable project settings and
workflow upgrades. Anything listed as missing below really is missing in this
build. For a one-page setup path aimed at a new internal tester, start with the
[web quickstart](guides/web-quickstart.md).

**Contents:** [What it is](#what-it-is-and-what-it-is-not) · [Build and run](#build-and-run) ·
[Sign in](#sign-in) · [Everyday use](#everyday-use) · [Work Items](#work-items) ·
[Planning](#planning) · [Knowledge](#knowledge) · [Attachments](#attachments) ·
[Agents](#agents) · [Workflow upgrades](#workflow-upgrades) ·
[Bundled workflow install and update](#bundled-workflow-install-and-update) ·
[Changes the server refuses](#changes-the-server-refuses) · [Keyboard](#keyboard) ·
[Accessibility and theme](#accessibility-and-theme) · [Troubleshooting](#troubleshooting) ·
[Terminology](#terminology) · [Not available yet](#not-available-yet) · [Evidence](#evidence) ·
[Developer note](#developer-note-internal-api-paths)

## What it is, and what it is not

| Piece | Role |
| --- | --- |
| `orbit` CLI, `orbit-tui` | Local-first clients. They work against a local SQLite database with no server running. |
| `orbit-server` | Optional authenticated authority. In connected mode its database is canonical. It also serves the web app and the JSON API. |
| Web app at `/` | The product UI: a browser client for the connected authority. |
| `/console` | A separate operator page: administrator token, direct task creation, automation schedules, agent proposals. Not the product UI. |

You do not need the server to use Orbit locally: the CLI and TUI run against a local database.
The web app holds no database, cannot work offline and has no outbox, and must be served by
the same `orbit-server` that answers its API (there is no CORS layer). Device enrollment,
revocation, automations and proposals are operator actions on the server, not buttons in the
product UI. Workflow Actions can be requested from the browser, but nothing here executes a
capability: the server stores the authorized intent and an enrolled executor process claims and
runs it. An agent job works the same way — the browser queues it and an executor claims it.

## Build and run

Build with Node 18.18 or newer. Dependencies must already be installed in `web/node_modules`
(the install step is documented in [`web/README.md`](../web/README.md)). Building the server
from this checkout also needs a Rust toolchain. The server serves static files from `web/dist`,
so build them once, and again after pulling frontend changes:

```sh
$ cd web
$ node node_modules/typescript/bin/tsc --noEmit
$ node node_modules/vite/bin/vite.js build
```

The type check exits silently; the build runs Vite and emits `web/dist/index.html` plus
`web/dist/assets/`. Those two Node commands are the verified path on this machine. The
`package.json` scripts (`npm run build`) cannot run through the Windows `npm` shim used inside
WSL: it fails with `UNC paths are not supported` before the script starts. A native Linux
`npm` runs the same two steps.

Run the server from the repository root, or point it at an absolute build path: the default
`web/dist` is resolved against the working directory and captured at startup.

```sh
$ export ORBIT_SERVER_ADMIN_KEY="$(openssl rand -hex 32)"
$ export ORBIT_SERVER_DB="$PWD/orbit-server.db"
$ export ORBIT_SERVER_BIND=127.0.0.1:8787
$ cargo build -p orbit-server
$ target/debug/orbit-server
```

`cargo run -p orbit-server` builds and starts the same binary. Then open
`http://127.0.0.1:8787/`. `curl -sS http://127.0.0.1:8787/health` answers
`{"status":"ok"}` when the process is listening.

| Variable | Default | Meaning |
| --- | --- | --- |
| `ORBIT_SERVER_ADMIN_KEY` | required | Administrator bearer credential for `/v1/devices` and operator routes. At least 32 characters. |
| `ORBIT_SERVER_DB` | `orbit-server.db` | SQLite file holding accepted state, devices, sessions, automations and proposals. |
| `ORBIT_SERVER_BIND` | `127.0.0.1:8787` | Listen address. Loopback by default on purpose. |
| `ORBIT_WEB_DIST` | `web/dist` | Directory with `index.html` and `assets/`. An absolute path avoids working-directory surprises. |
| `ORBIT_WEB_COOKIE_SECURE` | unset | Set to `1` only behind HTTPS; see [Sign in](#sign-in). |
| `ORBIT_ATTACHMENT_MAX_BYTES` | `67108864` (64 MiB) | Largest accepted upload body. The web app surfaces the server's refusal; it does not impose a smaller limit of its own. |

Session lifetimes are set with `ORBIT_WEB_SESSION_TTL_SECONDS` and
`ORBIT_WEB_SESSION_ABSOLUTE_SECONDS`.

If the page returns `503 Orbit web app is not built`, the server started without a readable
build directory. See [Troubleshooting](#troubleshooting).

## Sign in

There is no sign-up form and no administrator login in the product UI. A browser signs in with
a device ID and a device token that an operator enrolls on the server; they are exchanged once
for an HttpOnly session cookie.

### Enroll a browser device

Pick a fresh, stable device ID, generate a token, and enroll both with the administrator
key from the shell where you exported it:

```sh
$ TOKEN="$(openssl rand -hex 32)"
$ curl -sS -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:8787/v1/devices \
    -H "Authorization: Bearer $ORBIT_SERVER_ADMIN_KEY" \
    -H 'Content-Type: application/json' \
    -d "{\"device_id\":\"browser-laptop\",\"token\":\"$TOKEN\"}"
201
$ printf 'device token: %s\n' "$TOKEN"
```

The token must be at least 32 characters; any random string that long works, and the server
stores only a hash. It is a credential and is not recoverable from the server, so store it in
a password manager or rotate it (below). Use a device ID that is not already enrolled: an
existing ID cannot be re-enrolled, so rotate that device's token or choose a new ID.

### Sign in in the browser

Open `http://127.0.0.1:8787/` and use the **Sign in to Orbit** form. **Device ID** is the ID
you enrolled (for example `browser-laptop`); **Device token** is the token, and the eye button
reveals it while pasting. **Where do I get these?** says the same thing this guide does:
enrollment is an operator action, not something the app can do for you.

Sign-in failures are deliberately uniform: a wrong device ID and a wrong token both return
`Those credentials did not match`. After five consecutive failures for the same device ID
within five minutes the server answers `429` and the form says to wait a few minutes. A
successful sign-in resets the counter; restarting the server clears it, because the throttle
is in memory. The session travels only in the `orbit_session` cookie: `HttpOnly`,
`SameSite=Strict`, `Path=/`, and `Secure` when `ORBIT_WEB_COOKIE_SECURE=1`. The token is never
written to `localStorage`, `sessionStorage`, the URL or application state. The only durable
browser facts are the theme (`orbit.theme`), recently opened projects
(`orbit.recent-projects`) and a boolean `orbit.auth.seen` marker meaning "this browser signed
in before"; none of them is a credential or authority state. That marker is why a first-time
visitor sees a clean form while a genuine expiry or revocation is explained: the server returns
the same `401` in both cases. An explicit sign-out clears the marker, so it never produces an
expiry warning. Closing a tab does not sign you out, while signing out from the sidebar clears
the cookie and the server-side session row.

### Session lifetime and revocation

Each authenticated request slides the session forward by the TTL, capped by the absolute
lifetime. The defaults are 12 hours (`ORBIT_WEB_SESSION_TTL_SECONDS`) and 7 days
(`ORBIT_WEB_SESSION_ABSOLUTE_SECONDS`). Both are read as whole seconds at startup; zero,
negative or unparseable values fall back to the defaults, and a shorter absolute limit wins.
When a session expires, the next request signs the browser out and the sign-in form explains
why. An open tab does not wait to discover that: the app re-reads the session every ten minutes
and whenever a hidden tab becomes visible again, so the server can slide the lifetime of a
workspace left open all day. Requests never fall back to anonymous access.

Revoke a device when a laptop is lost or a token may have leaked:

```sh
$ curl -sS -o /dev/null -w '%{http_code}\n' -X DELETE \
    http://127.0.0.1:8787/v1/devices/browser-laptop \
    -H "Authorization: Bearer $ORBIT_SERVER_ADMIN_KEY"
204
```

Revocation is immediate and also deletes every live browser session for that device; the
next session check clears the cookie and returns to sign-in. Rotating a token has the same
effect, because sessions are delegated from the old credential:

```sh
$ curl -sS -o /dev/null -w '%{http_code}\n' -X POST \
    http://127.0.0.1:8787/v1/devices/browser-laptop/token \
    -H "Authorization: Bearer $ORBIT_SERVER_ADMIN_KEY" \
    -H 'Content-Type: application/json' \
    -d "{\"token\":\"$(openssl rand -hex 32)\"}"
204
```

A revoked device cannot be revived by rotation; enroll it again under a new ID.

**Behind TLS.** Put the server behind a TLS reverse proxy and set
`ORBIT_WEB_COOKIE_SECURE=1` (`export ORBIT_WEB_COOKIE_SECURE=1`) only when the browser
actually reaches the server over HTTPS. With `Secure` on a plain-HTTP origin, the browser
refuses to store or send the cookie, so sign-in appears to succeed and then immediately loses
the session. Do not expose a plaintext non-loopback listener; the default bind is loopback
for that reason.

## Everyday use

### Home

**Home** in the sidebar opens today's dashboard. The heading is today's date in your local
timezone; the document title is `Today`.

| Section | Meaning |
| --- | --- |
| **Overdue** | Past their deadline and still open. |
| **Due today** | Deadlines later today; overdue work is listed above, not here. |
| **Ready to start** | Tasks the server reports as actionable, excluding overdue and due-today work. |
| **Blocked** | Blocked or waiting tasks, with the server's reason and the tasks they wait on. |
| **Agent jobs needing attention** | Jobs whose server-side attention flag is set: a handoff needs review, a review asked for changes, or the job is waiting. |

The stat cells are **Open**, **Ready now**, **Blocked** and **Inbox**; the header shows open,
unassigned and done counts plus the last-read time and the server revision (`rev 42`). Each
task section shows five rows; when there are more, a **Show all N** toggle appears. Each row
opens the drawer and carries inline status and project controls. **New task** opens the task
dialog; the refresh button re-reads the snapshot and readiness. With no tasks, the page shows
**Nothing planned yet** and points at `c`; the right rail lists **Recently opened** projects
(this browser only) and the agent-attention panel links to the job pages and **All agents**.
Readiness, blocking, deadlines and ordering are server answers; the browser computes only
today's date.

### Inbox

**Inbox** is every task without a project. The page is labelled **Triage** and states how many
unassigned tasks are waiting.

- Search, filter by status or priority, and sort by priority, due date, status or title.
- Select rows with the checkbox or `x`, choose a bulk action — assign project, set status or set
  priority — and press **Apply**. Changes are sent one task at a time and report progress;
  rejected tasks are counted and named, and the rest stay applied.
- **Inbox is clear** appears when nothing is unassigned. Rows carry inline status, project and
  priority controls. The status control offers only the edges the server reports as legal from
  the task's current status; if that read fails, every status is offered and the server's
  refusal is shown.

The quick capture field is single-line: type a title and press `Enter` and the task is created
without a project. The field clears only after the server accepts it; a rejection leaves your
text in place with the server's message beneath. Titles are limited to 280 characters. Press
`c` outside a field to open the full **New task** dialog, which can also choose a project.

### Search

`Ctrl`/`Cmd`+`K` (or `/` when the page has no search field) opens one search over the whole
server, not just the screen you are on.

- An empty query keeps the command list. As soon as you type, the palette asks the server
  (`GET /authoritative/search`) once, after a short debounce, and replaces the commands with
  results grouped **Projects**, **Tasks** and **Knowledge**.
- Each group reports at most ten results and says when it was truncated; the server matches
  names, task titles and note paths/titles with a wildcard-escaped substring search.
- `↑`/`↓` move through the results across groups, `Enter` opens the highlighted one, `Esc`
  closes the palette and returns focus where it was. A project opens the project, a task opens
  its project with the drawer on that task, and a note opens the note.
- The loading, empty (`No matches for "…"`) and server-error states are stated rather than
  implied. An older server without the route says search is not available instead of failing
  silently.
- A note or task deleted after the results were fetched lands on an honest not-found state.

Search is scoped to the signed-in device, and it never downloads the workspace into the browser
to filter it there.

### Projects

**Projects** lists the projects on the server, with a grid or list layout toggle. Each entry
shows the project status, the pinned workflow badge (for example `Basic v1`), open/done/total
counts and a progress bar.

**New project** opens a dialog with two fields: **Project name**, required and not required to
be unique, and **Workflow**, where **Basic** is marked **Default** and preselected. Other
installed workflows are listed with description and version. A **Bundled workflows** block
appears when the server ships a workflow it has not published, or holds an older revision of
one; **Install** publishes it and the choice becomes selectable without reloading the page.
If nothing else is installed and nothing is installable, the dialog says so. Creating a project
navigates to it.

### A project's views

A project page shows its name, status, pinned workflow, task and milestone counts and a short
display key, then a compact set of canonical sections: **Overview**, **Tasks**, **Roadmap** and
**Milestones**. A workflow keeps its own extra views after those: Development keeps Agent jobs,
Research keeps Evidence, Workflow and References, Learning keeps Lessons and Progress.
**Planning** and **Settings** are project tools, not workflow views.

Canonical sections have stable routes (`/tasks`, `/roadmap`, `/milestones`). List, Board and
Backlog are not separate tabs: they are three presentations of the one Tasks tab, addressed as
`/projects/:id/tasks?view=list|board|backlog`. Legacy `/list`, `/board` and `/backlog` deep
links redirect to the matching view without dropping the rest of the query. Older declared-view
ids (`/timeline`, for example) still resolve to the section that owns them. A declared view is
consumed by a canonical section only when its source is tasks or unset; a workflow's own kanban
over Work Items keeps its own section and route, and Research **Workflow** or Learning
**Lessons** is never silently renamed Board.

**Milestone is the only project work-grouping entity.** The first M33 pass proposed a second
grouping concept; human review removed it, and no tab, column, filter, dialog or authority
family for it remains.

The renderer for a declared panel is chosen from the *component types a view declares*, not from
the workflow id, so bundled workflows share one code path. A declared panel this build does not
render keeps its tab and shows a named panel that says what is missing and why; it is never a
blank surface. Tabs that stand for a list carry a count. The header always offers a create action
for the current surface (**New task**, **New milestone** or **New work item**).

Implemented component types: **text**, **list**, **table**, **metric**, **detail**, **kanban**
(task or Work Item board), **task_list**, **timeline**, **milestone_progress**,
**knowledge_refs** and **agent_jobs**.

- **Overview** — the workflow's own mix of metric tiles, project detail and a task or milestone
  summary. Metric counts are counts of rows the server returned; a metric filtered by readiness
  reads the server's readiness report instead of guessing.
- **Tasks** — one task workspace with a List, Board and Backlog switcher and shared URL filters
  (see [the Tasks workspace](#the-tasks-workspace)). The tab owns one task read; switching a
  view never starts a second query.
- **Roadmap** — one Milestone hierarchy with child task windows, progress, blocked work and
  dependency highlighting (see [Roadmap](#roadmap)).
- **Milestones** — rich milestone cards, create/edit, membership, progress, activity and safe
  delete (see [Milestones](#milestones)). A declared `timeline` keeps its own route but is
  presented as **Milestones**.
- A workflow that declares no views still says so; it is not given a fabricated tab set.
- **Knowledge references** — the declared `knowledge_refs` panel reads the server's note
  inventory, links each row to the note, and refuses a filter or sort it cannot evaluate.
- **Agent jobs** — the declared `agent_jobs` panel reads the same device-scoped overview as the
  Agents page and links rows to the job detail. It never offers a start control.
- **Habit tracker** and **metric history** are declared by bundled workflows but not rendered in
  this build; each panel says so by name.
- A workflow definition that cannot be read falls back to Overview and Tasks with a warning.

`Planning` and `Settings` are project tools, not global tabs: they live under the project and
keep the same header and project switcher.

### The Tasks workspace

The **Tasks** tab is one surface with three presentations and one project task read:

- **One view switcher.** **List** is the default; **Board** and **Backlog** are the same
  workspace in another shape. The choice is `?view=` on the Tasks route, so a switch never
  remounts the page or starts a second query. The old `/list`, `/board` and `/backlog` routes
  redirect to their view and keep every other parameter.
- **URL-backed shared filters.** Search (`q`), status, priority, milestone, sort (`sort` and
  `dir`), grouping (`group`), visible columns (`cols`), selection (`sel`) and the open task
  (`task`) live in the URL, alongside paging (`page`). List, Board and Backlog read the same
  state, so switching view preserves the filter you set in the previous one. A clean view has a
  clean URL; neutral values are removed.
- **Column picker.** Title, status, priority, project, milestone, due, estimate, labels,
  readiness and dependencies. Title cannot be hidden; the workflow's declared table columns
  seed the initial set, and read-only **Readiness** and **Dependencies** columns are added from
  `/task-readiness` and `/task-graph`. They show the server's reason and counts; the title cell
  does not turn them into new rules.
- **Grouping.** The toolbar groups by **Milestone**, **Status** or **Priority**. Grouping is URL
  state (`group=`), and a milestone group is an expandable section with its title, child count
  and the server's progress bar. A task with no milestone is grouped under **No milestone**,
  never hidden.
- **Human-readable rows.** The task title is the primary target; the raw UUID is shown only in
  the drawer header as a short, stable reference. A project's display key is shortened the same
  way. Immutable ids stay the authority identity.
- **Bulk commands and selection.** Select rows with the checkbox, `x`, or select-all for the
  current page. Bulk status, project and priority send one command per task with the revision
  fence and idempotency key the rest of the app uses; a partial failure names the tasks and the
  server's message.
- **Windowing.** Up to 200 matching rows are rendered at once; beyond that the list renders one
  50-row page at a time, with the page in the URL, so a large project stays responsive.
- **Sorting and filtering.** Every column header offers its server-backed field; the shared bar
  offers search, status, priority, milestone and a sort select. Filtering and sorting are
  presentation over the snapshot, and never recompute readiness.
- **Saved views.** **Saved views** stores the current Tasks query and the display (`list` or
  `board`) under a name you choose, and reopens it on the one Tasks route later. Saved views are
  personal presentation preferences stored by the server: they survive a browser restart and a
  different browser profile, and saving one never changes a workflow definition. Rename and
  delete are separate commands; a stale version is refused with the server's words and the list
  refreshes.
- **Backlog** is the same task set filtered to `inbox`, `planned` or `deferred`, grouped by
  milestone by default, with priority, estimate, deadline and dependencies visible. Moving a
  task to **Ready** from its status control starts delivery. There are no mandatory sprints.

### Board

The Board is the second presentation of the Tasks workspace, not a separate destination.

- Columns follow the server's canonical task-status order and show their counts. Empty terminal
  columns are hidden; other empty columns stay visible with `0` so the workflow's shape is
  honest.
- **Compact is the default density**; **Comfortable** is the alternative. The choice is stored
  per browser at `orbit.board.density` and restored on the next visit; unreadable or unavailable
  storage falls back to Compact without breaking the board.
- A compact card shows the title, a priority cue, the deadline and a blocked indicator, with
  milestone and estimate when the card has them. It does not print raw ids or a permanent row of
  badges. Comfortable adds the priority label the compact card abbreviates.
- Clicking a card opens the existing task inspector on demand. There is no permanent side panel,
  and the card itself carries no permanent edit controls.
- `h`/`j`/`k`/`l` move focus between cards and columns, `Enter` opens the inspector, and `m` or
  the card's overflow menu opens **Move to…**.
- The Tasks filters narrow the board without changing its columns or status semantics. Filtering
  is presentation; the server still owns the card's state.
- **Move to…** lists only the statuses the server reports as legal from the card's current
  status. If the edge read fails, every status except the current one is offered and the menu
  says so. The server decides whether the move is legal; a rejected move restores the
  authoritative state, shows the server's message against the card, and announces it. An accepted
  move appears only because the accepted snapshot placed it there.
- Completion from `Ready` is two legal moves (`Ready → In progress → Done`), because the domain
  forbids `Ready → Done`. There is no single-key Done mnemonic; `m` plus the menu is the
  keyboard path. This is a known rough edge, not a hidden shortcut.
- **Saved views** works here too: a saved `board` display restores the board and the query the
  URL carries. Drag-and-drop is deliberately not implemented; the keyboard and the move menu are
  the complete interaction.

### Creating a task

The project's **New task** action opens the full create dialog. **Quick capture** on the Inbox,
and `c` outside a field, stays the lightweight path for a title-only task; it is not the project
create form.

- **One accepted operation.** The dialog sends title, project, description, milestone, initial
  status, priority, deadline, estimate, acceptance criteria and prerequisite tasks in a single
  create command with one receipt. Field mutations for the created values, criteria and
  prerequisite edges are written in the same immediate transaction.
- **Progressive disclosure.** Title, project, description and properties (status, priority,
  milestone, deadline) are visible; estimate, dependencies and acceptance criteria sit under
  **Advanced**. Priority defaults to Medium.
- **Initial states only.** Status offers `inbox`, `planned`, `ready` and `deferred`; the server
  refuses a create that asks for any other state.
- **Safe retry.** The command id and task id are minted for the attempt and reused while the
  payload is unchanged, so retrying after an unconfirmed failure returns the stored receipt
  instead of creating a second task. Editing a field starts a new attempt.
- **Leaving with unsaved content** asks before discarding. Success shows a toast with **Open
  task**; the dialog closes with the stored task.

### Milestones

A **Milestone** is the project's only grouping unit: a checkpoint (or a start-to-target window)
that owns Tasks and supported Workflow Work Items.

- **Create and edit** from **New milestone** in the header or the Milestones panel. The dialog
  takes name, description, priority, owner, start date and target date. Status is not part of the
  create form: a new milestone starts `planned`, and an existing one moves through its own status
  command. Every create attempt mints its id before sending, so an unconfirmed retry cannot
  create a second milestone.
- **Cards** show status, the date summary, owner, blocked count and the server's progress bar.
  They open a details dialog and a separate edit dialog; the whole card is not a single
  ambiguous button.
- **Membership.** Tasks join through `tasks.milestone_id`; supported Work Items join through the
  milestone's Work Item membership table. The details dialog lists child Tasks and Work Items,
  assigns and unassigns tasks, opens a task in the shared inspector, and unassigns Work Items;
  a Work Item is assigned from its own sheet. A task belongs to at most one milestone, and both
  sides must share a project.
- **Progress is derived by the server.** Tasks assigned to the milestone count unless they are
  `cancelled`; `done` counts complete, and `blocked` is reported as blocked. Work Items count
  total, and a state of `done`, `completed` or `closed` counts complete. Percentage is completed
  over total; an empty milestone is 0%. The list card, details dialog and roadmap bar read the
  same number.
- **Activity** from create, update, status, date and membership changes appears in the shared
  activity stream.
- **Lifecycle** uses `planned`, `in_progress`, `blocked`, `completed` and `cancelled` with the
  server's legal edges. Complete or cancel a milestone that still owns work.
- **Safe guards.** Delete and project move are refused while tasks or Work Items are attached;
  the refusal is quoted and the stored milestone is unchanged.
- **Authority commands.** `create`, `update`, `change_status`, `set_target`, `move_project`,
  `assign_task`, `unassign_task`, `assign_work_item`, `unassign_work_item` and `delete` all carry
  the usual command id and expected revision, replay their receipt, and emit a milestone
  semantic change. Cross-project membership and invalid status edges are refused atomically.
- **One-time conversion.** Opening an authority store that still has the legacy grouping tables
  converts their rows in one transaction: the epic id becomes the milestone id when free;
  otherwise the id gains `--epic`, and further collisions add `--epic-<hash>`, capped at 128
  characters. An existing `tasks.milestone_id` always wins, and an existing Work Item assignment
  always wins. Only work copied into the new milestone participates in the status decision:
  `cancelled` is preserved; `completed` is preserved when all copied work is finished, becomes
  `planned` when nothing was copied, and `in_progress` when copied work is unfinished. Activity
  is remapped to the milestone subject, and the superseded grouping tables are dropped. There is
  no permanent compatibility layer.
- The conversion never deletes tasks or Work Items; it only retires the superseded grouping
  tables. For a development database only, deleting the local database is an alternative way to
  start clean. That is destructive to that local database and is not part of the upgrade path.

### Roadmap

The Roadmap is a planning and navigation surface, not a decorative Gantt chart.

- **One hierarchy.** The left side lists milestones as parent rows and their child tasks
  (collapsible), then **Unassigned** tasks and an **Unscheduled** section. There are no rows for
  any second grouping entity. Titles stay visible while the time axis scrolls horizontally.
- **Scales.** Week, month and quarter buttons change the axis; the choice is URL state
  (`scale=`). There is no zoom gesture. A red line marks today.
- **Milestone shapes.** A milestone with a start and a target draws a duration bar. A milestone
  with only a target is a checkpoint marker on that day. A milestone with no target is
  **Unscheduled**, never a bar borrowed from its children.
- **Task windows.** A task with an earliest start and a deadline draws that window; one authored
  date is a checkpoint. A task with no date is listed under **Unscheduled**. Neither the roadmap
  nor the milestone invents a date.
- **Progress and blockers.** A milestone row carries the server's progress and blocked count;
  blocked tasks use the warning tone.
- **Dependencies.** A task with prerequisites carries a link indicator; focusing it highlights
  its prerequisite bars. The server's task graph is the source, not a client-side edge list.
- **Filters and editing.** Status and milestone filters narrow the timeline. Clicking a task
  opens the shared inspector, clicking a milestone opens its details, and editing a date opens
  the milestone target dialog or the task's planning fields. Drag-to-reschedule is not
  implemented; dates change through the authoritative dialog or inspector so validation and
  conflicts stay correct.

### The task drawer

Open a task from any title, row or next-action link. The drawer is URL state:
`?task=<task-id>` can be bookmarked or shared, survives a reload, and browser Back closes it.

- Header — short task ID, status and priority badges, milestone badge, inline editable title,
  and the project name.
- **Created and Updated** — the authoritative times, taken from the server's recorded activity
  for this task and shown as a readable date in the drawer. The full timestamp is available on
  hover and to a screen reader. A task with no recorded history shows **Unknown** rather than a
  guessed date. A change advances **Updated** and leaves **Created** alone.
- **Details** — status, priority, project and milestone selectors; the milestone selector is
  disabled until the task belongs to a project. The status selector offers only legal next
  states, with the same all-status fallback as the Inbox when the edge read fails.
- **Planning** — **Estimate (minutes)**, **Earliest start** and **Deadline**. Leaving a field
  empty clears it. The three fields are one `set_planning` command with the usual revision
  fence; a draft is flushed when you leave the field, press `Enter`, or close the drawer, and a
  rejected command resets each draft to the stored value. This is where a task is sized and
  given a deadline for the project's [Planning](#planning) screen.
- **Labels** — click to edit; separate labels with commas and press `Enter` or leave the field.
- **Description** — **Write** and **Preview** tabs; Markdown is rendered in Preview. Saves when
  you leave the field, or on `Ctrl`/`Cmd`+`Enter`. Only absolute `http(s)` links are live and
  they open in a new tab; a relative `/knowledge/...` link is plain text in this build.
- **Acceptance criteria** — add a criterion, check it off, reorder it, or remove it.
- **Dependencies** — the server's readiness sentence and the raw readiness reason, blocking
  tasks, earliest start and deadline, **Unblocks N tasks**, prerequisite depth, the editable
  **Prerequisites** list and the graph's dependents. A prerequisite the server would reject (a
  cycle, for example) is refused and explained.
- **Attachments** — upload, download, preview images and remove files (see
  [Attachments](#attachments)).
- **Activity** — the server's own record for this task. Writes from this browser read **This
  device**; another enrolled device reads **Another device**.

The footer says **Changes save when you leave a field**. While a command is in flight it shows
**Saving…**; on failure it prints the server's reason and the control keeps the stored value. A
stale-revision refusal offers **Reload this task and try again**, which refetches the task and
its snapshot and clears the rejected draft; you do not have to reload the page.

## Work Items

A **Work Item** is the workflow-defined record a project can hold beside its tasks. A workflow
declares one or more *kinds*; a kind declares its fields, its states and the transitions between
them. Nothing about a Work Item is hard-coded in the browser: the kind's declaration drives the
board columns, the table and list columns, the create dialog, the property controls and the
moves that are offered. The bundled Development workflow declares a **Feature** kind; Research,
Learning and Habits declare hypothesis, experiment, lesson, assessment, practice and check-in
kinds.

### Where Work Items appear

A view whose component reads `source: "work_items"` is rendered by the Work Item renderer for
its component type:

- **Board** — one column per declared state of the active kind, in the manifest's order. When
  the view does not pin a kind and the workflow declares more than one, kind chips above the
  board choose the kind and show each kind's count. Cards summarise the kind's declared fields.
  `h`/`j`/`k`/`l` move focus, `Enter` opens the sheet, and `m` opens the same sheet, where the
  declared moves are listed. There is no drag-and-drop.
- **Table** and **list** — rows are Work Items the server returned for the project, filtered and
  sorted by the declaration. A declaration this build cannot honour (an unknown filter, sort or
  column) is refused by name instead of being silently replaced; a declared limit names how many
  rows were withheld.
- **Detail** — shows one selected item with its declared fields, or a bounded chooser when no
  item is selected. **Open properties and moves** opens the sheet.

The project header offers **New work item** when the open view's main surface is Work Items.
The create dialog fixes the kind when the view names one (otherwise it offers the declared
kinds), renders one typed control per field, and validates required fields before sending.
The server owns the initial state; the dialog never invents one. Submitting again after an
unconfirmed failure repeats the same item id, so a retry cannot create a duplicate.

### The Work Item sheet

The sheet is URL state (`?item=<work-item-id>`), like the task drawer: a reload keeps it open,
Back closes it, and a link can point at one item. It contains:

- **Properties** — text, integer, boolean and choice fields, typed from the kind. Text saves on
  blur or `Enter`; a number must be a whole number and a stored number cannot be cleared
  (the API has no clear-property command, and the field says so); a choice is a select.
  Every write carries the revision the browser last read, and a refusal keeps the stored value.
- **Attachments** — the same attachment list the task drawer uses, with this Work Item as the
  owner.
- **Moves** — exactly the transitions the kind declares *out of the current state*. Each row
  shows the target state, the guards the workflow declares, and, if the transition requires a
  capability, a **Requires capability …** chip. A property guard is marked as satisfied or not
  from the item's own values; a relationship guard (`all_prerequisites_in_state`) cannot be
  judged in the browser and is left to the server. The button is never disabled by a guard:
  the server re-checks the move, and a refusal is quoted beside the row with the item left in
  place. A kind with no outgoing transition says so.
- **Actions and capabilities** — the Workflow Actions the workflow declares on transitions
  leaving the current state, plus the capability facts the server owns. Each capability shows
  whether it is **installed**, whether this project is **granted** it, whether an executor has
  **announced** it, what it requires, and its permission. Each offered action shows its
  availability: either it can be requested, or it carries the server's own code and reason
  (`not_authorized`, `capability_not_installed`, `executor_missing`, `requirement_missing`, …).
  `executor_missing` means no enrolled executor has announced the capability; it is not an
  authorization problem, and the panel says so separately.

**How a Workflow Action runs.** An available action opens a form generated from the capability's
declared inputs. An effectful or mutating action asks for confirmation in a separate step that
states the request waits for an **executor** and that this browser does not run it. The request
is then one fenced command with a pinned command id; the history shows `Waiting for an executor
to claim it`, then `An executor has taken it; no result yet`, then the outcome. The executed
result is reported by the executor as an immutable receipt with a reporter and a time, and the
panel shows it verbatim. An `uncertain` outcome is never retried: the panel offers an explicit
**Record what happened** decision (confirm succeeded or confirm failed with a reason) and nothing
else. Nothing in the panel claims the item moved — only the item read can say that.

**Executors.** Execution happens in a separate enrolled process, never in `orbit-server` and
never in the browser: an executor device announces the capabilities it can run and is the only
party that can claim or report them. Run `orbit capability serve --once` on such a device. The
server stores the authorized intent, the single claim, the receipt and the **declared effect**
the capability produces: the effect is validated against the capability's own contract and
written in the same transaction as the receipt, so a receipt can never claim an effect that is
not stored, and one command id can never produce two. The server holds no provider code.
The bundled workflows declare no Workflow Actions, so a bundled Work Item shows an empty Actions
section and an honest empty state.

## Planning

Planning is a project tool; it never books time on page load. The task's estimate, earliest
start and deadline are authored in the task drawer's **Planning** group (or a task's own API
client); the project's planning screen decides what the server proposes.

- **Day plan.** Type the minutes you actually have and press **Build day plan**. No request is
  sent while the capacity is empty, and nothing is guessed from the clock. The plan shows
  allocated, remaining and selected counts plus every task it evaluated with the server's
  disposition and explanation. Selected and actionable rows are listed; tasks the server could
  not plan are one disclosure away, with the server's reasons.
- **Unsized work is explicit.** A checkbox plans tasks without an estimate at an authored
  **Default minutes** value (the API's `include_unsized` and `default_minutes`). Off, the server
  reports them as **Missing an estimate**; the empty state names how many and offers **Open
  “… ” to add an estimate**, which opens the first unsized task at its Planning group. Nothing
  is silently sized.
- **Schedule a window.** The preview covers exactly the date, start time and end time in the
  pickers; the default is the next 09:00–12:00 local. The panel lists free windows, the
  server's proposed blocks with their ranking and explanations, and the tasks it could not
  place with the server's reason.
- **Review → approval → apply.** **Review this plan** is read-only and returns the server's
  fingerprint and command id for the exact proposed command; it writes nothing. **Apply approved
  plan** exists only after a review and stays disabled until you tick the approval box. Applying
  echoes the reviewed fingerprint and command id unchanged and shows the receipt: accepted
  revision, replayed flag and the schedule-block ids. A new window or a changed revision
  discards the review and asks for another.
- Approved blocks appear in the Timeline's saved schedule after a reload, as the receipt says.

## Knowledge

**Knowledge** is the server's Markdown vault, read and written as files. A note's stable id
lives in its frontmatter (`orbit_id`); the path is not the identity, so moving a file does not
break `[[id]]` links.

- **Tree and search.** The vault panel lists folders and notes; the header counts notes (with
  correct singular/plural). Search reads `/markdown-search` over titles and bodies and lists
  matches by path. Selecting a note opens it, and a folder is a real destination: the breadcrumb
  walks parents and children, the folder view lists exactly what is inside it, and the open folder
  survives a reload.
- **Folders.** **New folder** creates a folder at the root or inside the selected one; **Rename**
  and **Move** change its name or its parent. A rename moves every note underneath with it. The
  server refuses a missing parent, a destination that already exists and a folder moved inside
  itself, and the dialog quotes that refusal without changing the tree. Creating a folder that
  already exists is accepted by the server as a no-op and the toast says **Folder ready**, not
  "created".
- **New note.** The dialog takes a **Title**, Markdown and a **folder picked from the tree** — you
  never type a path by hand. The note id is minted before the request and written to the
  frontmatter. The chosen folder must exist, and the picker only offers folders that do.
- **Note page.** Renders the Markdown body and shows the note's **location** with the folder path,
  the stable id, and the actions **Reload**, **Edit**, **Rename**, **Move**, **Link a note** and,
  while editing, **Cancel** and **Save note**. Edit is disabled for a file with no `orbit_id`,
  because the server cannot accept an edit without one.
- **Saving is fenced by the file.** Before sending an update the editor re-reads the file and
  sends the bytes it just read as `expected`. If the file changed on disk, the write is refused
  and the page says to reload and reapply the change; if the id changed, it refuses too. A
  double-click cannot send the same edit twice.
- **Uncertain writes.** If the server's answer is lost, the page says **The write was not
  confirmed; the file may still have changed.** and offers **Retry the same change** (the same
  command id and payload) and **Reload note**. It does not claim the write failed or succeeded.
- **Rename and Move.** **Rename** changes the last path segment; **Move** changes the folder. Both
  re-read the file for the same `expected` fence, keep the stable id and keep every `[[id]]` link.
  After an accepted move the app follows the note to its new path, and a link to the old path says
  the note moved instead of pretending it never existed.
- **Linking notes.** **Link a note** picks a note by title from the inventory and appends a
  `[[id]]` reference through the same fenced `Update` command, so the picker cannot produce an id
  that does not exist. **Unlink** removes it. Writing the id by hand still works and an id no note
  owns is shown as unresolved.
- **Relations.** The Relations panel lists **Links to** (this note's outgoing `[[id]]`
  references resolved to paths), **Linked from** (backlinks), **Unresolved references** (ids no
  note in the vault owns) and **References** (typed project/task references, below). If the
  inventory cannot be read, ids are still shown and the panel says they could not be resolved.
- **A task on a note is context, not a task page.** `/knowledge/notes/x.md?task=<id>` opens
  the note with a **Task context** bar naming the task: **Open the task** goes to the task in its
  project (where the drawer lives), and **Stop linking** removes just the `task` parameter and
  keeps the note, its folder and its project context. On knowledge routes the task drawer is not
  mounted at all, so the note and its reference panel are never covered; on every other surface
  `?task=` still opens the drawer, `Esc` still closes it there, and on a note `Esc` leaves the
  note as it is.
- **Project and task references.** A note can reference a project or a task
  (`[project:orbit](orbit://project:orbit)` and the same shape with `task:`), added with
  **Reference this project** / **Reference this task** or the **Link a note** dialog's reference
  tab, and removed from the chip. The reference is one Markdown link, so it travels with the file
  and needs no database. `habit:` and work-item references are refused by the server: the wire
  contract covers projects and tasks.
- **A reference to something that no longer exists** is shown as a broken chip with the reason and
  stays removable. It is never silently dropped and never links to a dead end.
- **Project knowledge.** A project's `knowledge_refs` panel reads the notes that reference the
  project or any of its tasks, says which reference caused each row, and opens the note. Without a
  project it refuses by name instead of falling back to the whole vault. The task drawer's
  **Notes** section does the same for one task and can link or unlink a note.

There is no note deletion, no folder deletion, and no note kind or status in this build; see
[Not available yet](#not-available-yet).

## Attachments

Files are stored by the server's attachment object store and associated with a task or a Work
Item. The same attachment list is mounted in the task drawer and the Work Item sheet; a project,
note or agent job is a valid owner in the API but has no attachment screen here.

- **Upload** — **Choose a file to upload** (no drag-and-drop). The browser mints the attachment
  id and command ids before sending, then streams the bytes and shows progress. While the bytes
  are in flight the row says **Not stored yet — the server confirms when the bytes land.** The
  file is listed only after the server accepts it.
- **Size limit** — the server accepts bodies up to `ORBIT_ATTACHMENT_MAX_BYTES`, 64 MiB by
  default. A larger file is refused with **This file is larger than the server's upload limit.
  It was not stored; choose a smaller file.** and nothing is associated.
- **Uncertain uploads** — if the outcome is unknown, the row says the upload may or may not have
  landed and offers **Retry** (same ids) and **Refresh** (re-read the list before trying again).
  Uploading is blocked until that choice is made.
- **Download** — a link to the server's content route, with the stored filename.
- **Preview** — images only. **Preview** reads the content route and shows the stored bytes;
  **Hide preview** collapses it.
- **Remove** — asks **Remove this file from the server?** first. A confirmed removal is shown
  only after the server accepts it; an unconfirmed removal says the file may still be listed and
  offers a refresh.

## Agents

The Agents page shows the agent jobs this device can see. Jobs are owned by the device that
started them; an **enrolled executor device** — never the browser — claims and runs them. The
browser starts a job, watches it, records a review and can resume a finished one; it never runs
a command and never claims work on behalf of an executor.

- **Executors** — the page lists every enrolled device with whether it is active, when it last
  claimed work and how many jobs it is holding, from `GET /authoritative/agent-executors`. This
  is the honest answer to "is anything able to run this?". A device that has **never claimed a
  job** is flagged as such, and choosing it repeats the warning at the confirmation step: the job
  waits at *Preparing* until something on that device claims it, and *Preparing* does not mean
  work has started. Launching stays enabled because a roster cannot prove that no runner exists —
  refusing would be as dishonest as pretending. Revoked executors are listed as inactive, no
  active executor is a named disabled state, and a server without the route says launching is
  unavailable instead of guessing.
- **Starting a job** — from an eligible task in the task drawer, or from the Agents page after
  choosing a task. Eligible means the task is **Ready**, **In progress** or **Review** and has no
  active job; the server is still the judge. The form takes the executor, an optional
  **Objective**, the agent kind and the repository, and an explicit choice about running in a
  separate worktree. A second step confirms it, and the copy states plainly that this **queues**
  a job for an executor, that the browser does not start a model, and that approval will not
  complete the task. On acceptance you land on the job page and see the real state.
- **Claim state** — the job page distinguishes **Waiting for an executor** (nobody has claimed
  it), **Claimed** (an executor took it and has not confirmed), **Launch confirmed** (the
  executor reported a session) and **Ambiguous** — a launch whose outcome could not be confirmed.
  An ambiguous job warns **Do not relaunch** and does not offer resume: a second attempt could
  start a second agent. A job the server started for a foreign device is reported as **not
  visible**, never as missing.
- **Retries** — one launch is one `job_id` and one timestamp. A refusal keeps the confirmation
  open and **Retry the same launch** re-sends exactly that identity; editing the form starts a new
  attempt. An unconfirmed answer navigates to the job and reads it instead of re-sending blindly.
- **Overview** — **Needs attention** groups jobs whose server-side attention flag is set (a
  handoff needs review, a review asked for changes, or the job is waiting). Jobs that are queued
  or preparing with nobody claiming them are grouped separately as **Awaiting an executor**: they
  are not a person's problem yet, but they are not "nothing to do" either. Remaining jobs are
  grouped by state, each row carrying the task (or a short job id), project, state, handoff count
  and next action.
- **Job detail** — state and revision, claim state, runtime references (labels, not a live
  connection), timestamps, the task link (opens the task drawer), handoffs reported verbatim by
  the executor, the recorded review, and the resume context.
- **Review** — choose **Approve** or **Request changes**, add a short reason, and record one
  decision against the latest persisted handoff. The page states on screen that **approving
  records a review; it does not complete the Orbit task**, and the recorded review shows
  **Task not completed**. A job with no handoff cannot be reviewed. The server refuses a
  conflicting second decision; this page does not offer one.
- **Resume** — only for a terminal job with a durable handoff. Resume queues a new job in the same
  project with the same frozen brief (including any objective); it does not launch an executor,
  and an executor must claim the new job before it runs. An unconfirmed resume warns that a new
  job may already exist and to check the overview before retrying.

## Workflow upgrades

A project follows one published workflow version. **Settings → Workflow** compares the pinned
version with a target and applies a change only through a reviewed, confirmed preview.

1. Pick a target workflow version. Choosing one sends a read-only preview request; nothing is
   written.
2. The preview shows the fingerprint, views added and removed, and a full diff of kinds, states,
   fields, transitions, actions and required capabilities, with the server's notices and
   warnings. A note explains that removed tabs do not delete their data.
3. Blockers are listed with every subject id the server computed them from. **Apply** is
   disabled while any blocker is present or when the target is already pinned; a preview whose
   fingerprint changed can be opened but not confirmed.
4. Apply opens a confirmation dialog. An upgrade additionally requires ticking **I reviewed the
   preview for … and want to upgrade this project.**; changing the workflow requires the same
   explicit confirmation without that wording. The dialog repeats the preview and lets you open
   the complete diff again. If the preview changed while the dialog was open, the fingerprint
   check refuses and asks you to review again.
5. The receipt shows the previous → new version, the previewed fingerprint, and what the applied
   preview contained. The project's tabs update from the new workflow without a reload.
   Re-applying an already-pinned version reports **Workflow already pinned**.

The server re-computes the comparison and refuses a stale fingerprint or an unsatisfiable target.
A target version that is not newer is allowed but reported as `target_version_not_newer` in the
preview. The browser never diffs manifests or decides that a change is safe.

## Bundled workflow install and update

A clean server has published only the default workflow. The New project dialog and **Settings →
Workflow** both show a **Bundled workflows** block listing every bundled workflow the server has
not published, plus any bundled workflow whose shipped revision is newer than the stored one.
**Install** publishes it; a toast names the installed name and version, and the available list
is invalidated so it becomes selectable without a page reload. When the store already holds the
same or a newer revision, the workflow is not listed. Published versions are immutable: an
update publishes the bundled revision as a new version rather than editing the old one. A
project keeps its pinned version until an upgrade is previewed and confirmed.

Workflow authoring and publishing remain console/HTTP features. These screens only install
what the server ships and pin what the server has already published.

## Project settings

**Settings** edits project identity and pins the project's workflow:

- **Identity** — an editable **Project name** and **Status**, and the ID. Both edits are one
  fenced command (`action: update`) sent to the server; the domain decides whether a name is
  acceptable and whether a status move is legal. The status control offers exactly the moves the
  server reports (`GET /authoritative/project-statuses`) and nothing else, so a project created
  here starts **Proposed** and can only move where the lifecycle allows. **Save changes** is
  disabled until something changed; **Cancel** discards the draft. A refusal is shown inline in
  the server's words, a stale revision says the project changed elsewhere and refreshes without
  claiming success, and an unconfirmed answer says the outcome is unknown.
- **The project ID never changes.** A rename updates the name in the sidebar, the project header,
  the projects list, recent destinations and search results after the server accepts it, without
  a reload. Tasks, Work Items and the workflow binding still point at the same project.
- Archive and deletion are still not exposed.
- **Views in this workflow** — the tabs the project currently shows, with each view's panels and
  layout.
- **Workflow** — the pinned name and version, **Refresh list**, the available published
  versions, the bundled install/update block, and the preview-and-confirm flow described above.
- A footer repeats that changing a workflow re-pins the project, keeps existing Work Items, and
  is refused by the server when one cannot satisfy the target workflow.

Workflow authoring and publishing remain console features. This screen only pins a version the
server has already published.

## Changes the server refuses

The browser never decides whether a status change, transition, dependency or edit is legal, and
it never shows a change before the server has accepted it.

- Each write carries the revision the browser last read. If another device wrote first, the
  server refuses the stale revision and the UI says the change conflicted with a newer version
  and asks you to refresh and try again. Nothing was written. The task drawer offers **Reload
  this task and try again** beside the refusal.
- A rejected status change or Work Item move leaves the stored value on screen with the server's
  explanation. The legal edges the controls offer are a convenience; the server's answer is the
  rule.
- An expired session or revoked device gets `401`, after which the app signs out and explains
  why. A write that never reached the server is reported as unconfirmed, with a retry that reuses
  the same command id where the surface supports one; it is not retried or queued silently.
- An unconfirmed note write or attachment upload says the outcome is unknown rather than
  guessing, and offers the same-id retry plus a reload/refresh.

## Keyboard

| Keys | Action |
| --- | --- |
| `c` | Open **New task**. On the Inbox, if quick capture is visible, focus it instead. |
| `/` | Focus the page's search field where one exists; otherwise open search. |
| `Ctrl`+`K`, `Cmd`+`K` | Toggle the search/command palette. |
| `g` then `h` / `i` / `p` | Go to Home / Inbox / Projects. |
| `j` / `k`, `x` | Move between rows; `x` selects the focused row (Inbox and project lists). |
| `h` / `j` / `k` / `l` | On a board, move focus between cards and columns. |
| `m` | On a focused board card, open **Move to…** (task board) or the sheet with its Moves (Work Item board). |
| `Enter`, `Space` | Activate the focused row or button; in the palette, `Enter` runs the highlighted command or opens the highlighted search result. |
| `↑` / `↓` | Move inside the palette (commands and search results alike). Inside a table, `Tab` and `Shift`+`Tab` move through focusable controls. |
| `Esc` | Close the palette or the task drawer. |

The two-key `g` sequences must be completed within about a second. Shortcuts are ignored while
you type in an input, textarea, select or editable field, and while a dialog or the drawer has
focus. There is no `?` help overlay in the web app; this table is the reference.
With an empty query the palette lists only implemented actions — new task, new project, go to
Home, Inbox, Projects, Knowledge or Agents, and open a named project — because those are the
commands this build has. Knowledge, Agents and every direct route are
reachable from the sidebar; `g h` / `g i` / `g p` are the only letter shortcuts.

## Accessibility and theme

- A **Skip to content** link is the first focusable element. After client-side navigation focus
  moves to the main region; when the drawer closes, focus returns to the row that opened it.
- Status, priority and blocking use text and badges, not colour alone. Tables have captions and
  row actions have accessible names. Command outcomes are announced through a live region.
- The theme control offers **Light**, **Dark** and **System**, in the sidebar and on the sign-in
  screen. The preference is stored as `orbit.theme` and applied before first paint; **System**
  follows the operating system and updates when it changes. `prefers-reduced-motion` turns off
  animations and transitions.
- Below 900 px wide the sidebar becomes a drawer and content uses the full width; wide tables
  scroll inside their own region. Board columns scroll horizontally inside the board, never as
  page overflow.

## Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| `503 Orbit web app is not built. Run: cd web && npm ci && npm run build (or set ORBIT_WEB_DIST).` | The server started without a readable `web/dist`. | Build the client, or set `ORBIT_WEB_DIST` to an absolute path, then restart it. The API and `/console` keep working meanwhile. |
| Sign-in succeeds, then the app returns to the form; repeated `401` | The session expired, or the device was revoked or its token rotated. | Enroll the device again, or obtain the current token, and sign in. A revoked device cannot be reused. |
| `429 Too many attempts` | Five consecutive failed sign-ins for the same device ID within five minutes. | Wait five minutes, confirm the device ID and token, then retry. A success resets the counter. |
| `403 csrf check failed` | A scripted client sent a session cookie on a non-GET request without the CSRF header. | Non-browser clients only: send `x-orbit-csrf: 1` and do not send `Sec-Fetch-Site: cross-site`. Browsers do this automatically; bearer clients are unaffected. |
| **This file is larger than the server's upload limit** | The upload exceeds `ORBIT_ATTACHMENT_MAX_BYTES` (64 MiB by default). | Choose a smaller file, or raise the server's limit and restart it. Nothing was stored. |
| The hour is clipped in a 12-hour time field on Planning | A known layout defect at this width (`w-28` fields under an `en-US` locale). The stored value is correct. | Read the free-window chip or the proposed block's time; the value is not lost. |
| A panel says **Habit tracking is not available in the browser yet** or **Metric history is not charted in the browser yet** | The workflow declares a panel this build does not render. | Use the task list, board, milestones, knowledge or agents screens. Nothing is broken or lost. |
| A relative `/knowledge/...` Markdown link renders as plain text | The renderer only linkifies absolute `http(s)` URLs in this build. | Use the note's Relations panel, or an absolute URL (which opens a new tab). This is a recorded gap. |
| Activity text contains a raw task or milestone id | The server composes that summary sentence; the browser renders it verbatim. | Read the task title in the drawer header; this is a recorded server-side gap. |
| A board move is refused | The server's lifecycle rules do not allow that status edge. | The message on the card names the refusal; choose another status. |
| `That change conflicted with a newer version on the server` | Another device wrote first; your revision was stale. | Use **Reload this task and try again** in the drawer, then repeat the change. The refused change was not applied. |
| A project stays **Proposed** | Status moves only exist where the lifecycle allows them: `Proposed` leads to Active, Cancelled or Archived and to nothing else. | Use **Settings → Identity**; the status control lists exactly the moves the server reports. |
| `/console` looks nothing like this guide | `/console` is the operator page, not the product UI. | Use `/` for the product; `/console` is for administrator tasks such as direct task creation, automations and proposals. |

## Terminology

The product says **Workflow**, **Work Items** and **Workflow Actions** everywhere: project
badges, the new-project dialog, the Overview panel, the Work Item sheet and the error copy. A
Work Item is the record a workflow's kind declares; a Workflow Action is an action declared on a
transition between its states. Workflow definitions and bindings existed under an older name in
earlier releases; older operator notes or API clients may still use it for the same concept,
while this guide does not. The only place the legacy word remains is the developer note below,
where it is part of the HTTP path.

## Not available yet

This build is an honest subset. Do not expect the following, even though the server API
supports much of it:

- **Project archive or deletion.** Rename and status transitions are now part of Settings; a
  project cannot be archived or deleted from the browser.
- **Note or folder deletion, note kinds/statuses, and work-item references.** Folders and notes
  can be created, renamed and moved; nothing can be deleted, and the reference grammar covers
  projects and tasks only.
- **Relative in-app Markdown links.** Only absolute `http(s)` links and the two reference forms
  above are rendered; a relative `[text](notes/other.md)` link is plain text.
- **Enrolling or revoking an executor from the browser.** Devices are enrolled with the
  operator's admin key; the Agents page can only report what is already enrolled. Nothing in the
  product UI runs a job, and no action here completes a task.
- **Installing or granting a capability from the product UI.** `capabilities/install`,
  `authorize` and `revoke` are operator routes (bearer credential); the browser can request and
  reconcile, not administer. Provider code runs only in an executor process.
- Drag-and-drop on the board or roadmap. Moving a task/card uses the keyboard/`m` menu or the status control; dates change in the drawer.
- Workload, portfolio-capacity and scenario screens; planning focus, action menus and handoff
  previews. Their API reads exist but have no screen here.
- Habit tracking and metric-history charts.
- Workflow authoring and publishing. Bundled workflows can be installed and projects can pin a
  published version; everything else is console/HTTP only.
- Moving a milestone to another project from the browser. The authority command exists
  (`move_project`) and refuses while work is attached, but no screen offers it.
- Device enrollment, revocation, automation and proposal review in the product UI.
- Offline use, background sync, or a browser-side command queue.

Known rough edges inside shipped screens (recorded in the I5 cycle, not hidden):

- Board completion needs two legal moves and there is no single-key Done mnemonic.
- Schedule-window time inputs clip the hour in a 12-hour locale; the value itself is correct.
- Activity summaries can embed raw task or milestone ids, because the server composes the
  sentence.
- New Features created from a workflow whose title field has a default can share the default
  name until edited; the short id is the reliable distinction.

## Evidence

Committed captures live under `web/evidence/`; each directory has a README naming the commit,
build and screens:

| Directory | Screens |
| --- | --- |
| `web/evidence/i1/` | Home, Inbox, Projects, project Overview, Tasks, task drawer |
| `web/evidence/i2/` | Workflow workspaces, board, planning, review, receipt, milestones, settings, task list |
| `web/evidence/i3/` | Work Item board, sheet, create dialog, actions, refusal, upgrade preview/receipt/blocked |
| `web/evidence/i4/` | Knowledge note/create/search, attachments, agent overview/detail/review, Home attention, declared knowledge and agent panels |
| `web/evidence/i5/` | The real dogfooding cycle: `README.md` generated from `run.json`, captures, and the database backup |
| `web/evidence/m33/` | The first M33 pass at 1440×900, 1280×800 and 390×844 in light and dark; retained as history and superseded by the corrected Milestone design, so these captures are not acceptance evidence for the current model |
| `web/evidence/m33-corrective/` | The corrected M33 design: Tasks List/Board/Backlog, full create, Milestones, milestone details and the Milestone Roadmap at 1440×900 and 390×844 in light and dark, captured from a real legacy database after the one-time conversion |

The corrective set has 28 real captures and its own `README.md`, including the legacy
conversion and restart-persistence result. Human review of the running app and those
captures is still required; the directory is evidence, not approval.

Every one is a real browser capture against a real `orbit-server` seeded through the public
API (no mocked responses, no preview server). The I2–I4 capture sets are produced by the
opt-in evidence specs (`tests/e2e/evidence-i2.spec.ts`, `evidence-i3.spec.ts`,
`evidence-i4.spec.ts`) with the matching `<SET>_EVIDENCE=1` variable; each evidence README
records the exact command, commit and build it was captured from. The I5 set is produced by
the dogfooding cycle instead of an evidence spec:

```sh
$ cd web
$ node tests/e2e/i5/dogfood.mjs
```

That cycle refuses to start on a dirty git tree, wipes `/tmp/orbit-i5`, starts an isolated
server on `127.0.0.1:8790`, drives every product change through the browser, and rewrites
`web/evidence/i5/` with `run.json`, a generated README and the SQLite backup. It is a long,
stateful cycle and is intentionally not collected by the Playwright suite. See
[the I5 README](../web/evidence/i5/README.md) for the recorded run and
`web/tests/e2e/i5/dogfood.mjs` for the script.

## Developer note: internal API paths

This section is for people reading the HTTP API; none of it appears in the UI. The workflow
endpoints are still mounted under `/v1/authoritative/blueprints/…` (definitions, bundled list,
install, bind, binding, items, and the read-only `upgrade-preview` plus confirmed `upgrade`).
Work Item kinds and transitions travel on those definitions, and the task-side read used to
filter status controls is `/v1/authoritative/task-transitions`. The `blueprints` segment is a
compatibility path, not a product name: the app's UI and copy call this a Workflow throughout,
and that copy is checked by `node web/scripts/check-terminology.mjs`. See the
[Server API](server-api.md) for the authority model and
[self-hosting](guides/self-hosting.md) for deployment boundaries.
