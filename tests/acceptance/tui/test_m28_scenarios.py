"""M28 TUI acceptance scenarios against the real ``orbit-tui`` binary.

Run from the repository root after ``cargo build -p orbit-tui`` (and
``cargo build -p orbit-cli`` for the project-seeding fixture)::

    python3 -m pytest -q tests/acceptance/tui

These tests drive a PTY, not a simulation.  They intentionally describe the
target behaviour from the M28 acceptance brief; any failures caused by missing
Rust fixes are reported as failures rather than relaxed assertions.

Evidence for every session is written under
``target/tui-evidence/<run-id>/<test>-<n>/`` as ``raw.ansi``, ``text.txt`` and
``meta.json``.  Set ``ORBIT_TUI_EVIDENCE_DIR`` to relocate it.  If a test
fails, the assertion message includes the last captured output and the exact
evidence directory.
"""
from __future__ import annotations

import json
import os
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
ROOT = Path(__file__).resolve().parents[3]

from pty_harness import (  # noqa: E402
    CTRL_C,
    CTRL_Q,
    ENTER,
    ESC,
    QUIT_SHORTCUT_RE,
    TuiSession,
    Workspace,
)

QUIT_KEYS = [
    pytest.param(CTRL_Q, id="ctrl-q"),
    pytest.param(CTRL_C, id="ctrl-c"),
]

FOOTER_SCREENS = ["home", "projects", "project", "help", "prompt", "palette"]

HOME_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Home / Today"
PROJECTS_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Projects"


def footer_line(session: TuiSession) -> str:
    for line in reversed(session.screen_lines):
        if line.strip():
            return line
    return ""


@pytest.fixture
def tui_factory(tmp_path):
    """Create isolated TUI sessions and always reap/write evidence on teardown."""
    sessions: list[TuiSession] = []
    counter = {"value": 0}

    def start(
        cols: int = 100,
        rows: int = 30,
        *,
        seed_project: str | None = None,
        label: str | None = None,
    ) -> TuiSession:
        counter["value"] += 1
        workspace = Workspace.create(tmp_path)
        if seed_project is not None:
            workspace.create_project(seed_project)
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
        session.wait_for_text(
            "ORBIT", timeout=20.0, description="first Orbit frame"
        )
        binary = Path(os.environ.get("ORBIT_TUI_BIN", "target/debug/orbit-tui")).resolve()
        try:
            revision = subprocess.run(
                ["git", "rev-parse", "--short", "HEAD"],
                cwd=ROOT,
                text=True,
                capture_output=True,
                check=False,
            ).stdout.strip()
            dirty = bool(
                subprocess.run(
                    ["git", "status", "--porcelain"],
                    cwd=ROOT,
                    text=True,
                    capture_output=True,
                    check=False,
                ).stdout.strip()
            )
        except OSError:
            revision, dirty = "", None
        session.note(
            "workspace",
            {
                "db": str(workspace.db),
                "cols": cols,
                "rows": rows,
                "binary": str(binary),
                "binary_mtime_ns": binary.stat().st_mtime_ns if binary.is_file() else None,
                "git_revision": revision,
                "git_dirty": dirty,
            },
        )
        return session

    yield start

    for session in reversed(sessions):
        try:
            session.close()
        except Exception as error:  # teardown must not hide the test failure
            print(f"orbit-tui harness teardown error: {error}", flush=True)


@pytest.fixture
def tui(tui_factory):
    return tui_factory(100, 30)


def wait_home(session: TuiSession) -> None:
    session.wait_for_screen_regex(
        HOME_HEADER_RE, timeout=10.0, description="Home / Today header"
    )


def open_projects(session: TuiSession) -> int:
    mark = session.mark()
    session.send_keys(b"g", b"p", delay=0.15)
    session.wait_for_screen_regex(
        PROJECTS_HEADER_RE + r"|" + r"\b\d+\s+project\(s\)",
        timeout=8.0,
        description="Projects screen",
    )
    return mark


def open_first_project(session: TuiSession) -> int:
    mark = session.mark()
    session.send(ENTER, note="open first project")
    session.wait_for_screen_text(
        "generated default", timeout=8.0, description="first project workspace"
    )
    return mark


