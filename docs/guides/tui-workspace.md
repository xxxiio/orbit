# The Orbit terminal workspace

`orbit-tui` is Orbit's project-centric terminal client. It reads and mutates
through the same local authority as the CLI; it does not maintain a second
project model or write SQLite directly.

The shell has two regions:

- **Sidebar** — Home/Today, Inbox, Projects, Agents and then the project list.
  `»` marks the current destination, `▸` marks the selected row, and a project
  row's status glyph is separate (`●` means the project status is `active`).
- **Workspace** — the selected global aggregation or the active project view. It
  receives every column the sidebar does not use.

The selected item's properties, note and available actions live in the **item
inspector**, which is never a third column: `Enter` or `D` opens it over the
workspace and `Esc` or `Enter` closes it. See
[The item inspector](#the-item-inspector).

The header is a breadcrumb (`ORBIT › Atlas Console`). A project screen adds the
pinned workflow (`Basic v1` or `generated default`) and `[stale]` when its last
refresh failed; a short count summary follows when the row has space. Below the regions a
one-line action bar lists at most three executable operations for the current
selection **with their real keys**, then `a more actions` and `q quit`. The last
row reports the latest accepted, pending, rejected, stale or unavailable result.
The action bar never advertises an action the application cannot authorize;
disabled actions stay in the action menu with an explicit reason.

The layout adapts to the terminal width:

| Width | Layout |
|---|---|
| 76 columns or wider | Sidebar and workspace, side by side |
| narrower than 76 columns | One pane at a time; `Tab` switches sidebar and workspace |

Nothing is docked on the right at any width. The sidebar stays a fixed width so
the workspace does not jump when you switch projects; from 160 columns it may
widen to fit longer project names. The workspace is the flexible region and
absorbs every remaining column, at 160 columns as much as at 80.

The inspector adapts instead of the shell:

| Terminal | Inspector |
|---|---|
| 80 columns and 20 rows or larger | Inset overlay; the shell stays visible around it |
| narrower than 80 columns or shorter than 20 rows | Whole frame; the shell chrome is covered |

Selection changes build the inspector model only when you open it, refresh or
resume it after an edit; rendering itself never reads detail data.

Below 76 columns a pane bar replaces the hidden region:

```text
▸ Workspace │  Sidebar │ Tab switch · Enter inspect
```

It names both panes, marks the focused one with `▸`, and keeps the way back
visible. Focus itself is unchanged, so `Tab` round-trips exactly.

## Focus and going back

Focus is a border and a title, never only a colour: the pane that owns the
keyboard has a thick border and a bright title. Selection is a `▸` rail plus a
filled row. In a view that does not fit, the workspace collapses to one focused
component and shows a rail such as `component 2/5 · h/l switch · focus`.

`Esc` is contextual:

- with an overlay open it closes the overlay (a dirty form asks before
  discarding, and the item inspector closes on `Esc`, `Enter` or `D`);
- on a normal screen it steps back through navigation history — project →
  Projects → Home — restoring the view, focus, sidebar position and selected
  row you came from;
- with no history left it goes Home, and says `Already Home` when you are there.

Navigation history is in memory; only the current destination is restored after
a restart (see [Session restore](#session-restore)).

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

Connected mode is selected by deployment configuration, not by a TUI flag: a
replica plus an authority server (and that device's token). The same
project-centric shell reads the accepted replica and queues project/task
mutations as durable intents; there is no separate connected presentation and no
launch flag. See [Connected mode](#connected-mode).

## Keyboard

`j`/`k` and the arrow keys move the selection in the focused region; `h`/`l`
move between workspace components while the workspace is focused; `Tab` and
`Shift+Tab` cycle the two painted panes, sidebar and workspace, and nothing
else. `Enter` activates the focused sidebar row, opens the workspace's selected
project on the Projects screen, or opens the selected item in the inspector.
Activating Home, Inbox, Agents or Projects keeps the rail focused so `j`/`k`
keeps driving the right pane; opening a project hands the keyboard to the
workspace, and `Shift+Tab` returns to the rail.

| Key | Action |
|---|---|
| `j` / `k`, `↓` / `↑` | Move selection, or scroll the focused text pane / inspector |
| `h` / `l`, `←` / `→` | Previous/next workspace component (workspace focus) |
| `Tab` / `Shift+Tab` | Cycle focus: sidebar, workspace |
| `Enter` | Activate the focused sidebar row, open the selected project row on Projects, or open the selected item's inspector |
| `Esc` | Close an overlay, else go back one navigation step |
| `[` / `]` | Previous/next project view; planning horizon on Home; component on Inbox/Agents |
| `1`–`9` | Jump to a project view by position |
| `g` then `h` | Home / Today |
| `g` then `i` | Inbox |
| `g` then `p` | Projects |
| `g` then `a` | Agents |
| `a` or `s` | Contextual action menu |
| `c` | New task, or new project on the Projects screen |
| `C` | New workflow work item in the open project (choose a kind, then its fields) |
| `e` | Edit the selected task, project or work item |
| `E` | Quick edit the selected task: title and priority only |
| `S` | Status transitions for the selected task |
| `K` | Acceptance criteria for the selected task |
| `H` | Activity history for the selected task |
| `i` | Attach a file to the selected task, project or work item |
| `F` | List attachments for the selected item |
| `W` | Change the selected project's workflow (local authority only) |
| `Ctrl+S` | Save a form, including from a multi-line field |
| `n` | New knowledge note (local authority) |
| `Space` | Record the selected habit occurrence |
| `p` / `P` | Preview / apply a task schedule on a project |
| `d` | Completion impact for the selected task (Home/Inbox) |
| `v` | Task focus and neighbours (Home/Inbox) |
| `A` | Today/Next accepted action destinations (Home/Inbox) |
| `G` | Project context for the selected task (Home/Inbox) |
| `b` | Bind habit or metric evidence to the open project |
| `o` / `t` | Record a metric observation / set a metric target |
| `:` or `Ctrl+P` | Command palette |
| `/` | Search |
| `r` | Refresh accepted data |
| `f` | Connected mode: flush pending commands |
| `!` | Connected mode: inspect pending and rejected commands |
| `:` then *Connected commands…* | Connected command console |
| `D` | Open the selected item's inspector, or close it while it is open |
| `?` | Keyboard help |
| `q` | Quit a normal screen or a non-text overlay |
| `Ctrl+Q` / `Ctrl+C` | Quit from anywhere, including text inputs and overlays |

Text entry wins over shortcuts: while a form field, palette query, search box,
conflict replacement or console prompt is focused, printable keys including `q`
are entered as text. Use `Ctrl+Q` or `Ctrl+C` to quit from that state.

Outside those text surfaces, `q` quits normal screens, the action menu, the
item inspector, message and generic confirmation overlays, and the
pending/conflict inspector; Help closes. It is ignored in the criteria and attachments lists.
`Ctrl+Q`/`Ctrl+C` always work, including at the quit confirmation. If typed text
is unsaved, Orbit asks for confirmation before discarding it.

## The item inspector

The inspector is the one surface that shows everything Orbit knows about the
selected row. It is an overlay, never a docked column, so the workspace keeps its
full width while you are working and the panel appears only when you ask for it.

Open it with `Enter` on the selected task, work item, note, habit, agent job or
aggregation row. With the workspace focused on the Projects screen, `Enter`
opens the selected project row; with the rail focused, `Enter` activates the
highlighted rail entry instead, so use `D` to inspect the selected project row.
`D` opens the inspector anywhere and
closes it again while it is open; `Esc` and `Enter` close it too. When the
focused component has no selected row, Orbit reports `No selection` instead of
opening an empty panel.

| Key | In the inspector |
|---|---|
| `j` / `k`, arrows | Scroll one line |
| PageUp / PageDown | Scroll ten lines |
| `a` / `s` | Contextual action menu for the inspected entity |
| `e` / `E` | Full edit / quick edit of the inspected entity |
| `r` | Re-read accepted state and rebuild this panel only |
| `Esc`, `Enter`, `D` | Close and hand the keyboard back |

The body is the title with its status glyph and subtitle, then `PROPERTIES`
(status, readiness, estimate, due date, deadline, attachments and so on), `NOTE`
when the entity carries one, and `ACTIONS` with the number of operations the
selection authorizes. Blueprint work items add `CAPABILITY ACTIONS` and
`RECENT CAPABILITY ACTIONS`; see [Capability actions](#capability-actions).

Opening and closing the inspector leaves the rest of the shell untouched: the
selected row, the active workspace component and the list scroll do not change,
and focus returns to the pane that owned the keyboard (sidebar or workspace).
`Tab` and `Shift+Tab` cycle only those two panes, so an open inspector never
joins the focus ring.

`a`, `e` and `E` act on the *inspected* entity rather than on whatever the
workspace selection happens to be, and a successful edit reopens the inspector
on the same entity with the new values. If the entity disappeared between
opening and acting — a deleted row, a closed project, a row filtered out of the
view — the panel says `This item is no longer available` with the last known
title and kind instead of an empty frame, and `r` re-checks it.

The footer names the keys that work: `Esc close · j/k scroll · Enter close ·
a actions · e edit`, and only `Esc close · j/k scroll · Enter close` for an
entity that no longer resolves.

## Projects and workflow views

Open an existing project from the sidebar or the Projects screen. A project's
workspace shows one tab per workflow view, with `▸` on the active tab and
`[ ] switch view` on the right. Orbit calls these **workflows** in the TUI and in
prose. Internal crate names, persisted identifiers, the `orbit blueprint`
command group and `blueprint.*` MCP tools keep the `blueprint` name for
compatibility; they refer to the same objects.

Every new project is bound to the bundled **Basic** workflow (version 1). Basic
works offline, declares no specialized work items, and is the default workflow
with four views:

- **Basic — Overview** — task metrics, project details, recent activity and
  next actions.
- **Basic — Tasks** — every task with status, priority, deadline and title.
- **Basic — Board** — tasks grouped by status with authorized status changes.
- **Basic — Timeline** — milestones and scheduled work in chronological order.

The other bundled workflows are version 2 and add specialized views:

- **Development** — Overview, Board, Milestones, Agent jobs.
- **Research** — Research overview, Evidence, Workflow, References.
- **Learning** — Learning overview, Lessons, Tasks, Progress.
- **Habits** — Habit overview, Tracker, Metrics, Goals.

Older pinned versions remain readable; they render through generated default
views until the project is explicitly given a newer workflow version. If a
project has no binding at all, the header says `generated default` and Orbit
generates functional views from the project's data.

### Apply or change a workflow

Press `W` (the bar labels it `W Workflow`; the menu calls it **Change workflow**)
on a project. The
picker lists the bundled workflows with **Basic** first and works offline because
the descriptors are compiled in.

| Workflow | Version | Declares |
|---|---|---|
| Basic | 1 | Ordinary tasks only; Overview, Tasks, Board, Timeline |
| Development | 2 | Development work items |
| Research | 2 | Research work items |
| Learning | 2 | Learning work items |
| Habits | 2 | Habit work items |

Applying a workflow is explicit and pins the version. It does not migrate or
reinterpret existing work items. A project whose binding is removed falls back
to generated default views; unpinning never deletes the project or its tasks.
Changing a workflow needs the local authoritative workspace; in connected mode
`W` explains that instead of pretending the change was queued.

### Install and apply a bundled workflow without Rust changes

1. Open the command palette with `:`.
2. Choose **Install bundled blueprint** and enter `research`, `development`,
   `learning` or `habits`.
3. Open the target project.
4. Open the palette, choose **Bind blueprint version**, and select the installed
   `research@2` version.

Installing a newer version does not silently change an existing project's
views. You can also use the CLI, which retains the compatibility command group:

```sh
orbit blueprint install research
orbit blueprint bind --help
```

## Tasks: quick edit versus full edit

`Enter` on a task opens its inspector: status, readiness, estimate, due date,
deadline, description, priority, labels, dependency state, attachment count and
the actions the selection authorizes. `e` opens the full edit form; `E` opens
the quick edit.

- **Quick edit (`E`)** — title and priority only. It exists so a typo fix does
  not walk through planning and scope fields. Status changes are deliberately
  not part of quick edit; press `S` for those. On a project screen a title-only
  save re-resolves that project and patches the one row instead of re-reading
  the whole workspace; from Home or Inbox the same save takes the ordinary
  refresh, and a priority change always does.
- **Full edit (`e`)** — title, priority, description (multi-line Markdown),
  labels, project, milestone, estimate, earliest start and deadline. Everyday
  fields come first and scope/planning follow, so `Tab` through the common case
  and `Ctrl+S` from anywhere to save.

Create a task with `c`. On a project screen the task is scoped to that project;
on Home/Inbox it is an inbox task.

Other task surfaces:

- `S` opens the status menu for the selected task. It lists only transitions the
  domain workflow permits (plus completion); the authority validates the
  transition again. Unavailable rows stay visible with their reason.
- `K` opens the ordered acceptance criteria. `Space` checks or reopens,
  `c` adds, `r` edits, `J`/`K` move down/up and `x` removes after confirmation.
  Checking every criterion does **not** complete the task.
- `H` opens the task's semantic activity history.
- `i` attaches a file and `F` lists attachments. Open a row with `Enter` to
  preview metadata and text, `o` to export and hand it to an external viewer,
  `E` to export to a chosen path, and `x` to remove the association.
- Use `a` for the full contextual action menu, including dependencies, planning
  preview and workflow-specific actions.

Create a typed workflow work item with `C`. Orbit first asks for the work-item
kind, then renders a form from the pinned workflow's declared fields. Required
fields, choices and integer fields are validated before submission. Move a work
item with the action menu: the pinned workflow controls the allowed states and
guards, and required capabilities still need a trusted host grant.

## Forms: save, cancel, multi-line, validation and discard

Project, task, knowledge, work-item, criterion and workflow operations use a
structured form rather than a single encoded string.

- `Tab`/`Shift+Tab` move between fields and wrap around; `↑`/`↓` move without
  wrapping.
- `←`/`→` select a choice or toggle a boolean; on a text field they move the
  caret. `Home`/`End` jump to either end of the value.
- `Enter` submits, except in a multi-line field where it inserts a newline.
  `Ctrl+S` saves from any field, including inside multi-line text.
- `Esc` cancels. If the form has unsaved edits, Orbit asks
  `Discard unsaved changes?`; `Esc`/`n` returns to the form with every value
  intact, `Enter`/`y` discards.
- The title bar carries the explicit Save and Cancel affordances
  (`Save Ctrl+S · Cancel Esc`) and the footer repeats the keys.
- A focused multi-line field shows several wrapped lines around the caret
  instead of hiding them behind a "more lines" count.
- Validation is per field: the offending field is marked `!`, its message is
  printed under it, focus moves there, and every entered value is kept. Required
  fields, integer fields, invalid choices and an earliest start after the
  deadline are all refused before anything is submitted.
- The guard against double submission is armed only after a successful submit
  and compares the form's values: the same values submitted again within 250 ms
  are refused, while a corrected re-submit after a validation error goes
  through.

## Action bar, action menu and command palette

The compact action bar shows at most three context-ranked operations with their
real keys, then `a more actions` and `q quit` (below 70 columns it shortens to
`a more`). It never shows a disabled action. With the sidebar focused on a
narrow terminal the workspace selection is not visible, so the bar lists no
selection actions until you return to the workspace.

The action menu (`a` or `s`) groups **Suggested** operations first and then
**More actions**, and scrolls with the selection. Disabled rows stay visible,
dimmed, with the reason on the row; `Enter` on one keeps the menu open and
reports the reason instead of running anything. A key is only advertised on a
row when it identifies that row uniquely.

The action menu and command palette share the same action catalogue. Palette
matching is deterministic, from strongest to weakest: label prefix, word start,
subsequence, substring, description, then multi-word token matches. Actions that
belong to the current screen, project, view or selection get a small boost, and
ties keep construction order, so the list never reshuffles randomly.

Destinations you open are remembered for the session (most recent first, up to
five) and appear first while the query is empty. `Ctrl+U` clears the query.
Disabled entries stay listed with their reason; `Enter` never runs them.

The palette includes:

- global destinations and open projects;
- the current project's views;
- contextual item actions, including acceptance criteria, activity history,
  attachments, workflow changes and capability actions;
- new project and exactly one **New task** entry (it carries the contextual
  project), plus new knowledge note on the local authority;
- **Install bundled blueprint** and **Bind blueprint version**;
- refresh, help and quit.

In connected mode it also lists **Pending commands** (the same list as `!`) and
**Connected commands…**, which opens the console described below.

`/` opens search instead of the palette: it matches projects, the current
view's rows and recent knowledge notes (at most 50 hits), and `Enter` jumps to
the hit.

## Capability actions

A blueprint work item can offer a **capability action**: a typed operation that a
trusted host executes, with its own durable request and receipt. Capability
actions exist only in the local authoritative workspace.

They appear in exactly three places, all fed by one catalogue:

- the contextual action menu (`a`) for the selected work item;
- the command palette (`:`) — search for the action's description;
- the inspector (`a` inside it), where `CAPABILITY ACTIONS` lists the offers and
  `RECENT CAPABILITY ACTIONS` lists the durable ones.

An offer that cannot run stays listed and dimmed with its code and reason
instead of being hidden, and `Enter` on it reports the reason without running
anything:

| Code | Meaning |
|---|---|
| `not_authorized` | The capability is installed but not granted for this project |
| `capability_not_installed` | The deployment has no durable installation for it |
| `executor_missing` | The trusted composition has no executable action or executor |
| `requirement_missing` | A capability requirement or a declared requirement is unsatisfied |
| `capability_missing` | The capability is not registered in the trusted composition |
| `capability_disabled` | The capability is registered but disabled |

Running one opens a form built from the action's declared input schema. The
reference capability asks for `note` (required text) and an optional `tag`;
required fields, integer fields and invalid values are validated before
submission, exactly like the other forms. Submitting records a **pending**
request with a stable command id and then asks
`Request recorded (pending, command <id>). Run the trusted executor now?`.
`Esc` leaves the request pending — it can be run later with
`orbit blueprint action execute <id>` — and `Enter` executes it once and only
once: the same command id never appends a second effect.

The result panel reports `succeeded`, `failed` or `uncertain` with the provider,
action, capability, item, command id and detail:

- `succeeded` — the trusted executor recorded the outcome; re-running the command
  returns the same receipt;
- `failed` — the action failed and is never retried automatically; a new attempt
  needs a new explicit request;
- `uncertain` — the provider did not confirm a result. This is **not completed**
  and Orbit says so. The panel offers `Reconcile as succeeded (--succeed)` and
  `Reconcile as failed (--fail)` behind a second `Reconcile uncertain outcome`
  confirmation, and the capability is never re-run by reconciling. Closing the
  panel without a decision leaves the outcome uncertain instead of promoting it.

After a request, execution or reconciliation the cached offers and history are
re-read, so the inspector shows the durable state (`pending`, `claimed`,
`succeeded`, `failed`, `uncertain`) rather than the state of the previous frame.

Connected mode has no capability projection. The actions are not listed at all,
nothing is queued, and the local-authority path behind them refuses with
`Capability actions require the local authoritative workspace; connected parity is tracked as M25 EC-004.`

## Boards, tables and small terminals

A **kanban** shows the workflow's status columns in workflow order, including
empty ones (`— empty`). Columns never shrink into unreadable slivers: each
column keeps a readable minimum of about 20 cells, and the board windows
horizontally instead.
The legend under the board reports the window (`columns 2-4 of 10`) and the
board keys. The window follows the selected card and leaves it only when another
component becomes the active workspace component; opening the inspector does not
move the window.

- `j`/`k` move through the flattened cards across columns; the column window
  follows.
- `h`/`l` switch workspace components, not columns.
- The selected column header is marked `▾` and underlined with `━`; the
  selected card carries `▸` when the board owns the keyboard and `›` when the
  column is selected but focus is elsewhere.
- `S` opens status transitions, `E` quick edit, `Enter` the inspector.

**Table** components keep the title column plus the status, priority and date
columns that fit. Secondary columns are dropped rather than squashed into
single characters, and cells end in `…` when truncated; open the row for the
full value. A status column narrower than six cells falls back to its glyph.

Lists, tables and boards scroll with the selection. A text pane scrolls with
`j`/`k` when it owns focus, and so does the inspector while it is open. On very narrow or short terminals a
view with several components shows one focused component at a time with the
`component n/m` rail, so `h`/`l` and `Tab` never lose data.

## Session restore

Orbit persists navigation intent, not data. On exit and whenever navigation
changes it writes `tui-state.json` beside the deployment state (override the
path with `ORBIT_TUI_STATE`) using an atomic replace:

- the screen, active project and view (by identifier, so reordered views still
  restore correctly);
- the focused pane and the sidebar/project-list positions;
- the active component, the selected item's stable id and its index, plus the
  scroll offset.

Restarting `orbit-tui` reopens the same project and view with the same selected
row. If the project or view no longer exists, Orbit falls back to the nearest
valid destination — the project list, or Home when the workspace never loaded —
and keeps the previous navigation intent rather than overwriting it with an
empty shell. A corrupt or future-version state file is ignored; startup is never
blocked by it.

A session file written by the previous docked-pane build still restores: a
saved `"focus": "detail"` loads as the workspace focus, and the obsolete
`detail_scroll` field is ignored instead of failing the restore.

These never persist: form text and unsaved edits, the open overlay (including
the inspector), palette, search or console queries, the in-memory
recent-destinations list, and any credential or token. A queued connected command persists because it is durable
authority intent, not because it is TUI state.

## First run and empty states

An empty dashboard is never left unexplained. With no projects, the workspace
shows a **Getting started** banner. Open **Projects** (`g p`) and press `c`
for the new-project form; from Home the same key creates an inbox task.
**Basic** is the default workflow, and every new project opens with Overview,
Tasks, Board and Timeline views without any extra setup.

New projects are created against the bundled Basic workflow, which works
offline, declares no specialized work items, and needs no network or agent
runtime. Each view explains its own empty state:

- Overview shows zeroed task/milestone metrics, project details and next actions.
- Tasks shows the task table as soon as the first task exists.
- Board groups tasks by status once there are tasks to group.
- Timeline lists milestones and scheduled work; it stays empty until you add
  either.
- An empty acceptance-criteria list says `No acceptance criteria yet` and `c`
  adds the first one.
- An empty attachments list says `No attachments yet` and `A` (inside the
  modal) or `i` (on the task) attaches a file.

## Habits, metrics and research

Habits and metrics remain global Orbit domain records. Project views reference
them without pretending they are owned by the project. A habits workflow shows
the tracker, observations and goals; a research workflow shows typed research
work items and global knowledge references. Record a habit with `Space`; record
a metric observation with `o` or a target with `t`, or use the action menu.
These are local-authority forms; in connected mode they report that recording
goes through the connected console. Accepted changes refresh the view.

## Connected mode

Connected mode reads the accepted replica/snapshot and sends mutations through
the connected command path. The header shows `CONNECTED`, the project count and
the pending count, where the pending count includes both durable queued commands
and staged attachment uploads. On a very narrow terminal the summary is dropped
so the breadcrumb stays readable; `!` is still the way to see queued work.

Queued changes are labelled **Pending (not accepted)** and never appear as
accepted rows. Press `f` (or choose **Flush pending commands**) to submit the
outbox, then `r` to load accepted state. The flush result reports, per command,
what the server accepted and what is still pending or rejected, plus how many
staged attachment uploads remain. If the transport is unavailable, the accepted
view stays visible and is labelled `stale`; nothing is silently promoted.

### Inspect and recover queued work

`!` opens the pending and conflict inspector. It lists durable queued commands
and staged uploads with their `[pending]`/`[conflict]` tag, kind and summary,
and shows the selected command's canonical payload and recorded state (including
`attempts: not recorded by this replica` when the replica keeps none) plus
`accepted receipt: none (not accepted)` until the authority acknowledges it.

| Key | In the inspector |
|---|---|
| `j` / `k`, arrows | Select a command and update the detail |
| `Enter` | Re-read the selected command's detail |
| `r` | Reload the outbox and the accepted view |
| `f` | Flush the queued commands and report each result |
| `c` | Review a rejected command (only a `[conflict]` row) |
| PageUp / PageDown | Scroll the detail |
| `Esc` | Close; queued intentions are preserved |

A `[pending]` row is not a conflict: `c` explains that only a rejected command
can be reviewed. A rejected command stays visible until you explicitly replace
it or leave it in place.

`c` on a conflict opens a snapshot-bound review: command id and kind, the local
intent (explicitly **not accepted**), outbox position, accepted server revision,
accepted history fingerprint and the rejection reason. Type a replacement
command at `> replacement:` and press `Enter`; Orbit validates it, queues it as
new **PENDING** intent and keeps the rejected audit row. Nothing is rebased
automatically. The review fails closed: if it cannot be established, `Enter`
refuses and says so; `Esc` cancels and keeps the rejected command. `Ctrl+U`
clears the replacement and the arrow/page keys scroll the review.

Multi-field connected edits can queue more than one intent. If the server
rejects a later intent, earlier intents can remain queued; inspect the outbox
with `!` or the CLI before retrying.

### The connected command console

Open `:` and choose **Connected commands…**. One console inside the shared shell
covers the connected grammar:

- writes for projects, tasks, habits, metrics, schedules, routines, milestones,
  strategies, notes and folders, including `batch <command; command>` and
  `resolve <rejected-command-id> <replacement-command>`;
- commands that queue nothing: `sync.status`, `sync.observe`, `note.search`,
  `note.sync`, `note.cached`, `note.show <note-id|path.md>`, `routine.list`,
  `strategy.list`, `milestone.list`, `schedule.list`;
- read-only planning: `plan.portfolio`, `plan.compare`, `plan.review`,
  `plan.focus <task-id>` and `plan.group --project <id> [--milestone <id>]`;
- `plan.approve --confirm`, which applies exactly one previously reviewed
  scenario at the same accepted revision, fails closed without a review or when
  the accepted revision changed, and reports the authoritative receipt.

`Tab` fills the selected suggestion, `Ctrl+U` clears the line. Read-only
commands print their output inline and never queue anything. Every write is
shown for explicit confirmation and is queued as a durable **PENDING**
intention — **queued work is not accepted until you flush (`f`) and the server
acknowledges it**, and refreshing (`r`) is what brings the accepted state into
the workspace. Invalid input queues nothing and reports why.

A `resolve <id> <replacement>` in the console opens the same conflict review as
`!`; it does not bypass the review.

The legacy connected section browser is gone. Its `/` record filter and `e`
command-prefill shortcuts were not ported into the shared shell; `/` is search,
`!` is the pending inspector and `:` opens the console. Reads and writes are
preserved; only the discoverability of those two shortcuts differs.

### Rich task details in connected mode

Rich task details work in connected mode:

- `e` opens the full task form. Values load from the accepted snapshot; the
  edits are queued and marked pending until the server accepts them.
- `K` reads and queues ordered acceptance-criteria changes. Criteria come from
  the server route, so this needs connectivity; the modal shows the accepted
  list, and a queued change appears only after flush and refresh.
- `H` reads the task's authoritative semantic activity from the server.
- `i` stages a file, `F` opens the attachment modal, `o`/`E` export accepted
  content and `x` removes an association.

While a queued `SetDetails` change for a task is still pending, another rich
detail save (full `e` or `E` quick edit) for that task is refused with
`A detail change for this task is already pending; close this form with Esc, then press f to flush before editing again`.
This stops one queued edit from silently reverting the other. Flush (`f`) and
refresh, then edit again.

`W` (workflow) and `n` (knowledge note) are local-authority only; the connected
help overlay marks them `[local authority only — unavailable in connected mode]`.

Project status/mode edits, workflow binding, work-item configuration, habit and
metric recording, and note authoring are not key-driven connected surfaces:
switch to the local authoritative workspace for those forms, or use the
equivalent connected console command where one exists (`strategy.objective`,
`strategy.configure`, `project.create`, `habit.*`, `metric.*`, `note.*`). `e` on a project in connected
mode reports this instead of opening a form that cannot submit.

The attachment modal lists accepted attachments first and then locally staged
uploads as `[pending]` rows, with their `pending`/`conflict` state and last
error. Staged bytes remain visible when the server is unreachable; accepted
metadata and accepted content require the server. Removing an accepted
attachment is online-only. Removing a staged upload discards that local intent
without contacting the server. The TUI sends no implicit launch or upload retry
beyond an explicit `f` flush.

If `f` reports a blocked or conflicted upload, the reason and last error stay on
the row. A `conflict` row stops automatic retry: remove and re-import the staged
file, or reconnect and refresh before an explicit retry. The TUI does not
silently reorder or discard durable upload intents.

Server-side attachment garbage collection is not exposed to clients, so the
local **Clean up unreferenced attachment content** action is refused in
connected mode with that explanation.

## Agent monitoring

Open **Agents** in the sidebar for the local workspace list, or the project's
**Agent jobs** view. Select a job and press `Enter`/`a` to inspect its state,
runtime, handoff count and any recorded review decision. Job completion is
execution state: it does not complete the Orbit task. Review/resume is shown as
a disabled action when no explicitly configured agent runtime or connected review
service is available, with the reason visible. Starting or resuming an agent is
never implicit. Connected mode does not project accepted agent jobs into the
shared snapshot, so the Agents view says so explicitly; agent transport is not
part of the connected console.

## Feedback and uncertainty

The status line reports exactly what happened:

- **Accepted** — the authority committed the change (for connected work, after
  the server acknowledged it).
- **Pending** — an external effect or connected command is not yet accepted.
- **Rejected** — the authority refused the change; the prior view remains and a
  connected rejection names its state and reason.
- **Stale** — refresh failed and the previous accepted view is still visible.
- **Unavailable** — a component or extension is not installed. The rest of the
  view remains usable.

A pending command is never shown as accepted state.

## Quit and terminal recovery

`q` quits normal screens and the non-text overlays described under
[Keyboard](#keyboard); `Ctrl+Q` and `Ctrl+C` quit from anywhere, including text
fields and confirmations. If typed input is unsaved, Orbit asks before quitting:
`Enter`/`y` quits and discards it, `Esc`/`n` returns to the editor with the text
intact, and a second `Ctrl+Q`/`Ctrl+C` forces the quit.

Terminal raw mode, the alternate screen and cursor visibility are restored by an
RAII terminal guard on normal exit, recoverable errors and unwinding panics. If a
terminal is left in an unusual state by an external kill, run `stty sane` and
`reset` from a shell.

## Troubleshooting

- **No colors or a limited-color terminal** — Orbit also uses glyphs, bold,
  borders, a `▸` selection rail and status glyphs. `NO_COLOR=1` or `TERM=dumb`
  forces the monochrome styles.
- **A component says unavailable** — the workflow references a component that is
  not registered in this build. Update Orbit or install the providing extension;
  the rest of the view stays usable.
- **A project opens with generated views** — the project has no pinned workflow,
  or the pinned version has no `views` section. That is expected for unbound or
  older projects. Apply a newer workflow (`W`) if you want its declared views.
- **A field shows `!` and a message** — validation refused the form and kept
  your input. Fix the marked field; `Ctrl+S` saves from anywhere.
- **An edit or attachment stays `[pending]`** — the connected change is durable
  outbox/staging intent, not accepted state. Press `f` to flush while online,
  then `r` to refresh. A conflicted upload must be removed and re-imported.
- **Accepted attachment content is unavailable offline** — accepted metadata and
  bytes require the server. Staged uploads still open from local bytes.
- **The workspace is stale** — press `r`. If refresh still fails, the last
  accepted view remains labelled `stale`; check `orbit doctor` and the database
  path.
- **The connected header has no `CONNECTED` label** — the header drops its
  summary only when the row cannot fit it, so the breadcrumb stays readable. The
  session is still connected; `!` shows queued work.
- **Narrow terminal** — use `Tab` to switch panes and `h`/`l` for components.
  `D`/`Enter` opens the inspector: an inset overlay from 80x20, and the whole
  frame below that. Its content is never silently truncated; it scrolls with
  `j`/`k` and PageUp/PageDown.
- **A capability action is dimmed with a code** — the offer is real but cannot
  run yet. `not_authorized` needs `orbit blueprint capability authorize`,
  `capability_not_installed` needs `orbit blueprint capability install`, and the
  other codes name a trusted-composition gap. Nothing is queued while an offer
  is unavailable.
- **A capability outcome is `uncertain`** — the provider did not confirm a
  result, so the request is not completed. Reconcile it explicitly in the result
  panel with `Reconcile as succeeded (--succeed)` or
  `Reconcile as failed (--fail)`; Orbit never re-runs the action to resolve it.
- **`q` does nothing in a text prompt, criteria or attachments list** — use
  `Ctrl+Q` or `Ctrl+C`; elsewhere `q` leaves the screen or overlay.
- **Nothing renders** — confirm the terminal is interactive and at least a few
  rows tall. `orbit-tui` is not a non-interactive command; use CLI commands for
  scripts and pipes.
