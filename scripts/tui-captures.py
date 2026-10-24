#!/usr/bin/env python3
"""Capture real Orbit TUI screens as text for documentation and evidence.

The script drives the compiled ``target/debug/orbit-tui`` through the shared
PTY harness (``tests/acceptance/tui/pty_harness.py``) and writes the
*reconstructed screen* — not the ANSI stream log — for each documented surface.
Every session runs in a throw-away local deployment seeded deterministically
through the real ``orbit`` CLI, so the captures never depend on a developer's
workspace.

Output layout::

    .agent/evidence/m29-captures/
        60x18-home.txt
        100x30-board.txt
        ...
        README.md          index: what each file shows and how to reproduce it

Connected captures (pending inspector, connected console) start a disposable
``orbit-server`` on a free local port and enrol a throw-away device. When the
server binary is missing or the authority cannot start, those two captures are
reported as skipped with the reason instead of writing a misleading screen.

Usage::

    python3 scripts/tui-captures.py                 # write every capture
    python3 scripts/tui-captures.py --list          # show the inventory only
    python3 scripts/tui-captures.py --only board    # one surface (all sizes)
    python3 scripts/tui-captures.py --out /tmp/opencode/caps

Requires ``cargo build -p orbit-tui -p orbit-cli`` (and ``-p orbit-server`` for
the connected captures, or ``ORBIT_SERVER_BIN`` pointing at a built server).
"""
from __future__ import annotations

import argparse
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
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

REPO_ROOT = Path(__file__).resolve().parents[1]
TUI_DIR = REPO_ROOT / "tests" / "acceptance" / "tui"
DEFAULT_OUT = REPO_ROOT / ".agent" / "evidence" / "m29-captures"

if str(TUI_DIR) not in sys.path:
    sys.path.insert(0, str(TUI_DIR))

from pty_harness import ENTER, TuiSession, Workspace  # noqa: E402

DEFAULT_TUI_BINARY = REPO_ROOT / "target" / "debug" / "orbit-tui"
DEFAULT_CLI_BINARY = REPO_ROOT / "target" / "debug" / "orbit"
DEFAULT_SERVER_BINARY = REPO_ROOT / "target" / "debug" / "orbit-server"

HOME_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Home / Today"
PROJECTS_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Projects"

# Fixed seed data: names, ids and statuses never change between runs. Dates are
# relative to the run so the planning views always show live work.
PROJECTS = ["Atlas Console", "Beacon Sync", "Cipher Notes"]

# (project, title, ordered legal status transitions from inbox, estimate minutes)
TASKS: list[tuple[str, str, tuple[str, ...], int | None]] = [
    ("Atlas Console", "Map the command palette", ("planned", "ready", "in-progress"), 90),
    ("Atlas Console", "Wire the task table", ("planned", "ready"), 45),
    ("Atlas Console", "Audit sync retries", ("planned", "ready", "in-progress", "review"), 60),
    ("Atlas Console", "Harden the offline queue", ("planned", "ready", "in-progress"), 120),
    ("Atlas Console", "Review attachment staging", ("planned", "ready", "blocked"), 30),
    ("Atlas Console", "Document the connected console", ("planned", "ready", "in-progress", "done"), 60),
    ("Atlas Console", "Trim the board cards", ("deferred",), None),
    ("Atlas Console", "Cancel the old export path", ("cancelled",), None),
    ("Beacon Sync", "Reconnect the replica", ("planned", "ready"), 30),
    ("Beacon Sync", "Verify the conflict review", ("planned", "ready", "in-progress"), 90),
    ("Cipher Notes", "Collect note search examples", (), 15),
]


# ---------------------------------------------------------------------------
# Ambiguous-width / height safety: all waits go through the harness deadlines.
# ---------------------------------------------------------------------------
def _settle(session: TuiSession, timeout: float = 4.0) -> str:
    """Wait until the reconstructed screen stops changing.

    ``screen_text`` only advances when the harness reads the PTY, so this pumps
    ``wait_until_screen`` with a predicate that never matches and treats the
    timeout as "one more frame arrived".
    """
    deadline = time.monotonic() + timeout
    previous = None
    while time.monotonic() < deadline:
        try:
            session.wait_until_screen(
                lambda _view: False, timeout=0.15, description="next frame"
            )
        except AssertionError:
            pass
        current = session.screen_text
        if current.strip() and current == previous:
            return current
        previous = current
    return session.screen_text


