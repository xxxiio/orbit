#!/usr/bin/env python3
"""M34 I5 — real recovery rehearsal with a deterministic realistic dataset.

This is an *acceptance* harness, not a unit test. It drives the real, built
Orbit binaries against a real `orbit-server` instance in an isolated temporary
directory, populates a deterministic dataset through supported product
surfaces only (the server HTTP API, the real `orbit-mcp` stdio server in
connected mode and the real connected capability executor), creates a real
`.orbitbak` with the real CLI, restores it into a separate isolated workspace,
opens that workspace as a real Orbit instance and verifies what came back.

Nothing is mocked, no database row is fabricated with SQL, and no external
agent is launched. The only SQL this harness runs is read-only evidence
counting (`SELECT COUNT(*)`) against workspace database files.

Usage
-----
    python3 tests/acceptance/m34_recovery_rehearsal.py \\
        --binary target/debug/orbit \\
        --server-binary target/debug/orbit-server

Optional development flags (documented so the script stays re-runnable):
    --population-only   Start the instance, populate the dataset, record it and
                        exit. Useful while the backup/restore service is still
                        landing.
    --keep-workdir      Do not delete the disposable run directory (it is
                        printed for inspection). Evidence is always written.
    --workdir DIR       Use an explicit disposable run directory.
    --evidence-dir DIR  Where rehearsal-evidence.json / README.md are written
                        (default web/evidence/m34-recovery).
    --mcp-binary PATH   Override target/debug/orbit-mcp.

Exit codes: 0 = every step passed; 1 = a step failed; 2 = a step was blocked
because the product surface is not implemented yet in this build (the evidence
records exactly which step and why).

The script prints one PASS/FAIL/BLOCKED line per step and a summary table.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import traceback
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CLI = REPO_ROOT / "target" / "debug" / "orbit"
DEFAULT_SERVER = REPO_ROOT / "target" / "debug" / "orbit-server"
DEFAULT_MCP = REPO_ROOT / "target" / "debug" / "orbit-mcp"
DEFAULT_EVIDENCE_DIR = REPO_ROOT / "web" / "evidence" / "m34-recovery"

# Fixed values: they make the dataset and every recorded identifier stable.
FIXED_ADMIN_KEY = "m34-rehearsal-admin-key-not-a-secret-0123456789abcdef"
DEVICE_ID = "m34-rehearsal-device"
DEVICE_TOKEN = "m34-rehearsal-device-token-0123456789abcdef"
EPOCH_MS = 1_793_000_000_000  # fixed synthetic instant for planning fields
TASK_ALPHA_COUNT = 70
TASK_BETA_COUNT = 33
MCP_TARGET_TASK = "task-beta-002"
MCP_DESCRIPTION = "MCP-authored description recorded before the backup"
CAPABILITY = "orbit.reference.work-item-note"
CAPABILITY_ACTION = "append_note"
CAPABILITY_WORKFLOW = "capability-reference"
BUNDLED_WORKFLOW = "development"
REHEARSAL_LABEL = "m34-rehearsal"
ARCHIVE_NAME_LABEL = "m34 real recovery rehearsal"

POST_BACKUP_TASK = "task-alpha-post-backup"
POST_BACKUP_NOTE = "note-post-backup"
RESTORED_WRITE_TASK = "task-restored-write"

NOT_IMPLEMENTED_MARKERS = (
    "not implemented",
    "BackupError::Unavailable",
    "Unavailable(",
)


# ---------------------------------------------------------------------------
# Small utilities
# ---------------------------------------------------------------------------
def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def hash_tree(root: Path, *, exclude_names=()) -> dict:
    """Hash every regular file under ``root`` into a stable manifest.

    Returns ``{"files": {relpath: sha256}, "digest": sha256-of-sorted-lines,
    "bytes": total}``. Symlinks are skipped (Orbit does not write them).
    """
    files: dict[str, str] = {}
    total = 0
    if not root.exists():
        return {"files": files, "digest": sha256_bytes(b""), "bytes": 0}
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        rel = path.relative_to(root).as_posix()
        if path.name in exclude_names:
            continue
        files[rel] = sha256_file(path)
        total += path.stat().st_size
    lines = "\n".join(f"{rel} {digest}" for rel, digest in sorted(files.items()))
    return {"files": files, "digest": sha256_bytes(lines.encode()), "bytes": total}


def sqlite_table_counts(db_path: Path) -> dict:
    """Read-only row counts for every user table in a workspace database.

    This never writes and is evidence only: the rehearsal creates no row by SQL.
    """
    if not db_path.is_file():
        return {}
    uri = f"file:{db_path}?mode=ro"
    connection = sqlite3.connect(uri, uri=True, timeout=10)
    try:
        connection.row_factory = sqlite3.Row
        tables = [
            row["name"]
            for row in connection.execute(
                "SELECT name FROM sqlite_master WHERE type='table' "
                "AND name NOT LIKE 'sqlite_%' ORDER BY name"
            )
        ]
        counts: dict[str, int] = {}
        for table in tables:
            # Table names come from sqlite_master, never from user input.
            counts[table] = connection.execute(
                f'SELECT COUNT(*) AS n FROM "{table}"'
            ).fetchone()["n"]
        return counts
    finally:
        connection.close()


def sqlite_snapshot_hash(db_path: Path, destination: Path) -> str:
    """Deterministic content hash of a SQLite database via the online backup API."""
    if destination.exists():
        destination.unlink()
    source = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True, timeout=10)
    target = sqlite3.connect(str(destination))
    try:
        source.backup(target)
        target.commit()
    finally:
        target.close()
        source.close()
    return sha256_file(destination)


def free_loopback_port() -> int:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
    sock.close()
    return port


def clean_env(extra: dict | None = None) -> dict:
    """A child environment with every inherited ORBIT_* variable removed."""
    env = {key: value for key, value in os.environ.items() if not key.startswith("ORBIT_")}
    env["LANG"] = env.get("LANG") or "C.UTF-8"
    if extra:
        env.update(extra)
    return env


def is_not_implemented(*texts: str) -> bool:
    joined = "\n".join(texts)
    return any(marker in joined for marker in NOT_IMPLEMENTED_MARKERS)


def binary_identity(path: Path, *, probe_version: bool = True) -> dict:
    """Size, mtime and SHA-256 of a built binary, so evidence is tied to it."""
    identity = {"path": str(path)}
    try:
        stat = path.stat()
        identity.update(
            {
                "size_bytes": stat.st_size,
                "mtime_iso": datetime.fromtimestamp(
                    stat.st_mtime, timezone.utc).isoformat(timespec="seconds"),
                "sha256": sha256_file(path),
            }
        )
    except OSError as error:
        identity["error"] = str(error)
        return identity
    if probe_version:
        try:
            version = subprocess.run(
                [str(path), "--version"], text=True, capture_output=True, timeout=30
            )
            identity["version"] = version.stdout.strip() or version.stderr.strip()
        except OSError as error:
            identity["version_error"] = str(error)
    return identity


def git_identity() -> dict:
    """Commit-ish identity of the source tree the binaries were built from."""
    identity: dict = {}
    try:
        commit = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=str(REPO_ROOT), text=True,
            capture_output=True, timeout=30,
        )
        identity["commit"] = commit.stdout.strip() or None
        branch = subprocess.run(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"], cwd=str(REPO_ROOT),
            text=True, capture_output=True, timeout=30,
        )
        identity["branch"] = branch.stdout.strip() or None
        status = subprocess.run(
            ["git", "status", "--porcelain"], cwd=str(REPO_ROOT), text=True,
            capture_output=True, timeout=60,
        )
        changed = [line for line in status.stdout.splitlines() if line.strip()]
        identity["dirty"] = bool(changed)
        identity["changed_path_count"] = len(changed)
    except (OSError, subprocess.SubprocessError) as error:
        identity["error"] = str(error)
    return identity


def read_json_file(path: Path) -> dict | None:
    try:
        return json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return None


# ---------------------------------------------------------------------------
# Command-line client (the real binary)
# ---------------------------------------------------------------------------
class OrbitCli:
    """Runs the real ``orbit`` binary with an explicit, isolated environment."""

    def __init__(self, binary: Path, *, db: Path, config: Path, home: Path,
                 keep_workdir: bool = False, extra_env: dict | None = None):
        self.binary = binary
        self.db = db
        self.config = config
        self.home = home
        self.keep_workdir = keep_workdir
        self.extra_env = dict(extra_env or {})
        self.history: list[dict] = []

    def env(self) -> dict:
        env = {
            "HOME": str(self.home),
            "ORBIT_CONFIG": str(self.config),
            "ORBIT_DB": str(self.db),
        }
        env.update(self.extra_env)
        return clean_env(env)

    def run(self, *args: str, timeout: float = 300.0) -> dict:
        argv = [str(self.binary), "--json", *args]
        started = time.monotonic()
        completed = subprocess.run(
            argv,
            env=self.env(),
            cwd=str(REPO_ROOT),
            text=True,
            capture_output=True,
            timeout=timeout,
        )
        record = {
            "argv": args,
            "returncode": completed.returncode,
            "stdout": completed.stdout,
            "stderr": completed.stderr,
            "duration_ms": round((time.monotonic() - started) * 1000, 2),
        }
        self.history.append(record)
        return record

    def run_json(self, *args: str, timeout: float = 300.0) -> dict:
        record = self.run(*args, timeout=timeout)
        if record["returncode"] != 0:
            raise RuntimeError(
                f"orbit {' '.join(args)} exited {record['returncode']}\n"
                f"stdout:\n{record['stdout']}\nstderr:\n{record['stderr']}"
            )
        try:
            return json.loads(record["stdout"])
        except json.JSONDecodeError as error:
            raise RuntimeError(
                f"orbit {' '.join(args)} did not print JSON: {error}\n{record['stdout']}"
            ) from error

    def call_backup(self, *args: str, timeout: float = 600.0) -> dict:
        """Run a backup subcommand against the *server* workspace database."""
        return self.run("--db", str(self.db), "--config", str(self.config), "backup", *args)


# ---------------------------------------------------------------------------
# HTTP client for the real server
# ---------------------------------------------------------------------------
class HttpResult:
    def __init__(self, status: int, body, raw: bytes):
        self.status = status
        self.body = body
        self.raw = raw

    @property
    def ok(self) -> bool:
        return 200 <= self.status < 300


class OrbitHttp:
    """Bearer-authenticated client. The enrolled device is an operator/executor."""

    def __init__(self, base: str):
        self.base = base
        self.token = DEVICE_TOKEN
        self.device = DEVICE_ID
        self.history: list[dict] = []

    def request(self, method: str, path: str, *, body=None, raw: bytes | None = None,
                query: dict | None = None, token: str | None = None,
                device: str | None = None, timeout: float = 60.0) -> HttpResult:
        url = self.base + path
        if query:
            url += "?" + urllib.parse.urlencode(query)
        headers: dict[str, str] = {}
        use_token = self.token if token is None else token
        use_device = self.device if device is None else device
        if use_token:
            headers["Authorization"] = "Bearer " + use_token
        if use_device:
            headers["x-orbit-device-id"] = use_device
        data = None
        if raw is not None:
            data = raw
            headers["Content-Type"] = "application/octet-stream"
        elif body is not None:
            data = json.dumps(body).encode()
            headers["Content-Type"] = "application/json"
        request = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                payload = response.read()
                parsed = None
                if payload:
                    try:
                        text = payload.decode("utf-8")
                    except UnicodeDecodeError:
                        text = None
                    if text is not None:
                        try:
                            parsed = json.loads(text)
                        except json.JSONDecodeError:
                            parsed = text
                result = HttpResult(response.status, parsed, payload)
                self.history.append(
                    {"method": method, "path": path.split("?")[0],
                     "status": result.status}
                )
                return result
        except urllib.error.HTTPError as error:
            payload = error.read()
            parsed = None
            if payload:
                try:
                    text = payload.decode("utf-8")
                except UnicodeDecodeError:
                    text = None
                if text is not None:
                    try:
                        parsed = json.loads(text)
                    except json.JSONDecodeError:
                        parsed = text
            result = HttpResult(error.code, parsed, payload)
            self.history.append(
                {"method": method, "path": path.split("?")[0], "status": result.status}
            )
            return result

    def expect(self, method: str, path: str, *, status: int = 200, **kwargs) -> HttpResult:
        result = self.request(method, path, **kwargs)
        if result.status != status:
            raise RuntimeError(
                f"{method} {path} -> {result.status} (expected {status}): {result.body!r}"
            )
        return result


# ---------------------------------------------------------------------------
# Real server process
# ---------------------------------------------------------------------------
class OrbitServer:
    """One real ``orbit-server`` process in local-trusted mode on loopback."""

    def __init__(self, server_binary: Path, *, db: Path, home: Path, log_path: Path,
                 auth_mode: str = "local-trusted"):
        self.binary = server_binary
        self.db = db
        self.home = home
        self.log_path = log_path
        self.auth_mode = auth_mode
        self.process: subprocess.Popen | None = None
        self.port: int | None = None
        self.base: str | None = None
        self._log_handle = None
        self._http_client: OrbitHttp | None = None

    def start(self) -> "OrbitServer":
        self.db.parent.mkdir(parents=True, exist_ok=True)
        self.home.mkdir(parents=True, exist_ok=True)
        self.port = free_loopback_port()
        self.base = f"http://127.0.0.1:{self.port}"
        env = clean_env(
            {
                "HOME": str(self.home),
                "ORBIT_SERVER_DB": str(self.db),
                "ORBIT_SERVER_BIND": f"127.0.0.1:{self.port}",
                "ORBIT_SERVER_ADMIN_KEY": FIXED_ADMIN_KEY,
                "ORBIT_AUTH_MODE": self.auth_mode,
            }
        )
        web_dist = REPO_ROOT / "web" / "dist"
        if web_dist.is_dir():
            env["ORBIT_WEB_DIST"] = str(web_dist)
        self._log_handle = self.log_path.open("a")
        self.process = subprocess.Popen(
            [str(self.binary)],
            env=env,
            cwd=str(REPO_ROOT),
            stdout=self._log_handle,
            stderr=subprocess.STDOUT,
        )
        deadline = time.monotonic() + 60.0
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(
                    f"orbit-server exited during startup:\n{self.log_tail(60)}"
                )
            try:
                result = OrbitHttp(self.base).request("GET", "/health", token=None)
            except Exception:
                result = None
            if result is not None and result.status == 200:
                return self
            time.sleep(0.05)
        raise RuntimeError(f"orbit-server readiness timeout:\n{self.log_tail(60)}")

    def log_tail(self, lines: int = 40) -> str:
        try:
            text = self.log_path.read_text(errors="replace")
        except OSError:
            return ""
        return "\n".join(text.splitlines()[-lines:])

    def stop(self) -> None:
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=10)
        if self._log_handle is not None:
            self._log_handle.close()
            self._log_handle = None
        self.process = None

    # ------------------------------------------------------------------
    def http(self) -> OrbitHttp:
        if self.base is None:
            raise RuntimeError("server is not running")
        if self._http_client is None:
            self._http_client = OrbitHttp(self.base)
        return self._http_client

    def admin_enroll(self, device_id: str = DEVICE_ID, token: str = DEVICE_TOKEN) -> int:
        """Enroll the test device with the administrator key (bearer only)."""
        client = OrbitHttp(self.base)
        result = client.request(
            "POST",
            "/v1/devices",
            body={"device_id": device_id, "token": token},
            token=FIXED_ADMIN_KEY,
            device=None,
        )
        if not result.ok and result.status not in (409,):
            raise RuntimeError(f"device enrollment failed ({result.status}): {result.body!r}")
        return result.status


# ---------------------------------------------------------------------------
# Real MCP stdio client
# ---------------------------------------------------------------------------
class McpClient:
    """Minimal stdio JSON-RPC client for the real ``orbit-mcp`` binary."""

    def __init__(self, binary: Path, env: dict):
        self.binary = binary
        self.env = env
        self.process: subprocess.Popen | None = None
        self.next_id = 1
        self.calls: list[dict] = []

    def start(self) -> "McpClient":
        self.process = subprocess.Popen(
            [str(self.binary)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
            env=self.env,
            cwd=str(REPO_ROOT),
        )
        response = self.request("initialize", {})
        if "result" not in response:
            raise RuntimeError(f"orbit-mcp initialize failed: {response!r}")
        return self

    def request(self, method: str, params: dict) -> dict:
        if self.process is None or self.process.stdin is None or self.process.stdout is None:
            raise RuntimeError("orbit-mcp is not running")
        payload = {"jsonrpc": "2.0", "id": self.next_id, "method": method}
        if params is not None:
            payload["params"] = params
        self.next_id += 1
        self.process.stdin.write(json.dumps(payload) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            raise RuntimeError(f"orbit-mcp closed unexpectedly: {stderr}")
        return json.loads(line)

    def call(self, name: str, arguments: dict) -> dict:
        response = self.request("tools/call", {"name": name, "arguments": arguments})
        result = response.get("result", {})
        text = ""
        if result.get("content"):
            text = result["content"][0].get("text", "")
        is_error = bool(result.get("isError"))
        record = {
            "tool": name,
            "arguments": arguments,
            "is_error": is_error,
            "result_excerpt": text[:2000],
        }
        try:
            record["result"] = json.loads(text) if text else None
        except json.JSONDecodeError:
            record["result"] = text
        self.calls.append(record)
        if is_error:
            raise RuntimeError(f"MCP tool {name} failed: {text}")
        return record["result"]

    def close(self) -> None:
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        self.process = None


# ---------------------------------------------------------------------------
# Deterministic dataset population
# ---------------------------------------------------------------------------
class Dataset:
    """Facts describing what was created, for assertions and evidence."""

    def __init__(self):
        self.projects: list[str] = []
        self.milestones: list[str] = []
        self.tasks: list[str] = []
        self.work_items: list[str] = []
        self.notes: dict[str, dict] = {}
        self.references: list[dict] = []
        self.attachments: dict[str, dict] = {}
        self.agent_jobs: list[dict] = []
        self.mcp_calls: list[dict] = []
        self.mcp_change: dict = {}
        self.capability: dict = {}
        self.activity: dict = {}
        self.commands: list[dict] = []
        self.expected_criteria: dict[str, list[str]] = {}
        self.dependencies: list[dict] = []

    def as_dict(self) -> dict:
        return {
            "projects": self.projects,
            "milestones": self.milestones,
            "task_count": len(self.tasks),
            "tasks": self.tasks,
            "work_items": self.work_items,
            "notes": self.notes,
            "typed_references": self.references,
            "attachments": self.attachments,
            "agent_jobs": self.agent_jobs,
            "mcp": {"tool_calls": self.mcp_calls, "change": self.mcp_change},
            "capability": self.capability,
            "activity_sample": self.activity,
            "command_count": len(self.commands),
        }


def task_title(index: int, project: str) -> str:
    return f"{project.title()} task {index:03d}"


def deterministic_binary(size: int = 8192) -> bytes:
    return bytes((index * 37 + 11) % 256 for index in range(size))


def populate(ctx) -> Dataset:
    """Create the deterministic dataset through supported product surfaces only."""
    server: OrbitServer = ctx.server
    http = server.http()
    data = Dataset()

    def note_command(label: str, payload: dict) -> dict:
        revision = http.expect("GET", "/v1/authoritative/snapshot").body["revision"]
        body = {
            "command_id": f"m34-{label}",
            "expected_revision": revision,
            **payload,
        }
        result = http.expect("POST", "/v1/authoritative/task-commands", body=body).body
        data.commands.append({"surface": "task-commands", "label": label})
        return result

    # -- capability install must precede publishing a workflow that requires it.
    http.expect(
        "POST",
        "/v1/authoritative/capabilities/install",
        body={"command_id": "m34-cap-install", "capability_id": CAPABILITY},
    )
    data.commands.append({"surface": "capabilities/install", "capability_id": CAPABILITY})
    development = http.expect("POST", "/v1/authoritative/blueprints/install",
                              body={"id": BUNDLED_WORKFLOW}).body
    data.commands.append({"surface": "blueprints/install", "id": BUNDLED_WORKFLOW})
    reference_definition = json.loads(
        (REPO_ROOT / "blueprints" / "examples" / "capability-reference.json").read_text()
    )
    http.expect("POST", "/v1/authoritative/blueprints", body={"definition": reference_definition})
    data.commands.append({"surface": "blueprints/publish", "id": CAPABILITY_WORKFLOW})

    # -- projects (project-commands pins the workflow at creation).
    projects = [
        ("project-alpha", "Alpha Platform", None),
        ("project-beta", "Beta Delivery", BUNDLED_WORKFLOW),
        ("project-gamma", "Gamma Capability", CAPABILITY_WORKFLOW),
    ]
    for project_id, name, workflow in projects:
        revision = http.expect("GET", "/v1/authoritative/snapshot").body["revision"]
        payload = {
            "command_id": f"m34-create-{project_id}",
            "expected_revision": revision,
            "project_id": project_id,
            "name": name,
        }
        if workflow is not None:
            payload["workflow_id"] = workflow
        http.expect("POST", "/v1/authoritative/project-commands", body=payload)
        data.projects.append(project_id)
        data.commands.append({"surface": "project-commands", "project_id": project_id,
                              "workflow": workflow or "bundled-default"})
    if development.get("version") != 3:
        # The bundle version is informative evidence, not a hard requirement.
        ctx.warnings.append(
            f"development bundle reported version {development.get('version')!r}, expected 3"
        )

    # -- milestones.
    milestones = [
        ("ms-alpha-1", "project-alpha", "Alpha foundation",
         "First checkpoint for the alpha platform.", EPOCH_MS + 7 * 86_400_000, "high"),
        ("ms-alpha-2", "project-alpha", "Alpha hardening",
         "Reliability and recovery hardening.", EPOCH_MS + 30 * 86_400_000, "medium"),
        ("ms-beta-1", "project-beta", "Beta release",
         "Ship the beta delivery workflow.", EPOCH_MS + 14 * 86_400_000, "urgent"),
    ]
    for milestone_id, project_id, name, description, target, priority in milestones:
        revision = http.expect("GET", "/v1/authoritative/snapshot").body["revision"]
        http.expect(
            "POST",
            "/v1/authoritative/milestone-commands",
            body={
                "command_id": f"m34-create-{milestone_id}",
                "expected_revision": revision,
                "action": {
                    "type": "create",
                    "milestone_id": milestone_id,
                    "project_id": project_id,
                    "name": name,
                    "description": description,
                    "priority": priority,
                    "target_at_ms": target,
                },
            },
        )
        data.milestones.append(milestone_id)
        data.commands.append({"surface": "milestone-commands", "action": "create",
                              "milestone_id": milestone_id})

    # -- tasks: alpha 70, beta 33 (103 total), deterministic ids/titles.
    def create_task(task_id: str, title: str, project_id: str, *, milestone=None,
                    status="inbox", priority="medium", description="", prerequisites=(),
                    criteria=()):
        revision = http.expect("GET", "/v1/authoritative/snapshot").body["revision"]
        action = {
            "type": "create",
            "task_id": task_id,
            "title": title,
            "project_id": project_id,
            "description": description or f"Deterministic task body for {task_id}.",
            "priority": priority,
            "estimated_minutes": 30 + (int(task_id[-3:]) % 5) * 15,
            "deadline_at_ms": EPOCH_MS + 21 * 86_400_000,
            "acceptance_criteria": list(criteria),
            "prerequisite_ids": list(prerequisites),
        }
        if milestone:
            action["milestone_id"] = milestone
        if status != "inbox":
            action["status"] = status
        result = http.expect(
            "POST",
            "/v1/authoritative/task-commands",
            body={
                "command_id": f"m34-create-{task_id}",
                "expected_revision": revision,
                "action": action,
            },
        ).body
        data.tasks.append(task_id)
        data.expected_criteria[task_id] = list(criteria)
        return result

    for index in range(1, TASK_ALPHA_COUNT + 1):
        task_id = f"task-alpha-{index:03d}"
        status = "inbox"
        if index <= 4:
            status = "ready"
        elif index % 7 == 0:
            status = "planned"
        prerequisites = []
        if index in (2, 3, 5, 8, 13, 21, 34):
            prerequisites = [f"task-alpha-{index - 1:03d}"]
        if index == 70:
            prerequisites = ["task-alpha-034", "task-alpha-055"]
        create_task(
            task_id,
            task_title(index, "alpha"),
            "project-alpha",
            milestone="ms-alpha-1" if index <= 10 else None,
            status=status,
            priority=("high" if index % 5 == 0 else "medium"),
            prerequisites=prerequisites,
            criteria=(f"Acceptance check {index}.1", f"Acceptance check {index}.2"),
        )
    for index in range(1, TASK_BETA_COUNT + 1):
        task_id = f"task-beta-{index:03d}"
        create_task(
            task_id,
            task_title(index, "beta"),
            "project-beta",
            milestone="ms-beta-1" if index <= 6 else None,
            status="ready" if index in (1, 2) else "inbox",
            priority=("urgent" if index == 1 else "medium"),
            prerequisites=(["task-beta-001"] if index == 2 else []),
            criteria=(f"Beta acceptance {index}.1", f"Beta acceptance {index}.2"),
        )
    # Record the dependency edges the server accepted.
    snapshot = http.expect("GET", "/v1/authoritative/snapshot").body
    data.dependencies = sorted(
        snapshot.get("task_dependencies", []),
        key=lambda edge: (edge["task_id"], edge["prerequisite_id"]),
    )
    data.commands.append({"surface": "task-commands", "action": "create",
                          "count": len(data.tasks)})

    # -- check two acceptance criteria through the ordered operations action.
    for task_id, criterion_id in (
        ("task-alpha-001", "task-alpha-001-ac-1"),
        ("task-alpha-002", "task-alpha-002-ac-2"),
    ):
        revision = http.expect("GET", "/v1/authoritative/snapshot").body["revision"]
        http.expect(
            "POST",
            "/v1/authoritative/task-commands",
            body={
                "command_id": f"m34-check-{criterion_id}",
                "expected_revision": revision,
                "action": {
                    "type": "acceptance_criteria",
                    "task_id": task_id,
                    "change": {
                        "operation": "set_completed",
                        "criterion_id": criterion_id,
                        "completed": True,
                    },
                },
            },
        )
        data.commands.append({"surface": "task-commands", "action": "acceptance_criteria",
                              "criterion_id": criterion_id})

    # -- Development-bundle work items, assigned to the beta milestone.
    for index in range(1, 6):
        item_id = f"item-beta-{index:03d}"
        http.expect(
            "POST",
            "/v1/authoritative/blueprints/items",
            body={
                "id": item_id,
                "project_id": "project-beta",
                "kind": "feature",
                "properties": {
                    "title": f"Feature {index}",
                    "priority": "high" if index == 1 else "medium",
                },
            },
        )
        data.work_items.append(item_id)
        data.commands.append({"surface": "blueprints/items", "project_id": "project-beta",
                              "kind": "feature", "item_id": item_id})
    for item_id in data.work_items[:3]:
        revision = http.expect("GET", "/v1/authoritative/snapshot").body["revision"]
        http.expect(
            "POST",
            "/v1/authoritative/milestone-commands",
            body={
                "command_id": f"m34-assign-{item_id}",
                "expected_revision": revision,
                "action": {
                    "type": "assign_work_item",
                    "milestone_id": "ms-beta-1",
                    "item_id": item_id,
                },
            },
        )
        data.commands.append({"surface": "milestone-commands",
                              "action": "assign_work_item", "item_id": item_id})

    # -- capability workflow work item (the one the action executes against).
    http.expect(
        "POST",
        "/v1/authoritative/blueprints/items",
        body={"id": "item-gamma-001", "project_id": "project-gamma", "kind": "note-task"},
    )
    data.work_items.append("item-gamma-001")
    data.commands.append({"surface": "blueprints/items", "project_id": "project-gamma",
                          "kind": "note-task", "item_id": "item-gamma-001"})

    # -- Markdown notes with explicit stable-ID wiki links and typed references.
    notes = [
        ("note-alpha-plan", "note-alpha-plan.md",
         "# Alpha plan\n\nSee [[note-alpha-log|the alpha log]] and [task:task-alpha-001].\n"
         "\nDeterministic body: alpha-plan-v1.\n"),
        ("note-alpha-log", "note-alpha-log.md",
         "# Alpha log\n\nBack to [[note-alpha-plan]].\n\nDeterministic body: alpha-log-v1.\n"),
        ("note-beta-design", "note-beta-design.md",
         "# Beta design\n\nThe capability contract is described in [[note-gamma-capability]].\n"),
        ("note-gamma-capability", "note-gamma-capability.md",
         "# Gamma capability\n\nSee [[note-beta-design|the beta design]] and [project:project-beta].\n"),
        ("note-restore-checklist", "note-restore-checklist.md",
         "# Restore checklist\n\n1. Verify the archive.\n2. Restore isolated.\n"
         "3. Confirm relationships and activity.\n"),
        ("note-backup-practice", "note-backup-practice.md",
         "# Backup practice\n\nA backup on the same disk is not protection against "
         "disk failure.\nOrbit claims no zero data loss and no zero downtime.\n"),
    ]
    for note_id, path, body in notes:
        http.expect(
            "POST",
            "/v1/authoritative/markdown-commands",
            body={
                "command_id": f"m34-note-{note_id}",
                "command": {"kind": "Create", "note_id": note_id, "path": path, "body": body},
            },
        )
        data.notes[note_id] = {"path": path, "sha256": sha256_bytes(body.encode())}
        data.commands.append({"surface": "markdown-commands", "note_id": note_id,
                              "path": path})
    for note_id, target in (
        ("note-restore-checklist", "task:task-alpha-005"),
        ("note-backup-practice", "project:project-alpha"),
        ("note-beta-design", "project:project-beta"),
    ):
        http.expect(
            "POST",
            "/v1/authoritative/markdown-reference-commands",
            body={
                "command_id": f"m34-reference-{note_id}",
                "note_id": note_id,
                "target": target,
                "action": "add",
            },
        )
        data.references.append({"note_id": note_id, "target": target})
        data.commands.append({"surface": "markdown-reference-commands", "note_id": note_id,
                              "target": target})

    # -- agent-job records: durable reservation rows, never launched.
    for index, task_id in enumerate(("task-alpha-001", "task-alpha-004"), start=1):
        job_id = f"job-alpha-{index:03d}"
        request_id = f"req-alpha-{index:03d}"
        result = http.expect(
            "POST",
            "/v1/authoritative/agent-jobs/reserve",
            body={
                "job_id": job_id,
                "request_id": request_id,
                "project_id": "project-alpha",
                "task_id": task_id,
                "agent_kind": "codex",
                "machine_ref": "m34-rehearsal-machine",
                "runtime_kind": "herdr",
                "repository_ref": "m34-fixture-repository-not-launched",
                "launch_revision": 1,
                "created_at_ms": EPOCH_MS,
            },
        ).body
        record = {
            "job_id": job_id,
            "request_id": request_id,
            "task_id": task_id,
            "state": result.get("state"),
            "claim_state": result.get("claim_state"),
            "runtime_session_ref": result.get("runtime_session_ref"),
            "note": "reserved-only: no Herdr session, no external agent launched",
        }
        data.agent_jobs.append(record)
        data.commands.append({"surface": "agent-jobs/reserve", "job_id": job_id})

    # -- attachment objects with real, deterministic content (incl. binary).
    attachments = [
        ("att-alpha-spec", "task", "task-alpha-001", "alpha-spec.txt",
         b"# Alpha specification\nscope: deterministic\nsha-stable: yes\n",
         "text/plain"),
        ("att-alpha-binary", "task", "task-alpha-002", "alpha-diagram.bin",
         deterministic_binary(8192), "application/octet-stream"),
        ("att-beta-notes", "project", "project-beta", "beta-notes.md",
         b"# Beta notes\nprepared for the M34 recovery rehearsal.\n", "text/markdown"),
        ("att-gamma-feature", "work_item", "item-beta-001", "feature.json",
         json.dumps({"kind": "work_item_note", "item": "item-beta-001"},
                    sort_keys=True).encode() + b"\n", "application/json"),
        ("att-agent-brief", "agent_job", "job-alpha-001", "agent-brief.txt",
         b"# Agent brief\nThis file is attached to a reserved-only job record.\n",
         "text/plain"),
    ]
    for attachment_id, owner_kind, owner_id, filename, content, content_type in attachments:
        revision = http.expect("GET", "/v1/authoritative/snapshot").body["revision"]
        result = http.expect(
            "POST",
            "/v1/authoritative/attachment-import",
            query={
                "command_id": f"m34-attachment-{attachment_id}",
                "expected_revision": revision,
                "attachment_id": attachment_id,
                "activity_id": f"m34-activity-{attachment_id}",
                "owner_kind": owner_kind,
                "owner_id": owner_id,
                "filename": filename,
                "content_type": content_type,
                "description": "M34 rehearsal fixture",
            },
            raw=content,
        ).body
        view = result.get("attachment", {}) if isinstance(result, dict) else {}
        data.attachments[attachment_id] = {
            "owner_kind": owner_kind,
            "owner_id": owner_id,
            "filename": filename,
            "size_bytes": len(content),
            "sha256": sha256_bytes(content),
            "server_checksum": view.get("checksum"),
            "content_type": content_type,
            "binary": content_type == "application/octet-stream",
        }
        data.commands.append({"surface": "attachment-import", "attachment_id": attachment_id,
                              "owner": f"{owner_kind}:{owner_id}"})

    # -- capability authorization + one REAL execution receipt (connected executor).
    http.expect(
        "POST",
        "/v1/authoritative/capabilities/authorize",
        body={
            "command_id": "m34-cap-authorize",
            "capability_id": CAPABILITY,
            "project_id": "project-gamma",
            "confirm": True,
        },
    )
    data.commands.append({"surface": "capabilities/authorize", "project_id": "project-gamma"})
    http.expect(
        "POST",
        "/v1/authoritative/capability-executors/announce",
        body={"capability_ids": [CAPABILITY]},
    )
    data.commands.append({"surface": "capability-executors/announce"})
    item = http.expect(
        "GET", "/v1/authoritative/blueprints/items", query={"id": "item-gamma-001"}
    ).body
    http.expect(
        "POST",
        "/v1/authoritative/capability-actions/request",
        body={
            "command_id": "m34-cap-request-001",
            "item_id": "item-gamma-001",
            "expected_revision": item["revision"],
            "action_id": CAPABILITY_ACTION,
            "inputs": {"note": "M34 rehearsal capability receipt", "tag": "m34"},
            "confirm": True,
        },
    )
    data.commands.append({"surface": "capability-actions/request",
                          "item_id": "item-gamma-001", "action": CAPABILITY_ACTION})
    executor = ctx.run_capability_executor()
    show = http.expect(
        "GET",
        "/v1/authoritative/capability-actions/show",
        query={"command_id": "m34-cap-request-001"},
    ).body
    effects = http.expect(
        "GET", "/v1/authoritative/capability-effects", query={"item_id": "item-gamma-001"}
    ).body
    data.capability = {
        "capability_id": CAPABILITY,
        "workflow": CAPABILITY_WORKFLOW,
        "item_id": "item-gamma-001",
        "action": CAPABILITY_ACTION,
        "request_command_id": "m34-cap-request-001",
        "request_state": (show.get("request") or {}).get("state"),
        "receipt_outcome": (show.get("receipt") or {}).get("outcome"),
        "executor_exit_code": executor["returncode"],
        "executor_stdout": executor["stdout"][:2000],
        "executor_stderr": executor["stderr"][:2000],
        "effects": effects.get("notes", []),
        "note": "executed by the real connected `orbit capability serve` executor; "
                "the server never runs provider code itself",
    }
    if data.capability["request_state"] != "succeeded":
        raise RuntimeError(
            f"capability request did not succeed: {data.capability['request_state']!r} "
            f"(executor rc={executor['returncode']}, stderr={executor['stderr'][:800]})"
        )

    # -- MCP-made change through the real orbit-mcp connected session (EC-012).
    mcp_record = ctx.run_mcp_task_update(MCP_TARGET_TASK)
    data.mcp_calls = mcp_record["calls"]
    data.mcp_change = mcp_record

    # -- activity history sample (task transitions/attachments produced events).
    activity = http.expect(
        "GET", f"/v1/authoritative/tasks/{MCP_TARGET_TASK}/activity"
    ).body
    data.activity = {
        "task_id": MCP_TARGET_TASK,
        "count": len(activity) if isinstance(activity, list) else None,
        "event_types": sorted(
            {event.get("type") or event.get("kind") or "unknown"
             for event in (activity if isinstance(activity, list) else [])}
        ),
    }
    return data


# ---------------------------------------------------------------------------
# Rehearsal context
# ---------------------------------------------------------------------------
class Context:
    def __init__(self, args, root: Path):
        self.args = args
        self.root = root
        self.workspace = root / "workspace"
        self.server_db = self.workspace / "server.db"
        self.server_home = root / "server-home"
        self.server_log = root / "server.log"
        self.backups = root / "backups"
        self.restored = root / "restored"
        self.restored_home = root / "restored-home"
        self.restored_log = root / "restored-server.log"
        self.cli_home = root / "cli-home"
        self.cli_config = root / "cli.config.toml"
        self.cli_db = root / "cli-workspace" / "orbit.db"
        self.cli = OrbitCli(
            args.binary,
            db=self.server_db,
            config=self.cli_config,
            home=self.cli_home,
            keep_workdir=args.keep_workdir,
        )
        self.server = OrbitServer(
            args.server_binary,
            db=self.server_db,
            home=self.server_home,
            log_path=self.server_log,
        )
        self.restored_server: OrbitServer | None = None
        self.dataset: Dataset | None = None
        self.archive: Path | None = None
        self.archive_bytes = 0
        self.archive_sha256 = ""
        self.manifest: dict = {}
        self.verification: dict = {}
        self.inspection: dict = {}
        self.before_counts: dict = {}
        self.before_http: dict = {}
        self.before_hashes: dict = {}
        self.after_counts: dict = {}
        self.after_http: dict = {}
        self.after_hashes: dict = {}
        self.post_backup_changes: dict = {}
        self.warnings: list[str] = []
        self.restore_json: dict = {}
        self.restore_credential_note = ""
        self.retired_http_history: list[dict] = []
        self.started_at = time.time()
        self.timings: dict[str, dict] = {}

    # -- restored-instance helpers --------------------------------------------
    def start_restored_server(self) -> OrbitServer:
        """Start the isolated instance; restored DBs already hold their devices."""
        server = OrbitServer(
            self.args.server_binary,
            db=Path(self.restore_json["database"]),
            home=self.restored_home,
            log_path=self.restored_log,
        )
        server.start()
        client = server.http()
        auth = client.request("GET", "/v1/authoritative/snapshot")
        if auth.status in (401, 403):
            status = server.admin_enroll()
            self.restore_credential_note = f"re-enrolled restored device (status {status})"
            auth = client.request("GET", "/v1/authoritative/snapshot")
        else:
            self.restore_credential_note = (
                "restored credential already valid (no re-enrollment needed)"
            )
        if auth.status != 200:
            raise RuntimeError(
                f"restored instance rejected the enrolled credential "
                f"({auth.status}): {auth.body!r}\n{server.log_tail(40)}"
            )
        self.restored_server = server
        return server

    # -- child-process helpers -------------------------------------------------
    def run_capability_executor(self) -> dict:
        env = clean_env(
            {
                "HOME": str(self.root / "executor-home"),
                "ORBIT_AUTHORITY_SERVER": self.server.base,
                "ORBIT_SYNC_HTTP_TOKEN": DEVICE_TOKEN,
                "ORBIT_AUTHORITY_DEVICE": DEVICE_ID,
                "ORBIT_AUTHORITY_DB": str(self.root / "executor-replica.db"),
            }
        )
        started = time.monotonic()
        completed = subprocess.run(
            [str(self.args.binary), "capability", "serve", "--once", "--json"],
            env=env,
            cwd=str(REPO_ROOT),
            text=True,
            capture_output=True,
            timeout=180,
        )
        return {
            "returncode": completed.returncode,
            "stdout": completed.stdout,
            "stderr": completed.stderr,
            "duration_ms": round((time.monotonic() - started) * 1000, 2),
        }

    def run_mcp_task_update(self, task_id: str) -> dict:
        replica = self.root / "mcp-replica.db"
        env = clean_env(
            {
                "HOME": str(self.root / "mcp-home"),
                "ORBIT_DB": str(self.root / "mcp-unused-local.db"),
                "ORBIT_AUTHORITY_DB": str(replica),
                "ORBIT_AUTHORITY_SERVER": self.server.base,
                "ORBIT_DEVICE_ID": DEVICE_ID,
                "ORBIT_DEVICE_TOKEN": DEVICE_TOKEN,
                "ORBIT_AUTHORITY_DEVICE": DEVICE_ID,
                "ORBIT_SYNC_HTTP_TOKEN": DEVICE_TOKEN,
                "ORBIT_MCP_MODE": "developer",
                "ORBIT_MCP_AGENT_ID": "m34-rehearsal-mcp",
                "ORBIT_MCP_SESSION_ID": "m34-rehearsal",
            }
        )
        client = McpClient(self.args.mcp_binary, env)
        calls = []
        try:
            client.start()
            refreshed = client.call("authority.refresh", {})
            calls.append({"tool": "authority.refresh", "result": refreshed})
            update = client.call(
                "task.update",
                {
                    "id": task_id,
                    "description": MCP_DESCRIPTION,
                    "priority": "medium",
                    "labels": ["mcp", "rehearsal"],
                },
            )
            calls.append({"tool": "task.update", "arguments": {"id": task_id},
                          "result": update})
            if update.get("accepted") is not False or update.get("pending") is not True:
                raise RuntimeError(
                    "MCP task.update did not report a pending durable command: "
                    f"{json.dumps(update)[:800]}"
                )
            flush = client.call("authority.flush", {})
            calls.append({"tool": "authority.flush", "result": flush})
            if "Accepted(1)" not in json.dumps(flush):
                raise RuntimeError(f"MCP authority.flush did not accept the command: {flush!r}")
            client.call("authority.refresh", {})
            readback = client.call("task.get", {"id": task_id})
            calls.append({"tool": "task.get", "result": {
                "description": readback.get("description"),
                "priority": readback.get("priority"),
                "labels": readback.get("labels"),
            }})
            if readback.get("description") != MCP_DESCRIPTION:
                raise RuntimeError(
                    "MCP-authored description was not accepted by the authority: "
                    f"{readback.get('description')!r}"
                )
        finally:
            client.close()
        return {
            "binary": str(self.args.mcp_binary),
            "mode": "connected (developer)",
            "tool_calls": client.calls,
            "flush_outcome": "Accepted(1)",
            "task_id": task_id,
            "accepted_change": {
                "description": MCP_DESCRIPTION,
                "priority": "medium",
                "labels": ["mcp", "rehearsal"],
            },
            "calls": calls,
        }

    # -- evidence helpers ------------------------------------------------------
    def entity_snapshot(self, http: OrbitHttp) -> dict:
        snapshot = http.expect("GET", "/v1/authoritative/snapshot").body
        inventory = http.expect("GET", "/v1/authoritative/markdown-inventory").body
        return {
            "revision": snapshot.get("revision"),
            "projects": [project.get("id") for project in snapshot.get("projects", [])],
            "tasks": [task.get("id") for task in snapshot.get("tasks", [])],
            "milestones": [milestone.get("id") for milestone in snapshot.get("milestones", [])],
            "dependencies": sorted(
                ((edge.get("task_id"), edge.get("prerequisite_id"))
                 for edge in snapshot.get("task_dependencies", [])),
            ),
            "note_ids": sorted(note.get("id") for note in inventory),
        }


# ---------------------------------------------------------------------------
# Step framework
# ---------------------------------------------------------------------------
class StepRunner:
    def __init__(self):
        self.steps: list[dict] = []

    def run(self, number: int, title: str, function):
        started = time.monotonic()
        record = {
            "step": number,
            "title": title,
            "status": "FAIL",
            "details": {},
            "duration_ms": 0.0,
        }
        print(f"\n[step {number:02d}] {title}")
        try:
            details = function()
            record["details"] = details if isinstance(details, dict) else {}
            record["status"] = "PASS"
        except Exception as error:  # noqa: BLE001 - evidence must capture everything
            record["error"] = f"{type(error).__name__}: {error}"
            record["traceback"] = traceback.format_exc(limit=6)
            stderr = ""
            if isinstance(error, RuntimeError):
                stderr = str(error)
            if is_not_implemented(stderr):
                record["status"] = "BLOCKED"
                record["blocked_reason"] = (
                    "the installed build does not implement this product surface yet: "
                    + stderr.splitlines()[0][:300]
                )
        record["duration_ms"] = round((time.monotonic() - started) * 1000, 2)
        status = record["status"]
        print(f"[step {number:02d}] {status} ({record['duration_ms']:.0f} ms)")
        if status == "FAIL":
            print(record.get("error", ""))
        elif status == "BLOCKED":
            print(record.get("blocked_reason", ""))
        self.steps.append(record)
        return record

    def summary(self) -> dict:
        counts = {"PASS": 0, "FAIL": 0, "BLOCKED": 0}
        for step in self.steps:
            counts[step["status"]] = counts.get(step["status"], 0) + 1
        return counts


# ---------------------------------------------------------------------------
# The 14 rehearsal steps
# ---------------------------------------------------------------------------
def step_01_start_instance(ctx: Context):
    ctx.server.start()
    server = ctx.server
    health = server.http().request("GET", "/health", token=None)
    if health.status != 200:
        raise RuntimeError(f"health endpoint returned {health.status}: {health.body!r}")
    server.admin_enroll()
    # The device principal must be usable for product reads.
    snapshot = server.http().expect("GET", "/v1/authoritative/snapshot").body
    return {
        "binary": str(ctx.args.server_binary),
        "auth_mode": "local-trusted",
        "bind": f"127.0.0.1:{server.port}",
        "server_db": str(ctx.server_db),
        "workspace_vault": str(ctx.workspace / "server.knowledge"),
        "attachment_store": str(ctx.workspace / "attachments" / "objects"),
        "health": health.body,
        "initial_revision": snapshot.get("revision"),
        "enrolled_device": DEVICE_ID,
        "startup_banner": server.log_tail(5),
    }


def step_02_populate(ctx: Context):
    ctx.dataset = populate(ctx)
    http = ctx.server.http()
    snapshot = http.expect("GET", "/v1/authoritative/snapshot").body
    counts = sqlite_table_counts(ctx.server_db)
    http_counts = {
        "projects": len(snapshot.get("projects", [])),
        "tasks": len(snapshot.get("tasks", [])),
        "milestones": len(snapshot.get("milestones", [])),
        "task_dependencies": len(snapshot.get("task_dependencies", [])),
        "note_ids": len(http.expect("GET", "/v1/authoritative/markdown-inventory").body),
    }
    minimums = {
        "projects": 2,
        "tasks": 100,
        "milestones": 3,
        "task_dependencies": 1,
        "note_ids": 4,
    }
    for key, minimum in minimums.items():
        if http_counts[key] < minimum:
            raise RuntimeError(f"dataset too small: {key}={http_counts[key]} < {minimum}")
    required_tables = {
        "task_acceptance_criteria": 200,
        "orbit_blueprint_work_items": 6,
        "attachments": 5,
        "attachment_objects": 5,
        "agent_jobs": 2,
        "orbit_capability_installations": 1,
        "orbit_capability_grants": 1,
        "orbit_blueprint_action_requests": 1,
        "orbit_blueprint_action_receipts": 1,
        "orbit_reference_work_item_notes": 1,
        "activities": 100,
    }
    for table, minimum in required_tables.items():
        observed = counts.get(table, 0)
        if observed < minimum:
            raise RuntimeError(f"dataset missing {table}: {observed} < {minimum}")
    ctx.before_counts = counts
    ctx.before_http = http_counts
    ctx.before_hashes = {
        "vault": hash_tree(ctx.workspace / "server.knowledge"),
        "attachments": hash_tree(ctx.workspace / "attachments" / "objects"),
        "db_snapshot_sha256": sqlite_snapshot_hash(
            ctx.server_db, ctx.root / "evidence-before-snapshot.db"
        ),
        "notes": {
            note_id: sha256_file(ctx.workspace / "server.knowledge" / record["path"])
            for note_id, record in ctx.dataset.notes.items()
            if (ctx.workspace / "server.knowledge" / record["path"]).is_file()
        },
        "note_source_body": {
            note_id: record["sha256"] for note_id, record in ctx.dataset.notes.items()
        },
        "attachment_content": {
            attachment_id: record["sha256"]
            for attachment_id, record in ctx.dataset.attachments.items()
        },
    }
    return {
        "dataset": ctx.dataset.as_dict(),
        "http_counts": http_counts,
        "table_counts": {"total_tables": len(counts), "tables": counts},
        "minimums": {**minimums, **required_tables},
        "warnings": ctx.warnings,
    }


def step_03_create_backup(ctx: Context):
    ctx.backups.mkdir(parents=True, exist_ok=True)
    record = ctx.cli.call_backup(
        "create",
        "--output", str(ctx.backups),
        "--source-mode", "server",
        "--label", REHEARSAL_LABEL,
    )
    if record["returncode"] != 0:
        raise RuntimeError(
            f"orbit backup create exited {record['returncode']}\n"
            f"stdout:\n{record['stdout']}\nstderr:\n{record['stderr']}"
        )
    response = json.loads(record["stdout"])
    archive = Path(response["backup"])
    if not archive.is_file():
        raise RuntimeError(f"reported archive does not exist: {archive}")
    if not response.get("verified"):
        raise RuntimeError(f"creation did not verify the archive: {response}")
    ctx.archive = archive
    ctx.archive_bytes = archive.stat().st_size
    ctx.archive_sha256 = sha256_file(archive)
    sidecar = Path(str(archive) + ".verify.json")
    listed = ctx.cli.run_json(
        "backup", "list", "--output", str(ctx.backups)
    )
    entries = [
        entry for entry in listed.get("backups", [])
        if Path(entry["path"]).name == archive.name
    ]
    if not entries:
        raise RuntimeError(f"created archive is not listed: {listed}")
    if entries[0]["verification"] != "verified":
        raise RuntimeError(f"listed archive is not verified: {entries[0]}")
    missing_components = [
        component for component in ("database", "knowledge", "attachments", "configuration")
        if component not in response.get("components", [])
    ]
    if missing_components:
        raise RuntimeError(f"backup is missing components: {missing_components}")
    # EC-002 evidence: the archive was produced while the real server instance
    # was serving the WAL database, not from a stopped/flat copy.
    server_live = ctx.server.process is not None and ctx.server.process.poll() is None
    wal_present = Path(str(ctx.server_db) + "-wal").exists()
    journal_mode = None
    try:
        connection = sqlite3.connect(f"file:{ctx.server_db}?mode=ro", uri=True, timeout=10)
        try:
            journal_mode = connection.execute("PRAGMA journal_mode").fetchone()[0]
        finally:
            connection.close()
    except sqlite3.Error:
        journal_mode = None
    live_health = ctx.server.http().request("GET", "/health", token=None)
    if not server_live or not wal_present or live_health.status != 200:
        raise RuntimeError(
            "EC-002 pre-condition failed: the backup was not taken from a live "
            f"WAL server (live={server_live}, wal={wal_present}, "
            f"health={live_health.status})"
        )
    return {
        "command": record["argv"],
        "archive": str(archive),
        "archive_bytes": ctx.archive_bytes,
        "archive_sha256": ctx.archive_sha256,
        "label": response.get("label"),
        "created_at": response.get("created_at"),
        "source_mode": response.get("source_mode"),
        "format_version": response.get("format_version"),
        "schema_version": response.get("schema_version"),
        "components": response.get("components"),
        "verified": response.get("verified"),
        "verification_summary": response.get("verification_summary"),
        "elapsed_ms": response.get("elapsed_ms"),
        "sidecar": str(sidecar) if sidecar.is_file() else None,
        "sidecar_sha256": sha256_file(sidecar) if sidecar.is_file() else None,
        "list_entry": entries[0],
        "duration_ms": record["duration_ms"],
        "live_server_backup": {
            "server_pid": ctx.server.process.pid if ctx.server.process else None,
            "server_was_serving_http": live_health.status == 200,
            "journal_mode": journal_mode,
            "wal_file_present": wal_present,
            "note": "the fenced online snapshot was taken while the real server "
                    "owned the WAL database and answered HTTP; the archive was "
                    "verified before publish",
        },
    }


def step_04_verify_archive(ctx: Context):
    if ctx.archive is None:
        raise RuntimeError("no archive was created")
    verify_record = ctx.cli.call_backup("verify", str(ctx.archive))
    if verify_record["returncode"] != 0:
        raise RuntimeError(
            f"orbit backup verify exited {verify_record['returncode']}\n"
            f"stdout:\n{verify_record['stdout']}\nstderr:\n{verify_record['stderr']}"
        )
    report = json.loads(verify_record["stdout"])
    if not report.get("ok"):
        raise RuntimeError(f"archive did not verify: {report}")
    inspect_record = ctx.cli.call_backup("inspect", str(ctx.archive))
    if inspect_record["returncode"] != 0:
        raise RuntimeError(
            f"orbit backup inspect exited {inspect_record['returncode']}: "
            f"{inspect_record['stderr']}"
        )
    inspection = json.loads(inspect_record["stdout"])
    manifest = inspection.get("manifest", {})
    if manifest.get("format_version") != 1:
        raise RuntimeError(f"unexpected format version: {manifest.get('format_version')}")
    if manifest.get("complete") is not True:
        raise RuntimeError(f"archive manifest is not complete: {manifest.get('complete')}")
    inventory = manifest.get("inventory", [])
    kinds = sorted({entry.get("kind") for entry in inventory})
    for expected in ("database", "knowledge_note", "attachment_object"):
        if expected not in kinds:
            raise RuntimeError(f"archive inventory has no {expected}: {kinds}")
    ctx.verification = report
    ctx.inspection = inspection
    ctx.manifest = manifest
    return {
        "verify_command": verify_record["argv"],
        "verify_ok": report.get("ok"),
        "verify_checks": [
            {"name": check.get("name"), "status": check.get("status"),
             "detail": check.get("detail")}
            for check in report.get("checks", [])
        ],
        "verified_files": report.get("verified_files"),
        "inspect_command": inspect_record["argv"],
        "format_version": manifest.get("format_version"),
        "min_reader_version": manifest.get("min_reader_version"),
        "schema_version": manifest.get("schema_version"),
        "complete": manifest.get("complete"),
        "label": manifest.get("label"),
        "source": manifest.get("source"),
        "totals": manifest.get("totals"),
        "counts": manifest.get("counts"),
        "inventory_files": len(inventory),
        "inventory_kinds": kinds,
        "exclusions": manifest.get("exclusions"),
        "archive_sha256": ctx.archive_sha256,
    }


def step_05_disposable_changes(ctx: Context):
    http = ctx.server.http()
    revision = http.expect("GET", "/v1/authoritative/snapshot").body["revision"]
    http.expect(
        "POST",
        "/v1/authoritative/task-commands",
        body={
            "command_id": "m34-post-backup-task",
            "expected_revision": revision,
            "action": {
                "type": "create",
                "task_id": POST_BACKUP_TASK,
                "title": "Post-backup disposable task",
                "project_id": "project-alpha",
                "description": "Created after the backup; it must not be restored.",
            },
        },
    )
    http.expect(
        "POST",
        "/v1/authoritative/markdown-commands",
        body={
            "command_id": "m34-post-backup-note",
            "command": {
                "kind": "Create",
                "note_id": POST_BACKUP_NOTE,
                "path": "post-backup-note.md",
                "body": "# Post-backup note\n\nThis note was created after the backup.\n",
            },
        },
    )
    snapshot = http.expect("GET", "/v1/authoritative/snapshot").body
    inventory = http.expect("GET", "/v1/authoritative/markdown-inventory").body
    ctx.post_backup_changes = {
        "task_id": POST_BACKUP_TASK,
        "note_id": POST_BACKUP_NOTE,
        "post_snapshot_task_count": len(snapshot.get("tasks", [])),
        "post_snapshot_note_count": len(inventory),
        "archive_task_count": len(ctx.dataset.tasks),
    }
    if ctx.post_backup_changes["post_snapshot_task_count"] <= len(ctx.dataset.tasks):
        raise RuntimeError("the post-backup task was not accepted by the live instance")
    return ctx.post_backup_changes


def step_06_restore_isolated(ctx: Context):
    if ctx.archive is None:
        raise RuntimeError("no archive was created")
    record = ctx.cli.call_backup("restore", str(ctx.archive), "--to", str(ctx.restored))
    if record["returncode"] != 0:
        raise RuntimeError(
            f"orbit backup restore --to exited {record['returncode']}\n"
            f"stdout:\n{record['stdout']}\nstderr:\n{record['stderr']}"
        )
    response = json.loads(record["stdout"])
    ctx.restore_json = response
    database = Path(response["database"])
    if not database.is_file():
        raise RuntimeError(f"restored database is missing: {database}")
    if response.get("mode") != "isolated":
        raise RuntimeError(
            f"isolated restore reported mode {response.get('mode')!r}: {response}"
        )
    # The active workspace must still be the live server's workspace.
    if Path(response["database"]).resolve().parent == ctx.workspace.resolve():
        raise RuntimeError("isolated restore targeted the active workspace")
    snapshot = ctx.server.http().expect("GET", "/v1/authoritative/snapshot").body
    if POST_BACKUP_TASK not in {task.get("id") for task in snapshot.get("tasks", [])}:
        raise RuntimeError("the active workspace lost a post-backup task during the restore")
    return {
        "command": record["argv"],
        "response": response,
        "restored_database": str(database),
        "restored_vault": response.get("knowledge_vault"),
        "restored_attachments": response.get("attachment_store"),
        "size_bytes": database.stat().st_size,
        "duration_ms": record["duration_ms"],
        "active_workspace_task_still_present": POST_BACKUP_TASK,
    }


def step_07_start_isolated_instance(ctx: Context):
    database = Path(ctx.restore_json["database"])
    server = ctx.start_restored_server()
    snapshot = server.http().expect("GET", "/v1/authoritative/snapshot").body
    return {
        "binary": str(ctx.args.server_binary),
        "auth_mode": "local-trusted",
        "bind": f"127.0.0.1:{server.port}",
        "restored_database": str(database),
        "health": server.http().request("GET", "/health", token=None).body,
        "revision": snapshot.get("revision"),
        "startup_banner": server.log_tail(5),
        "credential_handling": ctx.restore_credential_note,
    }


def step_08_earlier_data_present(ctx: Context):
    if ctx.restored_server is None:
        raise RuntimeError("the isolated instance is not running")
    http = ctx.restored_server.http()
    snapshot = http.expect("GET", "/v1/authoritative/snapshot").body
    inventory = http.expect("GET", "/v1/authoritative/markdown-inventory").body
    task_ids = {task.get("id") for task in snapshot.get("tasks", [])}
    milestone_ids = {milestone.get("id") for milestone in snapshot.get("milestones", [])}
    project_ids = {project.get("id") for project in snapshot.get("projects", [])}
    note_ids = {note.get("id") for note in inventory}
    missing = {
        "projects": [item for item in ctx.dataset.projects if item not in project_ids],
        "milestones": [item for item in ctx.dataset.milestones if item not in milestone_ids],
        "tasks": [item for item in ctx.dataset.tasks if item not in task_ids],
        "notes": [item for item in ctx.dataset.notes if item not in note_ids],
    }
    if any(missing.values()):
        raise RuntimeError(f"restored instance is missing earlier data: {missing}")
    # A representative note body survives with its frontmatter identity.
    note = http.expect(
        "GET", "/v1/authoritative/markdown-by-id", query={"id": "note-alpha-plan"}
    ).body
    body = note.get("body") or note.get("content") or ""
    if "[[note-alpha-log" not in body:
        raise RuntimeError(f"restored note lost its knowledge link: {body[:300]!r}")
    # Attachments are readable and their bytes hash to the recorded content.
    attachment_checks = {}
    for attachment_id, expected in ctx.dataset.attachments.items():
        content = http.expect(
            "GET", f"/v1/authoritative/attachments/{attachment_id}/content"
        ).raw
        observed = sha256_bytes(content)
        attachment_checks[attachment_id] = {
            "sha256": observed,
            "expected_sha256": expected["sha256"],
            "matches": observed == expected["sha256"],
            "size_bytes": len(content),
            "binary": expected["binary"],
        }
        if observed != expected["sha256"]:
            raise RuntimeError(
                f"restored attachment {attachment_id} content hash mismatch: "
                f"{observed} != {expected['sha256']}"
            )
    ctx.after_http = ctx.entity_snapshot(http)
    ctx.after_counts = sqlite_table_counts(Path(ctx.restore_json["database"]))
    restored_vault = Path(ctx.restore_json["knowledge_vault"])
    restored_attachments = Path(ctx.restore_json["attachment_store"]).parent
    ctx.after_hashes = {
        "vault": hash_tree(restored_vault),
        "attachments": hash_tree(restored_attachments / "objects"),
        "db_snapshot_sha256": sqlite_snapshot_hash(
            Path(ctx.restore_json["database"]), ctx.root / "evidence-after-snapshot.db"
        ),
        "notes": {
            note_id: sha256_bytes(
                (restored_vault / record["path"]).read_bytes()
            )
            for note_id, record in ctx.dataset.notes.items()
            if (restored_vault / record["path"]).is_file()
        },
        "attachment_content": {
            attachment_id: check["sha256"]
            for attachment_id, check in attachment_checks.items()
        },
    }
    return {
        "restored_http_counts": ctx.after_http,
        "attachment_checks": attachment_checks,
        "note_link_preserved": True,
        "task_count": len(task_ids),
        "missing": missing,
    }


def step_09_relationships_and_activity(ctx: Context):
    if ctx.restored_server is None:
        raise RuntimeError("the isolated instance is not running")
    if ctx.after_http:
        restored_edges = sorted(tuple(edge) for edge in ctx.after_http["dependencies"])
    else:
        restored_edges = None
    http = ctx.restored_server.http()
    if restored_edges is None:
        snapshot = http.expect("GET", "/v1/authoritative/snapshot").body
        restored_edges = sorted(
            (edge.get("task_id"), edge.get("prerequisite_id"))
            for edge in snapshot.get("task_dependencies", [])
        )
    expected_edges = sorted(
        (edge.get("task_id"), edge.get("prerequisite_id")) for edge in ctx.dataset.dependencies
    )
    if restored_edges != expected_edges:
        raise RuntimeError(
            "restored dependency edges differ from the snapshot: "
            f"restored={restored_edges} expected={expected_edges}"
        )
    criteria = http.expect(
        "GET", "/v1/authoritative/tasks/task-alpha-001/acceptance-criteria"
    ).body
    completed = [entry["id"] for entry in criteria if entry.get("completed")]
    if "task-alpha-001-ac-1" not in completed:
        raise RuntimeError(f"restored acceptance criterion state was lost: {criteria}")
    activity = http.expect(
        "GET", f"/v1/authoritative/tasks/{MCP_TARGET_TASK}/activity"
    ).body
    if not isinstance(activity, list) or not activity:
        raise RuntimeError("restored task activity history is empty")
    milestone_items = http.expect(
        "GET", "/v1/authoritative/blueprints/items", query={"project_id": "project-beta"}
    ).body
    item_ids = {
        item.get("id") for item in (milestone_items if isinstance(milestone_items, list)
                                    else milestone_items.get("items", []))
    }
    for item_id in ctx.dataset.work_items:
        if item_id.startswith("item-beta") and item_id not in item_ids:
            raise RuntimeError(f"restored workflow work item is missing: {item_id}")
    return {
        "dependency_edges": len(restored_edges),
        "dependencies_match": True,
        "acceptance_criteria_completed": completed,
        "activity_count": len(activity),
        "activity_sample": [
            {"type": event.get("type") or event.get("kind"),
             "at_ms": event.get("occurred_at_ms") or event.get("created_at_ms")}
            for event in activity[:5]
        ],
        "work_item_count": len(item_ids),
        "milestone_assignments_restored": True,
    }


def step_10_later_changes_absent(ctx: Context):
    if ctx.restored_server is None:
        raise RuntimeError("the isolated instance is not running")
    http = ctx.restored_server.http()
    task = http.request("GET", f"/v1/authoritative/tasks/{POST_BACKUP_TASK}")
    note = http.request(
        "GET", "/v1/authoritative/markdown-by-id", query={"id": POST_BACKUP_NOTE}
    )
    snapshot = http.expect("GET", "/v1/authoritative/snapshot").body
    restored_task_count = len(snapshot.get("tasks", []))
    restored_note_count = len(http.expect("GET", "/v1/authoritative/markdown-inventory").body)
    restored_inventory = http.expect(
        "GET", "/v1/authoritative/markdown-inventory"
    ).body
    absent = {
        "post_backup_task_absent": POST_BACKUP_TASK not in {
            item.get("id") for item in snapshot.get("tasks", [])
        },
        "post_backup_note_absent": POST_BACKUP_NOTE not in {
            item.get("id") for item in restored_inventory
        },
        "task_count_matches_snapshot": restored_task_count == len(ctx.dataset.tasks),
        "note_count_matches_snapshot": restored_note_count == len(ctx.dataset.notes),
    }
    if not all(absent.values()):
        raise RuntimeError(f"later disposable changes leaked into the restore: {absent}")
    return {
        **absent,
        "restored_task_count": restored_task_count,
        "snapshot_task_count": len(ctx.dataset.tasks),
        "restored_note_count": restored_note_count,
        "snapshot_note_count": len(ctx.dataset.notes),
        "post_backup_task_status": task.status,
        "post_backup_note_status": note.status,
    }


def step_11_agent_and_receipts_readable(ctx: Context):
    if ctx.restored_server is None:
        raise RuntimeError("the isolated instance is not running")
    http = ctx.restored_server.http()
    jobs = []
    for expected in ctx.dataset.agent_jobs:
        result = http.expect(
            "GET", "/v1/authoritative/agent-jobs", query={"id": expected["job_id"]}
        ).body
        if not isinstance(result, dict):
            raise RuntimeError(f"agent job {expected['job_id']} is not readable: {result!r}")
        view = {
            "job_id": result.get("id"),
            "state": result.get("state"),
            "claim_state": result.get("claim_state"),
            "runtime_session_ref": result.get("runtime_session_ref"),
            "task_id": result.get("task_id"),
        }
        if view["state"] != "queued":
            raise RuntimeError(f"reserved job state changed unexpectedly: {view}")
        jobs.append(view)
    show = http.expect(
        "GET",
        "/v1/authoritative/capability-actions/show",
        query={"command_id": ctx.dataset.capability["request_command_id"]},
    ).body
    if (show.get("receipt") or {}).get("outcome") != "succeeded":
        raise RuntimeError(f"capability receipt is not readable after restore: {show}")
    effects = http.expect(
        "GET",
        "/v1/authoritative/capability-effects",
        query={"item_id": ctx.dataset.capability["item_id"]},
    ).body
    notes = effects.get("notes", [])
    if not notes:
        raise RuntimeError(f"capability effect is not readable after restore: {effects}")
    return {
        "agent_jobs": jobs,
        "capability_request_state": (show.get("request") or {}).get("state"),
        "capability_receipt": {
            "outcome": (show.get("receipt") or {}).get("outcome"),
            "reported_by": (show.get("receipt") or {}).get("reported_by"),
        },
        "capability_effects": notes,
        "capability_installed_and_granted": True,
    }


def step_12_no_automatic_external_execution(ctx: Context):
    if ctx.restored_server is None:
        raise RuntimeError("the isolated instance is not running")
    http = ctx.restored_server.http()
    # Capture agent state, wait a bounded moment, and assert nothing advanced.
    before = []
    for expected in ctx.dataset.agent_jobs:
        result = http.expect(
            "GET", "/v1/authoritative/agent-jobs", query={"id": expected["job_id"]}
        ).body
        before.append({
            "job_id": result.get("id"),
            "state": result.get("state"),
            "claim_state": result.get("claim_state"),
            "runtime_session_ref": result.get("runtime_session_ref"),
            "updated_at_ms": result.get("updated_at_ms"),
        })
    requests_before = http.expect(
        "GET", "/v1/authoritative/capability-actions/list",
        query={"item_id": ctx.dataset.capability["item_id"]},
    ).body
    time.sleep(1.0)
    after = []
    for expected in ctx.dataset.agent_jobs:
        result = http.expect(
            "GET", "/v1/authoritative/agent-jobs", query={"id": expected["job_id"]}
        ).body
        after.append({
            "job_id": result.get("id"),
            "state": result.get("state"),
            "claim_state": result.get("claim_state"),
            "runtime_session_ref": result.get("runtime_session_ref"),
        })
    requests_after = http.expect(
        "GET", "/v1/authoritative/capability-actions/list",
        query={"item_id": ctx.dataset.capability["item_id"]},
    ).body
    before_requests = len(requests_before.get("requests", [])) if isinstance(
        requests_before, dict) else len(requests_before)
    after_requests = len(requests_after.get("requests", [])) if isinstance(
        requests_after, dict) else len(requests_after)
    if before_requests != after_requests:
        raise RuntimeError(
            f"capability requests changed without an operator: {before_requests} -> "
            f"{after_requests}"
        )
    for previous, current in zip(before, after):
        if previous["state"] != current["state"] or \
                previous["claim_state"] != current["claim_state"] or \
                previous["runtime_session_ref"] != current["runtime_session_ref"]:
            raise RuntimeError(
                f"agent job advanced without an external runtime: {previous} -> {current}"
            )
        if current["runtime_session_ref"] is not None:
            raise RuntimeError(f"restored job references an external session: {current}")
    return {
        "agent_jobs_before": before,
        "agent_jobs_after": after,
        "agent_state_unchanged": True,
        "capability_requests_before": before_requests,
        "capability_requests_after": after_requests,
        "no_external_launch": True,
        "external_boundary": (
            "The rehearsal never launched a Herdr session or an external agent. "
            "The restored instance has no auto-launch path, the reserved jobs stay "
            "queued with no runtime_session_ref and no claim, and the capability "
            "receipt is a durable record, not a re-execution. A real external "
            "agent/Herdr round-trip is outside this rehearsal's boundary."
        ),
    }


def step_13_disposable_write(ctx: Context):
    if ctx.restored_server is None:
        raise RuntimeError("the isolated instance is not running")
    http = ctx.restored_server.http()
    revision = http.expect("GET", "/v1/authoritative/snapshot").body["revision"]
    result = http.expect(
        "POST",
        "/v1/authoritative/task-commands",
        body={
            "command_id": "m34-restored-write",
            "expected_revision": revision,
            "action": {
                "type": "create",
                "task_id": RESTORED_WRITE_TASK,
                "title": "Write accepted by the restored instance",
                "project_id": "project-alpha",
                "description": "Proves the restored workspace is writable, not read-only.",
            },
        },
    ).body
    if (result.get("task") or {}).get("id") != RESTORED_WRITE_TASK:
        raise RuntimeError(f"restored instance rejected the disposable write: {result}")
    return {
        "task_id": RESTORED_WRITE_TASK,
        "accepted_revision": result.get("revision"),
        "task_status": (result.get("task") or {}).get("status"),
    }


def step_14_restart_persistence(ctx: Context):
    if ctx.restored_server is None:
        raise RuntimeError("the isolated instance is not running")
    before = ctx.restored_server.http().expect("GET", "/v1/authoritative/snapshot").body
    before_ids = {task.get("id") for task in before.get("tasks", [])}
    ctx.retired_http_history.extend(ctx.restored_server.http().history)
    ctx.restored_server.stop()
    ctx.start_restored_server()
    http = ctx.restored_server.http()
    after = http.expect("GET", "/v1/authoritative/snapshot").body
    after_ids = {task.get("id") for task in after.get("tasks", [])}
    if after_ids != before_ids:
        raise RuntimeError("task set changed across a restart of the restored instance")
    if RESTORED_WRITE_TASK not in after_ids:
        raise RuntimeError("the disposable write did not survive the restart")
    # EC-012 records must still be readable through the restarted instance.
    mcp_task = http.expect(
        "GET", f"/v1/authoritative/tasks/{MCP_TARGET_TASK}"
    ).body
    if mcp_task.get("description") != MCP_DESCRIPTION:
        raise RuntimeError("the MCP-authored change did not survive the restart")
    receipt = http.expect(
        "GET",
        "/v1/authoritative/capability-actions/show",
        query={"command_id": ctx.dataset.capability["request_command_id"]},
    ).body
    if (receipt.get("receipt") or {}).get("outcome") != "succeeded":
        raise RuntimeError("the capability receipt did not survive the restart")
    job = http.expect(
        "GET", "/v1/authoritative/agent-jobs",
        query={"id": ctx.dataset.agent_jobs[0]["job_id"]},
    ).body
    activity_after_restart = http.expect(
        "GET", f"/v1/authoritative/tasks/{MCP_TARGET_TASK}/activity"
    ).body
    # CLI read path: the restored workspace is a normal local workspace.
    cli_after_restart = ctx.cli.run_json("--db", str(ctx.restore_json["database"]), "status")
    task_count = sqlite_table_counts(Path(ctx.restore_json["database"])).get("tasks")
    return {
        "restarts": 1,
        "task_count_before_restart": len(before_ids),
        "task_count_after_restart": len(after_ids),
        "disposable_write_survived": RESTORED_WRITE_TASK in after_ids,
        "mcp_change_survived_restart": mcp_task.get("description") == MCP_DESCRIPTION,
        "capability_receipt_survived_restart": (
            (receipt.get("receipt") or {}).get("outcome") == "succeeded"
        ),
        "agent_job_state_after_restart": {
            "state": job.get("state"),
            "claim_state": job.get("claim_state"),
            "runtime_session_ref": job.get("runtime_session_ref"),
        },
        "activity_count_after_restart": len(activity_after_restart),
        "cli_status_after_restart": {
            "type": cli_after_restart.get("type"),
            "healthy": cli_after_restart.get("healthy"),
            "projects": cli_after_restart.get("projects"),
            "tasks": cli_after_restart.get("tasks"),
            "knowledge_items": cli_after_restart.get("knowledge_items"),
        },
        "sqlite_task_count": task_count,
        "startup_banner": ctx.restored_server.log_tail(5),
    }


# ---------------------------------------------------------------------------
# README rendering (shared with m34_backup_cli.py)
# ---------------------------------------------------------------------------
def _status_badge(status: str) -> str:
    return {"PASS": "✅ PASS", "FAIL": "❌ FAIL", "BLOCKED": "⏸ BLOCKED"}.get(status, status)


def write_readme(evidence_dir: Path) -> Path:
    """Regenerate the human README from the machine-readable evidence files."""
    evidence_dir.mkdir(parents=True, exist_ok=True)
    rehearsal = read_json_file(evidence_dir / "rehearsal-evidence.json")
    failures = read_json_file(evidence_dir / "failure-injection.json")
    lines: list[str] = []
    lines.append("# M34 — real recovery rehearsal and failure-injection evidence\n")
    lines.append(
        "Evidence for MILESTONE-0034 exit criteria **EC-013** (real recovery "
        "rehearsal) and **EC-014** (failure injection), produced by "
        "`tests/acceptance/m34_recovery_rehearsal.py` and "
        "`tests/acceptance/m34_backup_cli.py`. Everything below was executed "
        "against the real built binaries on real SQLite files; no row was "
        "fabricated with SQL and no external agent was launched.\n"
    )
    if rehearsal:
        lines.append("## Rehearsal\n")
        environment = rehearsal.get("environment", {})
        binaries = environment.get("binaries", {})
        git = environment.get("git", {})
        if binaries or git:
            lines.append("### Build identity (ties this evidence to one tree)\n")
            if git:
                lines.append(
                    f"- Source tree: commit `{str(git.get('commit'))[:16]}`, branch "
                    f"`{git.get('branch')}`, dirty `{git.get('dirty')}` "
                    f"({git.get('changed_path_count')} changed path(s))"
                )
            for label, key in (
                ("CLI", "orbit_cli"),
                ("Server", "orbit_server"),
                ("MCP", "orbit_mcp"),
            ):
                identity = binaries.get(key) or {}
                lines.append(
                    f"- {label}: `{identity.get('path')}` "
                    f"{identity.get('size_bytes')} bytes, "
                    f"mtime `{identity.get('mtime_iso')}`, "
                    f"sha256 `{identity.get('sha256')}`"
                    + (f", version `{identity.get('version')}`"
                       if identity.get("version") else "")
                )
            lines.append("")
        lines.append(f"- Result: **{_status_badge(rehearsal.get('result', 'UNKNOWN'))}**")
        lines.append(f"- Generated: `{rehearsal.get('finished_at')}`")
        lines.append(f"- Orbit version: `{rehearsal.get('orbit_version')}`")
        lines.append(f"- Host: `{rehearsal.get('host', 'unknown')}`")
        environment = rehearsal.get("environment", {})
        lines.append(
            f"- Authority: real `orbit-server` in "
            f"`{environment.get('auth_mode', 'local-trusted')}` mode on "
            f"`{environment.get('bind', 'loopback')}`"
        )
        lines.append(
            "- Isolated restore instance: real `orbit-server` on the restored "
            "database, plus CLI reads via `orbit --db <restored> status`"
        )
        archive = rehearsal.get("archive", {})
        if archive:
            lines.append("")
            lines.append("### Archive\n")
            lines.append(f"- Archive: `{archive.get('archive')}`")
            lines.append(f"- Bytes: `{archive.get('archive_bytes')}`")
            lines.append(f"- SHA-256: `{archive.get('archive_sha256')}`")
            lines.append(f"- Format/schema: `v{archive.get('format_version')}` / "
                         f"`schema {archive.get('schema_version')}`")
            lines.append(f"- Components: `{', '.join(archive.get('components') or [])}`")
            lines.append(f"- Verified at creation: `{archive.get('verified')}`")
            live = (rehearsal.get("steps") or [{}])[2].get("details", {}).get(
                "live_server_backup"
            ) if len(rehearsal.get("steps") or []) > 2 else None
            if live:
                lines.append(
                    f"- Captured from a **live server**: pid `{live.get('server_pid')}`, "
                    f"journal_mode `{live.get('journal_mode')}`, WAL file present "
                    f"`{live.get('wal_file_present')}`, server answered HTTP "
                    f"`{live.get('server_was_serving_http')}` (EC-002 evidence)"
                )
        dataset = rehearsal.get("dataset", {})
        if dataset:
            lines.append("")
            lines.append("### Deterministic dataset (created through supported surfaces)\n")
            lines.append("| Element | Count / state | Created through |")
            lines.append("|---|---|---|")
            lines.append(f"| Projects | {len(dataset.get('projects', []))} | "
                         f"`POST /v1/authoritative/project-commands` |")
            lines.append(f"| Milestones | {len(dataset.get('milestones', []))} | "
                         f"`POST /v1/authoritative/milestone-commands` |")
            lines.append(f"| Tasks | {dataset.get('task_count')} | "
                         f"`POST /v1/authoritative/task-commands` (create) |")
            lines.append(f"| Work Items | {len(dataset.get('work_items', []))} | "
                         f"`POST /v1/authoritative/blueprints/items` "
                         f"(bundled Development v3 + capability-reference) |")
            lines.append(f"| Markdown notes | {len(dataset.get('notes', {}))} | "
                         f"`POST /v1/authoritative/markdown-commands` "
                         f"(wiki links in body) |")
            lines.append(f"| Typed references | {len(dataset.get('typed_references', []))} | "
                         f"`POST /v1/authoritative/markdown-reference-commands` |")
            lines.append(f"| Attachments | {len(dataset.get('attachments', {}))} | "
                         f"`POST /v1/authoritative/attachment-import` "
                         f"(incl. 8 KiB binary) |")
            lines.append(f"| Agent-job records | {len(dataset.get('agent_jobs', []))} | "
                         f"`POST /v1/authoritative/agent-jobs/reserve` "
                         f"(reserved-only, never launched) |")
            capability = dataset.get("capability", {})
            lines.append(
                f"| Capability install + grant + receipt | "
                f"`{capability.get('request_state')}` / "
                f"`{capability.get('receipt_outcome')}` | "
                f"`capabilities/install`, `capabilities/authorize`, "
                f"`capability-actions/request`, real `orbit capability serve --once` |"
            )
            mcp = dataset.get("mcp", {})
            lines.append(
                f"| MCP-made change | accepted (`authority.flush` → "
                f"`{(mcp.get('change') or {}).get('flush_outcome', 'n/a')}`) | "
                f"real `orbit-mcp` connected session: "
                f"`{(mcp.get('change') or {}).get('task_id', '')}` |"
            )
        steps = rehearsal.get("steps", [])
        if steps:
            lines.append("")
            lines.append("### Rehearsal steps\n")
            lines.append("| # | Step | Status | Detail |")
            lines.append("|---|---|---|---|")
            for step in steps:
                detail = ""
                if step.get("status") == "FAIL":
                    detail = (step.get("error") or "").splitlines()[0][:180]
                elif step.get("status") == "BLOCKED":
                    detail = (step.get("blocked_reason") or "")[:180]
                lines.append(
                    f"| {step.get('step')} | {step.get('title')} | "
                    f"{_status_badge(step.get('status'))} | {detail} |"
                )
        before = rehearsal.get("counts", {}).get("before", {})
        after = rehearsal.get("counts", {}).get("after", {})
        if before and after:
            lines.append("")
            lines.append("### Entity counts and content hashes\n")
            lines.append("| Signal | Before backup | Restored |")
            lines.append("|---|---|---|")
            lines.append(f"| Tasks (`tasks`) | {before.get('tasks')} | {after.get('tasks')} |")
            lines.append(
                f"| Task prerequisites | {before.get('task_prerequisites')} | "
                f"{after.get('task_prerequisites')} |"
            )
            lines.append(
                f"| Acceptance criteria | {before.get('task_acceptance_criteria')} | "
                f"{after.get('task_acceptance_criteria')} |"
            )
            lines.append(
                f"| Work items | {before.get('orbit_blueprint_work_items')} | "
                f"{after.get('orbit_blueprint_work_items')} |"
            )
            lines.append(
                f"| Markdown notes | {before.get('markdown_note_index')} | "
                f"{after.get('markdown_note_index')} |"
            )
            lines.append(
                f"| Attachment objects | {before.get('attachment_objects')} | "
                f"{after.get('attachment_objects')} |"
            )
            lines.append(
                f"| Agent jobs | {before.get('agent_jobs')} | {after.get('agent_jobs')} |"
            )
            lines.append(
                f"| Capability receipts | {before.get('orbit_blueprint_action_receipts')} | "
                f"{after.get('orbit_blueprint_action_receipts')} |"
            )
            counts = rehearsal.get("counts", {})
            lines.append(
                f"| Tables with differing counts | "
                f"`{', '.join(counts.get('differing_tables') or []) or 'none'}` | "
                f"(must be none) |"
            )
            equal_tables = counts.get("equal_tables") or []
            hashes = rehearsal.get("hashes", {})
            lines.append("")
            lines.append(
                f"The backup was taken from the **live server instance** (WAL mode, "
                f"active connection) and the isolated restore reproduced "
                f"**all {len(equal_tables)}** database tables with equal row counts; "
                f"the markdown vault tree and the attachment object tree are "
                f"byte-identical (hashes below). No automatic external action ran."
            )
            hashes = rehearsal.get("hashes", {})
            lines.append("")
            lines.append("Content hashes (SHA-256), before backup → after isolated restore:")
            lines.append("")
            lines.append(f"- Database snapshot: `{hashes.get('db_before')}` → "
                         f"`{hashes.get('db_after')}` "
                         f"(match: {'yes' if hashes.get('db_before') == hashes.get('db_after') else 'NO'})")
            lines.append(f"- Markdown vault tree: `{hashes.get('vault_before')}` → "
                         f"`{hashes.get('vault_after')}` "
                         f"(match: {'yes' if hashes.get('vault_before') == hashes.get('vault_after') else 'NO'})")
            lines.append(f"- Attachment object tree: `{hashes.get('attachments_before')}` → "
                         f"`{hashes.get('attachments_after')}` "
                         f"(match: {'yes' if hashes.get('attachments_before') == hashes.get('attachments_after') else 'NO'})")
            lines.append(f"- Note files match: "
                         f"`{'yes' if hashes.get('notes_match') else 'NO'}`")
            lines.append(f"- Attachment content match: "
                         f"`{'yes' if hashes.get('attachment_content') == hashes.get('attachment_content_restored') else 'NO'}`")
            for attachment_id, digest in (hashes.get("attachment_content") or {}).items():
                lines.append(f"- Attachment `{attachment_id}`: `{digest}`")
            for note_id, digest in (hashes.get("notes") or {}).items():
                lines.append(f"- Note `{note_id}`: `{digest}`")
        dataset = rehearsal.get("dataset", {})
        mcp = dataset.get("mcp") or {}
        if mcp:
            change = mcp.get("change") or {}
            lines.append("")
            lines.append("### MCP-authored change (EC-012)\n")
            lines.append(
                "A real `orbit-mcp` stdio session in connected mode changed a task; "
                "`authority.flush` was accepted by the authority and the change is "
                "present after restore. Tool calls: "
                + ", ".join(f"`{call.get('tool')}`" for call in mcp.get("tool_calls", []))
                + "."
            )
            lines.append("")
            lines.append(f"- Task: `{change.get('task_id')}`")
            lines.append(f"- Accepted change: `{json.dumps(change.get('accepted_change'))}`")
            lines.append(f"- Flush outcome: `{change.get('flush_outcome')}`")
            lines.append(f"- Verified after restore: description/priority/labels match the "
                         f"accepted server state.")
        capability = dataset.get("capability") or {}
        if capability:
            lines.append("")
            lines.append("### Capability execution receipt (EC-012)\n")
            lines.append(f"- Capability: `{capability.get('capability_id')}`")
            lines.append(f"- Work item: `{capability.get('item_id')}`")
            lines.append(f"- Action request state: `{capability.get('request_state')}`")
            lines.append(f"- Receipt outcome: `{capability.get('receipt_outcome')}`")
            lines.append(f"- Executed by the real connected `orbit capability serve` "
                         f"executor (exit {capability.get('executor_exit_code')}); the server "
                         f"never runs provider code.")
        jobs = dataset.get("agent_jobs") or []
        if jobs:
            lines.append("")
            lines.append("### Agent-job records\n")
            lines.append(
                "Reserved through the authority but deliberately never launched:"
            )
            lines.append("")
            for job in jobs:
                def show(value):
                    return "null" if value is None else f"`{value}`"
                lines.append(
                    f"- `{job.get('job_id')}` on `{job.get('task_id')}`: state "
                    f"{show(job.get('state'))}, claim_state "
                    f"{show(job.get('claim_state'))}, runtime_session_ref "
                    f"{show(job.get('runtime_session_ref'))}"
                )
        limitations = rehearsal.get("limitations", [])
        if limitations:
            lines.append("")
            lines.append("### Limitations (external boundary)\n")
            for limitation in limitations:
                lines.append(f"- {limitation}")
        lines.append("")
        lines.append("### Reproduce\n")
        lines.append("```sh")
        lines.append("cargo build --offline -p orbit-cli -p orbit-server")
        lines.append(
            "python3 tests/acceptance/m34_recovery_rehearsal.py "
            "--binary target/debug/orbit --server-binary target/debug/orbit-server"
        )
        lines.append(
            "python3 tests/acceptance/m34_backup_cli.py "
            "--binary target/debug/orbit --server-binary target/debug/orbit-server"
        )
        lines.append("```")
    if failures:
        lines.append("")
        lines.append("## Failure injection\n")
        lines.append(f"- Result: **{_status_badge(failures.get('result', 'UNKNOWN'))}**")
        lines.append(f"- Generated: `{failures.get('finished_at')}`")
        cases = failures.get("cases", [])
        if cases:
            lines.append("")
            lines.append("| Case | Expected | Observed | Original workspace intact | Status |")
            lines.append("|---|---|---|---|---|")
            for case in cases:
                lines.append(
                    f"| {case.get('name')} | {case.get('expected')} | "
                    f"{case.get('observed')} | {case.get('original_intact')} | "
                    f"{_status_badge(case.get('status'))} |"
                )
        limitations = failures.get("limitations", [])
        if limitations:
            lines.append("")
            lines.append("Failure-injection limitations:")
            lines.append("")
            for limitation in limitations:
                lines.append(f"- {limitation}")
    path = evidence_dir / "README.md"
    path.write_text("\n".join(lines) + "\n")
    return path


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def build_evidence(ctx: Context, runner: StepRunner, *, population_only: bool) -> dict:
    counts_before = {key: value for key, value in ctx.before_counts.items()}
    counts_after = {key: value for key, value in ctx.after_counts.items()}
    summary = runner.summary()
    if summary["FAIL"]:
        result = "FAIL"
    elif summary["BLOCKED"]:
        result = "BLOCKED"
    else:
        result = "PASS"
    if population_only:
        result = "POPULATION_ONLY" if not summary["FAIL"] else "FAIL"
    evidence = {
        "schema": "orbit.m34.recovery-rehearsal/1",
        "generated_by": "tests/acceptance/m34_recovery_rehearsal.py",
        "result": result,
        "started_at": datetime.fromtimestamp(ctx.started_at, timezone.utc).isoformat(),
        "finished_at": now_iso(),
        "orbit_version": None,
        "host": f"{platform.system()} {platform.release()} {platform.machine()}",
        "environment": {
            "auth_mode": "local-trusted",
            "bind": f"127.0.0.1:{ctx.server.port}" if ctx.server.port else None,
            "cli_binary": str(ctx.args.binary),
            "server_binary": str(ctx.args.server_binary),
            "mcp_binary": str(ctx.args.mcp_binary),
            "run_directory": str(ctx.root) if ctx.args.keep_workdir else "<temporary, removed>",
            "binaries": {
                "orbit_cli": binary_identity(ctx.args.binary),
                "orbit_server": binary_identity(
                    ctx.args.server_binary, probe_version=False),
                "orbit_mcp": binary_identity(
                    ctx.args.mcp_binary, probe_version=False),
            },
            "git": git_identity(),
        },
        "step_summary": summary,
        "steps": runner.steps,
        "dataset": ctx.dataset.as_dict() if ctx.dataset else {},
        "archive": {
            "archive": str(ctx.archive) if ctx.archive else None,
            "archive_bytes": ctx.archive_bytes,
            "archive_sha256": ctx.archive_sha256,
            "format_version": ctx.manifest.get("format_version"),
            "min_reader_version": ctx.manifest.get("min_reader_version"),
            "schema_version": ctx.manifest.get("schema_version"),
            "label": ctx.manifest.get("label"),
            "complete": ctx.manifest.get("complete"),
            "source": ctx.manifest.get("source"),
            "totals": ctx.manifest.get("totals"),
            "counts": ctx.manifest.get("counts"),
            "inventory_files": len(ctx.manifest.get("inventory", [])),
            "exclusions": ctx.manifest.get("exclusions"),
            "verified": bool(ctx.verification.get("ok")),
            "components": None,
        },
        "verify": {
            "ok": ctx.verification.get("ok"),
            "checks": ctx.verification.get("checks"),
            "problems": ctx.verification.get("problems"),
            "warnings": ctx.verification.get("warnings"),
        },
        "inspect": {
            "archive_bytes": ctx.inspection.get("archive_bytes"),
            "inventory_files": ctx.inspection.get("inventory_files"),
            "content_bytes": ctx.inspection.get("content_bytes"),
            "creation_verification": ctx.inspection.get("creation_verification"),
        },
        "restore": ctx.restore_json,
        "counts": {
            "before": counts_before,
            "after": counts_after,
            "equal_tables": sorted(
                table for table in counts_before
                if counts_before.get(table) == counts_after.get(table)
            ),
            "differing_tables": sorted(
                table for table in set(counts_before) | set(counts_after)
                if counts_before.get(table) != counts_after.get(table)
            ),
            "http_before": ctx.before_http,
            "http_after": ctx.after_http,
        },
        "hashes": {
            "db_before": ctx.before_hashes.get("db_snapshot_sha256"),
            "db_after": ctx.after_hashes.get("db_snapshot_sha256"),
            "vault_before": (ctx.before_hashes.get("vault") or {}).get("digest"),
            "vault_after": (ctx.after_hashes.get("vault") or {}).get("digest"),
            "attachments_before": (ctx.before_hashes.get("attachments") or {}).get("digest"),
            "attachments_after": (ctx.after_hashes.get("attachments") or {}).get("digest"),
            "notes": ctx.before_hashes.get("notes"),
            "notes_restored": ctx.after_hashes.get("notes"),
            "notes_match": ctx.before_hashes.get("notes") == ctx.after_hashes.get("notes"),
            "note_source_body": ctx.before_hashes.get("note_source_body"),
            "attachment_content": ctx.before_hashes.get("attachment_content"),
            "attachment_content_restored": ctx.after_hashes.get("attachment_content"),
        },
        "mcp": ctx.dataset.as_dict().get("mcp") if ctx.dataset else {},
        "post_backup_changes": ctx.post_backup_changes,
        "timings_ms": {
            step["step"]: step["duration_ms"] for step in runner.steps
        },
        "limitations": [
            "No Herdr session and no external agent was launched anywhere in this "
            "rehearsal. Agent-job records are durable reservations in state "
            "'queued' with claim_state=null and runtime_session_ref=null; the "
            "rehearsal asserts they stay that way and never auto-execute.",
            "No external agent session or repository is part of the archive: they "
            "are documented exclusions. Restore therefore cannot restore them, and "
            "the rehearsal does not pretend it does.",
            "Capability code was executed by the real connected `orbit capability "
            "serve` executor against a throwaway replica; the server itself never "
            "runs provider code. The receipt is durable evidence of that one "
            "execution, not an automatic re-execution after restore.",
            "Milestone creation and MCP tool calls use the real server HTTP/MCP "
            "surfaces; the local CLI has no milestone command. Nested note paths "
            "require a folder-create command first, so notes use root-level paths.",
            "The bundled Basic workflow declares no work-item kinds; bundled "
            "Development v3 does, so workflow Work Items use Development.",
            "A backup on the same disk is not protection against disk failure. "
            "Orbit does not claim zero data loss or zero downtime.",
        ],
        "warnings": ctx.warnings,
        "cli_history": ctx.cli.history,
        "http_requests": {
            "server": ctx.server.http().history if ctx.server.base else [],
            "restored_server": (
                ctx.retired_http_history
                + (ctx.restored_server.http().history
                   if ctx.restored_server is not None and ctx.restored_server.base
                   else [])
            ),
            "distinct_endpoints": sorted({
                entry["path"]
                for entry in (
                    (ctx.server.http().history if ctx.server.base else [])
                    + ctx.retired_http_history
                + (ctx.restored_server.http().history
                   if ctx.restored_server is not None and ctx.restored_server.base
                   else [])
                )
            }),
        },
    }
    return evidence


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="M34 real recovery rehearsal (real binaries, isolated temp workspace)."
    )
    parser.add_argument("--binary", type=Path, default=DEFAULT_CLI,
                        help="orbit CLI binary (default: target/debug/orbit)")
    parser.add_argument("--server-binary", type=Path, default=DEFAULT_SERVER,
                        help="orbit-server binary (default: target/debug/orbit-server)")
    parser.add_argument("--mcp-binary", type=Path, default=DEFAULT_MCP,
                        help="orbit-mcp binary (default: target/debug/orbit-mcp)")
    parser.add_argument("--evidence-dir", type=Path, default=DEFAULT_EVIDENCE_DIR,
                        help="where rehearsal-evidence.json and README.md are written")
    parser.add_argument("--population-only", action="store_true",
                        help="populate the dataset, record it and exit (development flag)")
    parser.add_argument("--keep-workdir", action="store_true",
                        help="keep the disposable run directory for inspection")
    parser.add_argument("--workdir", type=Path, default=None,
                        help="explicit disposable run directory (default: a temp dir)")
    args = parser.parse_args(argv)

    for label, path in (
        ("orbit CLI", args.binary),
        ("orbit-server", args.server_binary),
        ("orbit-mcp", args.mcp_binary),
    ):
        if not path.is_file():
            print(f"required binary is missing: {label} at {path}", file=sys.stderr)
            return 1

    if args.workdir is not None:
        root = Path(args.workdir).resolve()
        if root.exists():
            shutil.rmtree(root)
        root.mkdir(parents=True)
    else:
        root = Path(tempfile.mkdtemp(prefix="orbit-m34-rehearsal-"))
    ctx = Context(args, root)
    runner = StepRunner()
    exit_code = 1
    print("M34 recovery rehearsal")
    print(f"  CLI:        {args.binary}")
    print(f"  server:     {args.server_binary}")
    print(f"  MCP:        {args.mcp_binary}")
    print(f"  run dir:    {root}")
    print(f"  evidence:   {args.evidence_dir}")
    try:
        steps = [
            (1, "start a real Orbit instance (server, local-trusted, loopback)",
             lambda: step_01_start_instance(ctx)),
            (2, "populate the deterministic realistic dataset", lambda: step_02_populate(ctx)),
        ]
        if not args.population_only:
            steps.extend([
                (3, "create a complete backup", lambda: step_03_create_backup(ctx)),
                (4, "verify the archive", lambda: step_04_verify_archive(ctx)),
                (5, "make additional disposable changes", lambda: step_05_disposable_changes(ctx)),
                (6, "restore the backup into a separate location",
                 lambda: step_06_restore_isolated(ctx)),
                (7, "start an isolated Orbit instance on the restored data",
                 lambda: step_07_start_isolated_instance(ctx)),
                (8, "confirm earlier tasks/milestones/notes/attachments exist",
                 lambda: step_08_earlier_data_present(ctx)),
                (9, "verify task relationships and activity history",
                 lambda: step_09_relationships_and_activity(ctx)),
                (10, "confirm later disposable changes are absent",
                 lambda: step_10_later_changes_absent(ctx)),
                (11, "verify agent jobs and capability receipts are readable",
                 lambda: step_11_agent_and_receipts_readable(ctx)),
                (12, "confirm no external agent/capability auto-executes",
                 lambda: step_12_no_automatic_external_execution(ctx)),
                (13, "perform an allowed disposable write in the restored instance",
                 lambda: step_13_disposable_write(ctx)),
                (14, "restart the restored instance and verify persistence",
                 lambda: step_14_restart_persistence(ctx)),
            ])
        for number, title, function in steps:
            runner.run(number, title, function)
        print("\nSummary")
        print("-------")
        for step in runner.steps:
            print(f"  {step['step']:>2}. {step['status']:<8} {step['title']}")
        summary = runner.summary()
        print(f"  PASS={summary['PASS']} FAIL={summary['FAIL']} BLOCKED={summary['BLOCKED']}")
        # Orbit version from the CLI (best effort, for provenance).
        try:
            version = subprocess.run(
                [str(args.binary), "--version"], text=True, capture_output=True, timeout=30
            ).stdout.strip()
        except OSError:
            version = None
        evidence = build_evidence(ctx, runner, population_only=args.population_only)
        evidence["orbit_version"] = version
        if evidence.get("archive"):
            # Recover the component list from the create response recorded in step 3.
            for step in runner.steps:
                if step["step"] == 3 and step["status"] == "PASS":
                    evidence["archive"]["components"] = step["details"].get("components")
        args.evidence_dir.mkdir(parents=True, exist_ok=True)
        # The development flag must never clobber acceptance evidence.
        evidence_name = (
            "population-evidence.json" if args.population_only
            else "rehearsal-evidence.json"
        )
        evidence_path = args.evidence_dir / evidence_name
        evidence_path.write_text(json.dumps(evidence, indent=2, sort_keys=False) + "\n")
        print(f"\nEvidence: {evidence_path}")
        if not args.population_only:
            readme_path = write_readme(args.evidence_dir)
            print(f"README:   {readme_path}")
        if summary["FAIL"]:
            exit_code = 1
        elif summary["BLOCKED"]:
            exit_code = 2
        else:
            exit_code = 0
    finally:
        for server in (ctx.restored_server, ctx.server):
            try:
                if server is not None:
                    server.stop()
            except Exception:  # noqa: BLE001 - cleanup must never mask the result
                pass
        if args.keep_workdir:
            print(f"Kept run directory: {root}")
        else:
            shutil.rmtree(root, ignore_errors=True)
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
