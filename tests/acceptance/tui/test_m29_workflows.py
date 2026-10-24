"""M29 I6 acceptance: targeted workflow scenarios against the real ``orbit-tui``.

This file completes the M29 PTY acceptance coverage that is not already in
``test_m29_selection.py`` (Scenario A), ``test_m29_connected_recovery.py``
(Scenario E) or the older M28 files:

* Scenario B - everyday task editing: quick edit (``E``), multiline details
  (``e``), acceptance criteria (``K`` -> ``c``), a workflow status change
  (``S``), task detail / activity inspection and unsaved-form safety;
* Scenario C - switching between five projects through the sidebar and the
  command palette, opening different views, and restoring the selected task
  plus the ``g p`` / ``Esc`` history context;
* Scenario D - the responsive Kanban at 60x18, 80x24, 100x30, 120x35 and
  160x45: closed frame, column window legend, selected-card marker, ``j``
  traversal of every non-empty column and a status transition derived from
  the task workflow;
* Scenario F - quit and modal safety from the new surfaces (workspace,
  task detail, dirty form, search, palette, connected pending inspector and
  the connected console).

The tests drive ``target/debug/orbit-tui`` through the shared PTY harness and
assert against the reconstructed screen.  Every session ends with
``assert_clean_exit()`` so a crash, a leaked alternate screen or a panic is a
failure.  All waits have deadlines; there are no assertion sleeps.

Run from the repository root after::

    cargo build -p orbit-tui -p orbit-cli -p orbit-server --offline

Evidence is written under ``target/tui-evidence/<run-id>/<test>-<n>/``.
"""
from __future__ import annotations

import json
import os
import re
import secrets
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path
from typing import Callable

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
ROOT = Path(__file__).resolve().parents[3]

from pty_harness import (  # noqa: E402
    CTRL_Q,
    ENTER,
    ESC,
    TuiSession,
    Workspace,
)
from settle import wait_until_settled  # noqa: E402

CTRL_S = b"\x13"

DETAIL_WIDTH_140 = 30
COLS = 140
ROWS = 40

HOME_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Home / Today"
PROJECTS_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Projects"
PROJECT_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*{name}"
VIEW_SWITCH_HINT = "[ ] switch view"

# The task workflow columns rendered by the Basic board, in render order.
BOARD_COLUMNS = [
    ("inbox", "Inbox"),
    ("planned", "Planned"),
    ("ready", "Ready"),
    ("in_progress", "In progress"),
    ("waiting", "Waiting"),
    ("blocked", "Blocked"),
    ("review", "Review"),
    ("deferred", "Deferred"),
    ("done", "Done"),
]
BOARD_COLUMN_TITLES = [title for _, title in BOARD_COLUMNS]

TRANSITION_PATHS: dict[str, list[str]] = {
    "inbox": [],
    "planned": ["planned"],
    "ready": ["planned", "ready"],
    # CLI status values are kebab-cased; the database/projection uses the
    # canonical ``in_progress`` spelling.
    "in_progress": ["planned", "ready", "in-progress"],
    "review": ["planned", "ready", "in-progress", "review"],
    "done": ["planned", "ready", "in-progress", "done"],
}

SERVER = Path(
    os.environ.get("ORBIT_SERVER_BIN", ROOT / "target" / "debug" / "orbit-server")
)
TUI = Path(os.environ.get("ORBIT_TUI_BIN", ROOT / "target" / "debug" / "orbit-tui"))
REQUIRE_EVIDENCE = os.environ.get("ORBIT_REQUIRE_CONNECTED_TUI_EVIDENCE") == "1"
CONNECTED_WAIT = 10.0


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


def _sqlite_rows(workspace: Workspace, query: str, parameters: tuple = ()) -> list:
    connection = sqlite3.connect(workspace.db)
    try:
        return connection.execute(query, parameters).fetchall()
    finally:
        connection.close()


def _sqlite_scalar(workspace: Workspace, query: str, parameters: tuple = ()):
    rows = _sqlite_rows(workspace, query, parameters)
    assert rows, f"no row for query {query!r} {parameters!r}"
    return rows[0][0]


def _task_row(workspace: Workspace, task_id: str):
    rows = _sqlite_rows(
        workspace,
        "SELECT id, title, status, description, priority FROM tasks WHERE id=?1",
        (task_id,),
    )
    assert rows, f"task {task_id!r} not persisted"
    return rows[0]


def _seed_project(
    workspace: Workspace,
    project_name: str,
    task_specs: list[tuple[str, list[str]]],
) -> tuple[str, dict[str, str]]:
    """Seed a real project and tasks through the CLI, then return their ids.

    ``task_specs`` is ``[(title, transition_path), ...]`` where the path is the
    ordered list of CLI status transitions applied after creation (each one is
    validated by the authority, so seeding never bypasses domain rules).
    """
    workspace.create_project(project_name)
    project_id = _sqlite_scalar(
        workspace, "SELECT id FROM projects WHERE name=?1", (project_name,)
    )
    task_ids: dict[str, str] = {}
    for title, transitions in task_specs:
        payload = _cli_json(workspace, "task", "add", title, "--project", project_id)
        task_id = payload["task"]["id"]
        persisted_id = _sqlite_scalar(
            workspace, "SELECT id FROM tasks WHERE title=?1", (title,)
        )
        assert persisted_id == task_id, (persisted_id, task_id)
        for status in transitions:
            _cli_json(workspace, "task", "status", task_id, status)
        task_ids[title] = task_id
    return project_id, task_ids


def _clone_workspace(source: Workspace, base: Path, label: str) -> Workspace:
    """A fresh deployment over a consistent copy of the seeded database.

    Scenario D mutates one card per size; cloning keeps every size's column
    distribution identical without re-running the CLI seed.
    """
    root = Path(tempfile.mkdtemp(prefix=f"orbit-tui-{label}-", dir=str(base)))
    db = root / "orbit.db"
    with sqlite3.connect(source.db) as source_connection:
        with sqlite3.connect(db) as target_connection:
            source_connection.backup(target_connection)
    config = root / "orbit.config.toml"
    home = root / "home"
    home.mkdir(parents=True, exist_ok=True)
    env = {
        key: value for key, value in os.environ.items() if not key.startswith("ORBIT_")
    }
    env.update(
        {
            "HOME": str(home),
            "ORBIT_CONFIG": str(config),
            "ORBIT_DB": str(db),
            "TERM": "xterm-256color",
            "LANG": env.get("LANG") or "C.UTF-8",
        }
    )
    return Workspace(root=root, db=db, config=config, home=home, env=env)


def wait_home(session: TuiSession) -> None:
    session.wait_for_screen_regex(
        HOME_HEADER_RE, timeout=10.0, description="Home / Today header"
    )


