"""M30 workstream A acceptance: the explicit item inspector.

These tests drive the real ``orbit-tui`` binary through the shared PTY harness
and describe the contract that replaced the permanent docked Details pane:

* the default layout is exactly two regions (sidebar + workspace) at 80x24,
  120x35 and 160x45 - nothing is reserved for a third column;
* ``Enter`` opens an inspector overlay for the selected item and ``Esc``
  returns to the same selection and the same footer;
* ``D`` opens the inspector and closes it again;
* on a 60x18 terminal the inspector opens full screen (whole frame, no margin)
  and ``Esc`` restores the shell.

Run from the repository root after::

    cargo build -p orbit-tui -p orbit-cli --offline

Every session writes evidence under
``target/tui-evidence/<run-id>/<test>-<n>/`` (``raw.ansi``, ``text.txt``,
``meta.json``).
"""
from __future__ import annotations

import json
import os
import re
import sqlite3
import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
ROOT = Path(__file__).resolve().parents[3]

from pty_harness import (  # noqa: E402
    ENTER,
    ESC,
    TuiSession,
    Workspace,
)

# The shell is two regions at every width: sidebar | workspace. The item
# inspector is an on-demand overlay, never a docked third column.
SIDEBAR_WIDTH = 22
INSPECTOR_MARKER = "Inspector ·"
HOME_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Home / Today"
PROJECTS_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Projects"
PROJECT_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*{name}"

SIZES = [(80, 24), (120, 35), (160, 45)]


@pytest.fixture
def session_factory():
    """Launch sessions on caller-supplied workspaces and always reap them."""
    sessions: list[TuiSession] = []
    counter = {"value": 0}

    def start(
        workspace: Workspace,
        *,
        label: str | None = None,
        cols: int = 120,
        rows: int = 35,
    ) -> TuiSession:
        counter["value"] += 1
        current = os.environ.get("PYTEST_CURRENT_TEST", "").split(" ")[0]
        test_name = current.rsplit("::", 1)[-1] or "session"
        session = TuiSession(
            workspace,
            cols=cols,
            rows=rows,
            label=label or f"{test_name}-{counter['value']}",
            wait_for_launch=False,
        )
        sessions.append(session)
        session.wait_for_text("ORBIT", timeout=20.0, description="first Orbit frame")
        return session

    yield start

    for session in reversed(sessions):
        try:
            session.close()
        except Exception as error:  # teardown must not hide the test failure
            print(f"orbit-tui harness teardown error: {error}", flush=True)


def _cli_binary() -> Path:
    return Path(
        os.environ.get("ORBIT_CLI_BIN", str(ROOT / "target" / "debug" / "orbit"))
    ).expanduser()


def _cli_json(workspace: Workspace, *args: str) -> dict:
    completed = subprocess.run(
        [str(_cli_binary()), "--db", str(workspace.db), "--json", *args],
        env=dict(workspace.env),
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=60,
    )
    assert completed.returncode == 0, (
        f"CLI failed: {args!r}\nstdout: {completed.stdout}\nstderr: {completed.stderr}"
    )
    output = completed.stdout.strip()
    return json.loads(output) if output else {}


def _sqlite_scalar(workspace: Workspace, query: str, parameters: tuple = ()):
    connection = sqlite3.connect(workspace.db)
    try:
        row = connection.execute(query, parameters).fetchone()
    finally:
        connection.close()
    assert row is not None, f"no row for query {query!r} {parameters!r}"
    return row[0]


def _seed_project_with_tasks(
    workspace: Workspace, project_name: str, titles: list[str]
) -> str:
    workspace.create_project(project_name)
    project_id = _sqlite_scalar(
        workspace, "SELECT id FROM projects WHERE name=?1", (project_name,)
    )
    for title in titles:
        _cli_json(workspace, "task", "add", title, "--project", project_id)
    return project_id


