"""M30 I2 / M25 EC-004: connected-mode blueprint MCP writes are durable intents.

The real ``orbit-mcp`` binary runs over stdio JSON-RPC against an isolated
replica/outbox while the real ``orbit-server`` owns the database. Every covered
case is a write that used to POST straight at the authority:

1. an offline session queues the write and honestly reports ``pending`` with a
   command id (the server never sees a direct write);
2. reconnecting and re-submitting the same identity flushes the outbox and
   reports ``accepted`` with the server receipt; repeating it returns the same
   immutable receipt and does not apply the mutation again;
3. a second body under the same command id is refused as a conflict without a
   second mutation;
4. a command the authority already applied but whose acknowledgement the client
   never saw (lost ack) is replayed from the same identity: one receipt, one
   application;
5. a second command id for the same change is rejected by the server and the
   outbox reports a reviewable conflict instead of accepted.

Skips cleanly when the binaries are absent unless
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
import urllib.error
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

WRITE_TOOLS = (
    "blueprint.publish",
    "blueprint.install",
    "blueprint.bind",
    "blueprint.create_item",
    "blueprint.transition",
    "blueprint.properties",
    "blueprint.template",
)
READ_TOOLS = (
    "blueprint.list",
    "blueprint.show",
    "blueprint.binding",
    "blueprint.item",
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
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            raw = response.read()
            return json.loads(raw) if raw else None
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", "replace")
        raise AssertionError(
            f"{request.get_method()} {path} rejected: {error.code} {detail}"
        ) from error


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


class Deployment:
    def __init__(self, base, server_db, replica, env, project_id):
        self.base = base
        self.server_db = server_db
        self.replica = replica
        self.env = env
        self.device = env["ORBIT_DEVICE_ID"]
        self.token = env["ORBIT_DEVICE_TOKEN"]
        self.project_id = project_id

    def mcp_env(self, *, online: bool, mode: str = "developer") -> dict[str, str]:
        """One MCP session. ``online=False`` uses the documented offline pair
        (no device credentials) against the same durable replica."""
        env = dict(self.env)
        env.update(
            {
                "ORBIT_AUTHORITY_DB": str(self.replica),
                "ORBIT_AUTHORITY_SERVER": self.base,
                "ORBIT_MCP_MODE": mode,
                "ORBIT_MCP_AGENT_ID": "m30-blueprint-outbox",
                "ORBIT_MCP_SESSION_ID": "m30-blueprint-outbox",
                # Blueprint registry writes are workspace-scoped by contract.
                "ORBIT_MCP_ALLOW_UNSCOPED_EXECUTION": "true",
            }
        )
        if not online:
            env.pop("ORBIT_DEVICE_ID", None)
            env.pop("ORBIT_DEVICE_TOKEN", None)
        return env

    def blueprint_versions(self, blueprint_id: str) -> list:
        return _request(
            self.base,
            f"/v1/authoritative/blueprints?id={blueprint_id}",
            self.token,
            self.device,
        )

    def work_item(self, item_id: str) -> dict:
        return _request(
            self.base,
            f"/v1/authoritative/blueprints/items?id={item_id}",
            self.token,
            self.device,
        )

    def command_receipts(self, command_id: str) -> int:
        connection = sqlite3.connect(f"file:{self.server_db}?mode=ro", uri=True)
        try:
            return connection.execute(
                "SELECT COUNT(*) FROM orbit_blueprint_command_receipts"
                " WHERE command_id=?",
                (command_id,),
            ).fetchone()[0]
        finally:
            connection.close()


def _health(base) -> None:
    with urllib.request.urlopen(base + "/health", timeout=5) as response:
        response.read()


@pytest.fixture
def deployment(tmp_path):
    missing = [str(path) for path in (MCP_BINARY, SERVER_BINARY) if not path.is_file()]
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
    base_dir = tmp_path / "m30-blueprint-outbox"
    base_dir.mkdir()
    workspace = Workspace.create(base_dir)
    server_db = workspace.root / "server.db"
    replica = workspace.root / "replica.db"
    env = dict(workspace.env)
    env["ORBIT_DB"] = str(workspace.root / "unused-local.db")
    env["ORBIT_AUTHORITY_DB"] = str(replica)
    env["ORBIT_AUTHORITY_SERVER"] = base
    admin = secrets.token_hex(32)
    device = "m30-blueprint-device"
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
                raise AssertionError("server exited:\n" + log_path.read_text()[-4000:])
            try:
                _health(base)
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
        snapshot = _request(base, "/v1/authoritative/snapshot", token, device)
        _request(
            base,
            "/v1/authoritative/project-commands",
            token,
            device,
            {
                "command_id": "m30-blueprint-project",
                "expected_revision": snapshot["revision"],
                "project_id": "m30-blueprint-project",
                "name": "M30 Blueprint Project",
                # First binding wins, so the fixture binds at creation.
                "workflow_id": "development",
            },
        )
        deployment_env = {
            "ORBIT_AUTHORITY_SERVER": base,
            "ORBIT_AUTHORITY_DB": str(replica),
            "ORBIT_DEVICE_ID": device,
            "ORBIT_DEVICE_TOKEN": token,
            "ORBIT_AUTHORITY_DEVICE": device,
            "ORBIT_SYNC_HTTP_TOKEN": token,
            **workspace.env,
        }
        deployment_env["ORBIT_AUTHORITY_DB"] = str(replica)
        deployment_env["ORBIT_AUTHORITY_SERVER"] = base
        yield Deployment(base, server_db, replica, deployment_env, "m30-blueprint-project")
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        log.close()


def test_connected_blueprint_writes_are_queued_flushed_and_replayed(deployment):
    # The tool catalogue advertises the queue-first write contract.
    probe = McpClient(deployment.mcp_env(online=True))
    try:
        assert "result" in probe.request("initialize", {})
        tools = {
            tool["name"]: tool
            for tool in probe.request("tools/list", {})["result"]["tools"]
        }
        for name in WRITE_TOOLS:
            assert name in tools, f"{name} missing from the connected catalogue"
            properties = tools[name]["inputSchema"]["properties"]
            assert properties["command_id"]["type"] == "string", tools[name]
            assert "command_id" not in tools[name]["inputSchema"]["required"], tools[name]
        for name in READ_TOOLS:
            assert name in tools, f"{name} missing from the connected catalogue"
    finally:
        probe.close()

    # 1. Offline: the write is a durable intention, never a direct POST.
    offline = McpClient(deployment.mcp_env(online=False))
    try:
        assert "result" in offline.request("initialize", {})
        queued = offline.call(
            "blueprint.install",
            {"id": "learning", "command_id": "m30-bp-install-1"},
        )
        assert queued["disposition"] == "pending", queued
        assert queued["accepted"] is False, queued
        assert queued["pending"] is True, queued
        assert queued["command_id"] == "m30-bp-install-1", queued
        assert "receipt" not in queued, queued
        assert "result" not in queued, queued
        unresolved = offline.call("authority.status", {})
        assert unresolved["unresolved_count"] == 1, unresolved
    finally:
        offline.close()

    # Nothing reached the server while the session was offline. (`development`
    # is already present because the fixture's project binds it at creation.)
    assert deployment.blueprint_versions("learning") == []

    client = McpClient(deployment.mcp_env(online=True))
    try:
        assert "result" in client.request("initialize", {})
        # 2. The retry of the same identity replays the outbox and is accepted.
        accepted = client.call(
            "blueprint.install",
            {"id": "learning", "command_id": "m30-bp-install-1"},
        )
        assert accepted["disposition"] == "accepted", accepted
        assert accepted["accepted"] is True, accepted
        assert accepted["command_id"] == "m30-bp-install-1", accepted
        assert accepted["receipt"]["command_id"] == "m30-bp-install-1", accepted
        assert accepted["result"]["id"] == "learning", accepted
        assert deployment.command_receipts("m30-bp-install-1") == 1
        assert deployment.blueprint_versions("learning") == [["learning", 2]]
        assert client.call("authority.status", {})["unresolved_count"] == 0

        # Replaying the accepted identity returns the same immutable receipt.
        replay = client.call(
            "blueprint.install",
            {"id": "learning", "command_id": "m30-bp-install-1"},
        )
        assert replay == accepted, replay
        assert deployment.command_receipts("m30-bp-install-1") == 1

        # 2b. A project-scoped write is a queued intention too; items can only
        #     be created inside a project that is bound to a published version.
        bound = client.call(
            "blueprint.bind",
            {
                "project_id": deployment.project_id,
                "id": "development",
                "version": 2,
                "command_id": "m30-bp-bind-1",
            },
        )
        assert bound["disposition"] == "accepted", bound
        assert bound["result"]["blueprint_id"] == "development", bound
        assert deployment.command_receipts("m30-bp-bind-1") == 1
        binding = client.call(
            "blueprint.binding", {"project_id": deployment.project_id}
        )
        assert binding["blueprint_id"] == "development", binding
        assert binding["version"] == 2, binding

        # 3. The same identity with a different body must conflict, not mutate.
        conflict = client.call(
            "blueprint.install",
            {"id": "habits", "command_id": "m30-bp-install-1"},
        )
        assert conflict["disposition"] == "conflict", conflict
        assert conflict["accepted"] is False, conflict
        assert conflict["command_id"] == "m30-bp-install-1", conflict
        assert conflict["reason"], conflict
        assert deployment.blueprint_versions("habits") == []

        # 4. Lost acknowledgement: the authority already applied this identity
        #    (the first attempt), the client never saw the reply, and the retry
        #    receives the original receipt without a second mutation.
        item_body = {
            "project_id": deployment.project_id,
            "id": "m30-item-1",
            "kind": "feature",
            "properties": {"priority": "high"},
        }
        applied = _request(
            deployment.base,
            "/v1/authoritative/blueprints/commands",
            deployment.token,
            deployment.device,
            {"command_id": "m30-lost-ack", "route": "blueprints/items", "body": item_body},
        )
        assert applied["command_id"] == "m30-lost-ack", applied
        item = applied["response"]
        retried = client.call(
            "blueprint.create_item",
            {
                "project_id": deployment.project_id,
                "id": "m30-item-1",
                "kind": "feature",
                "properties": {"priority": "high"},
                "command_id": "m30-lost-ack",
            },
        )
        assert retried["disposition"] == "accepted", retried
        assert retried["receipt"] == applied, retried
        assert retried["result"] == item, retried
        assert deployment.command_receipts("m30-lost-ack") == 1
        unchanged = deployment.work_item("m30-item-1")
        assert unchanged["id"] == "m30-item-1", unchanged

        # 5. A second command id for the same change is rejected by the
        #    authority and surfaces as a reviewable conflict, not accepted.
        duplicate = client.call(
            "blueprint.create_item",
            {
                "project_id": deployment.project_id,
                "id": "m30-item-1",
                "kind": "feature",
                "properties": {"priority": "high"},
                "command_id": "m30-item-1-second-id",
            },
        )
        assert duplicate["disposition"] == "conflict", duplicate
        assert duplicate["reason"], duplicate
        assert duplicate["command_id"] == "m30-item-1-second-id", duplicate
        assert "receipt" not in duplicate, duplicate
        assert deployment.command_receipts("m30-item-1-second-id") == 0
        assert deployment.command_receipts("m30-lost-ack") == 1
        assert deployment.work_item("m30-item-1")["id"] == "m30-item-1"

        # Reads stay direct and never queue: they work with no pending work.
        assert client.call("authority.status", {})["unresolved_count"] == 1
        assert sorted(client.call("blueprint.list", {})) == [
            ["development", 2],
            ["learning", 2],
        ]
        assert client.call("authority.status", {})["unresolved_count"] == 1
    finally:
        client.close()


def test_connected_blueprint_write_policy_still_fails_closed(deployment):
    read_only = McpClient(deployment.mcp_env(online=False, mode="read_only"))
    try:
        assert "result" in read_only.request("initialize", {})
        denied, text = read_only.call_raw("blueprint.install", {"id": "development"})
        assert denied, "a read-only agent must not queue a blueprint write"
        assert "read-only" in text, text
        assert read_only.call("authority.status", {})["unresolved_count"] == 0
    finally:
        read_only.close()