def open_projects(session: TuiSession) -> None:
    session.send_keys(b"g", b"p", delay=0.15)
    session.wait_for_screen_regex(
        PROJECTS_HEADER_RE, timeout=8.0, description="Projects screen"
    )


def wait_project_header(session: TuiSession, name: str) -> None:
    session.wait_for_screen_regex(
        PROJECT_HEADER_RE.format(name=re.escape(name)),
        timeout=10.0,
        description=f"project header {name!r}",
    )


def open_first_project(session: TuiSession, project_name: str) -> None:
    session.send(ENTER, note="open first project")
    wait_project_header(session, project_name)
    wait_view(session, "Overview")


def _current_view_title(screen_text: str) -> str | None:
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


def _focused_pane(screen_text: str) -> str | None:
    """Which pane owns the heavy border (the harness has no colour model)."""
    for line in screen_text.splitlines():
        if "┏ Orbit" in line:
            return "sidebar"
        if "┏ Details" in line:
            return "detail"
        if "┏" in line:
            return "workspace"
    return None


def wait_workspace_focus(session: TuiSession, component_title: str) -> None:
    def focused(view: str) -> bool:
        return _focused_pane(view) == "workspace" and f"┏ {component_title}" in view

    session.wait_until_screen(
        focused,
        timeout=8.0,
        description=f"workspace focus on {component_title!r}",
    )
    assert "┏ Orbit" not in session.screen_text, (
        "the sidebar still owns the heavy border while the workspace should be "
        f"focused\nscreen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )


def focus_sidebar(session: TuiSession) -> None:
    """Tab until the sidebar owns the heavy border (wide layouts add Detail)."""
    for _ in range(3):
        if _focused_pane(session.screen_text) == "sidebar":
            return
        previous = _focused_pane(session.screen_text)
        session.send(b"\t", note="focus next pane")
        session.wait_until_screen(
            lambda view: _focused_pane(view) not in (None, previous),
            timeout=8.0,
            description="focus to leave the current pane",
        )
    assert _focused_pane(session.screen_text) == "sidebar", (
        f"could not focus the sidebar\nscreen:\n{session.screen_text}\n"
        f"Evidence: {session.evidence_dir}"
    )


def wait_tasks_painted(
    session: TuiSession, titles: list[str], timeout: float = 8.0
) -> None:
    """Wait until the Tasks body replaced the previous view's panels."""
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


def _marked_titles(screen_text: str, titles: list[str]) -> list[str]:
    """Titles whose focused table row carries the visible ``▸`` marker.

    Table rows start with the heavy workspace border (``┃▸``); requiring it
    keeps a sidebar marker on the same terminal row from being mistaken for a
    selected task.
    """
    marked: list[str] = []
    for line in screen_text.splitlines():
        if "┃▸" not in line:
            continue
        for title in titles:
            if title in line:
                marked.append(title)
                break
    return marked


def _table_selected_title(
    session: TuiSession, titles: list[str], timeout: float = 8.0
) -> str:
    session.wait_until_screen(
        lambda view: len(_marked_titles(view, titles)) == 1,
        timeout=timeout,
        description="exactly one visibly selected task row",
    )
    return _marked_titles(session.screen_text, titles)[0]


def wait_selected_title(session: TuiSession, title: str, titles: list[str]) -> None:
    session.wait_until_screen(
        lambda view: _marked_titles(view, titles) == [title],
        timeout=8.0,
        description=f"selected task row to be {title!r}",
    )


def _detail_pane_text(session: TuiSession, cols: int = COLS) -> str:
    return "\n".join(line[cols - DETAIL_WIDTH_140 :] for line in session.screen_lines)


def _sidebar_selected_name(screen_text: str, names: list[str]) -> str | None:
    for line in screen_text.splitlines():
        if "▸" not in line:
            continue
        for name in names:
            if name in line:
                return name
    return None


# ---------------------------------------------------------------------------
# Kanban helpers (Scenario D).
# ---------------------------------------------------------------------------
def _board_workspace_slice(line: str) -> str:
    """The part of a body row inside the focused board's heavy border.

    The sidebar shares a terminal row with the board, so its own ``▸`` marker
    must not be mistaken for a selected card.
    """
    border = line.find("┃")
    return line[border:] if border >= 0 else ""


def _board_marked_prefix(screen_text: str, prefixes: list[str]) -> str | None:
    """The card immediately after the ``▸`` marker.

    Several board columns can share one terminal row, so the card title is
    resolved relative to the marker instead of "any prefix on the line".
    """
    for line in screen_text.splitlines():
        slice_ = _board_workspace_slice(line)
        marker = slice_.find("▸")
        if marker < 0:
            continue
        tail = slice_[marker:]
        best: str | None = None
        best_position: int | None = None
        for prefix in prefixes:
            position = tail.find(prefix)
            if position >= 0 and (best_position is None or position < best_position):
                best = prefix
                best_position = position
        if best is not None:
            return best
    return None


def _board_settled(
    screen_text: str, prefixes: list[str], selected: str | None = None
) -> bool:
    """A settled board frame: the body and the selected card are painted.

    A closed modal's title disappears before the board below it is repainted,
    so "modal title absent" alone can sample a half-painted frame.
    """
    return (
        "Task board" in screen_text
        and "Actions:" in screen_text
        and (selected is None or _board_marked_prefix(screen_text, prefixes) == selected)
    )


def _board_selected_column(screen_text: str) -> str | None:
    for line in screen_text.splitlines():
        if "▾" not in line:
            continue
        match = re.search(r"▾\s+([A-Za-z][A-Za-z ]*?)\s+\d+", line)
        if match:
            return match.group(1).strip()
    return None


def _board_legend(screen_text: str) -> tuple[int, int, int] | None:
    match = re.search(r"columns (\d+)-(\d+) of (\d+)", screen_text)
    if not match:
        return None
    return int(match.group(1)), int(match.group(2)), int(match.group(3))


def _assert_legend_contains(screen_text: str, column_title: str, context: str) -> None:
    legend = _board_legend(screen_text)
    assert legend is not None, (
        f"{context}: the board legend did not name a column window\n"
        f"screen:\n{screen_text}"
    )
    start, end, total = legend
    assert total == len(BOARD_COLUMNS), (
        f"{context}: the board legend counted {total} columns, expected "
        f"{len(BOARD_COLUMNS)}\nscreen:\n{screen_text}"
    )
    assert 1 <= start <= end <= total, (
        f"{context}: invalid column window {start}-{end} of {total}\n"
        f"screen:\n{screen_text}"
    )
    index = BOARD_COLUMN_TITLES.index(column_title) + 1
    assert start <= index <= end, (
        f"{context}: selected column {column_title!r} ({index}) is outside the "
        f"visible window {start}-{end} of {total}\nscreen:\n{screen_text}"
    )


def _wait_board_card_change(
    session: TuiSession, prefixes: list[str], previous: str | None, description: str
) -> str:
    session.wait_until_screen(
        lambda view: (
            (current := _board_marked_prefix(view, prefixes)) is not None
            and current != previous
        ),
        timeout=8.0,
        description=description,
    )
    current = _board_marked_prefix(session.screen_text, prefixes)
    assert current is not None, session.screen_text
    return current


def _wait_board_card_selected(session: TuiSession, prefix: str) -> None:
    session.wait_until_screen(
        lambda view: _board_marked_prefix(view, [prefix]) == prefix,
        timeout=8.0,
        description=f"selected board card {prefix!r}",
    )


def _walk_board_cards(
    session: TuiSession,
    prefixes: list[str],
    status_by_prefix: dict[str, str],
    expected_columns: list[str],
) -> list[tuple[str, str]]:
    """Press ``j`` through every card, asserting the window follows each move."""
    session.wait_until_screen(
        lambda view: (
            _board_state(view, prefixes, status_by_prefix) is not None
            and _board_legend(view) is not None
            and "┏" in view
            and "┗" in view
        ),
        timeout=8.0,
        description="a fully painted board with a settled selection",
    )
    first_state = _board_state(session.screen_text, prefixes, status_by_prefix)
    assert first_state is not None, (
        f"no selected card or column after opening the board\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )
    first, first_column = first_state
    _assert_legend_contains(session.screen_text, first_column, "board opened")

    observed: list[tuple[str, str]] = [(first, first_column)]
    first_legend = _board_legend(session.screen_text)
    assert first_legend is not None
    window_starts: list[int] = [first_legend[0]]
    current = first
    for _ in range(len(prefixes) - 1):
        session.send(b"j", note="next board card")
        current, column = _wait_board_state_change(
            session,
            prefixes,
            status_by_prefix,
            current,
            "board selection to move to the next card",
        )
        context = f"after moving to {current!r}"
        _assert_legend_contains(session.screen_text, column, context)
        legend = _board_legend(session.screen_text)
        assert legend is not None
        window_starts.append(legend[0])
        observed.append((current, column))

    reached = {column for _, column in observed}
    assert reached == set(expected_columns), (
        "pressing j did not reach every non-empty column\n"
        f"reached: {sorted(reached)!r}\nexpected: {sorted(expected_columns)!r}\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )
    assert len({prefix for prefix, _ in observed}) == len(prefixes), (
        f"j traversed {len(observed)} distinct cards, expected {len(prefixes)}\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )
    assert len(set(window_starts)) > 1, (
        "the column window never followed the selection across the board\n"
        f"window starts: {window_starts!r}\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )
    return observed


def _board_state(
    screen_text: str,
    prefixes: list[str],
    status_by_prefix: dict[str, str],
) -> tuple[str, str] | None:
    """A stable board state: exactly one marked card that belongs to its column.

    ratatui writes a frame as cell diffs, so a screen sampled mid-frame can
    briefly show the old and the new ``▸`` marker (or a new column header with
    the old marker).  Requiring exactly one marked card *and* the column that
    the card's seeded status maps to rejects those transient frames instead of
    recording a phantom selection.
    """
    marked: list[str] = []
    for line in screen_text.splitlines():
        slice_ = _board_workspace_slice(line)
        marker = slice_.find("▸")
        if marker < 0:
            continue
        tail = slice_[marker:]
        best: str | None = None
        best_position: int | None = None
        for prefix in prefixes:
            position = tail.find(prefix)
            if position >= 0 and (best_position is None or position < best_position):
                best = prefix
                best_position = position
        if best is not None:
            marked.append(best)
    if len(marked) != 1:
        return None
    prefix = marked[0]
    column = _board_selected_column(screen_text)
    if column is None:
        return None
    if column != _column_title_for_status(status_by_prefix[prefix]):
        return None
    return prefix, column


def _wait_board_state_change(
    session: TuiSession,
    prefixes: list[str],
    status_by_prefix: dict[str, str],
    previous: str | None,
    description: str,
) -> tuple[str, str]:
    session.wait_until_screen(
        lambda view: (
            (state := _board_state(view, prefixes, status_by_prefix)) is not None
            and state[0] != previous
            and _board_legend(view) is not None
        ),
        timeout=8.0,
        description=description,
    )
    state = _board_state(session.screen_text, prefixes, status_by_prefix)
    assert state is not None, (
        f"{description}: the board never settled on one marked card\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )
    return state


def _column_title_for_status(status: str) -> str:
    return BOARD_COLUMN_TITLES[[key for key, _ in BOARD_COLUMNS].index(status)]


def _select_board_card_with_status(
    session: TuiSession,
    prefixes: list[str],
    status_by_prefix: dict[str, str],
    wanted_status: str,
) -> tuple[str, str]:
    """Move up with ``k`` until the selected card carries ``wanted_status``."""
    expected_column = _column_title_for_status(wanted_status)
    for _ in range(len(prefixes)):
        state = _board_state(session.screen_text, prefixes, status_by_prefix)
        if state is None:
            session.wait_until_screen(
                lambda view: _board_state(view, prefixes, status_by_prefix)
                is not None,
                timeout=8.0,
                description="board selection to settle",
            )
            state = _board_state(session.screen_text, prefixes, status_by_prefix)
        assert state is not None, session.screen_text
        current, column = state
        if status_by_prefix[current] == wanted_status:
            assert column == expected_column, (
                f"card {current!r} is in column {column!r}, expected "
                f"{expected_column!r}\nscreen:\n{session.screen_text}\n"
                f"Evidence: {session.evidence_dir}"
            )
            return current, column
        session.send(b"k", note="previous board card")
        _wait_board_state_change(
            session,
            prefixes,
            status_by_prefix,
            current,
            "board selection to move to the previous card",
        )
    raise AssertionError(
        f"could not reach a {wanted_status!r} card with k\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )


# ---------------------------------------------------------------------------
# Scenario B: everyday task editing on a real project.
# ---------------------------------------------------------------------------
def test_everyday_task_editing_workflow(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    project_name = "Everyday Flow"
    base_titles = [
        "Alpha planning note",
        "Bravo release checklist",
        "Charlie design review",
        "Delta support triage",
    ]
    project_id, task_ids = _seed_project(
        workspace, project_name, [(title, ["planned", "ready"]) for title in base_titles]
    )

    session = session_factory(workspace, label="m29b-everyday")
    wait_home(session)
    open_projects(session)
    session.wait_for_screen_text(
        project_name, timeout=8.0, description="seeded project row"
    )
    open_first_project(session, project_name)

    switch_view(session, b"]", "Tasks")
    wait_tasks_painted(session, base_titles)
    wait_workspace_focus(session, "Tasks")

    # Pick a non-first row so the selection work is real.
    first_row = _table_selected_title(session, base_titles)
    session.send(b"j", note="select a non-first task")
    session.wait_until_screen(
        lambda view: len(_marked_titles(view, base_titles)) == 1
        and _marked_titles(view, base_titles)[0] != first_row,
        timeout=8.0,
        description="selection to move off the first task",
    )
    selected = _table_selected_title(session, base_titles)
    task_id = task_ids[selected]
    prefix = selected[:12]

    # ------------------------------------------------------------------
    # 1. Quick edit: title + priority through the compact form.
    # ------------------------------------------------------------------
    session.send(b"E", note="quick edit the selected task")
    session.wait_for_screen_text(
        "Quick edit", timeout=8.0, description="quick edit form"
    )
    session.wait_for_screen_text(
        "Title and priority; status changes stay in the S menu",
        timeout=8.0,
        description="quick edit hint",
    )
    session.send_text(" updated")
    session.send(b"\t", note="focus priority")
    session.send_keys(b"\x1b[C", b"\x1b[C", b"\x1b[C", delay=0.05)
    session.send(CTRL_S, note="save quick edit")
    session.wait_for_screen_text(
        "Quick edit saved", timeout=10.0, description="quick edit ack"
    )

    updated_title = f"{selected} updated"
    session.wait_until_screen(
        lambda view: "Quick edit ·" not in view
        and any(
            "▸" in line and updated_title in line and "high" in line
            for line in view.splitlines()
        ),
        timeout=10.0,
        description="updated table row with the high priority",
    )
    assert "PRIORITY" in session.screen_text, session.screen_text
    detail = _detail_pane_text(session)
    assert "Priority" in detail and "high" in detail, (
        "the detail pane did not show the new priority\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )
    stored = _task_row(workspace, task_id)
    assert stored[1] == updated_title, stored
    assert stored[4] == "high", stored

    # ------------------------------------------------------------------
    # 2. Board: the M29 card workflow, starting from the same task.
    # ------------------------------------------------------------------
    switch_view(session, b"]", "Board")
    session.wait_for_screen_text(
        "Task board", timeout=8.0, description="Board body painted"
    )
    wait_workspace_focus(session, "Task board")
    board_prefixes = [title[:12] for title in base_titles]
    session.wait_until_screen(
        lambda view: _board_marked_prefix(view, board_prefixes) is not None,
        timeout=8.0,
        description="a board card to be selected",
    )
    current_card = _board_marked_prefix(session.screen_text, board_prefixes)
    for _ in range(len(board_prefixes)):
        if current_card == prefix:
            break
        session.send(b"j", note="select the edited card on the board")
        current_card = _wait_board_card_change(
            session, board_prefixes, current_card, "board selection to move"
        )
    assert current_card == prefix, (
        f"could not select the edited card {prefix!r} on the board\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )

    # Full edit: two description lines saved with Ctrl+S.
    session.send(b"e", note="open the full task edit form")
    session.wait_for_screen_text("Edit task", timeout=8.0, description="edit form")
    session.send(b"\t", note="focus priority")
    session.send(b"\t", note="focus description")
    session.send_text("First line of context")
    session.send(ENTER, note="newline inside the description")
    session.send_text("Second line of context")
    session.send(CTRL_S, note="save the multiline description")
    session.wait_for_screen_text(
        "Task updated", timeout=10.0, description="full edit ack"
    )
    wait_until_settled(
        session,
        lambda view: "Everyday fields first" not in view
        and _board_settled(view, [prefix], prefix),
        timeout=10.0,
        description="edit form closes and the board repaints with the same card",
    )
    stored = _task_row(workspace, task_id)
    assert stored[3] == "First line of context\nSecond line of context", stored
    assert stored[4] == "high", stored

    # Acceptance criteria: K -> c, add one, and prove it is ordered state.
    session.send(b"K", note="open acceptance criteria")
    session.wait_for_screen_text(
        "Acceptance criteria", timeout=8.0, description="criteria modal"
    )
    session.send(b"c", note="add a criterion")
    session.wait_for_screen_text(
        "Add acceptance criterion", timeout=8.0, description="criterion form"
    )
    session.send_text("Offline acceptance run passes")
    session.send(ENTER, note="save the criterion")
    session.wait_until_screen(
        lambda view: "Offline acceptance run passes" in view
        and "Add acceptance criterion" not in view,
        timeout=10.0,
        description="criterion listed after saving",
    )
    assert "[ ] Offline acceptance run passes" in session.screen_text, (
        f"the new criterion was not listed unchecked\nscreen:\n{session.screen_text}\n"
        f"Evidence: {session.evidence_dir}"
    )
    criteria = _sqlite_rows(
        workspace,
        "SELECT text FROM task_acceptance_criteria WHERE task_id=?1",
        (task_id,),
    )
    assert [row[0] for row in criteria] == ["Offline acceptance run passes"], criteria
    session.send(ESC, note="close acceptance criteria")
    wait_until_settled(
        session,
        lambda view: "Acceptance criteria ·" not in view
        and _board_settled(view, [prefix], prefix),
        timeout=8.0,
        description="criteria modal closes and the board repaints",
    )

    # Status: S lists the real workflow targets and executes the first one.
    session.send(b"S", note="open the status menu")
    session.wait_for_screen_text("Actions · Status ·", timeout=8.0, description="status menu")
    menu = session.wait_until_screen(
        lambda view: "Move to in_progress" in view and "Move to blocked" in view,
        timeout=8.0,
        description="allowed workflow transitions",
    )
    assert "Move to in_progress" in menu, menu
    session.send(ENTER, note="move the task to in_progress")
    session.wait_until_screen(
        lambda view: "Actions · Status ·" not in view
        and _board_selected_column(view) == "In progress",
        timeout=10.0,
        description="status menu closes and the card moves to In progress",
    )
    assert _board_marked_prefix(session.screen_text, [prefix]) == prefix, (
        "the status change lost the selected card\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )
    stored = _task_row(workspace, task_id)
    assert stored[2] == "in_progress", stored

    # Detail and activity for the same card.
    session.send(ENTER, note="inspect the task detail")
    session.wait_for_screen_text(" · detail", timeout=8.0, description="detail modal")
    detail_screen = session.wait_for_screen_text(
        "Description: First line of context",
        timeout=8.0,
        description="detail shows the multiline description",
    )
    assert updated_title in detail_screen, (
        f"the detail modal described another task\nscreen:\n{detail_screen}\n"
        f"Evidence: {session.evidence_dir}"
    )
    session.send(ESC, note="close the detail modal")
    wait_until_settled(
        session,
        lambda view: " · detail" not in view
        and _board_settled(view, [prefix], prefix),
        timeout=8.0,
        description="detail modal closes and the board repaints",
    )
    _wait_board_card_selected(session, prefix)

    session.send(b"H", note="inspect the task activity")
    session.wait_for_screen_text("Activity ·", timeout=8.0, description="activity modal")
    activity = session.wait_until_screen(
        lambda view: "Activity ·" in view and "No recorded activity" not in view,
        timeout=8.0,
        description="activity history with entries",
    )
    assert any(
        marker in activity for marker in ("status changed", "updated", "created")
    ), (
        f"the activity modal listed no meaningful entries\nscreen:\n{activity}\n"
        f"Evidence: {session.evidence_dir}"
    )
    session.send(ESC, note="close the activity modal")
    wait_until_settled(
        session,
        lambda view: "Activity ·" not in view
        and _board_settled(view, [prefix], prefix),
        timeout=8.0,
        description="activity modal closes and the board repaints",
    )
    _wait_board_card_selected(session, prefix)

    # ------------------------------------------------------------------
    # 3. Unsaved-form safety: Esc -> discard prompt -> n keeps the text.
    # ------------------------------------------------------------------
    session.send(b"e", note="open the edit form again")
    session.wait_for_screen_text("Edit task", timeout=8.0, description="edit form")
    session.send_text(" unsafe")
    session.send(ESC, note="escape a dirty form")
    session.wait_for_screen_text(
        "Discard unsaved changes?", timeout=8.0, description="discard confirmation"
    )
    session.send(b"n", note="keep editing")
    wait_until_settled(
        session,
        lambda view: "Discard unsaved changes?" not in view and " unsafe" in view,
        timeout=8.0,
        description="form restored with the unsaved text",
    )
    assert f"{updated_title} unsafe" in session.screen_text, (
        "cancelling the discard lost the typed title\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )
    session.send(ESC, note="escape the dirty form again")
    session.wait_for_screen_text(
        "Discard unsaved changes?", timeout=8.0, description="second discard prompt"
    )
    session.send(b"y", note="confirm discarding the form")
    wait_until_settled(
        session,
        lambda view: "Discard unsaved changes?" not in view
        and "Edit task" not in view
        and _board_settled(view, [prefix], prefix),
        timeout=8.0,
        description="form closes after discarding and the board repaints",
    )
    _wait_board_card_selected(session, prefix)
    # The discarded edit must not have been persisted.
    stored = _task_row(workspace, task_id)
    assert stored[1] == updated_title, stored

    session.quit_via(CTRL_Q)
    session.assert_clean_exit()


# ---------------------------------------------------------------------------
# Scenario C: switching five projects via the sidebar and the palette.
# ---------------------------------------------------------------------------
PROJECT_NAMES = ["Alpha Lab", "Bravo Lab", "Charlie Lab", "Delta Lab", "Echo Lab"]


def _project_tasks(name: str) -> list[str]:
    short = name.split(" ", 1)[0]
    return [
        f"{short} first task",
        f"{short} second task",
        f"{short} signature task",
    ]


def _open_project_via_palette(session: TuiSession, name: str) -> None:
    session.send(b":", note="open command palette")
    session.wait_for_screen_text(
        "Command palette", timeout=8.0, description="command palette"
    )
    session.send_text(name)
    session.wait_until_screen(
        lambda view: any(
            "▸" in line and f"Open project: {name}" in line
            for line in view.splitlines()
        ),
        timeout=8.0,
        description=f"palette selection on {name!r}",
    )
    session.send(ENTER, note=f"open project {name} via the palette")
    wait_project_header(session, name)


def _select_non_first_task(
    session: TuiSession, titles: list[str]
) -> tuple[str, str]:
    """Move down once and return (visible title, stable prefix)."""
    wait_tasks_painted(session, titles)
    first = _table_selected_title(session, titles)
    session.send(b"j", note="select a non-first task")
    session.wait_until_screen(
        lambda view: len(_marked_titles(view, titles)) == 1
        and _marked_titles(view, titles)[0] != first,
        timeout=8.0,
        description="selection to move off the first task",
    )
    title = _table_selected_title(session, titles)
    return title, title[:12]


def test_project_switching_and_context_return(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    task_titles: dict[str, list[str]] = {}
    for name in PROJECT_NAMES:
        titles = _project_tasks(name)
        _seed_project(workspace, name, [(title, []) for title in titles])
        task_titles[name] = titles

    session = session_factory(workspace, label="m29c-projects")
    wait_home(session)
    open_projects(session)
    session.wait_for_screen_text(
        PROJECT_NAMES[0], timeout=8.0, description="seeded project rows"
    )

    # The Projects list order is the authority's; open the first row and learn
    # which project it is from the header (never assume an ordering).
    session.send(ENTER, note="open the first project row")
    session.wait_until_screen(
        lambda view: any(line.startswith(" ORBIT ›") for line in view.splitlines())
        and _current_view_title(view) == "Overview",
        timeout=10.0,
        description="first project workspace",
    )
    header = "\n".join(
        line for line in session.screen_text.splitlines() if "ORBIT ›" in line
    )
    first_name = next((name for name in PROJECT_NAMES if name in header), None)
    assert first_name is not None, (
        f"the header did not name a seeded project: {header!r}\n"
        f"Evidence: {session.evidence_dir}"
    )

    selections: dict[str, tuple[str, str, int]] = {}
    # record(name, view_steps, selected title)
    steps_by_name: dict[str, int] = {}

    def record_selection(name: str, view_steps: int) -> None:
        for _ in range(view_steps):
            session.send(b"]", note="switch to the next project view")
        expected_view = "Tasks" if view_steps == 1 else "Board"
        wait_view(session, expected_view)
        if expected_view == "Tasks":
            wait_tasks_painted(session, task_titles[name])
            title, prefix = _select_non_first_task(session, task_titles[name])
            selections[name] = (title, prefix, view_steps)
        else:
            session.wait_for_screen_text(
                "Task board", timeout=8.0, description="Board painted"
            )
            prefixes = [task[:12] for task in task_titles[name]]
            session.wait_until_screen(
                lambda view: _board_marked_prefix(view, prefixes) is not None,
                timeout=8.0,
                description="a card to be selected",
            )
            first_card = _board_marked_prefix(session.screen_text, prefixes)
            assert first_card is not None
            session.send(b"j", note="select a non-first card")
            second = _wait_board_card_change(
                session, prefixes, first_card, "card selection to move"
            )
            selections[name] = (second, second, view_steps)
        wait_workspace_focus(session, "Tasks" if view_steps == 1 else "Task board")

    # Project 1: opened from the list; Tasks view.
    assert _focused_pane(session.screen_text) == "workspace", session.screen_text
    steps_by_name[first_name] = 1
    record_selection(first_name, 1)

    # Projects 2 and 3: switch through the sidebar with j + Enter.
    current = first_name
    for index in (1, 2):
        focus_sidebar(session)
        selected_name = _sidebar_selected_name(session.screen_text, PROJECT_NAMES)
        assert selected_name == current, (
            f"the sidebar did not mark the open project {current!r} "
            f"(saw {selected_name!r})\nEvidence: {session.evidence_dir}"
        )
        session.send(b"j", note="next sidebar project")
        session.wait_until_screen(
            lambda view: (
                (name := _sidebar_selected_name(view, PROJECT_NAMES)) is not None
                and name != current
            ),
            timeout=8.0,
            description="sidebar selection to move to the next project",
        )
        next_name = _sidebar_selected_name(session.screen_text, PROJECT_NAMES)
        assert next_name is not None and next_name != current
        session.send(ENTER, note=f"open {next_name} from the sidebar")
        wait_project_header(session, next_name)
        assert _focused_pane(session.screen_text) == "workspace", (
            "opening a project from the sidebar left the focus outside the "
            f"workspace\nscreen:\n{session.screen_text}\n"
            f"Evidence: {session.evidence_dir}"
        )
        steps = 1 if index == 1 else 2
        steps_by_name[next_name] = steps
        record_selection(next_name, steps)
        current = next_name

    # Projects 4 and 5: switch through the command palette.
    for index in (3, 4):
        name = PROJECT_NAMES[index]
        _open_project_via_palette(session, name)
        assert _focused_pane(session.screen_text) == "workspace", (
            "opening a project from the palette left the focus outside the "
            f"workspace\nscreen:\n{session.screen_text}\n"
            f"Evidence: {session.evidence_dir}"
        )
        steps = 1 if index % 2 == 0 else 2
        steps_by_name[name] = steps
        record_selection(name, steps)
        current = name

    # Return to every project and require the previously selected task back.
    for name in PROJECT_NAMES:
        _open_project_via_palette(session, name)
        steps = steps_by_name[name]
        for _ in range(steps):
            session.send(b"]", note="return to the recorded view")
        expected_view = "Tasks" if steps == 1 else "Board"
        wait_view(session, expected_view)
        title, prefix, _ = selections[name]
        if expected_view == "Tasks":
            wait_tasks_painted(session, task_titles[name])
            wait_selected_title(session, title, task_titles[name])
        else:
            session.wait_for_screen_text(
                "Task board", timeout=8.0, description="Board painted after return"
            )
            prefixes = [task[:12] for task in task_titles[name]]
            session.wait_until_screen(
                lambda view: _board_marked_prefix(view, prefixes) == prefix,
                timeout=8.0,
                description=f"restored card {prefix!r} in {name!r}",
            )
        wait_workspace_focus(session, "Tasks" if steps == 1 else "Task board")
        current = name

    # g p then Esc returns to this project, its view and its selection.
    restored_name = current
    steps = steps_by_name[restored_name]
    title, prefix, _ = selections[restored_name]
    session.send_keys(b"g", b"p", delay=0.15)
    session.wait_for_screen_regex(
        PROJECTS_HEADER_RE, timeout=8.0, description="Projects from g p"
    )
    session.send(ESC, note="return to the previous context")
    wait_project_header(session, restored_name)
    wait_view(session, "Tasks" if steps == 1 else "Board")
    if steps == 1:
        wait_tasks_painted(session, task_titles[restored_name])
        wait_selected_title(session, title, task_titles[restored_name])
    else:
        prefixes = [task[:12] for task in task_titles[restored_name]]
        session.wait_until_screen(
            lambda view: _board_marked_prefix(view, prefixes) == prefix,
            timeout=8.0,
            description="restored card after Esc from Projects",
        )
    assert _focused_pane(session.screen_text) == "workspace", session.screen_text

    session.quit_via(CTRL_Q)
    session.assert_clean_exit()


# ---------------------------------------------------------------------------
# Scenario D: responsive Kanban across the five acceptance sizes.
# ---------------------------------------------------------------------------
SIZES = [(60, 18), (80, 24), (100, 30), (120, 35), (160, 45)]
D_PROJECT = "Responsive Board"
D_CARD_PREFIXES = [f"Card {index:02d}" for index in range(12)]
D_STATUSES = [
    "inbox",
    "inbox",
    "planned",
    "planned",
    "ready",
    "ready",
    "in_progress",
    "in_progress",
    "review",
    "review",
    "done",
    "done",
]


def _seed_responsive_board(workspace: Workspace) -> dict[str, str]:
    specs: list[tuple[str, list[str]]] = []
    for prefix, status in zip(D_CARD_PREFIXES, D_STATUSES):
        title = (
            f"{prefix} - responsive board card with a deliberately long "
            "title that must never collapse a column"
        )
        specs.append((title, TRANSITION_PATHS[status]))
    _, task_ids = _seed_project(workspace, D_PROJECT, specs)
    return task_ids


def test_responsive_kanban_board_at_five_sizes(session_factory, tmp_path):
    seeded = Workspace.create(tmp_path)
    task_ids = _seed_responsive_board(seeded)
    title_by_prefix = {
        prefix: title for prefix, title in zip(D_CARD_PREFIXES, task_ids)
    }
    expected_columns: list[str] = []
    for status in D_STATUSES:
        column = _column_title_for_status(status)
        if column not in expected_columns:
            expected_columns.append(column)

    for cols, rows in SIZES:
        # Each size starts from the seeded distribution; the transition below
        # mutates this copy only.
        status_by_prefix = dict(zip(D_CARD_PREFIXES, D_STATUSES))
        # Each size is an independent deployment: the status transition below
        # must not empty the Ready column for the next size.
        workspace = _clone_workspace(seeded, tmp_path, f"board-{cols}x{rows}")
        session = session_factory(
            workspace, label=f"m29d-board-{cols}x{rows}", cols=cols, rows=rows
        )
        try:
            wait_home(session)
            open_projects(session)
            session.wait_for_screen_text(
                D_PROJECT, timeout=8.0, description="seeded board project"
            )
            session.send(ENTER, note="open the responsive project")
            wait_project_header(session, D_PROJECT)
            session.send_keys(b"]", b"]", delay=0.1)
            session.wait_for_screen_text(
                "Task board", timeout=8.0, description=f"Board at {cols}x{rows}"
            )
            wait_workspace_focus(session, "Task board")

            label = f"{cols}x{rows}"
            # The frame, the legend and the card marker are painted in that
            # order; wait for the complete frame before sampling it.
            session.wait_until_screen(
                lambda view: (
                    "┏" in view
                    and "┗" in view
                    and _board_legend(view) is not None
                    and any(
                        _board_marked_prefix(view, [prefix]) == prefix
                        for prefix in D_CARD_PREFIXES
                    )
                ),
                timeout=8.0,
                description=f"fully painted Board frame at {label}",
            )
            screen = session.screen_text
            assert "┏" in screen and "┗" in screen, (
                f"{label}: the board frame lost a corner\nscreen:\n{screen}\n"
                f"Evidence: {session.evidence_dir}"
            )
            assert "panicked at" not in session.text, (
                f"{label}: orbit-tui panicked\nEvidence: {session.evidence_dir}"
            )
            assert _board_legend(screen) is not None, (
                f"{label}: the board legend did not show a column window\n"
                f"screen:\n{screen}\nEvidence: {session.evidence_dir}"
            )
            start, end, total = _board_legend(screen)
            assert total == len(BOARD_COLUMNS) and (start > 1 or end < total), (
                f"{label}: expected a windowed legend over {len(BOARD_COLUMNS)} "
                f"columns, saw {start}-{end} of {total}\n"
                f"screen:\n{screen}\nEvidence: {session.evidence_dir}"
            )

            # Every card is reachable with j and the window follows each move.
            _walk_board_cards(
                session, D_CARD_PREFIXES, status_by_prefix, expected_columns
            )

            # Move back to a card the workflow allows to transition.
            card, column_title = _select_board_card_with_status(
                session, D_CARD_PREFIXES, status_by_prefix, "ready"
            )
            assert column_title == "Ready", (
                f"{label}: selected a {column_title!r} card instead of Ready\n"
                f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
            )
            session.send(b"S", note="open the status menu")
            session.wait_for_screen_text(
                "Actions · Status ·", timeout=8.0, description=f"status menu at {label}"
            )
            menu = session.wait_until_screen(
                lambda view: "Move to in_progress" in view
                and "Move to blocked" in view,
                timeout=8.0,
                description=f"workflow targets at {label}",
            )
            assert "Move to in_progress" in menu, menu
            session.send(ENTER, note="execute the workflow transition")
            status_by_prefix[card] = "in_progress"
            session.wait_until_screen(
                lambda view: "Actions · Status ·" not in view
                and _board_state(view, D_CARD_PREFIXES, status_by_prefix)
                == (card, "In progress")
                and _board_legend(view) is not None,
                timeout=10.0,
                description=f"card moved to In progress at {label}",
            )
            _assert_legend_contains(
                session.screen_text, "In progress", f"{label} after transition"
            )
            moved_id = task_ids[title_by_prefix[card]]
            stored = _task_row(workspace, moved_id)
            assert stored[2] == "in_progress", (
                f"{label}: the card was not persisted as in_progress: {stored!r}\n"
                f"Evidence: {session.evidence_dir}"
            )

            session.quit_via(CTRL_Q)
            session.assert_clean_exit()
        finally:
            session.close()


# ---------------------------------------------------------------------------
# Scenario F: quit and modal safety from the new surfaces.
# ---------------------------------------------------------------------------
def _assert_confirmation_then_preserved(
    session: TuiSession,
    *,
    surface: str,
    preserved_text: str,
    still_visible: Callable[[str], bool],
) -> None:
    mark = session.mark()
    session.send(CTRL_Q, note=f"quit request from {surface}")
    outcome = session.wait_for_quit_response(timeout=5.0, since=mark)
    assert outcome == "confirmation", (
        f"{surface}: unsaved text must produce a quit confirmation, not a "
        f"silent exit\nscreen:\n{session.screen_text}\n"
        f"Evidence: {session.evidence_dir}"
    )
    lower = session.screen_text.lower()
    assert "quit" in lower and "unsaved" in lower, (
        f"{surface}: the quit confirmation did not name Quit and unsaved input\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )
    session.send(b"n", note="cancel the quit and keep editing")
    wait_until_settled(
        session,
        lambda view: "Quit Orbit?" not in view
        and still_visible(view)
        and preserved_text in view,
        timeout=5.0,
        description=f"quit confirmation to close and {surface} to repaint",
    )
    assert session.alive, f"{surface}: n ended the application"
    session.wait_until_screen(
        lambda view: still_visible(view) and preserved_text in view,
        timeout=5.0,
        description=f"unsaved input preserved on {surface}",
    )
    session.quit_via(CTRL_Q)
    session.assert_clean_exit()


def test_quit_from_main_workspace_is_direct(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    _seed_project(workspace, "Quit Workspace", [("Only task", [])])
    session = session_factory(workspace, label="m29f-workspace")
    wait_home(session)
    open_projects(session)
    session.wait_for_screen_text(
        "Quit Workspace", timeout=8.0, description="seeded project"
    )
    session.send(ENTER, note="open the project workspace")
    wait_project_header(session, "Quit Workspace")
    wait_workspace_focus(session, "Tasks")

    session.send(CTRL_Q, note="quit from the main workspace")
    session.assert_clean_exit()
    assert "Quit Orbit?" not in session.text, (
        "a clean workspace must quit without an unsaved-input prompt\n"
        f"Evidence: {session.evidence_dir}"
    )


def test_quit_from_project_details_is_direct(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    _seed_project(workspace, "Quit Details", [("Detail task", [])])
    session = session_factory(workspace, label="m29f-details")
    wait_home(session)
    open_projects(session)
    session.wait_for_screen_text(
        "Quit Details", timeout=8.0, description="seeded project"
    )
    session.send(ENTER, note="open the project workspace")
    wait_project_header(session, "Quit Details")
    switch_view(session, b"]", "Tasks")
    wait_tasks_painted(session, ["Detail task"])
    session.send(ENTER, note="open the task detail")
    session.wait_for_screen_text(" · detail", timeout=8.0, description="detail modal")

    session.send(CTRL_Q, note="quit from the project detail modal")
    session.assert_clean_exit()
    assert "Quit Orbit?" not in session.text, (
        "a read-only detail modal must quit without an unsaved-input prompt\n"
        f"Evidence: {session.evidence_dir}"
    )


def test_quit_from_dirty_form_confirms_and_preserves(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    session = session_factory(workspace, label="m29f-form")
    wait_home(session)
    session.send(b"c", note="open the new task form")
    session.wait_for_screen_text("New task", timeout=8.0, description="task form")
    session.send_text("unsaved draft")
    _assert_confirmation_then_preserved(
        session,
        surface="dirty text form",
        preserved_text="unsaved draft",
        still_visible=lambda view: "New task" in view,
    )


def test_quit_from_search_with_text_confirms_and_preserves(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    session = session_factory(workspace, label="m29f-search")
    wait_home(session)
    session.send(b"/", note="open search")
    session.wait_for_screen_text("Search", timeout=8.0, description="search modal")
    session.send_text("alpha")
    session.wait_for_screen_text("/ alpha", timeout=8.0, description="search query")
    _assert_confirmation_then_preserved(
        session,
        surface="search with a query",
        preserved_text="/ alpha",
        still_visible=lambda view: "Search" in view,
    )


def test_quit_from_palette_with_text_confirms_and_preserves(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    session = session_factory(workspace, label="m29f-palette")
    wait_home(session)
    session.send(b":", note="open the command palette")
    session.wait_for_screen_text(
        "Command palette", timeout=8.0, description="command palette"
    )
    session.send_text("pro")
    session.wait_for_screen_text("> pro", timeout=8.0, description="palette query")
    _assert_confirmation_then_preserved(
        session,
        surface="command palette with a query",
        preserved_text="> pro",
        still_visible=lambda view: "Command palette" in view,
    )


# ---------------------------------------------------------------------------
# Connected fixture (copied from test_m29_connected_recovery.py; the harness
# and the other acceptance files stay untouched).
# ---------------------------------------------------------------------------
def _request(base, path, token=None, payload=None, device=None):
    headers = {}
    if token:
        headers["Authorization"] = "Bearer " + token
    if device:
        headers["x-orbit-device-id"] = device
    body = None if payload is None else json.dumps(payload).encode()
    if body is not None:
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(
        base + path,
        data=body,
        headers=headers,
        method="POST" if body is not None else "GET",
    )
    with urllib.request.urlopen(request, timeout=8) as response:
        raw = response.read()
        return json.loads(raw) if raw else None


class _AuthorityServer:
    """A restartable disposable server on one fixed port and database."""

    def __init__(self, *, root, env, base, port, database, admin):
        self.root = root
        self.env = env
        self.base = base
        self.port = port
        self.database = database
        self.admin = admin
        self.process = None
        self.log_path = root / "server.log"

    def start(self) -> None:
        self.log_path.touch(exist_ok=True)
        log = self.log_path.open("a")
        try:
            self.process = subprocess.Popen(
                [str(SERVER)],
                env={
                    **self.env,
                    "ORBIT_SERVER_DB": str(self.database),
                    "ORBIT_SERVER_BIND": f"127.0.0.1:{self.port}",
                    "ORBIT_SERVER_ADMIN_KEY": self.admin,
                },
                stdout=log,
                stderr=subprocess.STDOUT,
            )
        finally:
            log.close()
        for _ in range(150):
            if self.process.poll() is not None:
                raise AssertionError(
                    "server exited during startup:\n"
                    + self.log_path.read_text(encoding="utf-8", errors="replace")[-4000:]
                )
            try:
                _request(self.base, "/health")
                return
            except Exception:
                time.sleep(0.1)
        raise AssertionError(
            "server readiness timeout:\n"
            + self.log_path.read_text(encoding="utf-8", errors="replace")[-4000:]
        )

    def stop(self) -> None:
        if self.process is None:
            return
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        self.process = None


@pytest.fixture
def connected_workspace(tmp_path):
    if not SERVER.is_file() or not TUI.is_file():
        message = (
            f"orbit-server/orbit-tui binaries are not built "
            f"(server={SERVER.is_file()}, tui={TUI.is_file()})"
        )
        if REQUIRE_EVIDENCE:
            pytest.fail(
                f"{message} (ORBIT_REQUIRE_CONNECTED_TUI_EVIDENCE=1 forbids a skipped run)"
            )
        pytest.skip(message)
    port_socket = socket.socket()
    port_socket.bind(("127.0.0.1", 0))
    port = port_socket.getsockname()[1]
    port_socket.close()
    base = f"http://127.0.0.1:{port}"
    root = tmp_path / "connected"
    root.mkdir()
    server_db = root / "server.db"
    replica = root / "replica.db"
    config = root / "orbit.config.toml"
    home = root / "home"
    home.mkdir()
    admin = secrets.token_hex(32)
    device = "tui-workflow-device"
    token = secrets.token_hex(32)
    env = {
        key: value for key, value in os.environ.items() if not key.startswith("ORBIT_")
    }
    env.update(
        {
            "HOME": str(home),
            "ORBIT_CONFIG": str(config),
            "ORBIT_DB": str(root / "unused-local.db"),
            "ORBIT_AUTHORITY_DB": str(replica),
            "ORBIT_AUTHORITY_SERVER": base,
            "ORBIT_AUTHORITY_DEVICE": device,
            "ORBIT_AUTHORITY_TOKEN": token,
            "ORBIT_SYNC_HTTP_TOKEN": token,
            "TERM": "xterm-256color",
            "LANG": "C.UTF-8",
        }
    )
    authority = _AuthorityServer(
        root=root, env=env, base=base, port=port, database=server_db, admin=admin
    )
    authority.start()
    try:
        _request(
            base,
            "/v1/devices",
            token=admin,
            payload={"device_id": device, "token": token},
        )
        workspace = Workspace(root=root, db=replica, config=config, home=home, env=env)
        yield workspace, authority, base, device, token
    finally:
        authority.stop()


def test_quit_from_connected_pending_inspector_is_direct(connected_workspace):
    workspace, authority, _base, _device, _token = connected_workspace
    session = TuiSession(workspace, cols=120, rows=35, label="m29f-pending")
    try:
        session.wait_for_screen_text(
            "CONNECTED", timeout=20.0, description="connected header"
        )
        open_projects(session)
        session.wait_for_screen_text(
            "0 project(s)", timeout=8.0, description="no accepted projects yet"
        )
        # Queue one intention offline so the inspector has a real row.
        authority.stop()
        session.send(b"c", note="new project while the server is down")
        session.wait_for_screen_text(
            "New project", timeout=8.0, description="project form"
        )
        session.send_text("Pending Scope")
        session.send(ENTER, note="queue the project offline")
        session.wait_for_text(
            "not accepted yet", timeout=CONNECTED_WAIT, description="pending status"
        )
        session.send(b"!", note="open the pending inspector")
        session.wait_for_screen_text(
            "Pending commands", timeout=8.0, description="pending inspector"
        )
        session.wait_for_screen_text(
            "queued (not accepted)",
            timeout=8.0,
            description="the queued command is labelled unaccepted",
        )

        # The inspector is read-only: Ctrl+Q has no unsaved text to protect.
        session.send(CTRL_Q, note="quit from the pending inspector")
        session.assert_clean_exit()
        assert "Quit Orbit?" not in session.text, (
            "a read-only pending inspector must quit without an unsaved-input "
            f"prompt\nEvidence: {session.evidence_dir}"
        )
    finally:
        session.close()


def test_quit_from_connected_console_with_text_confirms_and_preserves(
    connected_workspace,
):
    workspace, _authority, _base, _device, _token = connected_workspace
    session = TuiSession(workspace, cols=120, rows=35, label="m29f-console")
    try:
        session.wait_for_screen_text(
            "CONNECTED", timeout=20.0, description="connected header"
        )
        session.send(b":", note="open the command palette")
        session.wait_for_screen_text(
            "Command palette", timeout=8.0, description="command palette"
        )
        session.send_text("Connected commands")
        session.wait_for_screen_text(
            "Connected commands", timeout=8.0, description="console entry"
        )
        session.send(ENTER, note="open the connected console")
        session.wait_for_screen_text(
            "queued intents are PENDING", timeout=8.0, description="connected console"
        )
        session.send_text("sync.status")
        session.wait_for_screen_text(
            "> sync.status", timeout=8.0, description="console input"
        )
        _assert_confirmation_then_preserved(
            session,
            surface="connected console with text",
            preserved_text="> sync.status",
            still_visible=lambda view: "queued intents are PENDING" in view,
        )
    finally:
        session.close()