def wait_tasks_painted(
    session: TuiSession, titles: list[str], timeout: float = 10.0
) -> None:
    """Wait until the Tasks table replaced the Overview panels."""
    session.wait_until_screen(
        lambda view: (
            "DEADLINE" in view
            and "Recent activity" not in view
            and all(title in view for title in titles)
        ),
        timeout=timeout,
        description="Tasks table fully painted",
    )


def open_tasks_workspace(
    session: TuiSession, project_name: str, titles: list[str]
) -> None:
    session.wait_for_screen_regex(
        HOME_HEADER_RE, timeout=10.0, description="Home / Today header"
    )
    session.send_keys(b"g", b"p", delay=0.15)
    session.wait_for_screen_regex(
        PROJECTS_HEADER_RE, timeout=8.0, description="Projects screen"
    )
    session.wait_for_screen_text(
        project_name, timeout=8.0, description="seeded project row"
    )
    session.send(ENTER, note="open the project workspace")
    session.wait_for_screen_regex(
        PROJECT_HEADER_RE.format(name=re.escape(project_name)),
        timeout=10.0,
        description=f"project workspace {project_name!r}",
    )
    # Overview -> Tasks is the only view with a task table and a row cursor.
    session.send(b"]", note="switch to the Tasks view")
    wait_tasks_painted(session, titles)


def _workspace_row(line: str, cols: int) -> str:
    # Below the medium breakpoint the shell stacks the panes, so the workspace
    # starts at column 0 instead of after the sidebar.
    if cols < 76:
        return line[:cols]
    return line[SIDEBAR_WIDTH:cols]


def _selected_title(session: TuiSession, titles: list[str], cols: int) -> str:
    def marked(view: str) -> list[str]:
        found = []
        for line in view.splitlines():
            segment = _workspace_row(line, cols)
            if "▸" not in segment:
                continue
            for title in titles:
                if title in segment and title not in found:
                    found.append(title)
                    break
        return found

    session.wait_until_screen(
        lambda view: len(marked(view)) == 1,
        timeout=8.0,
        description="exactly one visibly selected task row",
    )
    return marked(session.screen_text)[0]


def _footer(session: TuiSession) -> str:
    for line in reversed(session.screen_lines):
        if "Ctrl+Q quit" in line:
            return line.rstrip()
    raise AssertionError(
        f"no quit footer on screen\nscreen:\n{session.screen_text}\n"
        f"Evidence: {session.evidence_dir}"
    )


INSPECTOR_FOOTER = "Esc close · j/k scroll · Enter close"


def _open_inspector(session: TuiSession, title: str) -> str:
    session.send(ENTER, note="open the item inspector")
    session.wait_for_screen_text(
        INSPECTOR_MARKER, timeout=10.0, description="inspector overlay"
    )
    # Wait for the whole frame: the footer is the last row painted, so a match
    # on it means the overlay is not a half-written frame.
    screen = session.wait_for_screen_text(
        INSPECTOR_FOOTER, timeout=10.0, description="inspector footer"
    )
    assert title in screen, (
        f"the inspector did not name {title!r}\nscreen:\n{screen}\n"
        f"Evidence: {session.evidence_dir}"
    )
    assert "PROPERTIES" in screen, (
        f"the inspector did not paint fields\nscreen:\n{screen}\n"
        f"Evidence: {session.evidence_dir}"
    )
    return screen


def _close_inspector(session: TuiSession) -> str:
    session.send(ESC, note="close the inspector")
    return session.wait_until_screen(
        lambda view: INSPECTOR_MARKER not in view,
        timeout=8.0,
        description="inspector closes",
    )


