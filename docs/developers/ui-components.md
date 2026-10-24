# UI component and renderer development

Orbit separates declarative views, application-facing data, trusted component
registration, action dispatch and terminal rendering. Do not add a second
project/task model or query SQLite from a renderer.

## Data flow

```text
pinned Blueprint.views
        │
        ▼
orbit-ui :: resolve_blueprint_views
        │  uses ComponentRegistry + closed bindings
        ▼
ResolvedView { components, bodies, selections }
        │
        ├── orbit-tui renderer (Ratatui)
        └── future Flutter renderer
```

Application data enters through `orbit_ui::ProjectData` and
`orbit_ui::WorkspaceData`. The local implementation is
`orbit_client::view_data`; it maps `OrbitClient` application queries into these
contracts. The renderer only sees resolved data.

## Where the TUI code lives

The terminal client is split by responsibility; the full ownership table is in
the [TUI contract](../tui-contract.md#module-layout).

- `crates/orbit-tui/src/app/` — presentation state and actions (`mod.rs`,
  `state.rs`, `navigation.rs`, `selection.rs`, `actions.rs`, `forms.rs`,
  `connected.rs`).
- `crates/orbit-tui/src/input/` — the one dispatcher (`dispatch.rs`), overlay
  handling (`modal.rs`) and text primitives (`text.rs`).
- `crates/orbit-tui/src/render/` — `layout.rs` (space), `shell.rs` (chrome),
  `components.rs` (bodies) and `modals.rs` (overlays).
- `crates/orbit-tui/src/backend.rs` — the local/connected authority boundary.
  `local_backend.rs` is the only constructor that opens an embedded local store.
- `connected_actions.rs` and `connected_ops.rs` — pure connected command
  parsers. They perform no I/O and never queue anything themselves.
- `ui_state.rs` — durable navigation intent, not domain state.
- `theme.rs`, `terminal.rs`, `viewer.rs`, `planning_ui.rs` — tokens, terminal
  ownership, external viewer hand-off and read-only planning helpers.

### State ownership

`App` owns screen/focus/selection/scroll, forms, overlays, status text and the
caches used by the frame (detail model, durable pending count). It does not own
domain truth. Every accepted record comes from `Backend`, which wraps
`OrbitClient` / `AuthorityConnection`.

The renderer is a pure function of `App`. It may read the cached connection flag
and the cached pending count, but it must not read the outbox, build detail
models or query storage during a frame. Anything chrome needs is cached by an
explicit lifecycle event (queue, flush, refresh, navigation).

### One dispatch rule

Every key press goes through `input::dispatch::handle_key`. If an overlay is
open, that function delegates exclusively to `input::modal::handle_modal`; it
never also applies a normal-screen binding. New keys belong in one of those two
modules plus an `App` method — not in a renderer, constructor or widget. Text
surfaces own printable keys; only `Ctrl+S` (forms) and `Ctrl+U`
(palette/console/conflict) are treated as commands. Quit has one path,
`App::request_quit`, and is intercepted for `Ctrl+Q`/`Ctrl+C` in `main.rs`.

### Authority boundaries

- Mutations are typed `OrbitClient` calls. No SQL, no process spawning, no file
  writes to the database from the TUI.
- Workflow declarations are presentation hints. Availability comes from the
  selected `SelectionItem` and application services, and the authority validates
  again.
- Connected writes stay pending until acknowledged. `App::mutation_status`
  converts the per-command disposition into accepted/pending/rejected text and
  refreshes the cached pending count; a missing disposition fails closed as
  pending.
- `plan.approve --confirm` is the only read-side command that writes; it is
  fenced to the reviewed accepted revision and consumes the review.

### Detail-cache contract

The docked detail pane is drawn every frame at the wide breakpoint, so the draw
path is cache-only:

- `App::cached_detail_model()` is the only detail read the renderer may make. It
  returns the model for the current key or `None`; it never builds a model and
  never touches the backend. A miss paints a neutral placeholder for one frame.
- `App::seed_detail_cache()` builds and stores the model outside render. Input
  and lifecycle paths seed it after any subject/data change: selection and
  component movement, cursor restore, screen/project/view navigation, refresh
  and mutations, so the first frame after the change is a hit.
- the key covers screen, project id, view id, project-list selection, active
  component, selected index, selected entity id and the detail revision;
- every refresh, mutation, navigation, selection or restore change calls
  `invalidate_detail_cache()`, which bumps the revision; in-place refresh keeps
  identity, so the revision is what makes the next seed rebuild (including task
  detail, criteria count and attachment count reads);
- the renderer records the last painted maximum scroll so over-scrolled values
  recover immediately, and restored detail scroll is clamped.

## Adding a trusted component

1. Add the stable ID to `COMPONENT_TYPES` in
   `crates/orbit-blueprint/src/presentation.rs`.
2. Add a `ComponentDescriptor` in
   `crates/orbit-ui/src/registry.rs` with accepted sources, fields, user
   actions and renderer support.
3. Add a `ComponentBody` variant in `crates/orbit-ui/src/model.rs` if the
   existing bodies cannot represent the output.
4. Resolve the body in `crates/orbit-ui/src/resolve.rs`. Apply closed
   filter/sort/limit bindings through `binder.rs`; never accept raw SQL.
5. Give the body a readable minimum and preferred size in
   `crates/orbit-tui/src/render/layout.rs` (`component_min_size`,
   `preferred_height`, `preferred_width`) so the workspace can decide whether to
   keep the declared layout or collapse to one component.
6. Render the body in `crates/orbit-tui/src/render/components.rs`, including
   empty, loading, error, stale, focus, selection and narrow-terminal behaviour.
   A table must plan its columns rather than squash them; a board must window
   columns instead of shrinking below 20 cells.
7. Add schema, registry, resolution and terminal-buffer tests, including one
   buffer at 60×18 and one at the widest supported size.
8. Document the component and its options.

Registration is explicit and compiled in. The TUI never loads third-party
dynamic libraries, scripts or arbitrary executables. A future plugin can
register a descriptor through the same interface, but manifests may reference
only approved component IDs.

The built-in registry is constructed by `ComponentRegistry::builtin()`.
`register` rejects duplicate IDs and IDs absent from the shared schema, so a
new trusted component requires both a schema ID and a descriptor (plus a
resolution/renderer branch).

## Adding a modal or overlay

1. Add a variant to `Modal` in `crates/orbit-tui/src/app/state.rs` and any
   supporting state next to it. Keep the payload presentation-only.
2. Open it from an `App` method in `app/actions.rs`, `app/forms.rs` or
   `app/connected.rs`; do not construct the modal in a renderer.
3. Handle its keys in `input/modal.rs`. Text surfaces must ignore control
   chords other than the documented ones, and must be added to the `q`
   exclusion list at the top of the handler so `q` cannot quit mid-word.
4. If it holds typed text, add it to `App::has_unsaved_input` so quit and
   discard prompts protect the draft.
5. Render it in `render/modals.rs` under the `Modal` match, and give it a size
   in `modal_size`. Use `modal_block` for the double border and keep a footer
   that names the real keys.
6. Add buffer tests for opening, narrowing and closing, plus a PTY check when
   the overlay has its own key loop.

Overlays are drawn over the current screen; they must not mutate accepted state
on open. A destructive or queuing action goes through `Modal::Confirm` and
`App::execute_pending`.

## Data providers

`orbit_ui::ViewDataSource` is the client-facing read contract:

```rust
pub trait ViewDataSource {
    fn workspace(&self) -> Result<WorkspaceData, ViewDataError>;
    fn project(&self, project_id: &str) -> Result<ProjectData, ViewDataError>;
}
```

The local `OrbitClient` implements it in `crates/orbit-client/src/view_data.rs`.
The connected/embedded `AuthorityConnection` exposes `workspace_view_data`,
`project_view_data` and `resolve_connected_project_views` over its accepted
snapshot in `crates/orbit-client/src/unified.rs`; `orbit_ui::snapshot` performs
the accepted-only mapping. A provider must:

- use existing application/client queries, not repository internals;
- return project-scoped rows for project views;
- preserve accepted-versus-pending distinctions;
- bound collections and avoid reloading entire databases per render;
- map errors to `ViewDataError` with a useful code;
- never expose write handles through the view model.

`ProjectData` includes tasks with readiness/dependencies, workflow work items
with revisions and allowed transitions, milestones, knowledge, habits, metrics,
agent jobs and scheduled blocks. Workspace data includes aggregate projects,
inbox tasks and agent jobs for Home/Inbox/Agents. Connected snapshots do not
carry workflow presentation/work-item tables, so the adapter returns generated
views and never invents missing work items. Task detail (description, priority,
labels), acceptance criteria and activity are read through the TUI backend's
snapshot-first detail path, not from the thin workspace snapshot.

## Bindings and query safety

`orbit_blueprint::presentation` defines the closed source list and component
fields. `orbit_ui::binder` applies equality filters, stable sorting and a 1–200
limit. Unknown filter keys fail closed (empty result) rather than broadening a
query. There is no expression language, SQL fragment, file path or command field
in the schema.

Project views default to the selected project. Global Home/Inbox/Agents views
are generated by the host and aggregate workspace data explicitly; a workflow
cannot request a cross-project query.

## Actions

`orbit_ui::actions` owns the catalogue shared by the compact action bar, the
action menu and the command palette. Renderers must:

- derive availability from the selected `SelectionItem` and the current
  application context;
- ignore workflow `actions` declarations when deciding authority;
- route every mutation through `OrbitClient`/application services;
- show pending, accepted, rejected and uncertain states distinctly;
- suppress duplicate submissions;
- keep destructive operations behind a confirmation;
- keep disabled entries visible in the menu with their reason, never in the
  compact bar.

The TUI's `app/actions.rs` ranks the bar (at most three enabled actions with real
keys) and the palette (label prefix, word start, subsequence, substring,
description, multi-word; current-context boost; stable tie-break; session MRU for
destinations). Blueprint declarations are presentation hints. Capability grants
remain in trusted host composition, and external effects still use durable
execution claims.

