"""Connected-mode MCP acceptance against a disposable authoritative server.

The real ``orbit-mcp`` binary runs over stdio JSON-RPC against a replica/outbox
while the authoritative server owns the database. Mutations stay pending until
``authority.flush``; rich reads come from the live per-task routes. A second
read-only session proves connected reads work while writes are denied.

Skips when the binaries are absent unless
``ORBIT_REQUIRE_MCP_EVIDENCE=1`` is set, in which case a missing binary fails
instead of recording a vacuous pass.
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

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tui"))

from pty_harness import Workspace  # noqa: E402

ROOT = Path(__file__).resolve().parents[3]
MCP_BINARY = Path(os.environ.get("ORBIT_MCP_BIN", ROOT / "target" / "debug" / "orbit-mcp"))
SERVER_BINARY = Path(
    os.environ.get("ORBIT_SERVER_BIN", ROOT / "target" / "debug" / "orbit-server")
)
REQUIRE_EVIDENCE = os.environ.get("ORBIT_REQUIRE_MCP_EVIDENCE") == "1"

REQUIRED_TOOLS = (
    "task.get",
    "task.update",
    "task.acceptance.list",
    "task.acceptance.add",
    "task.acceptance.update",
    "task.acceptance.check",
    "task.acceptance.reorder",
    "task.acceptance.remove",
    "task.activity",
)


def _request(base, path, token=None, device=None, payload=None):
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
    with urllib.request.urlopen(request, timeout=10) as response:
        raw = response.read()
        return json.loads(raw) if raw else None


class McpClient:
    """Minimal stdio JSON-RPC client for the real orbit-mcp binary."""

    def __init__(self, env: dict[str, str]):
        self.process = subprocess.Popen(
            [str(MCP_BINARY)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            text=True,
            bufsize=1,
        )
        self.next_id = 1

    def request(self, method: str, params: dict | None = None) -> dict:
        payload = {"jsonrpc": "2.0", "id": self.next_id, "method": method}
        if params is not None:
            payload["params"] = params
        self.next_id += 1
        assert self.process.stdin is not None
        self.process.stdin.write(json.dumps(payload) + "\n")
        self.process.stdin.flush()
        assert self.process.stdout is not None
        line = self.process.stdout.readline()
        assert line, (
            "orbit-mcp closed unexpectedly: "
            + (self.process.stderr.read() if self.process.stderr else "")
        )
        return json.loads(line)

    def call_raw(self, name: str, arguments: dict) -> tuple[bool, str]:
        response = self.request("tools/call", {"name": name, "arguments": arguments})
        result = response["result"]
        text = result["content"][0]["text"]
        return bool(result.get("isError")), text

    def call(self, name: str, arguments: dict) -> dict:
        is_error, text = self.call_raw(name, arguments)
        if is_error:
            raise AssertionError(f"{name} failed: {text}")
        return json.loads(text)

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()


class ConnectedDeployment:
    def __init__(self, base, server_db, replica, env, device, token):
        self.base = base
        self.server_db = server_db
        self.replica = replica
        self.env = env
        self.device = device
        self.token = token
        self.project_id = "project-connected-mcp"
        self.task_id = "task-connected-mcp"

    def mcp_env(self, mode: str) -> dict[str, str]:
        env = dict(self.env)
        env.update(
            {
                "ORBIT_AUTHORITY_DB": str(self.replica),
                "ORBIT_AUTHORITY_SERVER": self.base,
                # MCP connected credentials (documented pair) plus the CLI/TUI
                # names the same deployment uses.
                "ORBIT_DEVICE_ID": self.device,
                "ORBIT_DEVICE_TOKEN": self.token,
                "ORBIT_AUTHORITY_DEVICE": self.device,
                "ORBIT_SYNC_HTTP_TOKEN": self.token,
                "ORBIT_MCP_MODE": mode,
                "ORBIT_MCP_AGENT_ID": "connected-acceptance",
                "ORBIT_MCP_SESSION_ID": "m28-connected",
            }
        )
        return env


def _seed_owner(base, token, device):
    """Create and accept a project and task through authenticated commands."""
    snapshot = _request(base, "/v1/authoritative/snapshot", token, device)
    revision = snapshot["revision"]
    project = _request(
        base,
        "/v1/authoritative/project-commands",
        token,
        device,
        {
            "command_id": "connected-mcp-project",
            "expected_revision": revision,
            "project_id": "project-connected-mcp",
            "name": "Connected MCP Project",
        },
    )
    task = _request(
        base,
        "/v1/authoritative/task-commands",
        token,
        device,
        {
            "command_id": "connected-mcp-task",
            "expected_revision": project["revision"],
            "action": {
                "type": "create",
                "task_id": "task-connected-mcp",
                "title": "Connected MCP task",
                "project_id": "project-connected-mcp",
            },
        },
    )
    assert task["task"]["id"] == "task-connected-mcp", task
    details = _request(
        base,
        "/v1/authoritative/task-commands",
        token,
        device,
        {
            "command_id": "connected-mcp-task-details",
            "expected_revision": task["revision"],
            "action": {
                "type": "set_details",
                "task_id": "task-connected-mcp",
                "description": "Original description",
                "priority": None,
                "labels": [],
            },
        },
    )
    assert details["task"]["description"] == "Original description", details


@pytest.fixture
def deployment(tmp_path):
    missing = [
        str(path)
        for path in (MCP_BINARY, SERVER_BINARY)
        if not path.is_file()
    ]
    if missing:
        message = "required binaries are not built: " + ", ".join(missing)
        if REQUIRE_EVIDENCE:
            pytest.fail(f"{message} (ORBIT_REQUIRE_MCP_EVIDENCE=1 forbids a skipped run)")
        pytest.skip(message)

    port_socket = socket.socket()
    port_socket.bind(("127.0.0.1", 0))
    port = port_socket.getsockname()[1]
    port_socket.close()
    base = f"http://127.0.0.1:{port}"
    base_dir = tmp_path / "connected-mcp"
    base_dir.mkdir()
    workspace = Workspace.create(base_dir)
    server_db = workspace.root / "server.db"
    replica = workspace.root / "replica.db"
    env = dict(workspace.env)
    env["ORBIT_DB"] = str(workspace.root / "unused-local.db")
    env["ORBIT_AUTHORITY_DB"] = str(replica)
    env["ORBIT_AUTHORITY_SERVER"] = base
    admin = secrets.token_hex(32)
    device = "connected-mcp-device"
    token = secrets.token_hex(32)
    log_path = workspace.root / "server.log"
    log = log_path.open("w")
    process = subprocess.Popen(
        [str(SERVER_BINARY)],
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
        for _ in range(150):
            if process.poll() is not None:
                raise AssertionError(
                    "server exited:\n" + log_path.read_text()[-4000:]
                )
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
        _seed_owner(base, token, device)
        yield ConnectedDeployment(base, server_db, replica, env, device, token)
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        log.close()


def _snapshot(deployment):
    return _request(
        deployment.base,
        "/v1/authoritative/snapshot",
        deployment.token,
        deployment.device,
    )


def test_connected_mcp_pending_flush_and_rich_task_workflow(deployment):
    developer = McpClient(deployment.mcp_env("developer"))
    try:
        assert "result" in developer.request("initialize", {})

        tools = developer.request("tools/list", {})["result"]["tools"]
        names = {tool["name"] for tool in tools}
        for required in REQUIRED_TOOLS:
            assert required in names, f"{required} missing from connected MCP catalog"
        assert not any(name.startswith("attachment.") for name in names), sorted(
            name for name in names if name.startswith("attachment.")
        )

        refreshed = developer.call("authority.refresh", {})
        assert refreshed["has_accepted_snapshot"] is True, refreshed

        # 1. Rich details are queued, never reported as accepted before flush.
        update = developer.call(
            "task.update",
            {
                "id": deployment.task_id,
                "description": "Connected body",
                "priority": "high",
                "labels": ["connected", "mcp"],
            },
        )
        assert update["accepted"] is False, update
        assert update["pending"] is True, update
        assert update["disposition"] == "pending", update
        assert update["server_state"]["description"] == "Original description", update
        assert "message" in update and "authority.flush" in update["message"], update

        # 2. A criterion is queued; the accepted list does not contain it yet.
        added = developer.call(
            "task.acceptance.add",
            {"task_id": deployment.task_id, "text": "Connected criterion"},
        )
        assert added["pending"] is True, added
        assert added["disposition"] == "pending", added
        assert added["server_state"]["criteria"] == [], added

        assert developer.call("authority.status", {})["unresolved_count"] == 2
        flushed = developer.call("authority.flush", {})
        assert "Empty" not in flushed["outcome"], flushed
        assert developer.call("authority.status", {})["unresolved_count"] == 0

        # 3. After flush + refresh the accepted details and criterion are real.
        developer.call("authority.refresh", {})
        task = developer.call("task.get", {"id": deployment.task_id})
        assert task["description"] == "Connected body", task
        assert task["priority"] == "high", task
        assert task["labels"] == ["connected", "mcp"], task
        assert task["criteria_available"] is True, task
        criteria = task["acceptance_criteria"]
        assert [row["text"] for row in criteria] == ["Connected criterion"], task
        criterion_id = criteria[0]["id"]

        listed = developer.call(
            "task.acceptance.list", {"task_id": deployment.task_id}
        )
        assert [row["id"] for row in listed["criteria"]] == [criterion_id], listed

        # 4. Checking a criterion is its own pending mutation.
        checked = developer.call(
            "task.acceptance.check",
            {
                "task_id": deployment.task_id,
                "criterion_id": criterion_id,
                "completed": True,
            },
        )
        assert checked["pending"] is True, checked
        assert checked["server_state"]["criteria"][0]["completed"] is False, checked
        developer.call("authority.flush", {})
        developer.call("authority.refresh", {})
        after_check = developer.call("task.get", {"id": deployment.task_id})
        assert after_check["acceptance_criteria"][0]["completed"] is True, after_check

        # 5. A later mutation's server_state reports the previously accepted edit.
        second = developer.call(
            "task.update", {"id": deployment.task_id, "priority": "urgent"}
        )
        assert second["pending"] is True, second
        assert second["server_state"]["description"] == "Connected body", second
        assert second["server_state"]["priority"] == "high", second
        developer.call("authority.flush", {})
        developer.call("authority.refresh", {})

        # 6. Drive a status transition to done through the canonical command.
        # Inbox cannot jump straight to in_progress, so walk the domain path.
        for status in ("ready", "in_progress", "done"):
            revision = developer.call("authority.status", {})["accepted_revision"]
            receipt = developer.call(
                "task.command",
                {
                    "command_id": f"connected-mcp-status-{status}",
                    "expected_revision": revision,
                    "action": {
                        "type": "change_status",
                        "task_id": deployment.task_id,
                        "status": status,
                    },
                },
            )
            assert receipt["disposition"] in ("pending", "accepted"), receipt
            flushed = developer.call("authority.flush", {})
            assert "Blocked" not in flushed["outcome"], flushed
            developer.call("authority.refresh", {})

        final = developer.call("task.get", {"id": deployment.task_id})
        assert final["status"] == "done", final
        assert final["priority"] == "urgent", final

        activity = developer.call(
            "task.activity", {"task_id": deployment.task_id, "limit": 100}
        )
        kinds = {row["kind"] for row in activity}
        assert "created" in kinds, kinds
        assert "status_changed" in kinds, kinds
        assert "updated" in kinds, kinds

        server_task = next(
            row
            for row in _snapshot(deployment)["tasks"]
            if row["id"] == deployment.task_id
        )
        assert server_task["status"] == "done", server_task
        assert server_task["priority"] == "urgent", server_task
    finally:
        developer.close()

    # 7. A read-only session reads the same accepted state but cannot write.
    read_only = McpClient(deployment.mcp_env("read_only"))
    try:
        assert "result" in read_only.request("initialize", {})
        read_only.call("authority.refresh", {})
        task = read_only.call("task.get", {"id": deployment.task_id})
        assert task["description"] == "Connected body", task
        listed = read_only.call(
            "task.acceptance.list", {"task_id": deployment.task_id}
        )
        assert listed["criteria"][0]["completed"] is True, listed
        activity = read_only.call(
            "task.activity", {"task_id": deployment.task_id}
        )
        assert any(row["kind"] == "status_changed" for row in activity), activity

        denied, text = read_only.call_raw(
            "task.update", {"id": deployment.task_id, "description": "not allowed"}
        )
        assert denied, "read-only task.update must be denied"
        assert "read-only" in text, text
    finally:
        read_only.close()

    # 8. No agent job or Herdr-managed execution may be created implicitly.
    connection = sqlite3.connect(f"file:{deployment.server_db}?mode=ro", uri=True)
    try:
        jobs = connection.execute("SELECT COUNT(*) FROM agent_jobs").fetchone()[0]
    finally:
        connection.close()
    assert jobs == 0, "connected MCP task work must not create agent jobs"