# ---------------------------------------------------------------------------
# Scenario 1: the default frame has exactly two regions.
# ---------------------------------------------------------------------------
@pytest.mark.parametrize("cols,rows", SIZES)
def test_initial_frame_is_sidebar_and_workspace_only(
    session_factory, tmp_path, cols, rows
):
    workspace = Workspace.create(tmp_path)
    titles = ["Alpha layout task", "Bravo layout task"]
    _seed_project_with_tasks(workspace, "Layout Project", titles)

    session = session_factory(
        workspace, label=f"m30-two-regions-{cols}x{rows}", cols=cols, rows=rows
    )
    open_tasks_workspace(session, "Layout Project", titles)

    view = session.screen_text
    assert "Details" not in view, (
        f"{cols}x{rows}: a Details block must never be painted\nscreen:\n{view}\n"
        f"Evidence: {session.evidence_dir}"
    )
    assert "Orbit" in view, f"{cols}x{rows}: sidebar missing\n{view}"

    # The workspace panel starts exactly after the 22-column sidebar and its
    # top border runs to the last column: the workspace owns every column
    # beyond the sidebar.
    panel_row = next(
        (
            line
            for line in session.screen_lines
            if line.rstrip().endswith(("┓", "╮")) and len(line.rstrip()) == cols
        ),
        None,
    )
    assert panel_row is not None, (
        f"{cols}x{rows}: no workspace panel border reaches the frame edge\n"
        f"screen:\n{view}\nEvidence: {session.evidence_dir}"
    )
    assert panel_row[SIDEBAR_WIDTH] in ("┏", "╭"), (
        f"{cols}x{rows}: workspace must start at column {SIDEBAR_WIDTH}, "
        f"found {panel_row[SIDEBAR_WIDTH]!r}: {panel_row!r}"
    )
    # ... and the sidebar border sits exactly in the column before it.
    sidebar_row = next(
        line for line in session.screen_lines if "Orbit" in line
    )
    assert sidebar_row[SIDEBAR_WIDTH - 1] in ("│", "┃", "╮", "┓", "╯", "┛"), (
        f"{cols}x{rows}: the sidebar border is not at column "
        f"{SIDEBAR_WIDTH - 1}: {sidebar_row!r}"
    )
    # Both seeded rows are readable in that width.
    for title in titles:
        assert title in view, f"{cols}x{rows}: {title!r} squeezed out\n{view}"

    session.quit_via(b"q")
    session.assert_clean_exit()


# ---------------------------------------------------------------------------
# Scenario 2: Enter opens the overlay, Esc restores the same selection.
# ---------------------------------------------------------------------------
@pytest.mark.parametrize("cols,rows", SIZES)
def test_enter_opens_inspector_and_esc_returns_to_the_same_selection(
    session_factory, tmp_path, cols, rows
):
    workspace = Workspace.create(tmp_path)
    titles = ["Charlie inspect task", "Delta inspect task", "Echo inspect task"]
    _seed_project_with_tasks(workspace, "Inspector Project", titles)

    session = session_factory(
        workspace, label=f"m30-open-close-{cols}x{rows}", cols=cols, rows=rows
    )
    open_tasks_workspace(session, "Inspector Project", titles)
    selected = _selected_title(session, titles, cols)
    footer = _footer(session)

    inspector = _open_inspector(session, selected)
    assert INSPECTOR_MARKER in inspector, inspector
    assert INSPECTOR_FOOTER in inspector, (
        f"{cols}x{rows}: the inspector footer is missing\n{inspector}"
    )
    # An overlay leaves the shell footer visible on these sizes.
    assert _footer(session) == footer, (
        f"{cols}x{rows}: opening the inspector changed the shell footer\n"
        f"{session.screen_text}"
    )

    closed = _close_inspector(session)
    assert INSPECTOR_MARKER not in closed
    assert _footer(session) == footer, (
        f"{cols}x{rows}: closing the inspector did not restore the footer\n"
        f"{session.screen_text}"
    )
    assert _selected_title(session, titles, cols) == selected, (
        f"{cols}x{rows}: the selected row changed across the inspector\n"
        f"{session.screen_text}"
    )

    session.quit_via(b"q")
    session.assert_clean_exit()


