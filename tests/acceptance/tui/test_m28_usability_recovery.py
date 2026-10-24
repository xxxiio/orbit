"""M28 usability recovery: task details, acceptance criteria, attachments,
session restoration and first-run/narrow-terminal behaviour, driven through a
real PTY.

Build first::

    cargo build -p orbit-tui -p orbit-cli
    python3 -m pytest -q tests/acceptance/tui
"""
from __future__ import annotations

import os
import sqlite3
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))

from pty_harness import CTRL_Q, ENTER, ESC, TuiSession, Workspace  # noqa: E402

CTRL_S = b"\x13"
HOME_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Home / Today"


@pytest.fixture
def sessions(tmp_path):
    created: list[TuiSession] = []

    def start(cols: int = 120, rows: int = 35, workspace: Workspace | None = None):
        current = Workspace.create(tmp_path) if workspace is None else workspace
        session = TuiSession(
            current, cols=cols, rows=rows, label="usability", wait_for_launch=False
        )
        created.append(session)
        session.wait_for_text("ORBIT", timeout=20.0, description="first frame")
        return session

    yield start
    for session in reversed(created):
        try:
            session.close()
        except Exception as error:  # teardown must not mask failures
            print(f"teardown error: {error}", flush=True)


def _project_id(db: Path, name: str) -> str:
    connection = sqlite3.connect(db)
    try:
        row = connection.execute(
            "SELECT id FROM projects WHERE name=?1", (name,)
        ).fetchone()
    finally:
        connection.close()
    assert row is not None, f"project {name!r} not persisted"
    return row[0]


def _task_row(db: Path, title: str):
    connection = sqlite3.connect(db)
    try:
        return connection.execute(
            "SELECT id, title, status, description, priority FROM tasks WHERE title=?1",
            (title,),
        ).fetchone()
    finally:
        connection.close()


def _attachment_rows(db: Path, task_id: str):
    connection = sqlite3.connect(db)
    try:
        return connection.execute(
            "SELECT id, filename, content_type, size_bytes, checksum, object_hash "
            "FROM attachments WHERE owner_kind='task' AND owner_id=?1",
            (task_id,),
        ).fetchall()
    finally:
        connection.close()


