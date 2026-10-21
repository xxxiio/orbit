"""Connected-mode PTY acceptance against a disposable authoritative server.

This is intentionally separate from the local scenarios: it starts the real
Orbit server, enrolls a device, launches the same TUI binary in connected mode,
and verifies pending -> flush -> accepted behavior. It skips when the server
binary is absent.
"""
from __future__ import annotations

import json
import os
import secrets
import socket
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))

from pty_harness import CTRL_Q, ENTER, TuiSession, Workspace, strip_ansi  # noqa: E402

ROOT = Path(__file__).resolve().parents[3]
SERVER = Path(os.environ.get("ORBIT_SERVER_BIN", ROOT / "target" / "debug" / "orbit-server"))
TUI = Path(os.environ.get("ORBIT_TUI_BIN", ROOT / "target" / "debug" / "orbit-tui"))


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
        base + path, data=body, headers=headers, method="POST" if body is not None else "GET"
    )
    with urllib.request.urlopen(request, timeout=8) as response:
        raw = response.read()
        return json.loads(raw) if raw else None


@pytest.fixture
def connected_workspace(tmp_path):
    if not SERVER.is_file() or not TUI.is_file():
        pytest.skip("orbit-server/orbit-tui binaries are not built")
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
    log = (root / "server.log").open("w")
    process = subprocess.Popen(
        [str(SERVER)],
        env={
            **env,
            "ORBIT_SERVER_DB": str(server_db),
            "ORBIT_SERVER_BIND": f"127.0.0.1:{port}",
            "ORBIT_SERVER_ADMIN_KEY": admin,
        },
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    try:
        for _ in range(100):
            if process.poll() is not None:
                raise AssertionError("server exited:\n" + (root / "server.log").read_text()[-3000:])
            try:
                _request(base, "/health")
                break
            except Exception:
                time.sleep(0.1)
        else:
            raise AssertionError("server readiness timeout")
        _request(
            base,
            "/v1/devices",
            token=admin,
            payload={"device_id": device, "token": token},
        )
        workspace = Workspace(
            root=root,
            db=replica,
            config=config,
            home=home,
            env=env,
        )
        yield workspace, base, admin, device, token
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        log.close()


def test_connected_shell_pending_flush_and_acceptance(connected_workspace):
    workspace, base, admin, device, token = connected_workspace
    session = TuiSession(workspace, cols=120, rows=35, label="connected")
    try:
        session.wait_for_text("ORBIT", timeout=20.0)
        session.wait_for_screen_text("CONNECTED", timeout=8.0, description="connected header")
        session.send(b"gp", note="Projects")
        session.send(b"c", note="new project form")
        session.wait_for_screen_text("New project", timeout=8.0, description="project form")
        session.send_text("Connected Project")
        session.send(ENTER, note="queue project")
        session.wait_for_text("pending", timeout=8.0, description="pending project")
        assert "Connected Project" not in session.screen_text or "pending" in session.screen_text.lower()

        session.send(b"f", note="flush outbox")
        session.wait_for_text("accepted", timeout=10.0, description="server acceptance")
        session.send(b"r", note="refresh accepted")
        session.send(b"gp", note="Projects after acceptance")
        session.wait_for_screen_text("Connected Project", timeout=10.0, description="accepted project")

        session.send(ENTER, note="open accepted project")
        session.wait_for_screen_regex(
            r"ORBIT › Connected Project", timeout=10.0, description="connected project opens"
        )
        session.send(b"c", note="new task form")
        session.wait_for_screen_text("New task", timeout=8.0, description="task form")
        session.send_text("Connected task")
        for _ in range(3):
            session.send(b"\t", note="focus earliest")
        session.send_text("1000")
        session.send(b"\t", note="focus estimate")
        session.send_text("30")
        session.send(ENTER, note="queue task")
        session.wait_for_text("pending", timeout=8.0, description="pending task")
        session.send(b"f", note="flush task")
        session.wait_for_text("accepted", timeout=10.0, description="task accepted")
        session.send(b"r", note="refresh task")
        session.send(b"]", note="Work view")
        session.wait_for_screen_text("Connected task", timeout=10.0, description="accepted task")

        session.quit_via(CTRL_Q)
        session.assert_clean_exit()

        snapshot = _request(
            base,
            "/v1/authoritative/snapshot",
            token=token,
            device=device,
        )
        assert snapshot is not None
        tasks = snapshot.get("tasks") or []
        task = next(row for row in tasks if row.get("title") == "Connected task")
        assert task.get("estimated_minutes") == 30, task
        assert task.get("earliest_start_at_ms") == 1000, task
    finally:
        session.close()
