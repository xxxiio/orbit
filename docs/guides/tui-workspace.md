# The Orbit terminal workspace

`orbit-tui` is Orbit's project-centric terminal client. It reads and mutates
through the same local authority as the CLI; it does not maintain a second
project model or write SQLite directly.

The shell has three regions:

- **Sidebar** — Home/Today, Inbox, Projects, Agents and the project list.
- **Workspace** — the selected global aggregation or a blueprint-defined
  project view.
- **Details** — the selected item's properties, dependencies and available
  actions. On narrow terminals details open as a modal.

The layout adapts to the terminal:

| Width | Layout |
|---|---|
| 120 columns or wider | Sidebar, workspace and details |
| 76–119 columns | Sidebar and workspace; details open with `D` or `Enter` |
| narrower than 76 columns | One primary pane; `Tab` moves between sidebar and workspace |

## Start the TUI

The TUI is a separate installed binary. It uses the same database resolution as
the CLI and MCP server:

```sh
orbit-tui
```

Select an isolated workspace explicitly when you do not want the default:

```sh
export ORBIT_DB="$HOME/.orbit/tutorial.db"
orbit-tui
```

Connected mode is selected by configuration, not by a TUI flag. The same
project-centric shell reads the accepted replica and queues project/task
mutations as durable intents. The header shows `CONNECTED` and the unresolved
count; queued changes are labelled **Pending (not accepted)** and never appear as
accepted project/task rows. Press `f` (or choose **Flush pending commands**) to
submit the outbox, then `r` to load the accepted server state. Project/task
creation and task status, rename, planning, scope and dependency actions use
this path. Connected project settings, blueprint/work-item, habit, metric and
note editing still report that they require the connected command surface.

Multi-field connected edits can queue more than one intent. If the server rejects
a later intent, earlier intents can remain queued; inspect the outbox with the
legacy connected surface or CLI before retrying. For compatibility while connected parity is completed, set
`ORBIT_TUI_LEGACY_CONNECTED=1` to launch the previous connected command palette.
It remains server-authoritative and preserves the outbox.

## Navigate

The sidebar is always the source of truth for global destinations:

- **Home / Today** aggregates the accepted planning brief, inbox, projects and
  active habits.
- **Inbox** shows unassigned inbox tasks for triage.
- **Projects** lists projects. `Enter` opens the selected project.
- **Agents** lists explicitly authorized agent jobs and their state.

Inside a project, the workspace shows the views declared by the pinned
blueprint. If the project is bound to an older blueprint without views, Orbit
generates a functional default overview and work view. The header names the
pinned blueprint version or says `generated default`.

## Keyboard

| Key | Action |
|---|---|
| `j` / `k`, `↓` / `↑` | Move selection |
| `h` / `l`, `←` / `→` | Move between components |
| `Tab` / `Shift+Tab` | Focus sidebar, workspace, details |
| `Enter` | Open the selected item, project or pane |
| `Esc` | Close a modal, or go back through navigation history |
| `[` / `]` | Previous/next project view; planning horizon on Home |
| `1`–`9` | Jump to a project view |
| `g` then `h` | Home / Today |
| `g` then `i` | Inbox |
| `g` then `p` | Projects |
| `g` then `a` | Agents |
| `a` or `s` | Contextual action menu |
| `c` | New task (or new project on the Projects screen) |
| `C` | New blueprint work item (choose kind, then declared fields) |
| `e` | Edit the selected task, project or work item |
| `n` | New knowledge note |
| `Space` | Record the selected habit occurrence |
| `p` / `P` | Preview / apply a task schedule |
| `d` | Completion impact for the selected task (Home/Inbox) |
| `v` | Task focus and neighbors (Home/Inbox) |
| `A` | Today/Next accepted action destinations (Home/Inbox) |
| `G` | Project context focus for the selected task (Home/Inbox) |
| `:` or `Ctrl+P` | Command palette |
| `/` | Search |
| `r` | Refresh accepted data |
| `f` | Connected mode: flush pending commands |
| `D` | Toggle details (wide) or open details (narrow) |
| `?` | Keyboard help |
| `q` | Quit from any non-text screen |
| `Ctrl+Q` / `Ctrl+C` | Quit from anywhere, including text inputs and modals |

`q` quits from any non-text screen. While a text prompt, palette query or search
field is focused, printable keys including `q` are entered as text; use
`Ctrl+Q` or `Ctrl+C` to quit from that state. If the text field contains
unsaved input, Orbit asks for confirmation before discarding it. `Esc` closes a
modal or goes back to the previous screen instead of quitting.

## Projects and blueprint views

Open an existing project from the sidebar or the Projects screen. A project's
workspace shows one tab per blueprint view. The built-in blueprints ship with
practical views:

- **Development** — Overview, Board, Milestones, Agent jobs.
- **Research** — Research overview, Evidence, Workflow, References.
- **Learning** — Learning overview, Lessons, Tasks, Progress.
- **Habits** — Habit overview, Tracker, Metrics, Goals.

New bundled manifests are version 2 and include views. Older pinned versions
remain readable; they render through generated defaults until the project is
explicitly rebound.

### Install and bind a blueprint without Rust changes

1. Open the command palette with `:`.
2. Choose **Install bundled blueprint** and enter `research`, `development`,
   `learning` or `habits`.
3. Open the target project.
4. Open the palette, choose **Bind blueprint version**, and select the installed
   `research@2` version from the form.

Binding is explicit and pins the version. Installing a newer version does not
silently change an existing project's views. Existing work items are not
migrated by binding.

