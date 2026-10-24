---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-24"
task_id: TASK-0007
---

# Journal Entry: 2026-10-24 — M29 I1–I3 interaction quality

## Context

M28 left a functional but rough TUI. The first concrete defect was in
`ui_state.rs`: `selected_id` was persisted as `None`, so restart lost the user's
working context. The action bar repeated the `e` key and omitted status; the
palette had no ranking, recents or disabled guard; task forms buried priority
after planning fields and reported validation only as a banner; the shell used
fixed sidebar/detail widths and boxed every Kanban column.

## Work completed

- Persisted navigation intent: screen/project/view/focus, active component,
  stable selected id, index, component scroll and detail scroll. Restore opens
  the project/view first, then applies focus/selection; id resolution searches
  the whole view, falls back to a clamped index, and preserves a skipped restore
  when the authority failed to load. Load ignores corrupt and newer-version
  files.
- Save-on-change is debounced to the idle poll and exit; writes are atomic
  (temp + fsync + rename).
- Compact action bar (max three context-ranked actions + `a more` + `q quit`),
  grouped action menu with disabled reasons, context-aware hotkey resolution
  (`E` quick edit, `S` status menu), ranked palette with prefix/word/fuzzy
  scoring, destination MRU and disabled refusal.
- Quick edit (title + priority) through existing commands; task forms reordered
  to title/priority/description/labels first; range/timestamp errors mark and
  focus the field; multiline caret window; explicit Save/Cancel affordances.
- Component-aware layout: content-weighted widths, per-kind minimums,
  single-component rail below the sum of minimums, compact pane bar, windowed
  Kanban with header/rule/card and ownership-aware selection, priority table
  columns, clamped detail scroll and a colour-independent theme with complete
  status glyphs.

## Decisions made

- Navigation state remains intent only; ids are resolved against accepted data
  and never persisted as domain payloads.
- A key is advertised only when pressing it performs that action; otherwise the
  action remains in the menu/palette without a key hint.
- Quick edit deliberately excludes status so terminal transitions keep the
  existing confirmation path; status lives in `S`.
- Kanban columns use a header + rule + card model instead of per-column boxes to
  return content width.
- Selection ownership is separate from pane focus so inactive boards never show
  a phantom cursor.

## Files or components changed

- `crates/orbit-tui/src/ui_state.rs`, `app/` (selection, navigation, actions,
  forms), `input/` (dispatch, modal), `render/` (layout, shell, components,
  modals), `theme.rs`, `backend.rs`.
- `tests/acceptance/tui/test_m29_selection.py`, `test_m29_workflows.py`.

## Validation performed

- `cargo test -p orbit-tui --offline` 208 passed; clippy `-D warnings`; fmt
  clean.
- PTY Scenario A (3 tests) and Scenarios B/C/D/F (10 tests) pass; full TUI suite
  54 passed/1 skipped.
- Five-size layout captures and assertions at 60x18, 80x24, 100x30, 120x35 and
  160x45.

## Problems encountered

- A PTY race coalesced `Esc` with the next key into `Alt+<char>`, typing into a
  search box and making one test flaky. Tests now wait for settled frames, and
  text surfaces ignore ALT-modified characters.
- An initial Kanban minimum width broke a 140-column board test; the minimum was
  reduced to keep four readable columns in the available workspace.
- Oracle gates found stale cursors on refresh, key hints that did not match
  behaviour, phantom selection in inactive boards and stale detail data; all
  were fixed with regression tests.

## Remaining work

- I4–I6: connected consolidation, module extraction, measured performance,
  scenarios A–G, documentation and captures.

## New TODOs

- None beyond the existing TODO-0013 (per-device ACLs) and TODO-0014 (server
  attachment GC).

## Risks or blockers

- None blocking. Interaction measurements are PTY wall-clock medians; process
  spawn dominates cold start.
