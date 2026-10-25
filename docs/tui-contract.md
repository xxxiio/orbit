# Orbit TUI contract

`orbit-tui` is a project-centric Ratatui client. It must not duplicate Orbit
business rules, open a second authoritative store, or mutate canonical domain
tables directly. Reads and mutations go through `orbit-client` and the shared
application services.

## Composition

- **Global navigation** — Home/Today, Inbox, Projects and Agents.
- **Project navigation** — the selected project list and workflow-defined views.
- **Workspace** — typed resolved components (text, list, table, metric, detail,
  kanban, task list, timeline, habit tracker, metric history, knowledge
  references, agent jobs, milestone progress).
- **Item inspector** — selected-item properties, dependencies, contextual and
  capability actions. It is an overlay (`Enter`/`D` opens, `Esc` closes) and
  becomes a whole-frame view below 80x20; there is no docked column.
- **Overlays** — prompts, confirmations, action menus, command palette, search,
  help, inspector, capability results, criteria, attachments, activity,
  pending/conflict review and the connected console.

One shell serves local and connected deployments. Connected mode uses the same
`App` and renderer over a connected `Backend`. Project and task reads come from
the accepted replica; supported mutations queue durable intents and are labelled
pending until server acceptance. Unsupported connected editing surfaces fail
closed with an explanation. There is no second connected event loop or render
tree: the palette opens the **Connected commands** console
(`:` → *Connected commands…*), `!` opens the pending/conflict inspector, and `f`
submits the outbox.

The shared presentation model lives in `orbit-ui`; workflow declarations live in
`orbit-blueprint`. The TUI owns terminal layout, focus, input, scrolling, modals
and theming only.

## Module layout

| Module | Owns |
|---|---|
| `main.rs` | Process entry, the single event loop, quit keys, debounced session-state writes. |
| `app/mod.rs` | `App`, the mutation-status chokepoint, the cached pending count, `request_quit`. |
| `app/state.rs` | `Screen`, `FocusPane`, `Modal`, form/modal state, `StatusKind`, palette and selection types. |
| `app/navigation.rs` | Screen/project/view switching, back history, refresh, detail-model cache and its key. |
| `app/selection.rs` | Cursor keys, selection/scroll restore per view, component switching, `Tab` focus order. |
| `app/actions.rs` | Action catalogue use, compact action bar selection, action menu, palette ranking/MRU, action execution. |
| `app/forms.rs` | Form construction from the selected record, field validation, submission through the backend. |
| `app/connected.rs` | Pending inspector, snapshot-bound conflict review, connected console parsing/execution. |
| `input/dispatch.rs` | The single global key dispatcher for normal screens. |
| `input/modal.rs` | Key handling for every open overlay. |
| `input/text.rs` | Text insertion, backspace and delete primitives with caret positions. |
| `render/mod.rs` | Frame split into header/body/status and the modal layer. |
| `render/layout.rs` | Shell width plan, component minimum/preferred sizes, rect allocation, single-component decision. |
| `render/shell.rs` | Header, sidebar, compact pane bar, project tabs, action bar, status line, panel blocks. |
| `render/components.rs` | Component bodies, table column planning, Kanban windowing, Projects screen, component bodies. |
| `render/modals.rs` | Every overlay, form field rendering and multi-line caret preview, keyboard help. |
| `backend.rs` | `Backend::{Local, Connected}`, typed client calls, refresh/flush, per-command outcomes, cached detail/pending reads. |
| `local_backend.rs` | The only constructor that opens an embedded local authority. |
| `connected_actions.rs` | Pure parser for the connected write grammar (intents, batch, resolve). No I/O. |
| `connected_ops.rs` | Pure parser for read-only planning/sync/note commands and revision-fenced approval. No I/O. |
| `planning_ui.rs` | Read-only planning rendering and request parsing shared with the CLI/runtime. |
| `theme.rs` | Colour tokens, glyphs, breakpoints and panel/selection styles. |
| `ui_state.rs` | Durable navigation-state schema, atomic save and safe restore. |
| `terminal.rs` | RAII raw-mode/alternate-screen ownership. |
| `viewer.rs` | Export-and-hand-off to a configured external viewer. |

