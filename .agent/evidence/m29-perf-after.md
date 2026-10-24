# M29 TUI interaction after

- Recorded baseline: `227edbe3ef5af7137c1a9496c0091a56073ebc2b` (dirty tree),
  `/tmp/opencode/m29-20261024T094956Z`, 5 repeats (`m29-perf-baseline.json`).
- After: `3111f5c815902022b7a035473a18c4d52a24bd43` + these changes (dirty tree),
  `/tmp/opencode/m29-after-20261024T100915Z`, 5 repeats
  (`m29-perf-after.json`).
- Binary: `target/debug/orbit-tui` (debug), rebuilt for this run.
- Same probe, sizes, session order, fixture shape (20 projects / 200 tasks /
  1017 activities / 6 attachments / 8 agent jobs) and session-state reset as
  the baseline. The fixture was rebuilt from `scripts/tui-fixture.py`, so the
  recorded before/after pairs use different database files with the same shape
  and titles: record ids and timestamps differ, and the probe never compares
  fixture bytes.

Because the recorded baseline was collected on a different binary build and
the machine drifts between runs, the first table also uses a **same-era
baseline**: the current HEAD without this diff, built at the same time
(`/tmp/opencode/orbit-tui-baseline`, 176802184 bytes) and probed against the
same fixture. The after binary is 176902880 bytes. The recorded 227edbe
baseline is shown for continuity.

## Before / after (median / p90, ms)

Same-era baseline (HEAD without this diff) → after:

| Operation | Before 100x30 | After 100x30 | Before 160x45 | After 160x45 |
|---|---:|---:|---:|---:|
| `cold_start_first_frame` | 100.0 / 101.2 | 101.2 / 102.6 | 102.2 / 102.6 | 101.7 / 108.3 |
| `search_open_query_first_result` | 59.7 / 60.2 | 60.0 / 62.2 | 61.7 / 62.8 | 62.1 / 62.9 |
| `project_switch_open_leave_open_next` | 82.3 / 84.0 | 82.6 / 84.1 | 89.0 / 101.3 | 88.5 / 90.9 |
| `open_task_detail` | 4.7 / 5.2 | 4.2 / 4.3 | 7.6 / 8.0 | **5.3 / 5.7** |
| `scroll_task_table_20_rows` | 104.3 / 104.6 | 104.2 / 104.9 | 107.2 / 108.9 | 106.3 / 106.7 |
| `save_quick_edit_title` | 173.2 / 180.2 | **82.6 / 83.9** | 173.3 / 189.5 | **87.3 / 92.2** |
| `refresh_project` | 20.7 / 22.7 | 19.9 / 22.2 | 26.3 / 26.6 | 25.1 / 28.4 |
| `switch_blueprint_view` | 21.5 / 22.8 | 21.5 / 21.9 | 93.5 / 100.6 | **77.6 / 80.6** |

Recorded 227edbe baseline → after:

| Operation | Before 100x30 | After 100x30 | Before 160x45 | After 160x45 |
|---|---:|---:|---:|---:|
| `cold_start_first_frame` | 96.3 / 102.1 | 101.2 / 102.6 | 95.9 / 106.0 | 101.7 / 108.3 |
| `search_open_query_first_result` | 60.2 / 60.3 | 60.0 / 62.2 | 61.5 / 61.5 | 62.1 / 62.9 |
| `project_switch_open_leave_open_next` | 82.6 / 84.3 | 82.6 / 84.1 | 87.6 / 88.0 | 88.5 / 90.9 |
| `open_task_detail` | 4.3 / 4.8 | 4.2 / 4.3 | 7.3 / 7.6 | 5.3 / 5.7 |
| `scroll_task_table_20_rows` | 104.7 / 105.8 | 104.2 / 104.9 | 107.6 / 109.6 | 106.3 / 106.7 |
| `save_quick_edit_title` | 161.6 / 174.4 | **82.6 / 83.9** | 178.3 / 197.6 | **87.3 / 92.2** |
| `refresh_project` | 19.5 / 20.3 | 19.9 / 22.2 | 25.9 / 29.0 | 25.1 / 28.4 |
| `switch_blueprint_view` | 20.9 / 22.0 | 21.5 / 21.9 | 98.5 / 112.7 | **77.6 / 80.6** |

## What was measured, and what dominated

The candidates were measured directly against the fixture before changing code,
both with a temporary `#[ignore]` timing test (debug build, same database) and
with a temporary env-gated trace of key-handler and draw durations inside the
real binary driven by the PTY probe.

- **Quick edit (173 ms same-era, 162/178 ms recorded).** Key-handler trace: a
  save spent ~147 ms inside the `Enter` handler. Direct timing split it into
  `task_dto` + `rename_task` ≈ 35 ms and `refresh_after_mutation` ≈ 103 ms,
  of which `refresh_workspace` was ~90 ms and `refresh_project` ~17 ms. The
  whole-workspace reload re-read projects, all tasks, readiness, per-project
  evidence, habit history, knowledge and agent jobs — none of which a title
  edit can change. Typing overhead in the harness (2 chars × 10 ms) accounts
  for the rest.
- **Scroll 20 rows (104–107 ms).** The probe writes 20 `j` keys with
  `delay=0.005`; that alone is 100 ms of the sample. Per-key handler time is
  ≤0.02 ms and each frame ~5 ms at 160x45. Dominated by the harness key pacing,
  not backend or rendering; left alone.
