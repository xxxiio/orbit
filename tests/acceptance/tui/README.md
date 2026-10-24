# Orbit TUI PTY acceptance tests

These tests launch the real `target/debug/orbit-tui` inside a sized pseudo
terminal and drive it with keystrokes. They use only the Python standard
library (`pty`, `select`, `fcntl`, `termios`, `struct`, `os`/`subprocess`); no
test dependency beyond pytest is required.

## Prerequisites

```sh
cargo build -p orbit-tui
cargo build -p orbit-cli    # seeds projects/tasks in local scenarios
cargo build -p orbit-server # connected scenarios (E, F-connected)
```

Override the binaries with `ORBIT_TUI_BIN`, `ORBIT_CLI_BIN` and
`ORBIT_SERVER_BIN`.

## Run

From the repository root:

```sh
python3 -m pytest -q tests/acceptance/tui
```

Run a single scenario, for example:

```sh
python3 -m pytest -q tests/acceptance/tui -k kanban -vv
```

Each test gets a fresh temporary directory containing its own SQLite database,
config path and `HOME`. `ORBIT_DB` selects local mode; `ORBIT_AUTHORITY_*`
variables from the caller are stripped so a session cannot silently become
connected mode unless the fixture sets them.

Current expected result on this suite: **54 passed, 1 skipped**. The skip is the
opt-in measured fixture described under Scenario G.

## Scenarios A–G

| Scenario | What it proves | Test file | Evidence |
|---|---|---|---|
| **A** — working context survives | Selected item/focus/component/scroll restore across a view switch and a real restart; the saved row is resolved by entity id; a deleted selected row falls back to a neighbour; a corrupt `tui-state.json` never blocks startup. | `test_m29_selection.py` | `target/tui-evidence/<run-id>/<test>-<n>/` per session; `.agent/evidence/m29-captures/*-home.txt`, `*-tasks.txt` show the restored selection. |
| **B** — everyday task editing | `E` quick edit (title + priority only), `e` full form with a multi-line description, `K` → `c` acceptance criterion, `S` workflow status change, task detail and `H` activity inspection, and unsaved-form discard safety. | `test_m29_workflows.py` — `test_everyday_task_editing_workflow` | `target/tui-evidence/<run-id>/m29b-*`; `.agent/evidence/m29-captures/100x30-quick-edit.txt`, `100x30-task-detail.txt`, `100x30-action-menu.txt`. |
| **C** — switching projects keeps context | Five projects switched through the sidebar and the command palette; different views opened; the selected task restored on return; `g p` / `Esc` return to the previous project, view and selection. | `test_m29_workflows.py` — `test_project_switching_and_context_return` | `target/tui-evidence/<run-id>/m29c-*`; `.agent/evidence/m29-captures/100x30-projects.txt`, `160x45-tasks.txt`. |
| **D** — responsive Kanban | Closed frame at 60×18, 80×24, 100×30, 120×35 and 160×45; windowed-column legend; selected-card marker; `j` traversal of every non-empty column (window follows); a status transition moves the card to the workflow's target column. | `test_m29_workflows.py` — `test_responsive_kanban_board_at_five_sizes` | `target/tui-evidence/<run-id>/m29d-*`; `.agent/evidence/m29-captures/{60x18,100x30,160x45}-board.txt`. |
| **E** — connected queue and conflict recovery | Offline queued command → pending row → flush after reconnect → accepted; a rejected command opens a snapshot-bound review; no replacement is queued without a review; the rejected audit row is retained. | `test_m29_connected_recovery.py` — `test_connected_offline_queue_and_conflict_recovery` | `target/tui-evidence/<run-id>/m29e-*`; `.agent/evidence/m29-captures/{60x18,100x30}-pending-inspector.txt`, `100x30-connected-console.txt`. |
| **F** — quit and modal safety | `Ctrl+Q` from the workspace, task detail, dirty form, search, palette, connected pending inspector and connected console; the unsaved-text confirmation appears only for typed text; `n` preserves it; every session exits 0 with the alternate screen restored. | `test_m29_workflows.py` — `test_quit_from_main_workspace_is_direct`, `test_quit_from_project_details_is_direct`, `test_quit_from_dirty_form_confirms_and_preserves`, `test_quit_from_search_with_text_confirms_and_preserves`, `test_quit_from_palette_with_text_confirms_and_preserves`, `test_quit_from_connected_pending_inspector_is_direct`, `test_quit_from_connected_console_with_text_confirms_and_preserves` | `target/tui-evidence/<run-id>/m29f-*`. |
| **G** — deterministic fixture launch | The shared fixture launches the real shell and runs Home/Projects, opens a project, switches views, searches, scrolls and quits. The fast launch smoke uses 5 projects / 40 tasks; the measured shape uses 20 projects / 200 tasks, 1,000 activities, 6 attachments and 8 agent jobs, with counts asserted before launch. | `test_m29_fixture.py` — `test_small_fixture_launch_smoke_home_projects_views_search_scroll_quit`, `test_full_fixture_measured_shape_launch` | `target/tui-evidence/<run-id>/fixture-*`; the fixture summary printed by the test. |

