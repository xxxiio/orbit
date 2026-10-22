"""Scenario B: an external MCP client manages a Basic project without Orbit
managing the agent.

Spawns the real ``orbit-mcp`` binary over stdio JSON-RPC (local authoritative
mode), performs the full task workflow, then inspects the same workspace in the
real TUI. No Herdr/agent job may be created implicitly.
"""
from __future__ import annotations

import base64
import json
import os
import sqlite3
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tui"))

from pty_harness import ENTER, TuiSession, Workspace  # noqa: E402

ROOT = Path(__file__).resolve().parents[3]
MCP_BINARY = Path(os.environ.get("ORBIT_MCP_BIN", ROOT / "target/debug/orbit-mcp"))


class McpClient:
    def __init__(self, env: dict[str, str]):
        if not MCP_BINARY.is_file():
            pytest.skip(f"orbit-mcp binary missing at {MCP_BINARY}")
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
        assert line, f"orbit-mcp closed unexpectedly: {self.process.stderr.read() if self.process.stderr else ''}"
        return json.loads(line)

    def call(self, name: str, arguments: dict) -> dict:
        response = self.request(
            "tools/call", {"name": name, "arguments": arguments}
        )
        result = response["result"]
        text = result["content"][0]["text"]
        if result.get("isError"):
            raise AssertionError(f"{name} failed: {text}")
        return json.loads(text)

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()


def find_key(value, key: str):
    if isinstance(value, dict):
        if key in value:
            return value[key]
        for item in value.values():
            found = find_key(item, key)
            if found is not None:
                return found
    elif isinstance(value, list):
        for item in value:
            found = find_key(item, key)
            if found is not None:
                return found
    return None


@pytest.fixture
def workspace(tmp_path):
    ws = Workspace.create(tmp_path)
    base = dict(ws.env)
    base.update(
        {
            "ORBIT_MCP_MODE": "developer",
            "ORBIT_MCP_AGENT_ID": "codex-acceptance",
            "ORBIT_MCP_SESSION_ID": "scenario-b",
        }
    )
    return base, ws


def test_external_agent_manages_basic_tasks_through_mcp(workspace):
    env, workspace = workspace
    client = McpClient(env)
    try:
        initialized = client.request("initialize", {})
        assert "result" in initialized, initialized

        tools = client.request("tools/list", {})["result"]["tools"]
        names = {tool["name"] for tool in tools}
        for required in (
            "task.get",
            "task.update",
            "task.acceptance.add",
            "task.acceptance.check",
            "task.activity",
            "project.create",
            "workflow.list",
            "attachment.add",
            "attachment.read",
        ):
            assert required in names, f"{required} missing from MCP catalog"

        project = client.call("project.create", {"name": "Agent Project"})
        project_id = project.get("id") or find_key(project, "project_id")
        assert project_id, project

        workflow = client.call("workflow.list", {"project_id": project_id})
        assert workflow["binding"]["workflow_id"] == "basic", workflow
        assert any(item.get("default") for item in workflow["workflows"]), workflow

        created = client.call(
            "task.create", {"title": "Agent task", "project_id": project_id}
        )
        task_id = created["id"]

        client.call(
            "task.update",
            {
                "id": task_id,
                "description": "# Plan\nImplement the change",
                "priority": "high",
                "labels": ["agent", "api"],
            },
        )
        first = client.call(
            "task.acceptance.add",
            {"task_id": task_id, "text": "Works offline"},
        )
        criterion_id = first["criteria"][0]["id"]
        client.call(
            "task.acceptance.add",
            {"task_id": task_id, "text": "Has a test"},
        )
        checked = client.call(
            "task.acceptance.check",
            {"task_id": task_id, "criterion_id": criterion_id},
        )
        assert checked["criteria"][0]["completed"] is True

        evidence = workspace.root / "agent-evidence.log"
        evidence.write_text("agent evidence", encoding="utf-8")
        attachment = client.call(
            "attachment.add",
            {
                "owner_kind": "task",
                "owner_id": task_id,
                "path": str(evidence),
                "description": "external agent evidence",
            },
        )
        attachment_id = attachment["id"]
        listed = client.call(
            "attachment.list", {"owner_kind": "task", "owner_id": task_id}
        )
        assert listed["attachments"][0]["id"] == attachment_id
        read = client.call("attachment.read", {"id": attachment_id})
        assert base64.b64decode(read["content"]) == b"agent evidence"

        client.call("task.set_status", {"id": task_id, "status": "ready"})
        client.call("task.set_status", {"id": task_id, "status": "in_progress"})
        client.call("task.set_status", {"id": task_id, "status": "done"})

        task = client.call("task.get", {"id": task_id})
        assert task["status"] == "done", task
        assert task["description"].startswith("# Plan"), task
        assert task["priority"] == "high", task
        assert task["labels"] == ["agent", "api"], task

        activity = client.call("task.activity", {"task_id": task_id})
        kinds = {item["kind"] for item in activity}
        assert "attachment_added" in kinds, kinds
        assert "status_changed" in kinds, kinds
        assert "updated" in kinds, kinds
    finally:
        client.close()

    # Nothing in this flow may create a managed agent job or Herdr execution.
    connection = sqlite3.connect(workspace.db)
    try:
        jobs = connection.execute(
            "SELECT COUNT(*) FROM agent_jobs WHERE task_id=?1", (task_id,)
        ).fetchone()[0]
        status = connection.execute(
            "SELECT status FROM tasks WHERE id=?1", (task_id,)
        ).fetchone()[0]
    finally:
        connection.close()
    assert jobs == 0, "MCP task management must not create agent jobs"
    assert status == "done"

    # Inspect the same accepted state in the real TUI.
    session = TuiSession(workspace, cols=120, rows=35, label="scenario-b-tui")
    try:
        session.wait_for_text("ORBIT", timeout=20.0)
        session.send_keys(b"g", b"p", delay=0.15)
        session.wait_for_screen_text("Agent Project", timeout=10.0, description="project list")
        session.send(ENTER, note="open agent project")
        session.wait_for_screen_text("Agent task", timeout=10.0, description="agent task visible")
        session.wait_for_screen_text("Basic", timeout=8.0, description="basic workflow")
        session.quit_via(b"\x11")
        session.assert_clean_exit()
    finally:
        session.close()