def _screen_dump(session: TuiSession) -> str:
    """The visible screen as plain text, without trailing padding or blanks."""
    lines = [line.rstrip() for line in _settle(session).splitlines()]
    while lines and not lines[-1]:
        lines.pop()
    return "\n".join(lines) + "\n"


def _wait_view(session: TuiSession, title: str, timeout: float = 8.0) -> None:
    """Wait for the project tab bar to mark ``title`` as the active view."""
    pattern = re.compile(r"▸\s*" + re.escape(title) + r"\b")
    session.wait_until_screen(
        lambda view: any(pattern.search(line) for line in view.splitlines()),
        timeout=timeout,
        description=f"{title} view to be active",
    )


def _wait_home(session: TuiSession) -> None:
    session.wait_for_screen_regex(
        HOME_HEADER_RE, timeout=15.0, description="Home / Today header"
    )


def _open_projects(session: TuiSession, *, expect_seed: bool = True) -> None:
    session.send_keys(b"g", b"p", delay=0.15)
    session.wait_for_screen_regex(
        PROJECTS_HEADER_RE, timeout=10.0, description="Projects screen"
    )
    if expect_seed:
        session.wait_for_screen_text(
            "Atlas Console", timeout=10.0, description="seeded project"
        )


def _open_atlas(session: TuiSession) -> None:
    _open_projects(session)
    session.send(ENTER, note="open Atlas Console")
    session.wait_for_screen_regex(
        r"(?m)^\s*ORBIT\s*›\s*Atlas Console", timeout=15.0, description="Atlas Console"
    )
    _wait_view(session, "Overview")


def _open_tasks(session: TuiSession) -> None:
    _open_atlas(session)
    session.send(b"]", note="switch to Tasks")
    _wait_view(session, "Tasks")
    # The table drops secondary columns on narrow terminals instead of
    # squashing them, so "DEADLINE" only exists at medium/wide widths. The
    # title column is always planned.
    session.wait_until_screen(
        lambda view: "TITLE" in view and "Map the command palette" in view,
        timeout=10.0,
        description="Tasks table",
    )


def _open_board(session: TuiSession) -> None:
    _open_atlas(session)
    session.send_keys(b"]", b"]", delay=0.2)
    _wait_view(session, "Board")
    session.wait_until_screen(
        lambda view: "columns" in view,
        timeout=10.0,
        description="board columns legend",
    )


# ---------------------------------------------------------------------------
# Surface actions: each receives a freshly launched session.
# ---------------------------------------------------------------------------
def _home(session: TuiSession) -> None:
    _wait_home(session)


def _projects(session: TuiSession) -> None:
    _open_projects(session)


def _tasks(session: TuiSession) -> None:
    _open_tasks(session)
    session.send_keys(b"j", b"j", delay=0.15)


def _board(session: TuiSession) -> None:
    _open_board(session)


def _task_detail(session: TuiSession) -> None:
    _open_tasks(session)
    session.send_keys(b"j", b"j", delay=0.15)
    session.send(ENTER, note="open the selected task detail")
    session.wait_for_screen_text(" · detail", timeout=10.0, description="task detail modal")


def _action_menu(session: TuiSession) -> None:
    _open_tasks(session)
    session.send_keys(b"j", b"j", delay=0.15)
    session.send(b"a", note="open the action menu")
    session.wait_for_screen_text("Actions ·", timeout=10.0, description="action menu")


def _palette_query(session: TuiSession) -> None:
    _wait_home(session)
    session.send(b":", note="open the command palette")
    session.wait_for_screen_text("Command palette", timeout=10.0, description="palette")
    session.send_text("tas", delay=0.08)
    session.wait_for_screen_text("> tas", timeout=10.0, description="palette query")


def _quick_edit(session: TuiSession) -> None:
    _open_tasks(session)
    session.send_keys(b"j", b"j", delay=0.15)
    session.send(b"E", note="open quick edit")
    session.wait_for_screen_text("Quick edit ·", timeout=10.0, description="quick-edit form")