def open_help(session: TuiSession) -> int:
    mark = session.mark()
    session.send(b"?", note="open help")
    session.wait_for_screen_text(
        "Keyboard help", timeout=8.0, description="help modal"
    )
    return mark


def open_text_prompt(session: TuiSession, text: str = "draft") -> int:
    mark = session.mark()
    session.send(b"c", note="open new task prompt")
    session.wait_for_screen_text(
        "New task", timeout=8.0, description="new task prompt"
    )
    session.send_text(text)
    session.wait_for_screen_text(
        text, timeout=8.0, description="typed prompt text"
    )
    return mark


def open_palette(session: TuiSession, query: str = "pro") -> int:
    mark = session.mark()
    session.send(b":", note="open command palette")
    session.wait_for_screen_text(
        "New project", timeout=8.0, description="command palette"
    )
    session.send_text(query)
    session.wait_for_screen_text(
        f"> {query}", timeout=8.0, description="palette query text"
    )
    return mark


def test_launch_100x30_help_escape_closes_modal(tui):
    wait_home(tui)
    assert "ORBIT" in tui.text

    open_help(tui)
    assert "Navigation" in tui.text

    tui.send(ESC, note="close help with escape")
    tui.wait_until_screen(
        lambda view: "Keyboard help" not in view,
        timeout=3.0,
        description="help modal to close after Escape",
    )
    assert tui.alive, "Escape closed the whole application instead of the help modal"

    tui.quit_via(CTRL_Q)
    tui.assert_clean_exit()


def test_gp_enter_opens_first_project_and_escape_goes_back_through_history(
    tui_factory,
):
    session = tui_factory(seed_project="Acceptance Project")
    wait_home(session)

    open_projects(session)
    session.wait_for_screen_text(
        "Acceptance Project", timeout=8.0, description="seeded project row"
    )
    open_first_project(session)
    assert session.alive

    session.send(ESC, note="back from project to Projects")
    session.wait_for_screen_regex(
        PROJECTS_HEADER_RE, timeout=8.0, description="Projects after first Escape"
    )
    assert session.alive

    session.send(ESC, note="back from Projects to Home")
    session.wait_for_screen_regex(
        HOME_HEADER_RE, timeout=8.0, description="Home after second Escape"
    )
    assert session.alive

    session.quit_via(CTRL_Q)
    session.assert_clean_exit()


def test_q_quits_from_projects_screen(tui):
    wait_home(tui)
    open_projects(tui)

    tui.send(b"q", note="q on Projects")
    tui.assert_clean_exit()


def test_q_quits_from_home(tui):
    wait_home(tui)
    tui.send(b"q", note="q on Home")
    tui.assert_clean_exit()


def test_q_quits_from_open_project(tui_factory):
    session = tui_factory(seed_project="Quit Project")
    wait_home(session)
    open_projects(session)
    session.wait_for_text(
        "Quit Project", timeout=8.0, description="seeded project row"
    )
    open_first_project(session)

    session.quit_via(b"q", timeout=8.0)
    session.assert_clean_exit()


@pytest.mark.parametrize("key", QUIT_KEYS)
def test_ctrl_keys_quit_from_home(tui, key):
    wait_home(tui)
    tui.quit_via(key)
    tui.assert_clean_exit()


@pytest.mark.parametrize("key", QUIT_KEYS)
def test_ctrl_keys_quit_from_projects(tui, key):
    wait_home(tui)
    open_projects(tui)
    tui.quit_via(key)
    tui.assert_clean_exit()


@pytest.mark.parametrize("key", QUIT_KEYS)
def test_ctrl_keys_quit_from_open_project(tui_factory, key):
    session = tui_factory(seed_project="Ctrl Quit Project")
    wait_home(session)
    open_projects(session)
    session.wait_for_screen_text(
        "Ctrl Quit Project", timeout=8.0, description="seeded project row"
    )
    open_first_project(session)
    session.quit_via(key)
    session.assert_clean_exit()


