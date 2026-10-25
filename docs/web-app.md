# The Orbit web app

The Orbit web app is an optional browser client for a self-hosted `orbit-server`. It reads and
writes the same authoritative state as the CLI and the terminal workspace, over the server's
HTTP API, and adds no second data model and no local database. This page documents the build
shipped in M31 iteration I2: the app shell, sign-in, Home, Inbox, Projects, workflow-driven
project workspaces (task list, board, milestones, timeline), planning with an explicit approval
gate, project settings, and the task drawer. Anything listed as missing below really is missing
in this build.

**Contents:** [What it is](#what-it-is-and-what-it-is-not) · [Build and run](#build-and-run) ·
[Sign in](#sign-in) · [Everyday use](#everyday-use) · [Keyboard](#keyboard) ·
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
product UI.

## Build and run

Build with Node 18.18 or newer and a working `npm`; building the server from this checkout
also needs a Rust toolchain. The server serves static files from `web/dist`, so build them
once, and again after pulling frontend changes:

```sh
$ cd web
$ npm ci
$ npm run build
```

`npm run build` runs a full TypeScript check and then Vite, emitting `web/dist/index.html`
plus `web/dist/assets/`. If `npm` resolves to a Windows shim that fails inside WSL, run the
two steps of that script with the Linux Node instead:

```sh
$ cd web
$ node node_modules/typescript/bin/tsc --noEmit
$ node node_modules/vite/bin/vite.js build
```

Run the server from the repository root, or point it at an absolute build path: the default
`web/dist` is resolved against the working directory and captured at startup.

```sh
$ export ORBIT_SERVER_ADMIN_KEY="$(openssl rand -hex 32)"
$ export ORBIT_SERVER_DB="$PWD/orbit-server.db"
$ export ORBIT_SERVER_BIND=127.0.0.1:8787
$ cargo run -p orbit-server
```

After `cargo build -p orbit-server` you can run `target/debug/orbit-server` directly. Then
open `http://127.0.0.1:8787/`.

| Variable | Default | Meaning |
| --- | --- | --- |
| `ORBIT_SERVER_ADMIN_KEY` | required | Administrator bearer credential for `/v1/devices` and operator routes. At least 32 characters. |
| `ORBIT_SERVER_DB` | `orbit-server.db` | SQLite file holding accepted state, devices, sessions, automations and proposals. |
| `ORBIT_SERVER_BIND` | `127.0.0.1:8787` | Listen address. Loopback by default on purpose. |
| `ORBIT_WEB_DIST` | `web/dist` | Directory with `index.html` and `assets/`. An absolute path avoids working-directory surprises. |
| `ORBIT_WEB_COOKIE_SECURE` | unset | Set to `1` only behind HTTPS; see [Sign in](#sign-in). |

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
browser preferences are the theme (`orbit.theme`) and the recently opened projects
(`orbit.recent-projects`); neither is authority state. Closing a tab does not sign you out,
while signing out from the sidebar clears the cookie and the server-side session row.

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

The stat cells are **Open**, **Ready now**, **Blocked** and **Inbox**; the header shows open,
unassigned and done counts plus the last-read time and the server revision (`rev 42`). Each
section shows five rows; when there are more, a **Show all N** toggle appears. Each row opens
the drawer and carries inline status and project controls. **New task** opens the task dialog;
the refresh button re-reads the snapshot and readiness. With no tasks, the page shows **Nothing
planned yet** and points at `c`; the right rail lists **Recently opened** projects (this
browser only). Readiness, blocking, deadlines and ordering are server answers; the browser
computes only today's date.

### Inbox

**Inbox** is every task without a project. The page is labelled **Triage** and states how many
unassigned tasks are waiting.

- Search, filter by status or priority, and sort by priority, due date, status or title.
- Select rows with the checkbox or `x`, choose a bulk action — assign project, set status or set
  priority — and press **Apply**. Changes are sent one task at a time and report progress;
  rejected tasks are counted and named, and the rest stay applied.
- **Inbox is clear** appears when nothing is unassigned. Rows carry inline status, project and
  priority controls; the server still validates every change.

The quick capture field is single-line: type a title and press `Enter` and the task is created
without a project. The field clears only after the server accepts it; a rejection leaves your
text in place with the server's message beneath. Titles are limited to 280 characters. Press
`c` outside a field to open the full **New task** dialog, which can also choose a project.

### Projects

**Projects** lists the projects on the server, with a grid or list layout toggle. Each entry
shows the project status, the pinned workflow badge (for example `Basic v1`), open/done/total
counts and a progress bar.

**New project** opens a dialog with two fields: **Project name**, required and not required to
be unique, and **Workflow**, where **Basic** is marked **Default** and preselected. Other
workflows installed on the server are listed with description and version; if none exist, the
dialog says so. Creating a project navigates to it.

### A project's views

A project page shows its name, status, pinned workflow, task and milestone counts and the
project ID, then the views its workflow declares and two project tools: **Planning** and
**Settings**. Basic declares Overview, Tasks, Board and Timeline; Development declares Overview,
Board, Milestones and Agent jobs; Research, Learning and Habits declare their own mixes.

The renderer is chosen from the *component types a view declares*, not from the workflow id, so
Basic, Development, Research, Learning and Habits share one code path. A declared panel this
build does not render keeps its tab and shows a named panel that says what is missing and why;
it is never a blank surface. Tabs that stand for a list carry its count badge. The header's
primary action comes from the view too: **New task** on task surfaces, **New milestone** on
milestone surfaces, and nothing on a read-only panel.

- **Overview** — the workflow's own mix of metric tiles, project detail and a task or milestone
  summary. Metric counts are counts of rows the server returned; a metric filtered by readiness
  reads the server's readiness report instead of guessing.
- **Tasks** — the full project task list (see below).
- **Board** — task columns in canonical status order (see below).
- **Timeline** — milestone status and task counts, plus saved calendar blocks. Milestone records
  carry no dates in this API yet, so the milestone panel says that instead of drawing a fake
  axis; schedule blocks do have real start and end times and are listed chronologically.
- **Milestones** — create, rename and change the lifecycle of milestones (see below).
- **Planning** — a project's day plan and schedule approval (see below).
- **Settings** — the pinned workflow and the project's identity (see below).
- **Agent jobs**, **habit tracker**, **metric history** and **knowledge references** are declared
  by some bundled workflows but not rendered in this build; each panel says so by name.
- A workflow definition that cannot be read falls back to Overview and Tasks with a warning.

`Planning` and `Settings` are project tools, not global tabs: they live under the project and
keep the same header and project switcher.

### A project's task list

The Tasks view is the shared task table with project-level state:

- **URL-backed view state.** Search (`q`), status, priority, sort (`sort`/`dir`), page (`page`),
  visible columns (`cols`) and the selection (`sel`) are in the URL, so the view can be linked,
  shared and reloaded. The open task's `?task=` stays in the same URL.
- **Column picker.** Title, status, priority, project, milestone, due, estimate, labels,
  readiness and dependencies. Title cannot be hidden; the workflow's declared table columns seed
  the initial set, and read-only **Readiness** and **Dependencies** columns are added from
  `/task-readiness` and `/task-graph`. They show the server's reason and counts; the title cell
  does not turn them into new rules.
- **Bulk commands and selection.** Select rows with the checkbox, `x`, or select-all for the
  current page. Bulk status, project and priority send one command per task with the revision
  fence and idempotency key the rest of the app uses; a partial failure names the tasks and the
  server's message.
- **Windowing.** Up to 200 matching rows are rendered at once; beyond that the list renders one
  50-row page at a time, with the page in the URL, so a large project stays responsive.
- **Sorting and filtering.** Every column header offers its server-backed field; the toolbar
  offers search, status, priority and a sort select. Filtering and sorting are presentation over
  the snapshot, and never recompute readiness.

### Board

The Board is a task Kanban for the project.

- Columns follow the server's canonical task-status order and show their counts. Empty terminal
  columns are hidden; other empty columns stay visible with `0` so the workflow's shape is
  honest.
- Cards show the title, project, priority, labels and due date (or **No due date**).
- `h`/`j`/`k`/`l` move focus between cards and columns, `Enter` opens the drawer, and `m` opens
  **Move to…** for the focused card. The menu lists every status except the current one.
- The server decides whether a move is legal. A rejected move shows the server's message on the
  card, announces it, and leaves the card in its column; an accepted move appears only because
  the accepted snapshot placed it there.
- Drag-and-drop is deliberately not implemented. Keyboard parity was the requirement for
  offering a pointer gesture, and a drag across ten horizontally scrolling columns could not be
  made reliable; the keyboard and the move menu are the complete interaction.

### Milestones

The Milestones view lists the project's milestones with their status, the tasks assigned to
each and a done percentage. **New milestone** appears in the page header when the view's main
surface is milestones, and in the panel on overviews that embed milestone progress.
Rename edits in place; the status selector offers every lifecycle status and the server accepts
or refuses each edge. A refusal is shown against the milestone and the stored status stays.
**Delete** asks once and explains that the server refuses while tasks are attached. Milestone
records have no start or end date in this API, so the view shows status and task progress and
never invents dates.

### Planning

Planning is a project tool; it never books time on page load.

- **Day plan.** Type the minutes you actually have and press **Build day plan**. No request is
  sent while the capacity is empty, and nothing is guessed from the clock. The server's plan
  shows allocated, remaining and selected counts plus every task it evaluated with the server's
  disposition and explanation. Selected and actionable rows are listed; tasks the server could
  not plan are one disclosure away, with the server's reasons.
- **Schedule a window.** The preview covers exactly the date, start time and end time in the
  pickers. The panel lists free windows, the server's proposed blocks with their ranking and
  explanations, and the tasks it could not place with the server's reason.
- **Review → approval → apply.** **Review this plan** is read-only and returns the server's
  fingerprint and command id for the exact proposed command. **Apply approved plan** exists only
  after a review and requires ticking the approval box. Applying echoes the reviewed fingerprint
  and command id unchanged and shows the receipt: accepted revision, replayed flag and the
  schedule-block ids. A new window or a changed revision discards the review and asks for another.
  Approved blocks appear in the Timeline's saved schedule after a reload.

### Project settings

**Settings** states what the API can and cannot do:

- **Identity** — name, status and ID. Renaming a project is not supported by the API, and the
  panel says so instead of offering a dead control. Archive and delete are likewise absent.
- **Workflow** — the pinned workflow and version, the views it declares, and the newest
  published version of each workflow. Changing it previews the comparison (views added, removed
  or changed, plus the server's warnings and blockers) and shows the preview fingerprint. A confirmation dialog states that the
  project's tabs will change, and the server refuses the change when a blocker is present.
  Applying re-pins the project; the tabs update from the new workflow without a reload.
- Workflow authoring and publishing remain console features. This screen only pins a version
  the server has already published.

### The task drawer

Open a task from any title, row or next-action link. The drawer is URL state:
`?task=<task-id>` can be bookmarked or shared, survives a reload, and browser Back closes it.

- Header — short task ID, status and priority badges, milestone badge, inline editable title,
  and the project name.
- **Details** — status, priority, project and milestone selectors; the milestone selector is
  disabled until the task belongs to a project.
- **Labels** — click to edit; separate labels with commas and press `Enter` or leave the field.
- **Description** — **Write** and **Preview** tabs; Markdown is rendered in Preview. Saves when
  you leave the field, or on `Ctrl`/`Cmd`+`Enter`.
- **Acceptance criteria** — add a criterion, check it off, reorder it, or remove it.
- **Dependencies** — the server's readiness sentence and the raw readiness reason, blocking
  tasks, earliest start and deadline, **Unblocks N tasks**, prerequisite depth, the editable
  **Prerequisites** list and the graph's dependents. A prerequisite the server would reject (a
  cycle, for example) is refused and explained.
- **Activity** — the server's own record for this task.

The footer says **Changes save when you leave a field**. While a command is in flight it shows
**Saving…**; on failure it prints the server's reason and the control keeps the stored value.

### Changes the server refuses

The browser never decides whether a status change, dependency or edit is legal, and it never
shows a change before the server has accepted it.

- Each write carries the revision the browser last read. If another device wrote first, the
  server refuses the stale revision and the UI says the change conflicted with a newer version
  and asks you to refresh and try again. Nothing was written.
- A rejected status change leaves the old status on screen with the server's explanation. An
  expired session or revoked device gets `401`, after which the app signs out and explains
  why. A write that never reached the server says so, and is not retried or queued.

### Refresh and persistence

Data is read on page load and navigation, and again after every accepted change (the accepted
write invalidates the reads it affects). There is no polling and no live push for workspace
data, and workspace reads do not refetch when the window regains focus: changes from another
device appear after a reload or after pressing the refresh button on Home. The session read is
the one low-frequency exception (every ten minutes and when a hidden tab becomes visible), so an
open tab keeps its cookie alive. The drawer's open state and the task-list view state live in
the URL; theme and recent projects live in `localStorage`; everything else shown is server state.
A failed refresh keeps the last accepted data visible and marks it instead of replacing it with
an empty list.

## Keyboard

| Keys | Action |
| --- | --- |
| `c` | Open **New task**. On the Inbox, if quick capture is visible, focus it instead. |
| `/` | Focus the page's search field where one exists; otherwise open the command palette. |
| `Ctrl`+`K`, `Cmd`+`K` | Toggle the command palette. |
| `g` then `h` / `i` / `p` | Go to Home / Inbox / Projects. |
| `j` / `k`, `x` | Move between rows; `x` selects the focused row (Inbox and project lists). |
| `h` / `j` / `k` / `l` | On a board, move focus between cards and columns. |
| `m` | On a focused board card, open **Move to…**. |
| `Enter`, `Space` | Activate the focused row or button; in the palette, `Enter` runs the highlighted command. |
| `↑` / `↓`, `Home` / `End` | Move inside a table (or the palette, with `↑`/`↓`). |
| `Esc` | Close the palette or the task drawer. |

The two-key `g` sequences must be completed within about a second. Shortcuts are ignored while
you type in an input, textarea, select or editable field, and while a dialog or the drawer has
focus. There is no `?` help overlay in the web app; this table is the reference.
The palette lists only implemented actions — new task, new project, go to Home, Inbox or
Projects, and open a named project.

## Accessibility and theme

- A **Skip to content** link is the first focusable element. After client-side navigation focus
  moves to the main region; when the drawer closes, focus returns to the row that opened it.
- Status, priority and blocking use text and badges, not colour alone. Tables have captions and
  row actions have accessible names.
- The theme control offers **Light**, **Dark** and **System**, in the sidebar and on the sign-in
  screen. The preference is stored as `orbit.theme` and applied before first paint; **System**
  follows the operating system and updates when it changes. `prefers-reduced-motion` turns off
  animations and transitions.
- Below 900 px wide the sidebar becomes a drawer and content uses the full width; wide tables
  scroll inside their own region.

## Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| `503 Orbit web app is not built. Run: cd web && npm ci && npm run build (or set ORBIT_WEB_DIST).` | The server started without a readable `web/dist`. | Build the client, or set `ORBIT_WEB_DIST` to an absolute path, then restart it. The API and `/console` keep working meanwhile. |
| Sign-in succeeds, then the app returns to the form; repeated `401` | The session expired, or the device was revoked or its token rotated. | Enroll the device again, or obtain the current token, and sign in. A revoked device cannot be reused. |
| `429 Too many attempts` | Five consecutive failed sign-ins for the same device ID within five minutes. | Wait five minutes, confirm the device ID and token, then retry. A success resets the counter. |
| `403 csrf check failed` | A scripted client sent a session cookie on a non-GET request without the CSRF header. | Non-browser clients only: send `x-orbit-csrf: 1` and do not send `Sec-Fetch-Site: cross-site`. Browsers do this automatically; bearer clients are unaffected. |
| A panel says `Not available in the browser yet` | The workflow declares a panel (agent jobs, habit tracker, metric history, knowledge references) this build does not render. | Use the task list, board or milestone screen. Nothing is broken or lost. |
| A board move is refused | The server's lifecycle rules do not allow that status edge. | The message on the card names the refusal; choose another status. |
| `That change conflicted with a newer version on the server` | Another device wrote first; your revision was stale. | Refresh the page and repeat the change. The refused change was not applied. |
| `/console` looks nothing like this guide | `/console` is the operator page, not the product UI. | Use `/` for the product; `/console` is for administrator tasks such as direct task creation, automations and proposals. |

## Terminology

The product says **Workflow** everywhere: the project badge, the new-project dialog, the
Overview panel and the error copy. Workflow definitions and bindings existed under an older
name in earlier releases; older operator notes or API clients may still use it for the same
concept, while this guide does not.

## Not available yet

This build is an honest subset. Do not expect the following, even though the server API
supports much of it:

- Drag-and-drop on the board. Moving a task is the keyboard/`m` move menu or the status control.
- Workload, portfolio-capacity and scenario screens; planning focus, action menus and handoff
  previews. Their API reads exist but have no screen here.
- Habit tracking, metric-history charts, knowledge browsing/editing and agent-job monitoring.
- Work item tables, lists and boards. An overview can count work items, but there is no browser
  surface for editing a work item record yet.
- Attachment upload, download or removal.
- Workflow authoring, publishing and installation. Project settings can pin a published version;
  everything else is console/HTTP only.
- Project rename, archive or deletion; moving a milestone to another project.
- Device enrollment, revocation, automation and proposal review.
- A global search over tasks, projects and milestones. The command palette navigates and starts
  the two creation dialogs.
- Offline use, background sync, or a browser-side command queue.

## Evidence

Committed captures live under `web/evidence/i1/` (Home, Inbox, Projects and the I1-era project
Overview, Tasks and drawer screens) and `web/evidence/i2/` (workflow workspaces, planning and
settings). The I2 set is:

| File | Screen |
| --- | --- |
| `board-light-1440x900.png`, `board-light-390x844.png`, `board-dark-1440x900.png` | Board |
| `planning-light-1440x900.png`, `planning-light-390x844.png`, `planning-dark-1440x900.png` | Day plan + schedule window |
| `planning-review-light-1440x900.png`, `planning-review-dark-1440x900.png` | Reviewed plan, explicit approval gate |
| `planning-receipt-light-1440x900.png` | Applied-plan receipt (fingerprint, command id, revision) |
| `milestones-light-1440x900.png`, `milestones-light-390x844.png` | Milestones |
| `settings-light-1440x900.png`, `settings-light-390x844.png` | Project settings |
| `task-list-light-1440x900.png`, `task-list-light-390x844.png` | Task list with URL state |

Every one is a real browser capture from the deterministic fixture seeded through the public
API (no mocked responses, no preview server). Reproduce them with:

```sh
$ cd web
$ node node_modules/vite/bin/vite.js build
$ ORBIT_I2_EVIDENCE=1 node node_modules/@playwright/test/cli.js test \
    tests/e2e/evidence-i2.spec.ts --project=chromium
```

## Developer note: internal API paths

This section is for people reading the HTTP API; none of it appears in the UI. The workflow
endpoints are still mounted under `/v1/authoritative/blueprints/…` (definitions, bundled list,
install, bind, binding, items, and the read-only `upgrade-preview` plus confirmed `upgrade`).
That segment is a compatibility path, not a product name: the app's UI and copy call this a
Workflow throughout, and that copy is checked by `node web/scripts/check-terminology.mjs`. See
the [Server API](server-api.md) for the authority model and
[self-hosting](guides/self-hosting.md) for deployment boundaries.
