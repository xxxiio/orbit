# Orbit TUI PTY acceptance tests

These tests launch the real `target/debug/orbit-tui` inside a sized pseudo
terminal and drive it with keystrokes. They use only the Python standard
library (`pty`, `select`, `fcntl`, `termios`, `struct`, `os`/`subprocess`); no
test dependency beyond pytest is required.

## Prerequisites

```sh
cargo build -p orbit-tui
cargo build -p orbit-cli   # only needed by tests that seed a project
cargo build -p orbit-server # connected-mode test
```

Override the binaries with `ORBIT_TUI_BIN` / `ORBIT_CLI_BIN`.

## Run

From the repository root:

```sh
python3 -m pytest -q tests/acceptance/tui
```

Run a single scenario, for example:

```sh
python3 -m pytest -q tests/acceptance/tui -k footer -vv
```

Each test gets a fresh temporary directory containing its own SQLite database,
config path and `HOME`. `ORBIT_DB` selects local mode; `ORBIT_AUTHORITY_*`
variables from the caller are stripped so a session cannot silently become
connected mode.

## Evidence

Every session writes:

- `raw.ansi` — the exact bytes read from the PTY master (ANSI included);
- `text.txt` — a stream-oriented ANSI-stripped view;
- `meta.json` — size, command, environment paths, exit status, alternate
  screen enter/leave flags, and an action log with timings.

Location: `target/tui-evidence/<run-id>/<test>-<n>/`. Override the root with
`ORBIT_TUI_EVIDENCE_DIR` or pin the run id with `ORBIT_TUI_RUN_ID`. Failure
messages include the last output (or current reconstructed screen) and the
evidence directory.

`TuiSession.screen_text` / `screen_lines` reconstruct the current visible
terminal screen from the ANSI stream; use these for state assertions.
`TuiSession.text` is the concatenation of printable output and is better for
checking that something was emitted at all.

## Scenarios

`test_m28_scenarios.py` covers:

- 100x30 launch, `?` help, and Esc closing the help modal;
- `gp` → Projects, Enter opening the first seeded project, then Esc stepping
  back through history (project → Projects → Home);
- `q` from Home, Projects, and an open project (plain `q` quits on every
  non-text screen);
- Ctrl+Q (`\x11`) and Ctrl+C (`\x03`) from Home, Projects, a project, Help, a
  focused text prompt, and a focused command palette;
- the unsaved-text quit confirmation (`y` confirms, `n` restores the draft);
- `q` typed inside a form field without quitting, and the connected/local quit footer;
- a quit shortcut in the footer on Home, Projects, a project, Help, a prompt,
  and the palette;
- project/task lifecycle and blueprint bind/work-item edit/transition with
  restart persistence;
- `test_m28_connected.py`: disposable authoritative server, connected header,
  pending project/task, `!` pending inspector, `f` flush, accepted refresh and
  clean exit. The connected console (`:` → *Connected commands…*) is the only
  connected presentation; there is no separate connected shell or launch flag.

`test_m29_fixture.py` is the shared-fixture launch coverage, and it separates
two jobs:

- `test_small_fixture_launch_smoke_*` is a fast **launch smoke** over 5
  projects / 40 tasks: launch, Home/Projects, open a project, switch views,
  search, scroll and quit. It is not the performance workload.
- `test_full_fixture_measured_shape_launch` is the **measured fixture**: 20
  projects / 200 tasks, the shape `perf_probe.py` builds with `--full` (its
  default). It is opt-in with `ORBIT_TUI_FULL_FIXTURE=1`, asserts the measured
  shape before launching, and only then drives the same session.

`test_m29_workflows.py` completes the targeted M29 workflow coverage:

- Scenario B: quick edit (`E`, title + priority), multiline task details (`e`),
  acceptance criteria (`K` → `c`), a workflow status change (`S`), task detail
  and activity inspection, and unsaved-form discard safety;
- Scenario C: switching five projects through the sidebar and the command
  palette, opening different views, restoring the selected task on return, and
  `g p` / `Esc` returning to the previous project, view and selection;
- Scenario D: the responsive Kanban at 60x18, 80x24, 100x30, 120x35 and
  160x45 — closed frame, windowed column legend, selected-card marker, `j`
  traversal of every non-empty column (the window follows), and a status
  transition that moves the card to the workflow's target column;
- Scenario F: `Ctrl+Q` from the workspace, task detail, dirty form, search,
  palette, connected pending inspector and connected console — the confirmation
  appears only for unsaved text, `n` preserves it, and every session exits 0
  with the alternate screen restored.

The harness treats these as target behaviour. A failure is reported as a
failure rather than hidden by relaxing the assertion.

## Hang handling

All waits have deadlines. On timeout the harness terminates the process group
(TERM then KILL), reaps it, and reports the captured screen/output. Fixture
teardown always closes sessions and writes evidence, including on failure.