@pytest.mark.parametrize("key", QUIT_KEYS)
def test_ctrl_keys_quit_from_help_modal(tui, key):
    wait_home(tui)
    open_help(tui)
    tui.quit_via(key)
    tui.assert_clean_exit()


@pytest.mark.parametrize("key", QUIT_KEYS)
def test_ctrl_keys_quit_from_focused_text_prompt(tui, key):
    wait_home(tui)
    open_text_prompt(tui, "unsaved draft")
    tui.quit_via(key)
    tui.assert_clean_exit()


@pytest.mark.parametrize("key", QUIT_KEYS)
def test_ctrl_keys_quit_from_focused_palette(tui, key):
    wait_home(tui)
    open_palette(tui, "pro")
    tui.quit_via(key)
    tui.assert_clean_exit()


def test_unsaved_text_quit_shows_clear_confirmation_or_quits(tui):
    wait_home(tui)
    open_text_prompt(tui, "unsaved draft")

    mark = tui.mark()
    # Ctrl+Q is the quit shortcut while a text prompt owns printable keys.
    tui.send(CTRL_Q, note="quit request with unsaved prompt text")
    outcome = tui.wait_for_quit_response(timeout=3.0, since=mark)
    assert outcome == "confirmation", (
        "unsaved form text must produce a quit confirmation, not silent exit; "
        f"evidence={tui.evidence_dir}"
    )
    message = tui.screen_text.lower()
    assert "quit" in message, (
        "quit confirmation did not mention Quit:\n"
        f"{tui.screen_text}\nEvidence: {tui.evidence_dir}"
    )
    assert "unsaved" in message, (
        "quit confirmation did not mention unsaved input:\n"
        f"{tui.screen_text}\nEvidence: {tui.evidence_dir}"
    )
    assert ("discard" in message or "confirm" in message or "without" in message), (
        "quit confirmation did not clearly describe discarding text:\n"
        f"{tui.screen_text}\nEvidence: {tui.evidence_dir}"
    )

    tui.send(b"n", note="cancel quit confirmation")
    tui.wait_until_screen(
        lambda view: "quit orbit" not in view.lower(),
        timeout=3.0,
        description="quit confirmation to close after n",
    )
    assert tui.alive, (
        "n did not cancel the quit confirmation; "
        f"evidence={tui.evidence_dir}"
    )
    assert "draft" in tui.screen_text, (
        "cancelling quit did not restore the unsaved form text; "
        f"screen:\n{tui.screen_text}\nEvidence: {tui.evidence_dir}"
    )

    tui.quit_via(CTRL_Q)
    tui.assert_clean_exit()




def test_q_is_typed_inside_a_form_and_does_not_quit(tui):
    wait_home(tui)
    open_text_prompt(tui, "ab")
    tui.send(b"q", note="q inside form text field")
    tui.wait_for_screen_text("abq", timeout=3.0, description="q appended to form title")
    assert tui.alive, "q inside a form text field quit the application"
    tui.quit_via(CTRL_Q)
    tui.assert_clean_exit()

@pytest.mark.parametrize("screen", FOOTER_SCREENS)
def test_footer_shows_quit_shortcut(tui_factory, screen):
    seeded = "Footer Project" if screen == "project" else None
    session = tui_factory(seed_project=seeded)
    wait_home(session)

    if screen == "projects":
        open_projects(session)
    elif screen == "project":
        open_projects(session)
        session.wait_for_text(
            "Footer Project", timeout=8.0, description="seeded project row"
        )
        open_first_project(session)
    elif screen == "help":
        open_help(session)
    elif screen == "prompt":
        open_text_prompt(session)
    elif screen == "palette":
        open_palette(session)

    def footer_has_quit(view: str) -> bool:
        lines = [line for line in view.splitlines() if line.strip()]
        return bool(lines and QUIT_SHORTCUT_RE.search(lines[-1]))

    # The footer is painted late in a frame; wait for a frame where it is
    # actually visible instead of sampling a mid-repaint screen.
    session.wait_until_screen(
        footer_has_quit,
        timeout=4.0,
        description=f"footer quit shortcut on {screen!r}",
    )
    footer = footer_line(session)
    assert QUIT_SHORTCUT_RE.search(footer), (
        f"footer on {screen!r} did not show a stable quit shortcut "
        "(expected e.g. 'q quit' or 'Ctrl+Q quit').\n"
        f"Footer: {footer!r}\nCurrent screen:\n{session.screen_text}\n"
        f"Evidence: {session.evidence_dir}"
    )

    session.quit_via(CTRL_Q)
    session.assert_clean_exit()


