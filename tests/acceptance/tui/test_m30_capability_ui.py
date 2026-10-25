"""M30 workstream C acceptance: trusted capability actions in the real TUI.

The fixture is created by the real ``orbit`` binary against an isolated local
workspace (project, capability install, manifest publish, binding, work item).
The TUI is then driven through the shared PTY harness:

* before authorization the capability action appears in the action menu with
  its honest ``not_authorized`` reason (it is never hidden);
* after authorization the same action is runnable;
* the schema form, the pending-request confirmation, the trusted execution and
  the ``succeeded`` result are driven end to end;
* the inspector shows the durable ``succeeded`` history and the CLI confirms
  exactly one durable note was appended.

Run from the repository root after::

    cargo build -p orbit-tui -p orbit-cli --offline
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
ROOT = Path(__file__).resolve().parents[3]

from pty_harness import ENTER, ESC, TuiSession, Workspace  # noqa: E402

CAPABILITY = "orbit.reference.work-item-note"
MANIFEST = ROOT / "blueprints" / "examples" / "capability-reference.json"
ACTION_LABEL = "Action Append a durable note"
ITEM_TITLE = "note-task item-1"


def _cli_binary() -> Path:
    return Path(
        os.environ.get("ORBIT_CLI_BIN", str(ROOT / "target" / "debug" / "orbit"))
    ).expanduser()


def _cli(workspace: Workspace, *args: str, expect: str = "success") -> dict:
    completed = subprocess.run(
        [str(_cli_binary()), "--db", str(workspace.db), "--json", *args],
        env=dict(workspace.env),
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=120,
    )
    if expect == "success" and completed.returncode != 0:
        raise AssertionError(
            f"CLI failed: {args!r}\nstdout: {completed.stdout}\nstderr: {completed.stderr}"
        )
    if expect == "failure" and completed.returncode == 0:
        raise AssertionError(f"CLI unexpectedly succeeded: {args!r}\n{completed.stdout}")
    output = completed.stdout.strip()
    return json.loads(output) if output else {}


def _result(workspace: Workspace, *args: str) -> object:
    return _cli(workspace, *args).get("result")


def seed_workspace(workspace: Workspace) -> str:
    """Create the project + installed/published/bound capability + item."""
    project = _cli(workspace, "project", "add", "Capability UI")["project"]["id"]
    _result(workspace, "blueprint", "capability", "install", CAPABILITY)
    _result(workspace, "blueprint", "import", str(MANIFEST))
    _result(
        workspace,
        "blueprint",
        "apply",
        project,
        "capability-reference",
        "--version",
        "1",
    )
    _result(workspace, "blueprint", "create-item", project, "item-1", "note-task")
    return project


def authorize(workspace: Workspace, project: str) -> None:
    _result(
        workspace,
        "blueprint",
        "capability",
        "authorize",
        CAPABILITY,
        "--project",
        project,
        "--confirm",
    )


@pytest.fixture
def session_factory():
    sessions: list[TuiSession] = []
    counter = {"value": 0}

    def start(workspace: Workspace, *, label: str, cols: int = 120, rows: int = 35) -> TuiSession:
        counter["value"] += 1
        session = TuiSession(
            workspace,
            cols=cols,
            rows=rows,
            label=f"{label}-{counter['value']}",
            wait_for_launch=False,
        )
        sessions.append(session)
        session.wait_for_text("ORBIT", timeout=20.0, description="first Orbit frame")
        return session

    yield start

    for session in reversed(sessions):
        try:
            session.close()
        except Exception as error:  # teardown must not hide the real failure
            print(f"orbit-tui harness teardown error: {error}", flush=True)


def open_project(session: TuiSession, name: str) -> None:
    session.send_keys(b"g", b"p", delay=0.15)
    session.wait_for_screen_text("Projects", timeout=10.0, description="Projects screen")
    session.wait_for_screen_text(name, timeout=10.0, description="seeded project row")
    session.send(ENTER, note="open the project workspace")
    session.wait_for_screen_text(name, timeout=10.0, description="project workspace")
    # Overview is the only view the reference manifest declares.
    session.wait_for_screen_text(ITEM_TITLE, timeout=10.0, description="work item row")


def select_work_item(session: TuiSession) -> None:
    """Put the list cursor on the work item, moving across components if needed."""
    for _ in range(6):
        if ITEM_TITLE in session.screen_text:
            return
        session.send(b"l", note="move to the next component")
        try:
            session.wait_for_screen_text(ITEM_TITLE, timeout=0.5)
        except AssertionError:
            continue
    if ITEM_TITLE in session.screen_text:
        return
    raise AssertionError(
        "could not select the work item row\n"
        f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
    )


def open_actions(session: TuiSession) -> str:
    session.send(b"a", note="open the contextual action menu")
    return session.wait_for_screen_text("Actions ·", timeout=10.0, description="action menu")


def _pump_until(session: TuiSession, predicate, *, attempts: int = 40, description: str) -> str:
    """Send Down until ``predicate`` holds on a freshly-read screen."""
    for _ in range(attempts):
        try:
            return session.wait_until_screen(predicate, timeout=0.35, description=description)
        except AssertionError:
            session.send(b"j", note="move to the next action")
    raise AssertionError(
        f"{description} never became true\nscreen:\n{session.screen_text}\n"
        f"Evidence: {session.evidence_dir}"
    )


def select_capability_action(session: TuiSession, label: str) -> str:
    session.send(b"a", note="open the contextual action menu")
    session.wait_for_screen_text("Actions ·", timeout=10.0, description="action menu")
    return _pump_until(
        session,
        lambda view: any(label in line and "▸" in line for line in view.splitlines()),
        description=f"capability action {label!r} selected",
    )


def reveal_action(session: TuiSession, label: str) -> str:
    return _pump_until(
        session,
        lambda view: label in view,
        description=f"capability action {label!r} painted",
    )


def test_unauthorized_capability_action_is_listed_with_reason(
    session_factory, tmp_path
):
    workspace = Workspace.create(tmp_path)
    seed_workspace(workspace)
    offers = _result(workspace, "blueprint", "action", "offers", "item-1")
    assert offers[0]["availability"]["state"] == "unavailable"
    assert offers[0]["availability"]["code"] == "not_authorized"

    session = session_factory(workspace, label="m30-cap-unauthorized")
    open_project(session, "Capability UI")
    select_work_item(session)
    open_actions(session)
    screen = reveal_action(session, ACTION_LABEL)
    assert ACTION_LABEL in screen, (
        f"the capability action must stay visible while unauthorized\n{screen}\n"
        f"Evidence: {session.evidence_dir}"
    )
    assert "not_authorized" in screen, (
        f"the menu must explain why the action is unavailable\n{screen}\n"
        f"Evidence: {session.evidence_dir}"
    )

    # Enter on the disabled row refuses and keeps the user in the shell.
    session.send(ENTER, note="try the disabled action")
    session.wait_for_screen_text("not_authorized", timeout=8.0)
    assert _result(workspace, "blueprint", "action", "list") == []

    session.quit_via(b"q")
    session.assert_clean_exit()


def test_capability_action_executes_end_to_end(session_factory, tmp_path):
    workspace = Workspace.create(tmp_path)
    project = seed_workspace(workspace)
    authorize(workspace, project)
    offers = _result(workspace, "blueprint", "action", "offers", "item-1")
    assert offers[0]["availability"] == {"state": "available"}

    session = session_factory(workspace, label="m30-cap-execute")
    open_project(session, "Capability UI")
    select_work_item(session)
    screen = select_capability_action(session, ACTION_LABEL)
    assert "not_authorized" not in screen, screen

    session.send(ENTER, note="open the capability form")
    form = session.wait_for_screen_text("Note body", timeout=10.0, description="schema form")
    assert "Optional label" in form, form
    assert "open → closed" in form, form

    session.send_text("pty durable note")
    session.send(ENTER, note="submit the capability request")
    pending = session.wait_for_screen_text(
        "Request recorded (pending", timeout=10.0, description="pending confirmation"
    )
    assert "orbit blueprint action execute" in pending, pending

    session.send(ENTER, note="confirm execution")
    result = session.wait_for_screen_text(
        "Capability action · succeeded", timeout=15.0, description="succeeded result"
    )
    assert "outcome: succeeded" in result, result
    assert "provider: orbit" in result, result

    session.send(ENTER, note="close the result")
    session.send(ENTER, note="open the inspector")
    session.wait_for_screen_text(
        "CAPABILITY ACTIONS", timeout=10.0, description="inspector capability block"
    )
    inspector = ""
    for _ in range(12):
        try:
            inspector = session.wait_for_screen_text(
                "RECENT CAPABILITY ACTIONS",
                timeout=0.4,
                description="durable history block",
            )
            break
        except AssertionError:
            session.send(b"\x1b[6~", note="scroll the inspector")
    assert "RECENT CAPABILITY ACTIONS" in inspector, session.screen_text
    assert "succeeded" in inspector, inspector
    assert "append_note" in inspector, inspector

    session.send(ESC, note="close the inspector")
    session.quit_via(b"q")
    session.assert_clean_exit()

    # The real durable effect through the public read projection.
    item = _result(workspace, "blueprint", "item", "item-1")
    notes = item["capability_notes"]
    assert [note["note"] for note in notes] == ["pty durable note"], notes
    assert notes[0]["command_id"].startswith("command-"), notes
    executed = _result(
        workspace,
        "blueprint",
        "action",
        "list",
        "--item",
        "item-1",
        "--state",
        "succeeded",
    )
    assert len(executed) == 1, executed