`test_m28_scenarios.py` and `test_m28_connected.py` remain the M28 regression
and lifecycle coverage (help/Esc, history, quit paths, task/project lifecycle,
workflow bind/edit/transition with restart, connected header/pending/flush). They
run in the same command; they are not renumbered as M29 scenarios.

### Full-fixture opt-in

The measured 20-project/200-task fixture is deliberately opt-in so an ordinary
run stays fast:

```sh
ORBIT_TUI_FULL_FIXTURE=1 python3 -m pytest -q tests/acceptance/tui -k full_fixture
```

That environment variable is the only switch; without it the measured test is
reported as skipped (the 1 skip above). `test_small_fixture_launch_smoke_*`
always runs and is a launch smoke, **not** the performance workload. Counts are
asserted before the TUI launches, so a silently smaller fixture cannot pass.

## Evidence

Every session writes:

- `raw.ansi` — the exact bytes read from the PTY master (ANSI included);
- `text.txt` — a stream-oriented ANSI-stripped view;
- `meta.json` — size, command, environment paths, exit status, alternate
  screen enter/leave flags, and an action log with timings.

Location: `target/tui-evidence/<run-id>/<label>/`. Override the root with
`ORBIT_TUI_EVIDENCE_DIR` or pin the run id with `ORBIT_TUI_RUN_ID`. Failure
messages include the last output (or current reconstructed screen) and the
evidence directory.

`TuiSession.screen_text` / `screen_lines` reconstruct the current visible
terminal screen from the ANSI stream; use these for state assertions.
`TuiSession.text` is the concatenation of printable output and is better for
checking that something was emitted at all.

Documentation captures are separate from the test evidence:
`python3 scripts/tui-captures.py` writes real reconstructed screens (text, not
stream logs) for Home, Projects, Tasks, Board at three sizes, task detail,
action menu, palette, quick edit and the connected pending/console surfaces into
`.agent/evidence/m29-captures/`, with an index in its `README.md`.

## Scenario contents

The paragraphs below are the detailed assertions each file owns; the table above
is the short map.

`test_m28_scenarios.py` covers:

- 100x30 launch, `?` help, and Esc closing the help modal;
- `gp` → Projects, Enter opening the first seeded project, then Esc stepping
  back through history (project → Projects → Home);
- `q` from Home, Projects, and an open project (plain `q` quits on every
  normal screen);
- Ctrl+Q (`\x11`) and Ctrl+C (`\x03`) from Home, Projects, a project, Help, a
  focused text prompt, and a focused command palette;
- the unsaved-text quit confirmation (`y` confirms, `n` restores the draft);
- `q` typed inside a form field without quitting, and the connected/local quit footer;
- a quit shortcut in the footer on Home, Projects, a project, Help, a prompt,
  and the palette;
- project/task lifecycle and workflow bind/work-item edit/transition with
  restart persistence;
- `test_m28_connected.py`: disposable authoritative server, connected header,
  pending project/task, `!` pending inspector, `f` flush, accepted refresh and
  clean exit.

`test_m29_selection.py` covers Scenario A: selection survives a view switch and a
process restart; a deleted selected row falls back safely; a corrupt state file
does not block startup.

`test_m29_workflows.py` covers Scenarios B, C, D and F, including the connected
quit cases with a disposable authority.

`test_m29_connected_recovery.py` covers Scenario E end to end.

`test_m29_fixture.py` covers Scenario G: the always-on small launch smoke and the
opt-in measured fixture.

The harness treats these as target behaviour. A failure is reported as a
failure rather than hidden by relaxing the assertion.

## Hang handling

All waits have deadlines. On timeout the harness terminates the process group
(TERM then KILL), reaps it, and reports the captured screen/output. Fixture
teardown always closes sessions and writes evidence, including on failure.