@pytest.mark.parametrize("size", [(60, 18), (160, 45)], ids=["60x18", "160x45"])
def test_extreme_terminal_sizes_launch_and_quit(tui_factory, size):
    cols, rows = size
    session = tui_factory(cols, rows)
    wait_home(session)
    assert "ORBIT" in session.text
    assert session.alternate_screen_entered, (
        f"orbit-tui did not enter the alternate screen at {cols}x{rows}; "
        f"evidence={session.evidence_dir}"
    )
    assert "panicked at" not in session.text

    session.quit_via(CTRL_Q)
    session.assert_clean_exit()


def test_project_task_lifecycle_persists_after_restart(tui_factory):
    session = tui_factory(120, 35)
    wait_home(session)
    open_projects(session)

    session.send(b"c", note="new project")
    session.wait_for_screen_text("New project", timeout=8.0, description="project form")
    session.send_text("Lifecycle Project")
    session.send(ENTER, note="submit project")
    session.wait_for_screen_regex(
        r"ORBIT › Lifecycle Project", timeout=10.0, description="new project opened"
    )

    session.send(b"c", note="new task")
    session.wait_for_screen_text("New task", timeout=8.0, description="task form")
    session.send_text("First task")
    session.send(ENTER, note="submit task")
    session.wait_until_screen(
        lambda view: "New task" not in view,
        timeout=10.0,
        description="task form to close",
    )
    session.wait_for_screen_text("1 tasks", timeout=10.0, description="project refresh")
    session.send(b"]", note="switch to Work view")
    session.wait_for_screen_text("Task board", timeout=8.0, description="Work view")
    session.wait_for_screen_text("First task", timeout=8.0, description="task card")
    session.send(b"e", note="edit selected task")
    session.wait_for_screen_text("Edit task", timeout=8.0, description="edit task form")
    for _ in range(len("First task")):
        session.send(b"\x7f", note="erase old title")
    session.send_text("Edited task")
    session.send(ENTER, note="submit task edit")
    session.wait_until_screen(
        lambda view: "Edit task" not in view,
        timeout=10.0,
        description="edit form to close",
    )
    session.wait_for_screen_text("Edited task", timeout=10.0, description="renamed task")

    session.send(b":", note="open palette")
    session.wait_for_screen_text("New project", timeout=8.0, description="palette")
    session.send_text("Move to ready")
    session.wait_for_screen_text("Move to ready", timeout=8.0, description="status action")
    session.send(ENTER, note="set task ready")
    session.wait_for_text("ready", timeout=10.0, description="ready status accepted")

    session.quit_via(CTRL_Q)
    session.assert_clean_exit()
    _, persisted_title, persisted_status = _task_row(session.workspace, "Edited task")
    assert persisted_title == "Edited task"
    assert persisted_status == "ready", persisted_status

    restarted = TuiSession(
        session.workspace,
        cols=120,
        rows=35,
        label="lifecycle-restart",
    )
    try:
        restarted.wait_for_text("ORBIT", timeout=10.0)
        open_projects(restarted)
        restarted.wait_for_screen_text(
            "Lifecycle Project", timeout=8.0, description="persisted project"
        )
        restarted.send(ENTER, note="open persisted project")
        restarted.wait_for_screen_regex(
            r"ORBIT › Lifecycle Project", timeout=10.0, description="persisted project opens"
        )
        restarted.send(b"]", note="switch to Work view after restart")
        restarted.wait_for_screen_text("Task board", timeout=8.0, description="persisted Work view")
        restarted.wait_for_screen_text(
            "Edited task", timeout=10.0, description="persisted task title"
        )
        restarted.quit_via(CTRL_Q)
        restarted.assert_clean_exit()
    finally:
        restarted.close()