You can also use the CLI:

```sh
orbit blueprint install research
orbit blueprint bind --help
```

## Tasks and work items

- Create a project task with `c`. On a project screen the task is scoped to that
  project; on Home/Inbox it is an inbox task.
- Open a task with `Enter` to see readiness, dependencies, estimate, deadline
  and available transitions.
- Use `a` to open the contextual action menu. Task status changes are offered
  only when the domain workflow permits them; the authority validates the
  transition again.
- Create a typed blueprint work item with `C`. Orbit first asks for the work-item
  kind, then renders a form from the pinned blueprint's declared fields. Required
  fields, choices and integer fields are validated before submission.
- Edit the selected task, project or work item with `e`, or use the action menu.
- Move a work item with the action menu. The pinned blueprint controls the
  allowed states and guards. Required capabilities still require a trusted
  host grant; a manifest cannot grant itself authority.

## Boards, tables and scrolling

A **kanban** component shows workflow columns in workflow order. Wide boards
display a readable window of columns and follow the selected card; `j`/`k` moves
through the flattened cards and the details pane follows the highlighted card.
At normal terminal widths columns are no longer squeezed into one-character
slivers. **Table** components show typed
columns and truncate long cells to the available width; open the row for the
full value. Lists and boards scroll with the selection.

On very narrow terminals a view with multiple components shows one focused
component at a time. The component counter and `h`/`l` let you move without
losing data.

## Structured forms

Project, task, knowledge, work-item and blueprint operations use a structured
form rather than a single encoded string. `Tab`/`Shift+Tab` moves between
fields; `←`/`→` selects a choice or boolean; printable keys edit text; `Enter`
submits; `Esc` cancels. Validation errors keep the form and all entered values
visible. The project form includes an optional objective and an already-installed
blueprint version; the task form includes project, milestone, estimate (minutes)
and deadline (Unix epoch milliseconds). Blueprint work-item forms are generated
from the pinned schema.

## Contextual actions and the command palette

The action menu and command palette share the same action catalogue. The
palette includes:

- global destinations and open projects;
- the current project's views;
- contextual item actions;
- new task, work item, knowledge note and project;
- blueprint install/bind;
- refresh, help and quit.

Actions are disabled with a reason when they are unavailable. A blueprint can
*mention* an action in presentation metadata, but that never enables it; the
renderer derives the offer from the selected item and the application services.

## Habits, metrics and research

Habits and metrics remain global Orbit domain records. Project views reference
them without pretending they are owned by the project. A habits blueprint shows
the tracker, observations and goals; a research blueprint shows typed research
work items and global knowledge references. Record a habit with `Space`; record
a metric observation or target from the action menu. Accepted changes refresh
the view; repeated submissions are ignored while the refresh is pending.

## Agent monitoring

Open **Agents** in the sidebar for the local workspace list, or the project's
**Agent jobs** view. Select a job and press Enter/`a` to inspect its state,
runtime, handoff count and any recorded review decision. Job completion is
execution state: it does not complete the Orbit task. Review/resume is shown as
a disabled action when no explicitly configured agent runtime or connected review
service is available, with the reason visible. Starting or resuming an agent is
never implicit. Connected mode does not yet project accepted agent jobs into the shared
snapshot, so the Agents view says so explicitly. The legacy connected command
surface remains available with `ORBIT_TUI_LEGACY_CONNECTED=1` for connected agent
transport operations.

## Feedback and uncertainty

The status bar reports exactly what happened:

- **Accepted** — the local authority committed the change.
- **Pending** — an external effect or connected command is not yet accepted.
- **Rejected** — the authority refused the change; the prior view remains.
- **Stale** — refresh failed and the previous accepted view is still visible.
- **Unavailable** — a component or extension is not installed. The rest of the
  view remains usable.

A pending command is never shown as accepted state. Repeated keypresses do not
create duplicate mutations; the TUI suppresses an identical submission until
the view refreshes.

## Quit and terminal recovery

Use `q` on a normal screen, or `Ctrl+Q` / `Ctrl+C` from anywhere. If a text
input contains unsaved text, Orbit shows a quit confirmation; `Enter`/`y` quits
and `Esc`/`n` returns to the editor with the text intact. Pressing `Ctrl+Q` or
`Ctrl+C` again on the confirmation forces quit.

Terminal raw mode, the alternate screen and cursor visibility are restored by
an RAII terminal guard on normal exit, recoverable errors and unwinding panics.
If a terminal is left in an unusual state by an external kill, run `stty sane`
and `reset` from a shell.

## Troubleshooting

- **No colors or a limited-color terminal** — Orbit also uses glyphs, bold,
  borders and `▸` markers. Set `NO_COLOR=1` to force the monochrome styles.
- **A component says unavailable** — the blueprint references a component that
  is not registered in this build. Update Orbit or install the providing
  extension; the rest of the view stays usable.
- **A project opens with generated views** — the pinned blueprint has no
  `views` section. That is expected for older versions. Bind a newer version
  explicitly if you want its declared views.
- **The workspace is stale** — press `r`. If refresh still fails, the last
  accepted view remains labelled `stale`; check `orbit doctor` and the database
  path.
- **Narrow terminal** — use `Tab` to switch between the sidebar and workspace,
  `h`/`l` for components, and `Enter` for the detail overlay.
- **Nothing renders** — confirm the terminal is interactive and at least a few
  rows tall. `orbit-tui` is not a non-interactive command; use CLI commands for
  scripts and pipes.