def test_basic_task_details_criteria_attachment_and_restart(sessions, tmp_path):
    session = sessions(120, 35)
    session.wait_for_screen_regex(HOME_HEADER_RE, timeout=10.0, description="home")

    # Create a Basic project entirely through the TUI (Projects screen).
    session.send_keys(b"g", b"p", delay=0.15)
    session.wait_for_screen_text("Projects", timeout=8.0, description="projects screen")
    session.send(b"c", note="new project")
    session.wait_for_screen_text("New project", timeout=8.0, description="project form")
    session.send_text("Tui Only")
    session.send(ENTER, note="create project")
    session.wait_for_screen_regex(
        r"ORBIT › Tui Only", timeout=10.0, description="project opened"
    )
    session.wait_for_screen_text("Basic", timeout=8.0, description="Basic workflow label")

    # Create a task.
    session.send(b"c", note="new task")
    session.wait_for_screen_text("Create a task", timeout=8.0, description="task form")
    session.send_text("Tui Task")
    session.send(ENTER, note="create task")
    session.wait_for_screen_text("Tui Task", timeout=10.0, description="task visible")

    # Open the selected task on the Basic board and edit rich details.
    session.send(b"]", note="Tasks view")
    session.send(b"]", note="Board view")
    session.wait_for_screen_text("Task board", timeout=8.0, description="board")
    session.wait_for_screen_text("Tui Task", timeout=8.0, description="task card")
    session.send(b"e", note="edit task")
    session.wait_for_screen_text("Edit task", timeout=8.0, description="edit form")

    # Everyday field order: title, priority, description, labels, then project
    # and planning. Set priority first, then write two description lines.
    session.send(b"\t", note="priority field")
    session.send(b"\x1b[C", note="select low priority")
    session.send(b"\t", note="description field")
    session.send_text("Line one")
    session.send(ENTER, note="newline in description")
    session.send_text("Line two")
    session.send(b"\t", note="labels field")
    session.send_text("smoke")
    session.send(CTRL_S, note="save task details")
    session.wait_for_screen_text("Task updated", timeout=10.0, description="save ack")

    project_id = _project_id(session.workspace.db, "Tui Only")
    assert project_id
    task = _task_row(session.workspace.db, "Tui Task")
    assert task is not None, "task not persisted"
    task_id, _, _, description, priority = task
    assert "Line one" in description and "Line two" in description, description
    assert priority == "low", priority

    # Acceptance criteria: add, then check without completing the task.
    session.send(b"K", note="acceptance criteria")
    session.wait_for_screen_text("Acceptance criteria", timeout=8.0, description="criteria modal")
    session.send(b"c", note="add criterion")
    session.wait_for_screen_text("Add acceptance criterion", timeout=8.0, description="criterion form")
    session.send_text("Works offline")
    session.send(ENTER, note="save criterion")
    session.wait_for_screen_text("Works offline", timeout=8.0, description="criterion row")
    session.send(b" ", note="check criterion")
    session.wait_for_screen_text("[x] Works offline", timeout=8.0, description="checked criterion")
    session.send(ESC, note="close criteria")

    connection = sqlite3.connect(session.workspace.db)
    try:
        criteria = connection.execute(
            "SELECT completed_at_ms FROM task_acceptance_criteria WHERE task_id=?1",
            (task_id,),
        ).fetchall()
        status = connection.execute(
            "SELECT status FROM tasks WHERE id=?1", (task_id,)
        ).fetchone()[0]
    finally:
        connection.close()
    assert criteria and criteria[0][0] is not None, "criterion completion not persisted"
    assert status == "inbox", "checking all criteria must not complete the task"

    # Attach a file through the explicit path-entry workflow.
    source = tmp_path / "evidence.txt"
    source.write_text("attached evidence", encoding="utf-8")
    session.send(b"i", note="attach file")
    session.wait_for_screen_text("Attach file", timeout=8.0, description="attach form")
    session.send_text(str(source))
    session.send(ENTER, note="submit attachment")
    session.wait_for_screen_text("Attachments", timeout=10.0, description="attachments modal")
    session.wait_for_screen_text("evidence.txt", timeout=8.0, description="attachment row")
    session.send(ESC, note="close attachments")

    attachments = _attachment_rows(session.workspace.db, task_id)
    assert len(attachments) == 1, attachments
    attachment_id, filename, content_type, size, checksum, object_hash = attachments[0]
    assert filename == "evidence.txt"
    assert content_type == "text/plain"
    assert size == len("attached evidence")
    assert checksum == object_hash
    object_path = (
        session.workspace.db.parent
        / "attachments"
        / "objects"
        / object_hash[:2]
        / object_hash
    )
    assert object_path.is_file(), object_path
    assert object_path.read_text(encoding="utf-8") == "attached evidence"

    session.quit_via(CTRL_Q)
    session.assert_clean_exit()

    # Restart: navigation intent and all edited data survive.
    restarted = sessions(120, 35, workspace=session.workspace)
    restarted.wait_for_screen_text("Tui Only", timeout=10.0, description="restored project")
    restarted.wait_for_screen_text("Basic", timeout=8.0, description="restored workflow")
    assert not restarted._returncode, restarted.screen_text  # still running

    # Jump directly to the Board view (1-based view number).
    restarted.send(b"2", note="Board after restart")
    restarted.wait_for_screen_text("Task board", timeout=8.0, description="Board after restart")
    restarted.wait_for_screen_text("Tui Task", timeout=8.0, description="persisted task card")
    restarted.send(b"K", note="criteria after restart")
    restarted.wait_for_screen_text("[x] Works offline", timeout=8.0, description="persisted criterion")
    restarted.send(ESC, note="close criteria")
    restarted.quit_via(CTRL_Q)
    restarted.assert_clean_exit()

    # The original fixture session is already closed; avoid double-close noise.
    assert attachment_id and os.path.sep


def test_narrow_terminal_keeps_actions_visible_and_preserves_unsaved_form(sessions):
    session = sessions(60, 18)
    session.wait_for_screen_regex(HOME_HEADER_RE, timeout=10.0, description="home")
    session.wait_for_screen_text("Actions:", timeout=8.0, description="action bar")
    assert "Ctrl+Q quit" in session.screen_text

    session.send_keys(b"g", b"p", delay=0.15)
    session.wait_for_screen_text("Projects", timeout=8.0, description="projects screen")
    session.send(b"c", note="new project on narrow terminal")
    session.wait_for_screen_text("New project", timeout=8.0, description="project form")
    session.send_text("Narrow")
    session.send(ESC, note="escape a dirty form")
    session.wait_for_screen_text(
        "Discard unsaved changes?", timeout=8.0, description="discard confirmation"
    )
    session.send(b"n", note="keep editing")
    session.wait_for_screen_text("Narrow", timeout=8.0, description="form restored")
    assert "Discard unsaved changes?" not in session.screen_text
    session.send(ESC, note="escape again")
    session.send(ENTER, note="discard changes")
    session.wait_until_screen(
        lambda view: "Discard unsaved changes?" not in view,
        timeout=8.0,
        description="form discarded",
    )
    assert session.alive
    session.quit_via(CTRL_Q)
    session.assert_clean_exit()