def _wait_connected_probe(session: TuiSession) -> None:
    """Prove the session is connected without relying on the header summary.

    At 60 columns the header drops its summary (and therefore the `CONNECTED`
    label) to keep the breadcrumb. `!` exists only in connected mode: a local
    session answers with a warning instead of the inspector.
    """
    session.send(b"!", note="probe the connected pending inspector")
    session.wait_for_screen_text(
        "Pending commands", timeout=25.0, description="connected-only pending inspector"
    )
    session.send(b"\x1b", note="close the inspector")
    session.wait_until_screen(
        lambda view: "Pending commands" not in view,
        timeout=10.0,
        description="inspector closed",
    )


def _pending_inspector(session: TuiSession) -> None:
    _wait_connected_probe(session)
    # Queue one durable intention without flushing it: the inspector must show
    # a real pending row, not an empty placeholder. `c` on Projects creates a
    # project (on Home it would be an inbox task).
    _open_projects(session, expect_seed=False)
    session.send(b"c", note="open the new-project form")
    session.wait_for_screen_text("New project", timeout=10.0, description="project form")
    session.send_text("Pending Scope", delay=0.03)
    session.send(ENTER, note="queue the project without flushing")
    session.wait_until_screen(
        lambda view: "New project" not in view,
        timeout=25.0,
        description="project form to close after queueing",
    )
    session.send(b"!", note="open the pending inspector")
    session.wait_for_screen_text("queued (not accepted)", timeout=15.0, description="inspector")
    session.wait_for_screen_text("[pending]", timeout=15.0, description="pending row")


def _connected_console(session: TuiSession) -> None:
    # This capture is only taken at 100x30, where the header summary (including
    # the CONNECTED label) still fits.
    session.wait_for_screen_text("CONNECTED", timeout=25.0, description="connected header")
    session.send(b":", note="open the command palette")
    session.wait_for_screen_text("Command palette", timeout=10.0, description="palette")
    session.send_text("Connected commands", delay=0.03)
    session.wait_for_screen_text("Connected commands", timeout=10.0, description="console entry")
    session.send(ENTER, note="open the connected console")
    session.wait_for_screen_text(
        "queued intents are PENDING", timeout=10.0, description="connected console"
    )
    # One read-only command proves the console is live, not a static help box.
    session.send_text("sync.status", delay=0.03)
    session.send(ENTER, note="run sync.status")
    session.wait_for_screen_text("Sync", timeout=25.0, description="sync status output")


@dataclass(frozen=True)
class Capture:
    slug: str
    cols: int
    rows: int
    connected: bool
    action: Callable[[TuiSession], None]
    shows: str
    keys: str

    @property
    def size(self) -> str:
        return f"{self.cols}x{self.rows}"

    @property
    def filename(self) -> str:
        return f"{self.size}-{self.slug}.txt"


