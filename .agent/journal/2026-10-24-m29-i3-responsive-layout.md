---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-24"
task_id: null
---

# Journal Entry: 2026-10-24 — M29 I3 responsive layout, board, table and detail density

## Context

MILESTONE-0029 I3 covers EC-005 (layout adapts at 60x18, 80x24, 100x30,
120x35 and 160x45 with component-aware minimums; Kanban scrolls horizontally,
keeps the selection visible and uses real transitions; tables and details
prioritise title/status/priority) and the theme half of EC-006. The M28 shell
worked but reserved fixed sidebar/detail widths, gave layout siblings equal
space, boxed every Kanban column in its own border, squashed table columns
equally and scrolled the detail pane past the end of its content.

## Work completed

- Replaced fixed shell widths with a `ShellPlan` computed before drawing: the
  sidebar is content-aware inside an 18..26 band from 160 columns up and pinned
  at 22 below that, the detail pane grows 30 -> 34 -> 40 with the terminal, and
  it only docks while the active component still receives its readable minimum.
  The workspace is a `Min` region, so it absorbs every column details do not
  claim, including after `D`.
- Added per-component minimum sizes (a board needs 56x8, a metric 16x3, text
  24x4) used by three layout paths: minimum-then-weighted `allocate`, adaptive
  grid columns, and a single-component rail when a declared layout cannot honour
  every minimum.
- Added a narrow-mode pane bar naming `Workspace`/`Sidebar`, marking the focused
  pane and showing `Tab switch · D details`, so the sidebar is never hidden
  without a visible way back. Focus state is untouched by the switch.
- Rebuilt Kanban presentation: 20-cell readable columns, horizontal windowing
  (`columns 2-4 of 9`), a `▾` selected-column marker plus a heavy rule, column
  item counts, ellipsized card titles that never wrap the frame, selection kept
  visible in the window (even while another pane has focus, with `›`), and a
  legend that fits `j/k card · h/l panel · s status · Enter open` to the width.
- Rebuilt table sizing around column roles: the title reserves half the row,
  status/priority are compact and dropped last, date/progress columns are
  dropped first, and a status column that cannot fit its word degrades to its
  glyph instead of one-character cells.
- Rebuilt list/table/projects rows with a full-width selected bar, ellipsized
  secondary text that disappears on narrow panes, and status-glyph-first rows.
- Rebuilt the detail pane: section labels, aligned property keys, wrapped values
  with continuation indentation, clamping so `detail_scroll` past the content
  never blanks the pane, and a `1-6 of 10 · ↑↓ scroll` position indicator.
- Refined the footer and header: the action bar drops trailing actions instead
  of clipping, the status message is never truncated (a key hint is only added
  when it fits), the header drops its project/inbox summary rather than pushing
  the breadcrumb off the row, and tab bars window around the active view.
- Refined `theme.rs`: one accent, quiet grey, three semantic signals, quiet
  section labels, table headers, italic empty states, and a `selected_fill` that
  stays non-colour (REVERSED) under `NO_COLOR`/`TERM=dumb`.
- Added seven TestBackend tests covering the five supported sizes, the compact
  pane bar, Kanban windowing with 7 long-titled columns, table column priority
  at 60 vs 160 columns, detail overflow clamping, selection after refresh,
  no-colour focus/selection, plus a very-small-terminal frame check.

## Decisions made

- Pin the sidebar at 22 columns below 160: the acceptance helpers slice the
  workspace at `SIDEBAR_WIDTH` and a shifting navigation column reads as
  instability; content-aware growth starts only where there is slack.
- Keep the detail pane docked only at `>= WIDE_BREAKPOINT`, matching
  `App::wide_detail` and `D` opening the detail modal below it, and let the
  workspace floor win when the active board/list would be squeezed.
- Drop Kanban column borders in favour of a header, count and rule: it removes
  nested boxes, returns two cells per column and still marks the selected column
  with `▾` and `━`.
- Never truncate the status message: an M28 connected scenario asserts its text,
  and the user's newest information outranks a key hint.
- Keep `render::draw(frame, &mut App)` and leave `app.rs` untouched; the only
  app-side contract still consumed is `viewport_width` through `set_viewport`.

## Files or components changed

- `crates/orbit-tui/src/render.rs`
- `crates/orbit-tui/src/theme.rs`
- `crates/orbit-tui/src/main.rs` (new `m29_layout_presentation_tests` module only)

## Validation performed

- `cargo test -p orbit-tui --offline` — 173 passed, 0 failed (165 pre-existing
  tests unchanged, 8 added).
- `cargo clippy --offline -p orbit-tui --all-targets -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean.
- `cargo build -p orbit-tui -p orbit-cli --offline` then
  `python3 -m pytest -q tests/acceptance/tui/test_m28_scenarios.py tests/acceptance/tui/test_m29_selection.py`
  — 39 passed.
- `python3 -m pytest -q tests/acceptance/tui` — 42 passed (full TUI suite).

## Problems encountered

- An early footer variant truncated the status text to make room for key hints,
  which broke `test_m28_connected.py` waiting for `pending project`; the status
  is now never truncated and hints yield instead.
- A 22-cell minimum Kanban column hid the `In progress` column at 140x40 and
  broke `m28_render_tests::board_renders_workflow_columns_and_selection`; the
  minimum is 20 cells, which keeps four readable columns in an 88-cell workspace.

## Remaining work

- `docs/` TUI key/layout captures and PTY scenarios A–G belong to I6.
- I2 owns the action-bar ranking and contextual palette; this pass only made the
  existing bar fit its width.
- I5 will move these renderers into `render/` modules; sizing helpers were kept
  together at the top of `render.rs` to make that move mechanical.

## New TODOs

- None.

## Risks or blockers

- Layout minima are tuned by measurement on the five supported sizes; a future
  font/terminal with different cell metrics should be re-checked with the
  five-size test before adjusting the constants.