def _task_row(workspace, title):
    connection = sqlite3.connect(workspace.db)
    try:
        row = connection.execute(
            "SELECT id,title,status FROM tasks WHERE title=?1", (title,)
        ).fetchone()
    finally:
        connection.close()
    assert row is not None, f"task {title!r} not persisted"
    return row


def _last_blueprint_item(workspace):
    connection = sqlite3.connect(workspace.db)
    try:
        row = connection.execute(
            "SELECT id,state,properties_json FROM orbit_blueprint_work_items "
            "ORDER BY rowid DESC LIMIT 1"
        ).fetchone()
    finally:
        connection.close()
    assert row is not None, "no blueprint work item persisted"
    return row[0], row[1], row[2]


def test_blueprint_bind_work_item_edit_and_transition_persist(tui_factory):
    session = tui_factory(120, 35)
    wait_home(session)
    open_projects(session)

    session.send(b"c", note="new project")
    session.wait_for_screen_text("New project", timeout=8.0, description="project form")
    session.send_text("Blueprint Project")
    session.send(ENTER, note="submit project")
    session.wait_for_screen_regex(
        r"ORBIT › Blueprint Project", timeout=10.0, description="project opened"
    )

    session.send(b":", note="palette install blueprint")
    session.wait_for_screen_text("New project", timeout=8.0, description="palette")
    session.send_text("Install bundled blueprint")
    session.wait_for_screen_text("Install bundled blueprint", timeout=8.0, description="install action")
    session.send(ENTER, note="open install form")
    session.wait_for_screen_text("Install bundled blueprint", timeout=8.0, description="install form")
    session.send(ENTER, note="install default bundled blueprint")
    session.wait_until_screen(
        lambda view: "Install bundled blueprint" not in view,
        timeout=10.0,
        description="install form closes",
    )

    session.send(b":", note="palette bind blueprint")
    session.wait_for_screen_text("New project", timeout=8.0, description="palette")
    session.send_text("Bind blueprint version")
    session.wait_for_screen_text("Bind blueprint version", timeout=8.0, description="bind action")
    session.send(ENTER, note="open bind form")
    session.wait_for_screen_text("Bind blueprint version", timeout=8.0, description="bind form")
    session.send(ENTER, note="bind default installed version")
    session.wait_until_screen(
        lambda view: "Bind blueprint version" not in view,
        timeout=10.0,
        description="bind form closes",
    )
    session.wait_for_screen_text("development@2", timeout=10.0, description="pinned blueprint header")

    session.send(b"C", note="new work item")
    session.wait_for_screen_text("New work item", timeout=8.0, description="kind form")
    session.send(ENTER, note="choose default kind")
    session.wait_for_screen_text("New feature", timeout=8.0, description="property form")
    session.send(ENTER, note="submit default properties")
    session.wait_until_screen(
        lambda view: "New feature" not in view,
        timeout=10.0,
        description="work item form closes",
    )

    session.send(b"]", note="switch to Board")
    session.wait_for_screen_text("Task board", timeout=8.0, description="Board view")
    session.wait_for_screen_text("feature", timeout=8.0, description="work-item board")
    session.send(b"l", note="focus work-item board")
    session.wait_for_screen_text("feature", timeout=5.0, description="work-item card")
    session.send(b"e", note="edit work item")
    session.wait_for_screen_text("Edit work item", timeout=8.0, description="work-item edit form")
    session.send(b"\x1b[C", note="select next priority")
    session.send(ENTER, note="submit work-item edit")
    session.wait_until_screen(
        lambda view: "Edit work item" not in view,
        timeout=10.0,
        description="work-item edit form closes",
    )

    session.send(b":", note="palette transition")
    session.wait_for_screen_text("New project", timeout=8.0, description="palette")
    session.send_text("Move to ready")
    session.wait_for_screen_text("Move to ready", timeout=8.0, description="transition action")
    session.send(ENTER, note="transition work item")
    session.wait_for_text("ready", timeout=10.0, description="transition accepted")

    session.quit_via(CTRL_Q)
    session.assert_clean_exit()

    item_id, state, properties = _last_blueprint_item(session.workspace)
    assert state == "ready", (item_id, state, properties)
    assert '"priority":"high"' in properties.replace(" ", ""), properties

    restarted = TuiSession(
        session.workspace,
        cols=120,
        rows=35,
        label="blueprint-restart",
    )
    try:
        restarted.wait_for_text("ORBIT", timeout=10.0)
        open_projects(restarted)
        restarted.wait_for_screen_text(
            "Blueprint Project", timeout=8.0, description="persisted blueprint project"
        )
        restarted.send(ENTER, note="open persisted project")
        restarted.wait_for_screen_regex(
            r"ORBIT › Blueprint Project", timeout=10.0, description="persisted project opens"
        )
        restarted.wait_for_screen_text("development@2", timeout=8.0, description="pinned blueprint")
        restarted.send(b"]", note="Board after restart")
        restarted.wait_for_screen_text("Task board", timeout=8.0, description="Board after restart")
        restarted.send(b"l", note="focus persisted work-item board")
        restarted.wait_for_screen_text("feature", timeout=8.0, description="persisted work item")
        restarted.quit_via(CTRL_Q)
        restarted.assert_clean_exit()
    finally:
        restarted.close()


