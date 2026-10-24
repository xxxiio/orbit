# M29 TUI interaction baseline

- Git revision: `227edbe3ef5af7137c1a9496c0091a56073ebc2b` (dirty tree)
- Binary: `/home/xliu/dev/orbit/target/debug/orbit-tui`  
  mtime `2026-10-24T09:24:37Z`, 179621744 bytes
- Workspace: `/tmp/opencode/m29-20261024T094956Z`
- Repeats per size: 5
- Sizes: 100x30, 160x45
- Counts: projects=20, tasks=200, activities=1017, attachments=6, agent_jobs=8, distinct pinned blueprint versions=5
- Pinned versions: `basic` v1, `development` v2, `habits` v2, `learning` v2, `research` v2

## Measurements

| Operation | 100x30 median / p90 (ms) | 160x45 median / p90 (ms) |
|---|---:|---:|
| `cold_start_first_frame` | 96.3 / 102.1 | 95.9 / 106.0 |
| `open_task_detail` | 4.3 / 4.8 | 7.3 / 7.6 |
| `project_switch_open_leave_open_next` | 82.6 / 84.3 | 87.6 / 88.0 |
| `refresh_project` | 19.5 / 20.3 | 25.9 / 29.0 |
| `save_quick_edit_title` | 161.6 / 174.4 | 178.3 / 197.6 |
| `scroll_task_table_20_rows` | 104.7 / 105.8 | 107.6 / 109.6 |
| `search_open_query_first_result` | 60.2 / 60.3 | 61.5 / 61.5 |
| `switch_blueprint_view` | 20.9 / 22.0 | 98.5 / 112.7 |

## How it was measured

Each repeat launches a fresh `orbit-tui` process in the shared PTY harness
against the same fixture workspace. `tui-state.json` is removed before every
launch so the session starts at Home. A single session performs the measured
operations in a fixed order; the wall-clock sample for an operation covers
keystroke writes plus the wait until the target state is reconstructed from
the PTY stream.

- **cold_start_first_frame** — process launch through the first `ORBIT` output.
- **search_open_query_first_result** — `/`, type `Retrieval`, wait for the first
  project hit (`Research: Retrieval Quality`).
- **project_switch_open_leave_open_next** — BackTab to the sidebar, `j` ×4 to
  the first project, Enter to open, Esc back to Home, `j` + Enter to open the
  next project.
- **open_task_detail** — Enter on the first task in `Performance Lab` Tasks; wait
  for the detail modal action line.
- **scroll_task_table_20_rows** — 20 `j` presses, then the `g` prefix hint as an
  input-order barrier (no sleeps are used as assertions).
- **save_quick_edit_title** — `E`, backspace one character, append a unique
  suffix, Enter; wait for the rename outcome notification.
- **refresh_project** — `r`, then the `g` prefix hint as an input-order barrier.
- **switch_blueprint_view** — `]` from Overview to Board in `Sync Reliability`
  (development workflow); wait for the `Blueprint work` panel.

## Reproduce

```sh
WS=/tmp/opencode/m29-$(date -u +%Y%m%dT%H%M%SZ)
python3 scripts/tui-fixture.py --out "$WS" --json
python3 tests/acceptance/tui/perf_probe.py --workspace "$WS" --repeat 5
python3 -m pytest -q tests/acceptance/tui/test_m29_fixture.py
```

## Caveats

These are interaction latencies measured through a PTY, not a benchmark claim.
Wall-clock time includes process scheduling, terminal emulation, harness
polling, and Rust debug-build overhead. The median and nearest-rank p90 are
descriptive for this machine and fixture; do not compare them across hosts
without rebuilding and re-running the probe. Later operations run with warm
OS caches inside each repeat, and the serial order is part of the method.
