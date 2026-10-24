"""M29 acceptance Scenario E: connected offline queue and conflict recovery.

This drives the real ``orbit-tui`` against a disposable authoritative
``orbit-server`` in connected mode and covers one end-to-end recovery story:

1. an isolated server is started on a free port with a disposable database and
   an enrolled device;
2. the server is stopped, a project is queued through the shared shell, and
   the accepted workspace still shows ``0 project(s)`` with a pending status;
3. the ``!`` pending inspector marks the row ``[pending]`` and the detail keeps
   calling it unaccepted;
4. the same server restarts on the same database/port, ``f`` flushes, ``r``
   refreshes, and the project is accepted with a zero pending count;
5. a second queued mutation is marked ``state='conflict'`` directly in the
   replica SQLite outbox (the same technique the client tests use), surfaced
   as ``[conflict]`` by inspector refresh, reviewed with ``c``, replaced with a
   fresh pending command, flushed, and accepted without deleting the rejected
   audit row;
6. the TUI quits cleanly and restores the alternate screen.

Assertions read the reconstructed terminal screen; the replica database is
only touched where the scenario explicitly requires injecting a conflict.

Run from the repository root after::

    cargo build -p orbit-tui -p orbit-cli -p orbit-server --offline
"""
from __future__ import annotations

import json
import os
import secrets
import socket
import sqlite3
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))

from pty_harness import CTRL_Q, ENTER, ESC, TuiSession, Workspace  # noqa: E402

ROOT = Path(__file__).resolve().parents[3]
SERVER = Path(os.environ.get("ORBIT_SERVER_BIN", ROOT / "target" / "debug" / "orbit-server"))
TUI = Path(os.environ.get("ORBIT_TUI_BIN", ROOT / "target" / "debug" / "orbit-tui"))
REQUIRE_EVIDENCE = os.environ.get("ORBIT_REQUIRE_CONNECTED_TUI_EVIDENCE") == "1"

PROJECTS_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Projects"

