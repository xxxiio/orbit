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

import os
import subprocess
import sys
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
