# Orbit TUI contract

`orbit-tui` is a project-centric Ratatui client. It must not duplicate Orbit
business rules, open a second authoritative store, or mutate canonical domain
tables directly. Reads and mutations go through `orbit-client` and the shared
application services.

## Composition

- **Global navigation** — Home/Today, Inbox, Projects and Agents.
- **Project navigation** — the selected project list and blueprint-defined
  views.
- **Workspace** — typed resolved components (text, list, table, metric, detail,
  kanban, task list, timeline, habit tracker, metric history, knowledge
  references, agent jobs, milestone progress).
- **Details** — selected-item properties, dependencies and contextual actions.
- **Overlays** — prompts, confirmations, action menus, command palette, search,
  help, details and messages.

Connected mode uses the same `App`/renderer over a connected `Backend`. Project
and task reads come from the accepted replica; supported mutations queue durable
intents and are labelled pending until server acceptance. Unsupported connected
editing surfaces fail closed with an explanation. There is no second connected
event loop or render tree: the palette opens the **Connected commands** console
(`:` → *Connected commands…*), `!` opens the pending/conflict inspector, and
`f` submits the outbox. The console covers the complete write grammar
(project/task/habit/metric/schedule/routine/milestone/strategy/note/folder,
batch and `resolve`) plus read-only `sync.status`, `sync.observe`,
`note.search`/`note.cached`/`note.show`, and
`plan.portfolio`/`plan.compare`/`plan.review`/`plan.focus`/`plan.group`.
`plan.approve --confirm` applies exactly one previously reviewed scenario at the
same accepted revision and reports the authoritative receipt.

The shared presentation model lives in `orbit-ui`; blueprint declarations live
in `orbit-blueprint`. The TUI owns terminal layout, focus, input, scrolling,
modals and theming only.

## Authority and safety

- Every mutation is a typed `OrbitClient` call; the renderer has no SQL and
  spawns no commands.
- Blueprint view/action declarations are presentation metadata, not grants.
- Contextual actions are derived from the selected item and validated again by
  the authority.
- Repeated identical submissions are suppressed until refresh. A pending
  command is never displayed as accepted state.
- Agent job completion is execution state; it does not complete an Orbit task.
- Missing components or failed refresh retain the rest of the view and show an
  explicit unavailable/stale state.

## Responsive behaviour

| Width | Layout |
|---|---|
| ≥ 120 columns | Sidebar, workspace, details |
| 76–119 columns | Sidebar and workspace; details are a modal |
| < 76 columns | One primary pane selected by focus |

Multi-component views collapse to one focused component on very narrow or short
terminals. Long text wraps or scrolls; details open as an overlay rather than
being silently truncated.

## Keyboard and feedback

`j`/`k`, arrows, `h`/`l`, `Tab`/`Shift+Tab`, `Enter`, `Esc` (back), `a`/`s`,
`c`, `C`, `n`, `Space`, `p`/`P`, `A`, `G`, `[`/`]`, `g h/i/p/a`, `:`, `/`, `r`,
`D`, `?`, `q`, `Ctrl+Q`, `Ctrl+C` are
documented in the [terminal workspace guide](guides/tui-workspace.md). Color is
never the only state signal: glyphs, borders and markers remain in monochrome
terminals.

## Testing

Focused tests cover schema validation, duplicate IDs, unsupported components,
bindings, legacy defaults, metrics/lists/tables/kanban rendering, wide and
narrow buffers, focus, scrolling, missing components, stale/error states,
project switching, palette cancellation, duplicate submission suppression and
the no-SQL/no-process architecture boundary. Broad executable acceptance
remains part of the M15 profile.

## Related

- [Terminal workspace guide](guides/tui-workspace.md)
- [Blueprint-defined views](guides/blueprint-views.md)
- [UI view reference](reference/ui-views.md)
- [UI component development](developers/ui-components.md)