`render/` never queries storage. `app/` owns presentation state and calls the
backend. `backend.rs` is the only module that reaches `OrbitClient` /
`AuthorityConnection`; `local_backend.rs` is the only place that opens a local
store.

## State ownership

`App` holds:

- **Navigation and presentation** — screen, focus, active component, selection,
  scroll offsets, history entries, per-view cursor memory, open modal, status
  message, prefix key, viewport width.
- **Caches, not truth** — the detail model cache, its revision counter, the
  durable pending count, and assembled view data from the last refresh.
- **No domain authority** — accepted project/task records always come from the
  backend. Nothing in `App` is written to the database except navigation intent
  through `ui_state`.

`Backend` holds the client, the connection mode and the durable outbox/staging
handles. It is the only authority boundary in the process.

`ui_state` persists navigation intent only: screen, project id, view id, focus,
sidebar/project-list positions, active component, selected entity id and index,
scroll. It never stores form text, queries, overlay state,
domain records or credentials. The palette's recent-destination list is
in-memory by design.

Rendering is a cache-only function of `App`: it may read the cached connection
flag (`Backend::is_connected`), the cached pending count and the cached detail
model. It does not ask the backend for data, build detail models or read the
outbox on a frame — including the first frame after a selection change.

### The mutation-status chokepoint

Every write reports through `App::mutation_status`. It re-reads the durable
pending sum and maps `Option<MutationOutcome>` to user-facing text:

- local authority with no recorded outcome → accepted;
- connected with no recorded outcome, or an unknown state → **pending**;
- `Accepted` → accepted;
- `Pending { state }` → pending, naming the state;
- `Conflict { state, reason }` → rejected/warning, instructing the user to
  inspect the outbox.

A connected mutation is never promoted to accepted because the call returned
quietly. Conflicting outcomes keep the prior accepted view.

## Input: one dispatch rule

All key presses enter `input::dispatch::handle_key` from the event loop. It
applies exactly one rule set, in this order:

1. If an overlay is open, hand the key to `input::modal::handle_modal` and
   return. The dispatch table never also acts on that key.
2. Resolve a pending `g` prefix (`h`, `i`, `p`, `a`), or cancel it with a notice.
3. `Ctrl+P` opens the palette.
4. Match the key against the screen-independent dispatch table.

Rules for changes:

- Add a keyboard behaviour in `input/dispatch.rs` (normal screens) or
  `input/modal.rs` (overlays), and nowhere else. Renderers and `App`
  constructors must not inspect keys.
- A keystroke has one meaning per state. Do not add a second path that performs
  the same mutation from a different module.
- Text surfaces own printable keys. Control chords are commands, never field
  text: forms accept only `Ctrl+S`; palette, console and conflict review accept
  `Ctrl+U` to clear. Other control chords are ignored so a shortcut cannot
  corrupt a value.
- Quit has one path: `App::request_quit`. It inspects
  `App::has_unsaved_input` (dirty form, non-empty palette/search/console/
  conflict text), opens `QuitConfirm` with the overlay preserved, or returns
  true. `Ctrl+Q`/`Ctrl+C` are intercepted in `main.rs` before dispatch so they
  work from every state.
- When adding a key, update the guide, the help overlay in `render/modals.rs`
  and the PTY/unit coverage together. Keys the shell does not bind must not be
  advertised in footers or help.

## Authority and safety

- Every mutation is a typed `OrbitClient` call; the renderer has no SQL and
  spawns no commands.
- Workflow view/action declarations are presentation metadata, not grants.
- Contextual actions are derived from the selected item and validated again by
  the authority. A workflow may mention an action; that never enables it.
- Repeated identical submissions are suppressed (a form submission inside
  250 ms is ignored) and a pending command is never displayed as accepted state.
- Connected writes are queued intents. Flush is explicit (`f`); refresh (`r`)
  reads accepted state. Conflict resolution requires a snapshot-bound review and
  queues a new intention; it never rebases automatically.
- Agent job completion is execution state; it does not complete an Orbit task.
- Missing components or failed refresh retain the rest of the view and show an
  explicit unavailable/stale state.

### Inspector model contract

The inspector is an overlay, not a docked pane, so its model is built once on an
input path and then painted from state:

- `App::open_inspector()` builds the model for the current selection, records the
  subject (`InspectorSubject`), the originating focus and the scroll offset, then
  opens `Modal::Inspector`. Rendering reads that stored model only: it never
  builds a model and never issues a backend read.
- Every path that changes the inspected subject or its accepted data calls
  `App::refresh_inspector()` (selection/component movement, cursor restore,
  screen/project/view navigation, refresh, mutations and capability execution).
  If the subject no longer exists the inspector shows
  `This item is no longer available` with its last known title/kind instead of
  silently switching to a different item.
- Closing the inspector (`Esc`, `Enter` or `D`) restores the pane that owned the
  keyboard before it opened. `Tab`/`Shift+Tab` cycle only the visible
  sidebar/workspace panes; there is no hidden focus stop.
- The inspector footer is `Esc close · j/k scroll · Enter close · a actions ·
  e edit`. `a` opens the contextual action menu, `e`/`E` launch the existing edit
  forms, and a successful save reopens the inspector on the same entity.
- A title-only quick edit is a **project-screen** fast path: it re-resolves that
  project and patches the one row instead of re-reading the whole workspace.
  From Home/Inbox the same save takes the ordinary refresh, and a priority
  change always does because ranking can change.
- Tests count inspector builds: repeated frames add none, opening or an explicit
  refresh builds exactly once, and render never builds.

## Responsive behaviour

| Width / height | Layout |
|---|---|
| ≥ 76 columns | Sidebar and workspace (two regions, no third column) |
| < 76 columns | One primary pane selected by focus, with a pane bar |
| Inspector at ≥ 80x20 | Inset overlay over the two regions |
| Inspector below 80x20 | Whole-frame inspector |

No width docks a details column; the workspace always receives every column the
sidebar does not claim.

Each component declares a readable minimum and a preferred size in
`render/layout.rs` (a board needs 56×8, a table 34×5, a metric 16×3). The view's
declared layout is used only while every component can receive its minimum;
otherwise the workspace shows one focused component with a `component n/m` rail.
Kanban columns keep a 20-cell minimum and window horizontally; tables drop
secondary columns instead of squashing them; details open as an overlay rather
than being silently truncated.

## Connected surfaces

The shared shell exposes:

- `!` — pending/conflict inspector: `[pending]`/`[conflict]` tags, payload,
  recorded state, attempts, `accepted receipt: none (not accepted)` until
  acknowledged; `r` reload, `f` flush, `c` review a conflict, `Enter` detail.
- `c` on a conflict — snapshot-bound review with command identity, local intent,
  outbox position, accepted revision and history fingerprint, rejection reason,
  and a replacement input. `Enter` validates and queues new pending intent; no
  review → no queue.
- `:` → **Connected commands…** — the complete write grammar
  (project/task/habit/metric/schedule/routine/milestone/strategy/note/folder,
  `batch`, `resolve`) plus read-only `sync.status`, `sync.observe`,
  `note.search`/`note.sync`/`note.cached`/`note.show`,
  `routine.list`/`strategy.list`/`milestone.list`/`schedule.list`, and
  `plan.portfolio`/`plan.compare`/`plan.review`/`plan.focus`/`plan.group`.
  `plan.approve --confirm` applies exactly one previously reviewed scenario at
  the same accepted revision and reports the authoritative receipt.
- `f` — flush; `r` — refresh accepted state.

The console and the inspector are overlays in the same `Modal` enum as every
local overlay. Adding a connected surface must not introduce a second shell,
render tree or event loop.

## Testing

`cargo test -p orbit-tui` covers layout, focus, selection restore, forms,
palette ranking, table planning, Kanban windowing, cache reuse, pending-count
caching, refusal paths and the no-SQL/no-process boundary. PTY scenarios A–G
live in `tests/acceptance/tui`; `tests/acceptance/tui/README.md` maps each
scenario to its file and evidence. `scripts/tui-captures.py` regenerates the
reconstructed screens in `.agent/evidence/m29-captures/`.

## Related

- [Terminal workspace guide](guides/tui-workspace.md)
- [Blueprint-defined views](guides/blueprint-views.md)
- [UI view reference](reference/ui-views.md)
- [UI component development](developers/ui-components.md)