- **Project switch (82–89 ms).** Four sidebar `j` presses at 10 ms delay
  (40 ms) plus two project opens. Each open costs ~17 ms (scoped project
  re-resolution), so ~34 ms is real backend work; the remaining ~10 ms is
  Enter/PTY latency. Left alone: opening a project must resolve its views, and
  no per-frame read was involved.
- **Blueprint view switch at 160x45 (93.5 ms same-era, 98.5 ms recorded).**
  Trace shows the `]` handler at 0.03 ms and the following draw at ~5 ms; the
  same sequence at 100x30 measures ~21 ms. The extra ~70 ms is PTY/terminal
  screen reconstruction in the harness for the wider frame, not application
  work. The docked detail pane was re-reading three tables per frame at 160x45
  (see below), which the cache removes, but this operation stays
  harness-dominated.
- **Per-frame detail model.** `detail_model()` cost ~1.1 ms per call (task
  detail + acceptance criteria + attachments) and `draw_detail` called it every
  frame at the wide breakpoint, so every 160x45 frame paid 2–3 backend queries.
- **`available_actions()` per frame.** Called through `compact_actions()` by the
  action bar; measured at 0.01 ms and purely in-memory. Not worth caching, so it
  was left unchanged.

## Changes

1. **Cached detail-pane model** (`app/state.rs`, `app/mod.rs`,
   `app/navigation.rs`). `detail_model()` now builds once per subject and reuses
   the cached `DetailModel`. The cache key is screen, project id, view id,
   `projects_selection`, active component, selected index/row id, plus a
   revision counter bumped by `refresh_workspace`, `refresh_project`, project
   open, screen navigation and view switches. Render now performs zero backend
   I/O for the docked detail pane. The stored modal detail path was already
   snapshot-based and is unchanged.
   - Effect in the isolated frame measurement: 160x45 render 4.2 → 2.9 ms
     (median).
   - Probe: `open_task_detail` 7.6 → 5.3 ms and `switch_blueprint_view`
     93.5 → 77.6 ms at 160x45. 100x30 is unchanged because no docked detail
     pane is drawn there.
2. **Targeted quick-edit refresh** (`app/forms.rs`, `app/navigation.rs`).
   A title-only `FormMode::QuickEditTask` now calls
   `refresh_after_task_title`, which re-resolves the open project (the one view
   that shows the edited row) and patches the single task row already held in
   `workspace.tasks` / `workspace.inbox` and the planning brief, then rebuilds
   the global views from that in-memory snapshot. If the task is not in the
   snapshot it falls back to the old full refresh. A priority change keeps the
   old full `refresh_after_mutation` because it can reorder Today/Next; all
   other form modes are unchanged.
   - Isolated cost: 103 → 20.7 ms per title save.
   - Probe: quick edit 173.2 → 82.6 ms (−52.3%) at 100x30 and
     173.3 → 87.3 ms (−49.6%) at 160x45 (same-era baseline). Against the
     recorded baseline it is 161.6/178.3 → 82.6/87.3 ms.
   - Behaviour is unchanged: the renamed row is visible in the project view
     immediately, and Home/Inbox/sidebar/planning views derive from the patched
     snapshot. Priority edits still take the full path.
3. **No optimisation of action catalogues or project open.** The numbers did
   not justify either: action catalogue rebuild is 0.01 ms, and project open is
   the scoped read the screen needs.

## Regression tests

`crates/orbit-tui/src/tests/m29_perf_render_tests.rs`:

- `detail_pane_model_is_seeded_by_input_paths_and_never_rebuilt_in_render` counts
  model builds and asserts that repeated 160x45 renders do not rebuild, while
  selection, view, screen and refresh all invalidate the cache.
- `quick_edit_refreshes_the_project_without_rereading_the_workspace` counts
  `refresh_workspace` calls and asserts a title-only quick edit performs none,
  while both the project view and the in-memory workspace snapshot show the new
  title; it also asserts a priority change still takes the full refresh.

## Honest notes

- `cold_start_first_frame` is flat within noise: same-era medians 100.0 →
  101.2 ms at 100x30 and 102.2 → 101.7 ms at 160x45. The recorded 227edbe
  baseline shows 96.3/95.9 ms, but the recorded **after** medians are
  101.2/101.7 ms (not 96 ms). The 96 ms figures did not reproduce on the current
  machine state even with the unmodified binary, so they are not evidence of a
  regression from this change. Cold start is dominated by `App::open` →
  `refresh_workspace` (~90 ms in this fixture) plus process launch, which this
  change deliberately does not touch.
- `refresh_project`, `scroll`, `search`, `project_switch` and cold start are
  statistically flat (≤2 ms), as expected: none exercised the changed paths.
- `switch_blueprint_view` at 160x45 remains much slower than at 100x30 because
  the PTY harness reconstructs a wider terminal screen, which is outside the
  application. It improved with the cache but is still not application-bound.
- One intermediate five-repeat run aborted because the PTY driver timed out
  during a project-switch wait and left a stray `orbit-tui` process; after
  killing it, a clean five-repeat run produced the numbers above. The same
  fixture and binary were used.
- These are PTY wall-clock interaction latencies on one machine, not a
  benchmark; do not compare across hosts without rebuilding and re-running.