CAPTURES: list[Capture] = [
    Capture("home", 60, 18, False, _home,
            "Compact shell: only the workspace is visible, and the pane bar above it names both panes "
            "plus `Tab switch · D details`. The Home grid still fits at this height, so every component "
            "is painted and long lines truncate inside their panel.",
            "launch at 60x18"),
    Capture("home", 100, 30, False, _home,
            "Sidebar + workspace (details dock only from 120 columns), the Today/Next planning component "
            "and the compact action bar.",
            "launch at 100x30"),
    Capture("home", 160, 45, False, _home,
            "Full three-region shell: the sidebar sizes to its labels, the workspace keeps every Home "
            "component and the detail pane is docked on the right.",
            "launch at 160x45"),
    Capture("projects", 60, 18, False, _projects,
            "Projects list at the smallest supported size, with the seeded project and its status glyph.",
            "g p"),
    Capture("projects", 100, 30, False, _projects,
            "Projects list with keyboard selection, project status glyph and the contextual action bar.",
            "g p"),
    Capture("projects", 160, 45, False, _projects,
            "Projects list with the docked detail pane for the selected project.",
            "g p"),
    Capture("tasks", 60, 18, False, _tasks,
            "The Tasks table in compact mode: one pane, the title column plus the status and priority "
            "columns that fit. The deadline column is dropped rather than squashed; open the row's "
            "detail for every field.",
            "g p, Enter, ], j j"),
    Capture("tasks", 100, 30, False, _tasks,
            "The Tasks table with a selected row (`▸` rail and filled row), the status glyph and "
            "priority; details are not docked at this width, so Enter/D opens them.",
            "g p, Enter, ], j j"),
    Capture("tasks", 160, 45, False, _tasks,
            "The Tasks table at full width: every column gets room and the detail pane stays docked.",
            "g p, Enter, ], j j"),
    Capture("board", 60, 18, False, _board,
            "The Kanban in compact mode: two readable columns of ten (`columns 2-3 of 10`), the column "
            "window centred on the selected card, and the `j/k card · h/l panel · S status` hints.",
            "g p, Enter, ] ]"),
    Capture("board", 100, 30, False, _board,
            "The Kanban at a medium width: three readable columns of ten (`columns 2-4 of 10`), the window "
            "following the selected card and the quick-edit/status hints.",
            "g p, Enter, ] ]"),
    Capture("board", 160, 45, False, _board,
            "The wide Kanban with the details pane docked: five readable columns of ten (`columns 1-5 of "
            "10`), column headers with counts, the selected-card marker and per-column status glyphs.",
            "g p, Enter, ] ]"),
    Capture("task-detail", 100, 30, False, _task_detail,
            "The task detail overlay (`… · detail`): status, readiness, estimate, due date, deadline, "
            "attachments and every action the selection authorizes.",
            "g p, Enter, ], j j, Enter"),
    Capture("task-detail", 160, 45, False, _task_detail,
            "The same task detail overlay on a large terminal, where the property list and the full "
            "action list fit without scrolling.",
            "g p, Enter, ], j j, Enter"),
    Capture("action-menu", 100, 30, False, _action_menu,
            "The grouped action menu (`Actions · …`): Suggested operations first, then More actions; "
            "only keys that identify a row uniquely are advertised, the rest stay keyless for Enter.",
            "g p, Enter, ], j j, a"),
    Capture("action-menu", 160, 45, False, _action_menu,
            "The action menu on a large terminal: the same groups and actions with every label on one "
            "line (a disabled row would print its reason here instead of being hidden).",
            "g p, Enter, ], j j, a"),
    Capture("palette-query", 100, 30, False, _palette_query,
            "The command palette with the query `tas`: ranked matches (`prefix · word · fuzzy · "
            "substring`) with the `Ctrl+U clear` hint; disabled entries stay listed with a reason.",
            "launch, :, type tas"),
    Capture("quick-edit", 100, 30, False, _quick_edit,
            "The `E` quick-edit form: only Title and Priority, with `Save Ctrl+S · Cancel Esc` in the "
            "title bar and the field-level footer; status changes stay in the `S` menu.",
            "g p, Enter, ], j j, E"),
    Capture("pending-inspector", 60, 18, True, _pending_inspector,
            "Connected pending inspector at 60x18 (`Pending commands · 1 queued (not accepted)`): the "
            "durable row is labelled `[pending]` and its detail is clipped to the modal width, never "
            "promoted to accepted data.",
            "connected launch, g p, c, type Pending Scope, Enter, !"),
    Capture("pending-inspector", 100, 30, True, _pending_inspector,
            "Connected pending inspector with the header's connected summary visible; the detail panel "
            "shows the canonical payload and recorded state, and the footer lists "
            "`f flush · c review conflict · Enter detail · r refresh · Esc close`.",
            "connected launch, g p, c, type Pending Scope, Enter, !"),
    Capture("connected-console", 100, 30, True, _connected_console,
            "The connected command console (`queued intents are PENDING`) after a read-only `sync.status`: "
            "read-only output appears inline, nothing is queued.",
            "connected launch, :, Connected commands, sync.status"),
]


# ---------------------------------------------------------------------------
# Deterministic local workspace (real CLI only; no direct storage writes).
# ---------------------------------------------------------------------------
def _cli(workspace: Workspace, *args: str) -> dict:
    binary = Path(os.environ.get("ORBIT_CLI_BIN", DEFAULT_CLI_BINARY)).expanduser()
    completed = subprocess.run(
        [str(binary), "--db", str(workspace.db), "--json", *args],
        env=dict(workspace.env),
        cwd=str(REPO_ROOT),
        text=True,
        capture_output=True,
        timeout=90,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"orbit {' '.join(args)} failed\nstdout: {completed.stdout}\nstderr: {completed.stderr}"
        )
    output = completed.stdout.strip()
    return json.loads(output) if output else {}


def _project_id(workspace: Workspace, name: str) -> str:
    with sqlite3.connect(workspace.db) as connection:
        row = connection.execute("SELECT id FROM projects WHERE name=?1", (name,)).fetchone()
    assert row is not None, f"project {name!r} was not persisted"
    return row[0]