def _project_row(workspace, name):
    connection = sqlite3.connect(workspace.db)
    try:
        return connection.execute(
            "SELECT id,objective FROM projects WHERE name=?1", (name,)
        ).fetchone()
    finally:
        connection.close()


def test_project_form_objective_and_validation_do_not_create_partial_rows(tui_factory):
    session = tui_factory(120, 35)
    wait_home(session)
    open_projects(session)

    session.send(b"c", note="new project")
    session.wait_for_screen_text("New project", timeout=8.0, description="project form")
    session.send_text("Objective Project")
    session.send(b"\t", note="focus objective")
    session.send_text("Ship the objective")
    session.send(ENTER, note="submit project with objective")
    session.wait_for_screen_regex(
        r"ORBIT › Objective Project", timeout=10.0, description="project with objective opens"
    )
    row = _project_row(session.workspace, "Objective Project")
    assert row is not None and row[1] == "Ship the objective", row

    session.send(b"c", note="invalid task form")
    session.wait_for_screen_text("New task", timeout=8.0, description="task form")
    session.send_text("Should not exist")
    for _ in range(4):
        session.send(b"\t", note="focus next field")
    session.send_text("0")
    session.send(ENTER, note="submit invalid estimate")
    session.wait_for_screen_text(
        "between 1 and 525600", timeout=5.0, description="invalid estimate error"
    )
    connection = sqlite3.connect(session.workspace.db)
    try:
        count = connection.execute(
            "SELECT COUNT(*) FROM tasks WHERE title='Should not exist'"
        ).fetchone()[0]
    finally:
        connection.close()
    assert count == 0, "invalid estimate created a task before validation"

    session.send(ESC, note="cancel invalid form")
    session.quit_via(CTRL_Q)
    session.assert_clean_exit()