# ---------------------------------------------------------------------------
# Scenario 3: D opens and closes the inspector.
# ---------------------------------------------------------------------------
def test_d_opens_and_closes_the_inspector(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    titles = ["Foxtrot toggle task", "Golf toggle task"]
    _seed_project_with_tasks(workspace, "Toggle Project", titles)

    session = session_factory(workspace, label="m30-toggle", cols=120, rows=35)
    open_tasks_workspace(session, "Toggle Project", titles)
    selected = _selected_title(session, titles, 120)

    session.send(b"D", note="open the inspector")
    screen = session.wait_for_screen_text(
        INSPECTOR_MARKER, timeout=10.0, description="inspector overlay"
    )
    screen = session.wait_for_screen_text(
        INSPECTOR_FOOTER, timeout=10.0, description="inspector footer"
    )
    assert selected in screen

    session.send(b"D", note="close the inspector")
    session.wait_until_screen(
        lambda view: INSPECTOR_MARKER not in view,
        timeout=8.0,
        description="inspector closes on D",
    )
    assert _selected_title(session, titles, 120) == selected

    session.quit_via(b"q")
    session.assert_clean_exit()


# ---------------------------------------------------------------------------
# Scenario 4: 60x18 opens the inspector full screen.
# ---------------------------------------------------------------------------
def test_small_terminal_opens_the_inspector_full_screen(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    titles = ["Hotel compact task", "India compact task"]
    _seed_project_with_tasks(workspace, "Compact Project", titles)

    session = session_factory(workspace, label="m30-compact", cols=60, rows=18)
    open_tasks_workspace(session, "Compact Project", titles)
    selected = _selected_title(session, titles, 60)

    screen = _open_inspector(session, selected)
    lines = session.screen_lines
    assert lines[0].startswith("╔"), (
        "60x18 must open the inspector at the top-left corner of the frame "
        f"(no margin)\nscreen:\n{screen}\nEvidence: {session.evidence_dir}"
    )
    assert lines[0].rstrip().endswith("╗"), (
        f"the inspector must span the whole frame width: {lines[0]!r}"
    )
    assert all("╔" not in line for line in lines[1:]), (
        "only one full-screen inspector frame should be painted\n"
        f"{screen}"
    )
    # The shell chrome is covered: a full-screen inspector is not an overlay.
    assert "ORBIT" not in screen, (
        f"the shell header is still visible behind a full-screen inspector\n{screen}"
    )

    restored = _close_inspector(session)
    assert "ORBIT" in restored, (
        f"Esc did not restore the shell\n{restored}\n"
        f"Evidence: {session.evidence_dir}"
    )
    assert _selected_title(session, titles, 60) == selected

    session.quit_via(b"q")
    session.assert_clean_exit()


def test_overlay_is_inset_at_the_readable_boundary(session_factory, tmp_path):
    """80x24 keeps the shell visible around an inset inspector overlay."""
    workspace = Workspace.create(tmp_path)
    titles = ["Juliet overlay task", "Kilo overlay task"]
    _seed_project_with_tasks(workspace, "Overlay Project", titles)

    session = session_factory(workspace, label="m30-overlay", cols=80, rows=24)
    open_tasks_workspace(session, "Overlay Project", titles)
    _open_inspector(session, _selected_title(session, titles, 80))

    lines = session.screen_lines
    assert not lines[0].startswith("╔"), (
        "80x24 must keep the inspector as an inset overlay, not full screen\n"
        f"{session.screen_text}"
    )
    assert any(
        len(line) > 2 and line[2] == "╔" and INSPECTOR_MARKER in line
        for line in lines
    ), (
        "the inset inspector should start two columns in, leaving the sidebar "
        f"and workspace visible around it\n{session.screen_text}\n"
        f"Evidence: {session.evidence_dir}"
    )

    session.quit_via(b"q")
    session.assert_clean_exit()