def seed_workspace(base: Path, *, verbose: bool = False) -> Workspace:
    base.mkdir(parents=True, exist_ok=True)
    workspace = Workspace.create(base)
    now_ms = int(time.time() * 1000)
    project_ids: dict[str, str] = {}
    for name in PROJECTS:
        workspace.create_project(name)
        project_ids[name] = _project_id(workspace, name)
        if verbose:
            print(f"  seeded project {name}", file=sys.stderr)

    for index, (project, title, transitions, estimate) in enumerate(TASKS):
        payload = _cli(
            workspace,
            "task",
            "add",
            title,
            "--project",
            project_ids[project],
            "--id",
            f"task-capture-{index + 1:02d}",
        )
        task_id = payload["task"]["id"]
        if estimate is not None:
            # A spread of deadlines keeps Today/Next and the table realistic.
            _cli(
                workspace,
                "task",
                "planning",
                task_id,
                "--estimate-minutes",
                str(estimate),
                "--deadline-ms",
                str(now_ms + (index + 1) * 86_400_000),
            )
        for status in transitions:
            _cli(workspace, "task", "status", task_id, status)
    return workspace


# ---------------------------------------------------------------------------
# Disposable connected authority for the pending/console captures.
# ---------------------------------------------------------------------------
def _http(base: str, path: str, *, token: str | None = None, payload: dict | None = None):
    headers = {"Content-Type": "application/json"} if payload is not None else {}
    if token:
        headers["Authorization"] = "Bearer " + token
    request = urllib.request.Request(
        base + path,
        data=None if payload is None else json.dumps(payload).encode(),
        headers=headers,
        method="POST" if payload is not None else "GET",
    )
    with urllib.request.urlopen(request, timeout=8) as response:
        body = response.read()
    return json.loads(body) if body else None


class Authority:
    """One disposable orbit-server on a free port, plus an enrolled device."""

    def __init__(self, root: Path) -> None:
        binary = Path(os.environ.get("ORBIT_SERVER_BIN", DEFAULT_SERVER_BINARY)).expanduser()
        if not binary.is_file():
            raise RuntimeError(
                f"orbit-server not found at {binary}; build it with "
                "`cargo build -p orbit-server` or set ORBIT_SERVER_BIN"
            )
        self.root = root
        root.mkdir(parents=True, exist_ok=True)
        self.binary = binary
        socket_handle = socket.socket()
        socket_handle.bind(("127.0.0.1", 0))
        self.port = socket_handle.getsockname()[1]
        socket_handle.close()
        self.base = f"http://127.0.0.1:{self.port}"
        self.admin = secrets.token_hex(32)
        self.device = "tui-capture-device"
        self.token = secrets.token_hex(32)
        self.process: subprocess.Popen | None = None
        self.workspace = self._workspace()
        self._start()

    def _workspace(self) -> Workspace:
        replica = self.root / "replica.db"
        config = self.root / "orbit.config.toml"
        home = self.root / "home"
        home.mkdir(parents=True, exist_ok=True)
        env = {key: value for key, value in os.environ.items() if not key.startswith("ORBIT_")}
        env.update(
            {
                "HOME": str(home),
                "ORBIT_CONFIG": str(config),
                "ORBIT_DB": str(self.root / "unused-local.db"),
                "ORBIT_AUTHORITY_DB": str(replica),
                "ORBIT_AUTHORITY_SERVER": self.base,
                "ORBIT_AUTHORITY_DEVICE": self.device,
                "ORBIT_AUTHORITY_TOKEN": self.token,
                "ORBIT_SYNC_HTTP_TOKEN": self.token,
                "TERM": "xterm-256color",
                "LANG": "C.UTF-8",
            }
        )
        return Workspace(root=self.root, db=replica, config=config, home=home, env=env)

    def _start(self) -> None:
        self.log = (self.root / "server.log").open("a")
        self.process = subprocess.Popen(
            [str(self.binary)],
            env={
                "ORBIT_SERVER_DB": str(self.root / "server.db"),
                "ORBIT_SERVER_BIND": f"127.0.0.1:{self.port}",
                "ORBIT_SERVER_ADMIN_KEY": self.admin,
            },
            stdout=self.log,
            stderr=subprocess.STDOUT,
        )
        for _ in range(150):
            if self.process.poll() is not None:
                raise RuntimeError("orbit-server exited during startup:\n" + self._log_tail())
            try:
                _http(self.base, "/health")
                break
            except Exception:
                time.sleep(0.1)
        else:
            raise RuntimeError("orbit-server readiness timeout:\n" + self._log_tail())
        _http(
            self.base,
            "/v1/devices",
            token=self.admin,
            payload={"device_id": self.device, "token": self.token},
        )

    def _log_tail(self) -> str:
        path = self.root / "server.log"
        return path.read_text(encoding="utf-8", errors="replace")[-4000:] if path.is_file() else ""

    def stop(self) -> None:
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        self.process = None
        self.log.close()


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------
def _start_session(
    workspace: Workspace,
    capture: Capture,
    index: int,
    evidence_root: Path,
    state_root: Path,
) -> TuiSession:
    # Every capture starts from Home: its own session state file means no
    # capture inherits the previous one's navigation or selection.
    env = dict(workspace.env)
    env["ORBIT_TUI_STATE"] = str(state_root / f"{capture.size}-{capture.slug}-{index}.json")
    workspace_for_session = Workspace(
        root=workspace.root,
        db=workspace.db,
        config=workspace.config,
        home=workspace.home,
        env=env,
    )
    return TuiSession(
        workspace_for_session,
        cols=capture.cols,
        rows=capture.rows,
        label=f"capture-{capture.size}-{capture.slug}-{index}",
        evidence_dir=evidence_root,
        wait_for_launch=False,
    )


