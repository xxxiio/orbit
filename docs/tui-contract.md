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
- **Details** — selected-item properties, dependencies and contextual actions;
  docked from 120 columns when the workspace keeps its readable minimum,
  otherwise an overlay.
- **Overlays** — prompts, confirmations, action menus, command palette, search,
  help, details, criteria, attachments, activity, pending/conflict review and the
  connected console.

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
| `render/components.rs` | Component bodies, table column planning, Kanban windowing, Projects screen, docked details. |
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
scroll and detail scroll. It never stores form text, queries, overlay state,
domain records or credentials. The palette's recent-destination list is
in-memory by design.

Rendering is a function of `App`: it may read the cached connection flag
(`Backend::is_connected`) and the cached pending count, but it must not ask the
backend for data, build detail models or read the outbox on a frame.

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

### Detail-cache contract

The docked detail pane is repainted every frame at the wide breakpoint, so its
model is cached rather than rebuilt:

- The key is (`screen`, project id, view id, projects selection, active
  component, selected index, selected entity id, detail revision).
- `App::detail_model()` returns the cached clone on a key hit; a miss builds the
  model once, including backend reads for task description/priority/labels,
  acceptance-criteria counts and attachment counts.
- Any refresh, mutation, screen/project/view/component change, selection change
  or restore calls `invalidate_detail_cache()`, which bumps the revision. The
  revision exists because an in-place refresh can keep the same identity.
- A single title-only quick edit patches one row and re-resolves the project
  instead of re-reading the whole workspace; priority changes deliberately keep
  the full refresh because ranking can change.
- Stale detail scroll is clamped (`MAX_DETAIL_SCROLL`) so a restored offset can
  never scroll the pane out of view. The renderer records the last painted
  maximum in `detail_max_scroll` so scrolling up from an over-scrolled value
  moves immediately.

## Responsive behaviour

| Width | Layout |
|---|---|
| ≥ 120 columns | Sidebar, workspace and docked details (if the active component keeps its minimum width) |
| 76–119 columns | Sidebar and workspace; details are an overlay |
| < 76 columns | One primary pane selected by focus, with a pane bar |

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
