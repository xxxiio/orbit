# Orbit TUI PTY acceptance tests

These tests launch the real `target/debug/orbit-tui` inside a sized pseudo
terminal and drive it with keystrokes. They use only the Python standard
library (`pty`, `select`, `fcntl`, `termios`, `struct`, `os`/`subprocess`); no
test dependency beyond pytest is required.

## Prerequisites

```sh
cargo build -p orbit-tui
cargo build -p orbit-cli   # only needed by tests that seed a project
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
- 60x18 and 160x45 launches, Ctrl+Q exit, normal exit status, and restoration
  of the non-alternate screen (`\x1b[?1049l` or equivalent).

The harness treats these as target behaviour. A failure is reported as a
failure rather than hidden by relaxing the assertion.

## Hang handling

All waits have deadlines. On timeout the harness terminates the process group
(TERM then KILL), reaps it, and reports the captured screen/output. Fixture
teardown always closes sessions and writes evidence, including on failure.