def run_captures(
    out: Path,
    *,
    only: str | None,
    verbose: bool,
) -> tuple[list[Path], list[tuple[Capture, str]]]:
    binary = Path(os.environ.get("ORBIT_TUI_BIN", DEFAULT_TUI_BINARY)).expanduser()
    cli = Path(os.environ.get("ORBIT_CLI_BIN", DEFAULT_CLI_BINARY)).expanduser()
    for required, package in ((binary, "orbit-tui"), (cli, "orbit-cli")):
        if not required.is_file():
            raise SystemExit(
                f"{required} is missing; run `cargo build -p {package}` or set the matching "
                "ORBIT_*_BIN environment variable"
            )
    out.mkdir(parents=True, exist_ok=True)
    scratch = Path("/tmp/opencode")
    scratch.mkdir(parents=True, exist_ok=True)
    work_root = Path(tempfile.mkdtemp(prefix="m29-captures-", dir=str(scratch)))
    evidence_root = work_root / "pty-evidence"
    state_root = work_root / "state"
    state_root.mkdir()

    print(f"seeding deterministic workspace under {work_root}/workspace", flush=True)
    local = seed_workspace(work_root / "workspace", verbose=verbose)
    selected = [
        capture
        for capture in CAPTURES
        if only is None or only in capture.slug or only in capture.filename
    ]
    if not selected:
        raise SystemExit(f"no capture matches {only!r}")

    written: list[Path] = []
    skipped: list[tuple[Capture, str]] = []
    for index, capture in enumerate(selected, start=1):
        label = f"{index:02d}/{len(selected)} {capture.size} {capture.slug}"
        authority: Authority | None = None
        if capture.connected:
            # One disposable authority per connected capture: no queued command
            # from an earlier capture leaks into the next screen.
            try:
                authority = Authority(work_root / f"connected-{index:02d}")
            except Exception as error:  # clearly skipped, never a fake screen
                reason = str(error)
                skipped.append((capture, reason))
                print(f"[{label}] SKIPPED ({reason})", flush=True)
                continue
            workspace = authority.workspace
        else:
            workspace = local
        print(f"[{label}] capturing…", flush=True)
        session: TuiSession | None = None
        try:
            session = _start_session(workspace, capture, index, evidence_root, state_root)
            capture.action(session)
            dump = _screen_dump(session)
        except AssertionError as error:
            hint = f"\nEvidence: {session.evidence_dir}" if session is not None else ""
            raise SystemExit(f"{label} failed:\n{error}{hint}")
        finally:
            if session is not None:
                session.close()
            if authority is not None:
                authority.stop()
        target = out / capture.filename
        target.write_text(dump, encoding="utf-8")
        written.append(target)
        print(f"[{label}] wrote {target.name} ({len(dump.splitlines())} rows)", flush=True)
    return written, skipped


