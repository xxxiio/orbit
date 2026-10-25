"""M29 I1 acceptance: selected-item restoration across views and restarts.

These tests drive the real ``orbit-tui`` binary through the shared PTY harness
and describe the M29 target behaviour (milestone EC-001):

* the selected row survives a view switch and a full process restart;
* the selected row is persisted by entity id, not just by index;
* a deleted selected row falls back to a valid neighbour instead of trapping
  the user or crashing;
* a corrupt ``tui-state.json`` never blocks startup.

Run from the repository root after::

    cargo build -p orbit-tui -p orbit-cli --offline

Every session writes evidence under
``target/tui-evidence/<run-id>/<test>-<n>/`` (``raw.ansi``, ``text.txt``,
``meta.json``).  Assertions are made against the reconstructed screen, never
against internal JSON, except for the explicit on-disk state-file check.
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

COLS = 140
ROWS = 40

HOME_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Home / Today"
PROJECTS_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Projects"
PROJECT_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*{name}"
VIEW_SWITCH_HINT = "[ ] switch view"
STATE_RELATIVE_PATH = Path(".orbit") / "state" / "tui-state.json"


# ---------------------------------------------------------------------------
# Local fixtures and helpers (the shared harness is read-only for this task).
# ---------------------------------------------------------------------------
@pytest.fixture
def session_factory():
    """Launch sessions on caller-supplied workspaces and always reap them."""
    sessions: list[TuiSession] = []
    counter = {"value": 0}

    def start(
        workspace: Workspace,
        *,
        label: str | None = None,
        cols: int = COLS,
        rows: int = ROWS,
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
) -> tuple[str, list[dict[str, str]]]:
    """Seed a real project and tasks through the CLI, then read their ids."""
    workspace.create_project(project_name)
    project_id = _sqlite_scalar(
        workspace, "SELECT id FROM projects WHERE name=?1", (project_name,)
    )
    tasks: list[dict[str, str]] = []
    for title in titles:
        payload = _cli_json(
            workspace, "task", "add", title, "--project", project_id
        )
        task_id = payload["task"]["id"]
        persisted_id = _sqlite_scalar(
            workspace, "SELECT id FROM tasks WHERE title=?1", (title,)
        )
        assert persisted_id == task_id, (persisted_id, task_id)
        tasks.append({"id": task_id, "title": title})
    return project_id, tasks


def _delete_task(workspace: Workspace, task_id: str) -> None:
    """Remove the task row the way an external authority mutation would."""
    connection = sqlite3.connect(workspace.db)
    try:
        connection.execute("DELETE FROM tasks WHERE id=?1", (task_id,))
        connection.commit()
    finally:
        connection.close()
    remaining = sqlite3.connect(workspace.db)
    try:
        count = remaining.execute(
            "SELECT COUNT(*) FROM tasks WHERE id=?1", (task_id,)
        ).fetchone()[0]
    finally:
        remaining.close()
    assert count == 0, f"task {task_id} still present after deletion"


def _state_path(workspace: Workspace) -> Path:
    return workspace.home / STATE_RELATIVE_PATH


def _read_state(workspace: Workspace) -> dict:
    path = _state_path(workspace)
    assert path.is_file(), f"tui-state.json was not written at {path}"
    return json.loads(path.read_text(encoding="utf-8"))


def wait_home(session: TuiSession) -> None:
    session.wait_for_screen_regex(
        HOME_HEADER_RE, timeout=10.0, description="Home / Today header"
    )


def open_projects(session: TuiSession) -> None:
    session.send_keys(b"g", b"p", delay=0.15)
    session.wait_for_screen_regex(
        PROJECTS_HEADER_RE, timeout=8.0, description="Projects screen"
    )


def open_first_project(session: TuiSession, project_name: str) -> None:
    session.send(ENTER, note="open first project")
    session.wait_for_screen_regex(
        PROJECT_HEADER_RE.format(name=re.escape(project_name)),
        timeout=10.0,
        description=f"project workspace {project_name!r}",
    )
    wait_view(session, "Overview")


def _current_view_title(screen_text: str) -> str | None:
    """Read the selected tab from the project view bar."""
    for line in screen_text.splitlines():
        if VIEW_SWITCH_HINT not in line:
            continue
        match = re.search(r"▸\s*([^│\[]+)", line)
        if match:
            return match.group(1).strip()
    return None


def wait_view(session: TuiSession, title: str, timeout: float = 8.0) -> None:
    session.wait_until_screen(
        lambda view: _current_view_title(view) == title,
        timeout=timeout,
        description=f"{title} view to be active",
    )


def switch_view(session: TuiSession, key: bytes, title: str) -> None:
    session.send(key, note=f"switch view to {title}")
    wait_view(session, title)


def wait_tasks_painted(
    session: TuiSession, titles: list[str], timeout: float = 8.0
) -> None:
    """Wait until the Tasks body replaced the previous view's panels.

    The tab label changes before the body repaint completes, so a tab-only
    wait can sample a half-painted frame.  The Overview panels must be gone
    and the table header plus every seeded row must be visible.
    """
    session.wait_until_screen(
        lambda view: (
            "DEADLINE" in view
            and "Recent activity" not in view
            and "Next actions" not in view
            and all(title in view for title in titles)
        ),
        timeout=timeout,
        description="Tasks table fully painted",
    )


def _workspace_segment(line: str, cols: int) -> str:
    """Slice the main workspace column out of a reconstructed screen row."""
    return line[SIDEBAR_WIDTH:cols]


def _selected_titles(screen_text: str, titles: list[str], cols: int = COLS) -> list[str]:
    """Titles whose workspace row carries the visible ``▸`` selection marker."""
    selected: list[str] = []
    for line in screen_text.splitlines():
        segment = _workspace_segment(line, cols)
        if "▸" not in segment:
            continue
        for title in titles:
            if title in segment:
                selected.append(title)
                break
    return selected


def _visible_rows(screen_text: str, titles: list[str], cols: int = COLS) -> list[str]:
    """Task titles in top-to-bottom workspace order, selected or not."""
    rows: list[str] = []
    for line in screen_text.splitlines():
        segment = _workspace_segment(line, cols)
        for title in titles:
            if title in segment and title not in rows:
                rows.append(title)
                break
    return rows


def _selected_title(session: TuiSession, titles: list[str], cols: int = COLS) -> str:
    session.wait_until_screen(
        lambda view: len(_selected_titles(view, titles, cols)) == 1,
        timeout=8.0,
        description="exactly one visibly selected task row",
    )
    return _selected_titles(session.screen_text, titles, cols)[0]


def wait_selected(session: TuiSession, title: str, titles: list[str]) -> None:
    session.wait_until_screen(
        lambda view: _selected_titles(view, titles) == [title],
        timeout=8.0,
        description=f"selected row to be {title!r}",
    )


def _open_inspector(session: TuiSession) -> str:
    """Open the item inspector for the selected row and return its screen."""
    session.send(ENTER, note="open the item inspector")
    return session.wait_for_screen_text(
        "Inspector ·", timeout=8.0, description="item inspector overlay"
    )


def _close_inspector(session: TuiSession) -> None:
    session.send(ESC, note="close the item inspector")
    session.wait_until_screen(
        lambda view: "Inspector ·" not in view,
        timeout=8.0,
        description="item inspector closes",
    )


def _assert_inspector_describes(session: TuiSession, title: str) -> None:
    """Enter opens the overlay for the selected row; Esc returns unchanged."""
    screen = _open_inspector(session)
    assert title in screen, (
        f"the inspector did not describe the selected task {title!r}\n"
        f"screen:\n{screen}\nEvidence: {session.evidence_dir}"
    )
    _close_inspector(session)


# ---------------------------------------------------------------------------
# Scenario 1: selection survives a view switch and a real process restart.
# ---------------------------------------------------------------------------
def test_selection_survives_view_switch_and_restart(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    project_name = "Selection Project"
    titles = ["Alpha task", "Bravo task", "Charlie task", "Delta task", "Echo task"]
    project_id, tasks = _seed_project_with_tasks(workspace, project_name, titles)

    session = session_factory(workspace, label="m29-selection-1")
    wait_home(session)
    open_projects(session)
    session.wait_for_screen_text(
        project_name, timeout=8.0, description="seeded project row"
    )
    open_first_project(session, project_name)

    # Overview -> Tasks is the only view with a task table and a row cursor.
    switch_view(session, b"]", "Tasks")
    wait_tasks_painted(session, titles)
    rows = _visible_rows(session.screen_text, titles)
    assert len(rows) == len(titles), (
        f"expected {len(titles)} visible task rows, got {rows!r}\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )

    first_title = _selected_title(session, titles)
    assert first_title == rows[0], (
        f"fresh Tasks view should select the first row {rows[0]!r}, "
        f"but {first_title!r} was marked\nscreen:\n{session.screen_text}\n"
        f"Evidence: {session.evidence_dir}"
    )
    _assert_inspector_describes(session, first_title)

    # Move to the third row with two real key presses.
    expected_title = rows[2]
    session.send_keys(b"j", b"j", delay=0.15)
    wait_selected(session, expected_title, titles)
    assert expected_title != first_title
    _assert_inspector_describes(session, expected_title)

    # Round-trip through another view and back.
    switch_view(session, b"]", "Board")
    session.wait_for_screen_text(
        "Task board", timeout=8.0, description="Board body painted"
    )
    switch_view(session, b"[", "Tasks")
    wait_tasks_painted(session, titles)
    wait_selected(session, expected_title, titles)
    _assert_inspector_describes(session, expected_title)

    session.quit_via(b"q")
    session.assert_clean_exit()

    # Durable state must record the entity identity, not just an index.
    state = _read_state(workspace)
    expected_id = next(
        task["id"] for task in tasks if task["title"] == expected_title
    )
    assert state.get("selected_id") is not None, (
        f"tui-state.json persisted a null selected_id: {state!r}"
    )
    assert state["selected_id"] == expected_id, (
        f"persisted selected_id {state['selected_id']!r} does not match the "
        f"sqlite task id {expected_id!r} for {expected_title!r}: {state!r}"
    )
    assert state.get("project_id") == project_id, state
    assert state.get("view_id") == "tasks", state
    assert state.get("screen") == "project", state

    # Real restart on the same isolated HOME/database.
    restarted = session_factory(workspace, label="m29-selection-restart")
    restarted.wait_for_screen_regex(
        PROJECT_HEADER_RE.format(name=re.escape(project_name)),
        timeout=10.0,
        description="project restored after restart",
    )
    wait_view(restarted, "Tasks")
    wait_tasks_painted(restarted, titles)
    wait_selected(restarted, expected_title, titles)
    _assert_inspector_describes(restarted, expected_title)
    assert "panicked at" not in restarted.text

    restarted.quit_via(b"q")
    restarted.assert_clean_exit()


# ---------------------------------------------------------------------------
# Scenario 2: a deleted selected row falls back to a valid neighbour.
# ---------------------------------------------------------------------------
def test_deleted_selected_task_falls_back_safely(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    project_name = "Fallback Project"
    titles = ["Foxtrot task", "Golf task", "Hotel task", "India task"]
    _, tasks = _seed_project_with_tasks(workspace, project_name, titles)

    session = session_factory(workspace, label="m29-fallback-1")
    wait_home(session)
    open_projects(session)
    session.wait_for_screen_text(
        project_name, timeout=8.0, description="seeded project row"
    )
    open_first_project(session, project_name)
    switch_view(session, b"]", "Tasks")
    wait_tasks_painted(session, titles)
    rows = _visible_rows(session.screen_text, titles)
    selected_title = rows[2]
    session.send_keys(b"j", b"j", delay=0.15)
    wait_selected(session, selected_title, titles)

    session.quit_via(b"q")
    session.assert_clean_exit()

    state = _read_state(workspace)
    deleted = next(task for task in tasks if task["title"] == selected_title)
    assert state.get("selected_id") == deleted["id"], state
    assert state.get("view_id") == "tasks", state
    assert state.get("selected_index") == 2, state

    # The selected entity disappears from the authority while the TUI is down.
    _delete_task(workspace, deleted["id"])

    restarted = session_factory(workspace, label="m29-fallback-restart")
    restarted.wait_for_screen_regex(
        PROJECT_HEADER_RE.format(name=re.escape(project_name)),
        timeout=10.0,
        description="project still restores after its selected task is deleted",
    )
    wait_view(restarted, "Tasks")
    assert "panicked at" not in restarted.text, (
        "orbit-tui panicked when restoring a deleted selection:\n"
        f"{restarted.text[-3000:]}\nEvidence: {restarted.evidence_dir}"
    )

    remaining = [task["title"] for task in tasks if task["id"] != deleted["id"]]
    wait_tasks_painted(restarted, remaining)
    restarted.wait_until_screen(
        lambda view: len(_selected_titles(view, remaining)) == 1,
        timeout=8.0,
        description="a valid row is still visibly selected after fallback",
    )
    fallback_title = _selected_titles(restarted.screen_text, remaining)[0]
    assert fallback_title != selected_title
    assert fallback_title in remaining, (
        f"fallback selected {fallback_title!r}, expected one of {remaining!r}\n"
        f"screen:\n{restarted.screen_text}\nEvidence: {restarted.evidence_dir}"
    )
    assert _visible_rows(restarted.screen_text, titles).count(selected_title) == 0, (
        "the deleted task still appears in the Tasks table\n"
        f"screen:\n{restarted.screen_text}\nEvidence: {restarted.evidence_dir}"
    )

    restarted.quit_via(b"q")
    restarted.assert_clean_exit()

    # The fallback is itself persisted, so the next launch is also stable.
    fallback_state = _read_state(workspace)
    fallback_id = next(
        task["id"] for task in tasks if task["title"] == fallback_title
    )
    assert fallback_state.get("selected_id") == fallback_id, (
        f"fallback selection was not persisted: {fallback_state!r}"
    )


# ---------------------------------------------------------------------------
# Scenario 3: a corrupt state file never blocks startup.
# ---------------------------------------------------------------------------
def test_corrupt_state_file_does_not_block_startup(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    workspace.create_project("Corrupt State Project")

    state_path = _state_path(workspace)
    state_path.parent.mkdir(parents=True, exist_ok=True)
    state_path.write_text('{"screen": "project", "selected_id": ', encoding="utf-8")

    session = session_factory(workspace, label="m29-corrupt-state")
    wait_home(session)
    assert "ORBIT" in session.text
    assert "panicked at" not in session.text, (
        "orbit-tui panicked on corrupt session state:\n"
        f"{session.text[-3000:]}\nEvidence: {session.evidence_dir}"
    )

    session.quit_via(b"q")
    session.assert_clean_exit()