## Version compatibility

`Blueprint.views` is optional. Legacy manifests resolve to generated defaults.
View IDs and component IDs are stable within a published version. Published
versions are immutable. New versions do not rebind existing projects or migrate
work items. A resolver must tolerate:

- a component type registered in a newer build than the renderer;
- a component removed from a future registry;
- a corrupted or locally edited view definition;
- a project whose pinned version no longer exists.

In those cases, show a missing/error component and keep the rest of the view.

## Renderer responsibilities

A renderer owns:

- terminal/window layout, responsive breakpoints and component minimums;
- theme, color tokens and monochrome fallback;
- focus, selection, scrolling and keyboard input via the one dispatcher;
- modals, prompts, confirmation and error surfaces;
- details and contextual action menus;
- terminal-buffer rendering tests.

A renderer must not:

- infer permissions from a workflow;
- open a second authoritative store;
- recalculate task/project business rules;
- query the backend while drawing (use `App`'s cached values);
- swallow missing-component or stale-data states;
- silently truncate long critical text.

## Future Flutter integration

The `orbit-ui` schema is framework-neutral and serializable: `ResolvedView`,
`ResolvedComponent`, `ComponentBody`, `SelectionItem`, `DetailModel`,
`AvailableAction`, `ProjectData` and `WorkspaceData`. A Flutter client can:

- consume the same resolved JSON through a client bridge;
- implement one widget per stable component ID;
- reuse the action catalogue for menus and the palette;
- display the same loading/empty/error/stale/missing distinctions;
- use the same project/version pinning and action authority rules.

Flutter is not implemented in this repository and is not required for the
terminal milestone. Renderers should not share Ratatui types across this
boundary.

## Tests

Run focused checks:

```sh
cargo test -p orbit-blueprint -p orbit-ui -p orbit-client -p orbit-tui --offline
cargo clippy -p orbit-blueprint -p orbit-ui -p orbit-client -p orbit-tui --all-targets -- -D warnings
python3 -m pytest -q tests/acceptance/tui
```

The terminal suite uses Ratatui's `TestBackend` for wide/narrow buffers, focus,
scrolling, missing components, modal cancellation and the detail-cache/pending
count invariants. Source-shape architecture checks supplement these tests but do
not replace them. The PTY suite drives the real binary; its scenario map is in
`tests/acceptance/tui/README.md`, and `scripts/tui-captures.py` writes the
reconstructed screens indexed in `.agent/evidence/m29-captures/README.md`.

See the [UI view reference](../reference/ui-views.md) for the exact schema.