def _readme(written: list[Path], skipped: list[tuple[Capture, str]]) -> str:
    present = {path.name for path in written}
    lines = [
        "# M29 terminal captures",
        "",
        "Real `target/debug/orbit-tui` sessions driven through",
        "`tests/acceptance/tui/pty_harness.py`. Each `.txt` file is the *reconstructed screen*",
        "at the moment named below — not a stream log, not a hand-written mock. Trailing",
        "padding and trailing blank lines are removed; everything else is exactly what the",
        "terminal showed.",
        "",
        "## Reproduce",
        "",
        "```sh",
        "cargo build -p orbit-tui -p orbit-cli -p orbit-server",
        "python3 scripts/tui-captures.py",
        "```",
        "",
        "The script seeds its own throw-away workspace with the real CLI (three projects,",
        "eleven tasks spanning the Basic workflow statuses, estimates and deadlines), writes",
        "PTY evidence under a temporary directory, and replaces this directory's `.txt` files.",
        "`--only <surface>` limits a run; `--list` prints the inventory.",
        "",
        "Connected captures each start a disposable `orbit-server` on a free loopback port,",
        "enrol a throw-away device and queue one real intention, so every connected screen shows",
        "exactly the work that session queued. They are skipped with a printed reason",
        "when `orbit-server` cannot be built or started.",
        "",
        "## Inventory",
        "",
        "| File | Size | Surface | Shows | Reproduce |",
        "|---|---|---|---|---|",
    ]
    for capture in CAPTURES:
        if capture.filename not in present:
            continue
        lines.append(
            f"| `{capture.filename}` | {capture.size} | {capture.slug} | {capture.shows} | "
            f"`{capture.keys.replace('`', '')}` |"
        )
    if skipped:
        lines.extend(["", "## Skipped", ""])
        for capture, reason in skipped:
            lines.append(f"- `{capture.filename}` — {reason}")
    lines.extend(
        [
            "",
            "## Reading a capture",
            "",
            "- The header row is `ORBIT › <screen>`. A project screen adds its pinned workflow",
            "  (`Basic v1`) and a summary; the summary is dropped when the row is too narrow, so it is",
            "  absent at 60 columns.",
            "- The focused pane has a thick border and an accent title. The selected row has a `▸` rail",
            "  and a filled style; the status glyph (`✓ ! ◐ ▶ ○ ~`) repeats the state without colour.",
            "- Below 76 columns only one pane is visible; the row above it reads",
            "  `▸ Workspace │ Sidebar │ Tab switch · D details`.",
            "- From 76 columns the sidebar is shown. Details dock from 120 columns only while the active",
            "  component still gets its readable minimum width; otherwise `D`/`Enter` opens the detail",
            "  overlay instead of squeezing the workspace.",
            "- The action bar is the second-to-last row; the status line is the last row.",
            "- Connected captures show `CONNECTED … N pending · ! inspect` in the header when it fits.",
            "  Every queued row is labelled `[pending]` or `[conflict]`; none of it is accepted data.",
            "- Dates and command ids are generated at run time; project names, task titles, statuses and",
            "  the capture sequence are fixed by the script.",
            "",
        ]
    )
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT, help="capture directory")
    parser.add_argument("--only", help="only surfaces whose slug contains this text")
    parser.add_argument("--list", action="store_true", help="print the inventory and exit")
    parser.add_argument("--verbose", action="store_true", help="seed progress on stderr")
    args = parser.parse_args(argv)

    if args.list:
        for capture in CAPTURES:
            if args.only and args.only not in capture.slug:
                continue
            print(f"{capture.filename:40} {capture.slug:18} {capture.keys}")
        return 0

    written, skipped = run_captures(args.out, only=args.only, verbose=args.verbose)
    readme = args.out / "README.md"
    readme.write_text(_readme(written, skipped), encoding="utf-8")

    print("")
    print(f"captures: {len(written)} written, {len(skipped)} skipped")
    for path in written:
        print(f"  {path.relative_to(REPO_ROOT) if path.is_relative_to(REPO_ROOT) else path} "
              f"({path.stat().st_size} bytes)")
    for capture, reason in skipped:
        print(f"  SKIPPED {capture.filename}: {reason}")
    print(f"  {readme}")

    missing = [
        capture.filename
        for capture in CAPTURES
        if (args.only is None or args.only in capture.slug)
        and not capture.connected
        and capture.filename not in {path.name for path in written}
    ]
    if missing:
        print(f"missing declared captures: {', '.join(missing)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