def test_project_settings_preserve_satisfied_criterion(tui_factory):
    session = tui_factory(120, 35)
    wait_home(session)
    session.workspace.create_project("Settings Project", cli_binary=os.environ.get("ORBIT_CLI_BIN"))
    project_id = _project_row(session.workspace, "Settings Project")[0]
    env = dict(os.environ, ORBIT_DB=str(session.workspace.db))
    configure = subprocess.run(
        [
            str(Path(os.environ.get("ORBIT_CLI_BIN", "target/debug/orbit"))),
            "--json",
            "project",
            "configure",
            project_id,
            "--mode",
            "standard",
            "--criterion",
            "ship=Ship",
        ],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
    )
    assert configure.returncode == 0, configure.stderr
    satisfied = subprocess.run(
        [
            str(Path(os.environ.get("ORBIT_CLI_BIN", "target/debug/orbit"))),
            "--json",
            "project",
            "criterion",
            project_id,
            "ship",
            "satisfied",
        ],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
    )
    assert satisfied.returncode == 0, satisfied.stderr

    open_projects(session)
    session.send(b"r", note="refresh seeded project")
    session.wait_for_screen_text("Settings Project", timeout=8.0, description="seeded settings project")
    session.send(ENTER, note="open settings project")
    session.wait_for_screen_regex(r"ORBIT › Settings Project", timeout=10.0, description="settings project opens")
    session.send(b":", note="palette project settings")
    session.wait_for_screen_text("New project", timeout=8.0, description="palette")
    session.send_text("Project settings")
    session.wait_for_screen_text("Project settings", timeout=8.0, description="settings action")
    session.send(ENTER, note="open project settings form")
    session.wait_for_screen_text("Project settings", timeout=8.0, description="settings form")
    session.send(b"\t", note="focus mode")
    session.send(b"\t", note="focus objective")
    session.send_text("Updated objective")
    session.send(ENTER, note="save settings")
    session.wait_until_screen(
        lambda view: "Project settings" not in view,
        timeout=10.0,
        description="settings form closes",
    )
    session.quit_via(CTRL_Q)
    session.assert_clean_exit()

    connection = sqlite3.connect(session.workspace.db)
    try:
        status = connection.execute(
            "SELECT status FROM project_success_criteria WHERE project_id=?1 AND criterion_key='ship'",
            (project_id,),
        ).fetchone()[0]
        objective = connection.execute(
            "SELECT objective FROM projects WHERE id=?1", (project_id,)
        ).fetchone()[0]
    finally:
        connection.close()
    assert status == "satisfied", status
    assert objective == "Updated objective", objective


@pytest.mark.parametrize(
    "cols,rows", [(60, 18), (80, 24), (100, 30), (120, 35), (160, 45)]
)
def test_responsive_layout_navigation_detail_and_exit(tui_factory, cols, rows):
    session = tui_factory(cols, rows, seed_project="A Very Long Project Name For Layout")
    project_id = _project_row(
        session.workspace, "A Very Long Project Name For Layout"
    )[0]
    cli = str(Path(os.environ.get("ORBIT_CLI_BIN", "target/debug/orbit")))
    for title, statuses in [
        ("Alpha task with a long title", ["ready"]),
        ("Beta task with a long title", ["ready"]),
        ("Gamma task with a long title", ["ready", "in-progress", "done"]),
    ]:
        added = subprocess.run(
            [cli, "--json", "task", "add", title, "--project", project_id],
            cwd=ROOT,
            env=dict(session.workspace.env),
            text=True,
            capture_output=True,
        )
        assert added.returncode == 0, added.stderr
        task_id = json.loads(added.stdout)["task"]["id"]
        for status in statuses:
            result = subprocess.run(
                [cli, "--json", "task", "status", task_id, status],
                cwd=ROOT,
                env=dict(session.workspace.env),
                text=True,
                capture_output=True,
            )
            assert result.returncode == 0, result.stderr
    wait_home(session)
    session.wait_for_screen_text("Ctrl+Q quit", timeout=5.0, description="quit footer")
    assert QUIT_SHORTCUT_RE.search(session.screen_text), session.screen_text

    open_projects(session)
    session.wait_for_screen_text(
        "A Very Long", timeout=8.0, description="long project row"
    )
    session.send(ENTER, note="open long project")
    session.wait_for_screen_regex(
        r"ORBIT › A Very Long", timeout=10.0, description="long project header"
    )
    assert QUIT_SHORTCUT_RE.search(session.screen_text), session.screen_text

    session.send(b"r", note="refresh seeded tasks")
    session.send(b"]", note="switch view")
    session.wait_for_screen_text("Task board", timeout=8.0, description="board view")
    session.wait_until_screen(
        lambda view: any(
            "▸" in line and "Alpha task" in line for line in view.splitlines()
        ),
        timeout=8.0,
        description="selected task card",
    )
    session.send(b"j", note="next task card")
    session.send(b"l", note="focus next component")
    session.send(b"?", note="contextual help")
    session.wait_for_screen_text("Keyboard help", timeout=8.0, description="help modal")
    session.send(ESC, note="close help")
    session.wait_until_screen(
        lambda view: "Keyboard help" not in view,
        timeout=5.0,
        description="help closes",
    )
    session.quit_via(CTRL_Q)
    session.assert_clean_exit()