# The authoritative server accepts semantic commands but never retries an
# ambiguous local intention. Every wait below has an explicit 8-10s deadline.
WAIT = 10.0


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

    def __init__(self, *, root: Path, env: dict, base: str, port: int, database: Path, admin: str):
        self.root = root
        self.env = env
        self.base = base
        self.port = port
        self.database = database
        self.admin = admin
        self.process: subprocess.Popen | None = None
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
            # The child keeps its own duplicate of the descriptor.
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

    def restart(self) -> None:
        self.stop()
        self.start()


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
    device = "tui-connected-device"
    token = secrets.token_hex(32)
    env = {
        key: value for key, value in os.environ.items() if not key.startswith("ORBIT_")
    }
    env.update(
        {
            "HOME": str(home),
            "ORBIT_CONFIG": str(config),
            # The local DB is deliberately unused: this is connected mode.
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


# ---------------------------------------------------------------------------
# Replica outbox helpers (the only direct database access in this scenario).
# ---------------------------------------------------------------------------
def _replica_db(workspace: Workspace) -> Path:
    return Path(workspace.env["ORBIT_AUTHORITY_DB"])


def _pending_command_id(workspace: Workspace) -> str:
    connection = sqlite3.connect(_replica_db(workspace), timeout=10)
    try:
        row = connection.execute(
            "SELECT command_id FROM authority_outbox "
            "WHERE state='pending' ORDER BY ordinal DESC LIMIT 1"
        ).fetchone()
    finally:
        connection.close()
    assert row is not None, (
        "the offline mutation never reached the replica authority_outbox\n"
        f"replica: {_replica_db(workspace)}"
    )
    return row[0]


def _mark_conflict(workspace: Workspace, command_id: str) -> None:
    """Inject the conflict state exactly as the client test suite does."""
    connection = sqlite3.connect(_replica_db(workspace), timeout=10)
    try:
        updated = connection.execute(
            "UPDATE authority_outbox SET state='conflict' "
            "WHERE command_id=?1 AND state='pending'",
            (command_id,),
        ).rowcount
        connection.commit()
    finally:
        connection.close()
    assert updated == 1, (
        f"expected to mark {command_id!r} conflicted; update touched {updated} row(s)"
    )


def _outbox_state(workspace: Workspace, command_id: str) -> str | None:
    connection = sqlite3.connect(_replica_db(workspace), timeout=10)
    try:
        row = connection.execute(
            "SELECT state FROM authority_outbox WHERE command_id=?1",
            (command_id,),
        ).fetchone()
    finally:
        connection.close()
    return None if row is None else row[0]


def _outbox_action(workspace: Workspace, command_id: str) -> dict:
    connection = sqlite3.connect(_replica_db(workspace), timeout=10)
    try:
        row = connection.execute(
            "SELECT action FROM authority_outbox WHERE command_id=?1",
            (command_id,),
        ).fetchone()
    finally:
        connection.close()
    assert row is not None, f"no outbox action for {command_id!r}"
    return json.loads(row[0])


def _open_projects(session: TuiSession) -> None:
    session.send_keys(b"g", b"p", delay=0.15)
    session.wait_for_screen_regex(
        PROJECTS_HEADER_RE, timeout=8.0, description="Projects screen"
    )


# ---------------------------------------------------------------------------
# Scenario E: offline queue -> acceptance -> conflict review -> replacement.
# ---------------------------------------------------------------------------
def test_connected_offline_queue_and_conflict_recovery(connected_workspace):
    workspace, authority, base, device, token = connected_workspace
    session = TuiSession(workspace, cols=120, rows=35, label="m29-connected-recovery")
    try:
        session.wait_for_screen_text(
            "CONNECTED", timeout=20.0, description="connected header"
        )
        _open_projects(session)
        session.wait_for_screen_text(
            "0 project(s)", timeout=8.0, description="no accepted projects yet"
        )

        # ------------------------------------------------------------------
        # 1. Queue a project while the authoritative server is down.
        # ------------------------------------------------------------------
        authority.stop()
        mark = session.mark()
        session.send(b"c", note="new project while the server is down")
        session.wait_for_screen_text("New project", timeout=8.0, description="project form")
        session.send_text("Offline Project")
        session.send(ENTER, note="queue project offline")
        session.wait_for_text(
            "not accepted yet", timeout=WAIT, since=mark, description="pending status"
        )
        offline_screen = session.wait_until_screen(
            lambda view: (
                "0 project(s)" in view
                and "1 pending" in view
                and "not accepted" in view
            ),
            timeout=WAIT,
            description="offline queue leaves the accepted workspace empty",
        )
        assert "1 project(s)" not in offline_screen, (
            "the TUI promoted an unacknowledged project to the accepted workspace\n"
            f"screen:\n{offline_screen}\nEvidence: {session.evidence_dir}"
        )

        # ------------------------------------------------------------------
        # 2. The pending inspector must stay honest about acceptance.
        # ------------------------------------------------------------------
        session.send(b"!", note="pending inspector")
        session.wait_for_screen_text(
            "Pending commands · 1 queued (not accepted)",
            timeout=8.0,
            description="pending inspector",
        )
        pending_screen = session.wait_until_screen(
            lambda view: "[pending] project" in view and "state: pending" in view,
            timeout=8.0,
            description="pending row and detailed status",
        )
        assert "queued (not accepted)" in pending_screen, (
            "pending inspector did not label the queued command as unaccepted\n"
            f"screen:\n{pending_screen}\nEvidence: {session.evidence_dir}"
        )
        # The six-line detail viewport initially ends before the receipt line;
        # scroll once to prove it explicitly says "none (not accepted)".
        session.send(b"\x1b[6~", note="detail page down")
        receipt_screen = session.wait_for_screen_text(
            "accepted receipt: none (not accepted)",
            timeout=8.0,
            description="explicitly absent accepted receipt",
        )
        assert "[pending] project" in receipt_screen, receipt_screen
        session.send(ESC, note="close pending inspector")
        session.wait_until_screen(
            lambda view: "Pending commands" not in view,
            timeout=8.0,
            description="pending inspector closes",
        )

        # ------------------------------------------------------------------
        # 3. Restart the same server/database/port, flush, and accept.
        # ------------------------------------------------------------------
        authority.restart()
        mark = session.mark()
        session.send(b"f", note="flush offline queue")
        session.wait_for_text(
            "Server accepted 1 pending command(s)",
            timeout=WAIT,
            since=mark,
            description="server acceptance after reconnect",
        )
        session.send(b"r", note="refresh accepted state")
        _open_projects(session)
        accepted_screen = session.wait_until_screen(
            lambda view: (
                "Offline Project" in view
                and "1 project(s)" in view
                and "0 pending" in view
            ),
            timeout=WAIT,
            description="offline project accepted after reconnect",
        )
        assert "CONNECTED" in accepted_screen, (
            "the accepted refresh did not re-enter the connected presentation\n"
            f"screen:\n{accepted_screen}\nEvidence: {session.evidence_dir}"
        )

        # ------------------------------------------------------------------
        # 4. Queue a second mutation, then inject a conflict into its row.
        # ------------------------------------------------------------------
        mark = session.mark()
        session.send(b"c", note="queue project for conflict recovery")
        session.wait_for_screen_text("New project", timeout=8.0, description="project form")
        session.send_text("Conflict Project")
        session.send(ENTER, note="queue project before conflict injection")
        session.wait_for_text(
            "not accepted yet", timeout=WAIT, since=mark, description="second pending status"
        )
        session.wait_until_screen(
            lambda view: "1 pending" in view,
            timeout=8.0,
            description="second command counted as pending",
        )
        conflicted_id = _pending_command_id(workspace)

        session.send(b"!", note="pending inspector before conflict")
        session.wait_for_screen_text(
            "Pending commands · 1 queued (not accepted)",
            timeout=8.0,
            description="pending inspector",
        )
        session.wait_for_screen_text(
            "[pending] project", timeout=8.0, description="second command starts pending"
        )
        _mark_conflict(workspace, conflicted_id)
        assert _outbox_state(workspace, conflicted_id) == "conflict", (
            "conflict injection did not persist in the replica outbox"
        )
        session.send(b"r", note="refresh inspector after conflict injection")
        conflict_screen = session.wait_until_screen(
            lambda view: "[conflict] project" in view and "state: conflict" in view,
            timeout=8.0,
            description="conflicted row and detail after inspector refresh",
        )
        assert "[pending] project" not in conflict_screen, (
            "the refreshed inspector still shows the row as pending\n"
            f"screen:\n{conflict_screen}\nEvidence: {session.evidence_dir}"
        )
        assert _outbox_state(workspace, conflicted_id) == "conflict", (
            "the original conflicted row disappeared before any replacement was reviewed"
        )

        # ------------------------------------------------------------------
        # 5. Review the conflict and stage a fresh valid replacement.
        # ------------------------------------------------------------------
        session.send(b"c", note="open conflict review")
        session.wait_for_screen_text(
            "Conflict review · rejected, not accepted",
            timeout=8.0,
            description="conflict review modal",
        )
        review_screen = session.wait_for_screen_text(
            "REJECTED by the authority, not accepted",
            timeout=8.0,
            description="explicit rejection wording",
        )
        assert "Accepted server revision:" in review_screen, (
            "conflict review did not show the accepted revision\n"
            f"screen:\n{review_screen}\nEvidence: {session.evidence_dir}"
        )
        assert "Local intent (NOT accepted)" in review_screen, review_screen
        assert "Recorded command state:" in review_screen, review_screen
        assert "state: conflict" in review_screen, review_screen
        assert _outbox_state(workspace, conflicted_id) == "conflict", (
            "the original conflicted row was removed before Enter submitted a replacement"
        )

        replacement = "project.create conflict-replacement Conflict Replacement"
        session.send_text(replacement)
        session.send(ENTER, note="queue reviewed replacement")
        # A valid replacement closes the modal as durable pending intent. The
        # resulting status line is longer than the viewport renders, so verify
        # the explicit durable rows rather than relying on clipped copy.
        session.wait_until_screen(
            lambda view: "Conflict review" not in view,
            timeout=WAIT,
            description="conflict review closes after a valid replacement",
        )
        replacement_command_id = _pending_command_id(workspace)
        assert replacement_command_id != conflicted_id, (
            "the replacement reused the rejected command identity"
        )
        replacement_action = _outbox_action(workspace, replacement_command_id)
        assert replacement_action.get("project_id") == "conflict-replacement", (
            f"unexpected replacement action: {replacement_action!r}"
        )
        assert replacement_action.get("name") == "Conflict Replacement", (
            f"unexpected replacement action: {replacement_action!r}"
        )
        assert _outbox_state(workspace, replacement_command_id) == "pending", (
            "the replacement was not staged as pending local intent"
        )
        # The rejected audit row is retained as superseded, never deleted.
        assert _outbox_state(workspace, conflicted_id) == "superseded", (
            "resolving the conflict silently removed the original rejected row"
        )
        assert "1 pending" in session.screen_text, (
            "the queued replacement was not counted as pending\n"
            f"screen:\n{session.screen_text}\nEvidence: {session.evidence_dir}"
        )

        # ------------------------------------------------------------------
        # 6. Flush the replacement and require acceptance, not a silence.
        # ------------------------------------------------------------------
        mark = session.mark()
        session.send(b"f", note="flush reviewed replacement")
        session.wait_for_text(
            "Server accepted 1 pending command(s)",
            timeout=WAIT,
            since=mark,
            description="replacement accepted after flush",
        )
        session.send(b"r", note="refresh accepted replacement")
        _open_projects(session)
        final_screen = session.wait_until_screen(
            lambda view: (
                "Conflict Replacement" in view
                and "2 project(s)" in view
                and "0 pending" in view
            ),
            timeout=WAIT,
            description="replacement project accepted with no pending work",
        )
        assert "Conflict Project" not in final_screen, (
            "the rejected command's project appeared as accepted data\n"
            f"screen:\n{final_screen}\nEvidence: {session.evidence_dir}"
        )
        assert _outbox_state(workspace, replacement_command_id) == "accepted", (
            "the replacement never reached an accepted state in the replica"
        )
        assert _outbox_state(workspace, conflicted_id) == "superseded", (
            "the rejected audit row was removed after acceptance"
        )
        snapshot = _request(base, "/v1/authoritative/snapshot", token=token, device=device)
        accepted_names = {project.get("name") for project in snapshot.get("projects") or []}
        assert "Conflict Replacement" in accepted_names, accepted_names
        assert "Conflict Project" not in accepted_names, accepted_names

        # ------------------------------------------------------------------
        # 7. Clean exit and alternate-screen restoration.
        # ------------------------------------------------------------------
        session.quit_via(CTRL_Q)
        session.assert_clean_exit()
        assert session.alternate_screen_entered, session.evidence_dir
        assert session.alternate_screen_left, (
            "the alternate screen was not restored on exit; "
            f"evidence={session.evidence_dir}"
        )
    finally:
        session.close()
