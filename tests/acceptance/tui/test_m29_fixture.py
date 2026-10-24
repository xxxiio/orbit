"""M29 fixture smoke test: launch the real TUI against the shared fixture.

The default run builds a small deterministic workspace (5 projects / 40 tasks)
through the same helpers the performance probe uses.  The full 20-project /
200-task fixture is opt-in with ``ORBIT_TUI_FULL_FIXTURE=1`` (or
``scripts/tui-fixture.py --full``) and is what the probe measures by default.

Every session is reaped and writes the usual PTY evidence under
``target/tui-evidence/<run-id>/<label>/``.
"""
from __future__ import annotations

import importlib.util
import json
import os
import re
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[3]
TUI_DIR = ROOT / "tests" / "acceptance" / "tui"
FIXTURE_PATH = ROOT / "scripts" / "tui-fixture.py"

sys.path.insert(0, str(TUI_DIR))

from pty_harness import ENTER, ESC, TuiSession  # noqa: E402

HOME_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Home / Today"
PROJECTS_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Projects"
SEARCH_MODAL_RE = re.compile(r"[╭╔]\s*Search\b")
VIEW_SWITCH_HINT = "[ ] switch view"
SIDEBAR_WIDTH = 22
DENSE_PROJECT = "Performance Lab"
SEARCH_TARGET = "Research: Retrieval Quality"


def _load_fixture():
    spec = importlib.util.spec_from_file_location("tui_fixture", FIXTURE_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load fixture helper at {FIXTURE_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


FIXTURE = _load_fixture()


def _build(tmp_path: Path, *, project_count: int, task_count: int):
    workspace = FIXTURE.build_fixture(
        tmp_path / "workspace",
        project_count=project_count,
        task_count=task_count,
        seed=20261024,
        activity_target=max(60, task_count),
        attachment_count=1,
        agent_job_count=2,
        verbose=False,
    )
    return FIXTURE.workspace_from_path(workspace["workspace"])


def _current_view_title(screen_text: str) -> str | None:
    for line in screen_text.splitlines():
        if VIEW_SWITCH_HINT not in line:
            continue
        match = re.search(r"▸\s*([^│\[]+)", line)
        if match:
            return match.group(1).strip()
    return None


def _selected_task_row(screen_text: str) -> str | None:
    """The task-table row carrying the cursor (ignore sidebar and view tabs)."""
    for line in screen_text.splitlines():
        segment = line[SIDEBAR_WIDTH:]
        # Table rows start with the workspace border; the view-tab bar does
        # not, and it also happens to contain a ``▸ <active view>`` marker.
        if segment[:3].find("┃") >= 0 and "▸" in segment:
            return segment.strip()
    return None


def _open_search(session: TuiSession) -> None:
    session.send(b"/", note="open search")
    session.wait_for_screen_regex(
        SEARCH_MODAL_RE, timeout=10.0, description="search modal"
    )


def _close_search(session: TuiSession) -> None:
    session.send(ESC, note="close search")
    session.wait_until_screen(
        lambda view: SEARCH_MODAL_RE.search(view) is None,
        timeout=5.0,
        description="search modal to close",
    )


def _open_project(session: TuiSession, name: str) -> None:
    _open_search(session)
    session.send_text(name, delay=0.006)
    session.wait_for_screen_text(name, timeout=10.0, description=f"hit {name!r}")
    session.send(ENTER, note=f"open project {name}")
    session.wait_for_screen_text(
        "switch view", timeout=15.0, description=f"project {name!r}"
    )
    session.wait_for_screen_text(
        name, timeout=5.0, description=f"project header {name!r}"
    )


def _run_smoke(workspace) -> None:
    session = TuiSession(
        workspace,
        cols=100,
        rows=30,
        wait_for_launch=False,
        label="m29-fixture-smoke",
        launch_timeout=20.0,
    )
    try:
        session.wait_for_text("ORBIT", timeout=20.0, description="first Orbit frame")
        session.wait_for_screen_regex(
            HOME_HEADER_RE, timeout=10.0, description="Home / Today header"
        )

        session.send_keys(b"g", b"p", delay=0.1)
        session.wait_for_screen_regex(
            PROJECTS_HEADER_RE, timeout=10.0, description="Projects screen"
        )
        assert "project" in session.screen_text.lower()

        _open_project(session, DENSE_PROJECT)
        overview = _current_view_title(session.screen_text)
        assert overview is not None, session.screen_text

        # Basic view 2 is the dense Tasks table.  Wait for the body, not just
        # the tab label, so a half-painted frame cannot satisfy the assertion.
        session.send(b"2", note="Tasks view")
        session.wait_until_screen(
            lambda view: _current_view_title(view) == "Tasks" and "DEADLINE" in view,
            timeout=10.0,
            description="Tasks table painted",
        )
        assert _current_view_title(session.screen_text) != overview

        # Search from the project workspace and close the modal with Escape.
        _open_search(session)
        session.send_text("Retrieval", delay=0.006)
        session.wait_for_screen_text(
            SEARCH_TARGET, timeout=10.0, description="search result"
        )
        _close_search(session)

        # Scroll: one real key press must move the visible task cursor.
        before = _selected_task_row(session.screen_text)
        assert before is not None, session.screen_text
        session.send(b"j", note="scroll task table")
        session.wait_until_screen(
            lambda view: _selected_task_row(view) not in (None, before),
            timeout=5.0,
            description="task cursor to move",
        )
        assert _selected_task_row(session.screen_text) != before

        session.quit_via(b"q")
        code = session.assert_clean_exit()
        assert code == 0
    finally:
        session.close()

    evidence = session.evidence_dir
    for name in ("raw.ansi", "text.txt", "meta.json"):
        assert (evidence / name).is_file(), f"missing PTY evidence {evidence / name}"
    meta = json.loads((evidence / "meta.json").read_text(encoding="utf-8"))
    assert meta["returncode"] == 0, meta
    assert meta["alternate_screen_entered"] and meta["alternate_screen_left"], meta


def test_small_fixture_home_projects_views_search_scroll_quit(tmp_path):
    workspace = _build(tmp_path, project_count=5, task_count=40)
    _run_smoke(workspace)


@pytest.mark.skipif(
    os.environ.get("ORBIT_TUI_FULL_FIXTURE") != "1",
    reason="set ORBIT_TUI_FULL_FIXTURE=1 to exercise the full 20/200 fixture",
)
def test_full_fixture_launch_is_opt_in(tmp_path):
    workspace = _build(tmp_path, project_count=20, task_count=200)
    _run_smoke(workspace)
