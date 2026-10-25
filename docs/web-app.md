# The Orbit web app

The Orbit web app is an optional browser client for a self-hosted `orbit-server`. It reads and
writes the same authoritative state as the CLI and the terminal workspace, over the server's
HTTP API, and adds no second data model and no local database. This page documents the build
shipped in M31 iteration I1: the app shell, sign-in, Home, Inbox, Projects, a project
workspace with Overview and Tasks views, and the task drawer. Anything listed as missing below
really is missing in this build.

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
why. Requests never fall back to anonymous access.

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

A project page shows its name, status, pinned workflow, task counts and ID, then a row of view
tabs declared by that workflow. Basic declares **Overview**, **Tasks**, **Board** and
**Timeline**; Board and Timeline are not rendered in this build and show the unavailable-view
message below. The Tasks tab carries the task count.

- **Overview** — completion percentage and status breakdown, **Next actions** ranked by the
  server (rank, tasks unblocked, deadline, priority), a **Workflow** panel with the pinned ID
  and version, and a **Milestones** panel.
- **Tasks** — search, status and priority filters, sorting, bulk actions and a table with inline
  status, project and priority controls. `j`/`k` move, `x` selects, `Enter` opens.
- A workflow definition that cannot be read falls back to Overview and Tasks with a warning. A
  declared view this build does not render (a board, for example) still gets a tab, but the
  page says **This view is not available yet** and links to Tasks. The work is intact; only
  the renderer is missing.

Milestones in Overview are read-only server data; the drawer can attach a task to an existing
milestone, but there is no creation or editing UI.

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
- **Dependencies** — the server's readiness sentence, blocking tasks, earliest start and
  deadline, the editable **Prerequisites** list, and a read-only **Dependents** list. A
  prerequisite the server would reject (a cycle, for example) is refused and explained.
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
write invalidates the reads it affects). There is no polling and no live push, and the browser
does not refetch when the window regains focus: changes from another device appear after a
reload or after pressing the refresh button on Home. The drawer's open state lives in the URL;
theme and recent projects live in `localStorage`; everything else shown is server state. A
failed refresh keeps the last accepted data visible and marks it instead of replacing it with
an empty list.

## Keyboard

| Keys | Action |
| --- | --- |
| `c` | Open **New task**. On the Inbox, if quick capture is visible, focus it instead. |
| `/` | Focus the page's search field where one exists; otherwise open the command palette. |
| `Ctrl`+`K`, `Cmd`+`K` | Toggle the command palette. |
| `g` then `h` / `i` / `p` | Go to Home / Inbox / Projects. |
| `j` / `k`, `x` | Move between rows; `x` selects the focused row (Inbox and project lists). |
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
| A view tab says `This view is not available yet` | The workflow declares a view this build does not render yet. | Use Overview or Tasks. Nothing is broken or lost. |
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

- Board, timeline and calendar views; creating, editing or reordering milestones.
- Planning screens: day plan, workload, schedule planning, scenarios and approvals.
- Notes and knowledge browsing or editing, and attachment upload, download or removal.
- Agent jobs and an Agents workspace. The sidebar's **Agents** entry says
  **No agents connected yet**; job monitoring is not implemented.
- Workflow authoring, publishing, binding and upgrade screens. Upgrades exist over HTTP only.
- Device enrollment, revocation, automation and proposal review in the product UI.
- Offline use, background sync, or a browser-side command queue.

## Evidence

Committed captures live under `web/evidence/i1/`: `home-light-1440x900.png` and its dark
variant `home-dark-1440x900.png`, `inbox-light-1440x900.png`, `projects-light-1440x900.png`,
`project-overview-light-1440x900.png`, `project-tasks-light-1440x900.png` and
`task-drawer-light-1440x900.png`. Each also has a `-390x844` narrow-screen variant. They are
the iteration's committed captures, not illustrations.

## Developer note: internal API paths

This section is for people reading the HTTP API; none of it appears in the UI. The workflow
endpoints are still mounted under `/v1/authoritative/blueprints/…` (definitions, bundled list,
install, bind, binding, items, and the read-only `upgrade-preview` plus confirmed `upgrade`).
That segment is a compatibility path, not a product name: the app's UI and copy call this a
Workflow throughout, and that copy is checked by `node web/scripts/check-terminology.mjs`. See
the [Server API](server-api.md) for the authority model and
[self-hosting](guides/self-hosting.md) for deployment boundaries.
