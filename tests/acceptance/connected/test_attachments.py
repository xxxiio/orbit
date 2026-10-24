"""Scenario D: connected attachment acceptance, pending upload, and retry.

Uses the real `orbit` CLI, the real `orbit-server`, and two enrolled devices
with independent replicas. Every assertion verifies authoritative server state
or verified client bytes; a staged upload is never treated as accepted.
The test skips cleanly when either binary is absent.
"""
from __future__ import annotations

import hashlib
import json
import os
import secrets
import socket
import sqlite3
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[3]
CLI = Path(os.environ.get("ORBIT_CLI_BIN", ROOT / "target" / "debug" / "orbit"))
SERVER = Path(os.environ.get("ORBIT_SERVER_BIN", ROOT / "target" / "debug" / "orbit-server"))
REQUIRE_EVIDENCE = os.environ.get("ORBIT_REQUIRE_CONNECTED_EVIDENCE") == "1"
# Deliberately small so the oversize path is exercised without a 64 MiB body.
# A 256 KiB metadata-only upload still fits.
ATTACHMENT_LIMIT = 512 * 1024


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _request(base, path, token=None, device=None, payload=None, method=None):
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
        method=method or ("POST" if body is not None else "GET"),
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        raw = response.read()
        return response.headers, json.loads(raw) if raw else None


def _raw_request(base, path, token=None, device=None, body=None, content_type="text/plain"):
    """Perform an HTTP request and return (status, headers, parsed body).

    Unlike `_request`, this does not raise on a non-2xx status, so tests can
    assert 401/413 and content headers directly.
    """
    headers = {}
    if token:
        headers["Authorization"] = "Bearer " + token
    if device:
        headers["x-orbit-device-id"] = device
    if body is not None:
        headers["Content-Type"] = content_type
    request = urllib.request.Request(
        base + path,
        data=body,
        headers=headers,
        method="POST" if body is not None else "GET",
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            raw = response.read()
            return response.status, response.headers, json.loads(raw) if raw else None
    except urllib.error.HTTPError as error:
        raw = error.read()
        try:
            parsed = json.loads(raw) if raw else None
        except json.JSONDecodeError:
            parsed = raw
        return error.code, error.headers, parsed


def _revision(base, token, device):
    _, value = _request(base, "/v1/authoritative/snapshot", token, device)
    return value["revision"]


def _command(base, token, device, path, payload):
    _, value = _request(base, path, token, device, payload)
    return value


def _cli(env, replica, device, token, *args):
    merged = dict(env)
    merged.update(
        {
            "ORBIT_AUTHORITY_DB": str(replica),
            "ORBIT_SYNC_HTTP_TOKEN": token,
            "ORBIT_AUTHORITY_DEVICE": device,
        }
    )
    result = subprocess.run(
        [str(CLI), "--json", "attachment", *args],
        env=merged,
        capture_output=True,
        text=True,
        timeout=60,
    )
    if result.returncode != 0:
        raise AssertionError(
            f"orbit attachment {' '.join(args)} failed ({result.returncode}):\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return json.loads(result.stdout)


def _db_rows(db, query, params=()):
    connection = sqlite3.connect(f"file:{db}?mode=ro", uri=True, timeout=10)
    try:
        return connection.execute(query, params).fetchall()
    finally:
        connection.close()


def _object_path(server_db, digest):
    return server_db.parent / "attachments" / "objects" / digest[:2] / digest


class Deployment:
    def __init__(self, root, base, port, admin, env, server_db):
        self.root = root
        self.base = base
        self.port = port
        self.admin = admin
        self.env = env
        self.server_db = server_db
        self.replica_a = root / "replica-a.db"
        self.replica_b = root / "replica-b.db"
        self.device_a = "scenario-d-a"
        self.device_b = "scenario-d-b"
        self.token_a = secrets.token_hex(32)
        self.token_b = secrets.token_hex(32)
        self.log_path = root / "server.log"
        self.process = None
        self.log = None

    def start(self):
        self.log = self.log_path.open("a")
        self.process = subprocess.Popen(
            [str(SERVER)],
            env={
                **self.env,
                "ORBIT_SERVER_DB": str(self.server_db),
                "ORBIT_SERVER_BIND": f"127.0.0.1:{self.port}",
                "ORBIT_SERVER_ADMIN_KEY": self.admin,
            },
            stdout=self.log,
            stderr=subprocess.STDOUT,
        )
        for _ in range(150):
            if self.process.poll() is not None:
                raise AssertionError("server exited:\n" + self.log_path.read_text()[-4000:])
            try:
                _request(self.base, "/health")
                return
            except Exception:
                time.sleep(0.1)
        raise AssertionError("server readiness timeout")

    def stop(self):
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
        if self.log is not None:
            self.log.close()
            self.log = None

    def enroll(self, device, token):
        _request(
            self.base,
            "/v1/devices",
            token=self.admin,
            payload={"device_id": device, "token": token},
        )

    def create_owner(self):
        revision = _revision(self.base, self.token_a, self.device_a)
        project = _command(
            self.base,
            self.token_a,
            self.device_a,
            "/v1/authoritative/project-commands",
            {
                "command_id": "scenario-d-project",
                "expected_revision": revision,
                "project_id": "project-scenario-d",
                "name": "Scenario D",
            },
        )
        revision = project["revision"]
        task = _command(
            self.base,
            self.token_a,
            self.device_a,
            "/v1/authoritative/task-commands",
            {
                "command_id": "scenario-d-task",
                "expected_revision": revision,
                "action": {
                    "type": "create",
                    "task_id": "task-scenario-d",
                    "title": "Attachment owner",
                    "project_id": "project-scenario-d",
                },
            },
        )
        assert task["task"]["id"] == "task-scenario-d"


@pytest.fixture
def deployment(tmp_path):
    if not CLI.is_file() or not SERVER.is_file():
        message = "orbit/orbit-server binaries are not built"
        if REQUIRE_EVIDENCE:
            pytest.fail(f"{message} (ORBIT_REQUIRE_CONNECTED_EVIDENCE=1 forbids a skipped run)")
        pytest.skip(message)
    port_socket = socket.socket()
    port_socket.bind(("127.0.0.1", 0))
    port = port_socket.getsockname()[1]
    port_socket.close()
    root = tmp_path / "scenario-d"
    root.mkdir()
    home = root / "home"
    home.mkdir()
    env = {key: value for key, value in os.environ.items() if not key.startswith("ORBIT_")}
    env.update(
        {
            "HOME": str(home),
            "ORBIT_CONFIG": str(root / "orbit.config.toml"),
            "ORBIT_DB": str(root / "unused-local.db"),
            "ORBIT_AUTHORITY_SERVER": f"http://127.0.0.1:{port}",
            "ORBIT_ATTACHMENT_MAX_BYTES": str(ATTACHMENT_LIMIT),
            "TERM": "xterm-256color",
            "LANG": "C.UTF-8",
        }
    )
    instance = Deployment(
        root=root,
        base=f"http://127.0.0.1:{port}",
        port=port,
        admin=secrets.token_hex(32),
        env=env,
        server_db=root / "server.db",
    )
    try:
        instance.start()
        instance.enroll(instance.device_a, instance.token_a)
        instance.enroll(instance.device_b, instance.token_b)
        instance.create_owner()
        yield instance
    finally:
        instance.stop()


def _server_attachments(instance, device=None, token=None):
    _, value = _request(
        instance.base,
        "/v1/authoritative/attachments?owner_kind=task&owner_id=task-scenario-d",
        token or instance.token_a,
        device or instance.device_a,
    )
    return value["attachments"]


def test_connected_attachment_scenario(deployment):
    instance = deployment
    owner = ("task", "task-scenario-d")
    first_bytes = b"scenario-d first attachment\n" + secrets.token_bytes(64)
    first_path = instance.root / "first.txt"
    first_path.write_bytes(first_bytes)

    # a. Connected CLI A imports an accepted attachment.
    added = _cli(
        instance.env,
        instance.replica_a,
        instance.device_a,
        instance.token_a,
        "add",
        *owner,
        str(first_path),
        "--description",
        "first",
    )
    assert added["accepted"] is True, added
    assert added["pending"] is False, added
    first = added["attachment"]
    assert first["checksum"] == _sha256(first_bytes)
    rows = _server_attachments(instance)
    assert [row["id"] for row in rows].count(first["id"]) == 1
    row = next(row for row in rows if row["id"] == first["id"])
    assert row["checksum"] == first["checksum"]
    assert row["size_bytes"] == len(first_bytes)
    assert _object_path(instance.server_db, first["checksum"]).is_file()
    assert _db_rows(instance.server_db, "SELECT COUNT(*) FROM attachments WHERE id=?", (first["id"],)) == [(1,)]
    print(f"PASS a: CLI A accepted {first['id']} ({first['checksum'][:12]}…)")

    # b. Device B lists and exports accepted bytes.
    listed = _cli(
        instance.env,
        instance.replica_b,
        instance.device_b,
        instance.token_b,
        "list",
        *owner,
    )
    assert listed["staged"] == [], listed
    assert [row["id"] for row in listed["accepted"]] == [first["id"]]
    export_path = instance.root / "device-b-export.txt"
    exported = _cli(
        instance.env,
        instance.replica_b,
        instance.device_b,
        instance.token_b,
        "export",
        first["id"],
        str(export_path),
    )
    assert exported["pending"] is False
    assert export_path.read_bytes() == first_bytes
    assert _sha256(export_path.read_bytes()) == first["checksum"]
    inspected = _cli(
        instance.env,
        instance.replica_b,
        instance.device_b,
        instance.token_b,
        "inspect",
        first["id"],
    )
    assert inspected["pending"] is False
    assert inspected["verified_size_bytes"] == len(first_bytes)
    print("PASS b: device B listed and exported identical bytes/checksum")

    # c. Server down: a new add stays pending; restart + flush accepts once.
    second_bytes = b"scenario-d second attachment\n" + secrets.token_bytes(64)
    second_path = instance.root / "second.txt"
    second_path.write_bytes(second_bytes)
    instance.stop()
    pending = _cli(
        instance.env,
        instance.replica_a,
        instance.device_a,
        instance.token_a,
        "add",
        *owner,
        str(second_path),
        "--description",
        "while offline",
    )
    assert pending["accepted"] is False, pending
    assert pending["pending"] is True, pending
    second_id = pending["attachment"]["id"]
    staged_rows = _db_rows(
        instance.server_db,
        "SELECT COUNT(*) FROM attachments WHERE id=?",
        (second_id,),
    )
    assert staged_rows == [(0,)], "pending upload must not exist on the server"
    local_staged = _db_rows(
        instance.replica_a,
        "SELECT state FROM attachment_uploads WHERE attachment_id=?",
        (second_id,),
    )
    assert local_staged == [("pending",)], local_staged
    instance.start()
    before_count = len(_server_attachments(instance))
    flushed = _cli(
        instance.env,
        instance.replica_a,
        instance.device_a,
        instance.token_a,
        "flush",
    )
    assert flushed["uploads_pending_before"] >= 1, flushed
    assert flushed["uploads_pending_after"] == 0, flushed
    after_rows = _server_attachments(instance)
    assert len(after_rows) == before_count + 1, (before_count, after_rows)
    assert [row["id"] for row in after_rows].count(second_id) == 1
    assert next(row for row in after_rows if row["id"] == second_id)["checksum"] == _sha256(second_bytes)
    local_after = _db_rows(
        instance.replica_a,
        "SELECT state FROM attachment_uploads WHERE attachment_id=?",
        (second_id,),
    )
    assert local_after == [("accepted",)], local_after
    print("PASS c: offline add reported pending, then flush accepted exactly once")

    # d1. Duplicate delivery with the same command ID is idempotent.
    duplicate_bytes = b"duplicate delivery payload\n" + secrets.token_bytes(32)
    duplicate_id = "attachment-scenario-d-duplicate"
    duplicate_activity = "activity-scenario-d-duplicate"
    command_id = "command-scenario-d-duplicate"
    revision = _revision(instance.base, instance.token_a, instance.device_a)
    path = (
        "/v1/authoritative/attachment-import"
        f"?command_id={command_id}&expected_revision={revision}"
        f"&attachment_id={duplicate_id}&activity_id={duplicate_activity}"
        "&owner_kind=task&owner_id=task-scenario-d&filename=duplicate.txt"
        "&content_type=text/plain"
    )
    request = urllib.request.Request(
        instance.base + path,
        data=duplicate_bytes,
        headers={
            "Authorization": "Bearer " + instance.token_a,
            "x-orbit-device-id": instance.device_a,
            "Content-Type": "text/plain",
        },
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        first_receipt = json.loads(response.read())
    with urllib.request.urlopen(request, timeout=10) as response:
        second_receipt = json.loads(response.read())
    assert first_receipt["duplicate"] is False, first_receipt
    assert second_receipt["duplicate"] is True, second_receipt
    assert second_receipt["revision"] == first_receipt["revision"]
    assert _db_rows(
        instance.server_db, "SELECT COUNT(*) FROM attachments WHERE id=?", (duplicate_id,)
    ) == [(1,)]
    assert _db_rows(
        instance.server_db,
        "SELECT COUNT(*) FROM activities WHERE id=?",
        (duplicate_activity,),
    ) == [(1,)]
    assert _object_path(instance.server_db, first_receipt["attachment"]["checksum"]).is_file()
    print("PASS d1: duplicate command ID returned duplicate=true with one row/activity/object")

    # d2. A truncated body is rejected before acceptance; the same command ID
    # may then deliver the complete body and produce one intact row.
    truncated_bytes = b"truncated then complete payload\n" + secrets.token_bytes(48)
    truncated_id = "attachment-scenario-d-truncated"
    truncated_activity = "activity-scenario-d-truncated"
    truncated_command = "command-scenario-d-truncated"
    revision = _revision(instance.base, instance.token_a, instance.device_a)
    truncated_path = (
        "/v1/authoritative/attachment-import"
        f"?command_id={truncated_command}&expected_revision={revision}"
        f"&attachment_id={truncated_id}&activity_id={truncated_activity}"
        "&owner_kind=task&owner_id=task-scenario-d&filename=truncated.txt"
        "&content_type=text/plain"
    )
    raw = socket.create_connection(("127.0.0.1", instance.port), timeout=10)
    head = (
        f"POST {truncated_path} HTTP/1.1\r\n"
        f"Host: 127.0.0.1:{instance.port}\r\n"
        f"Authorization: Bearer {instance.token_a}\r\n"
        f"x-orbit-device-id: {instance.device_a}\r\n"
        "Content-Type: text/plain\r\n"
        f"Content-Length: {len(truncated_bytes)}\r\n"
        "Connection: close\r\n\r\n"
    ).encode()
    raw.sendall(head + truncated_bytes[: len(truncated_bytes) // 2])
    raw.shutdown(socket.SHUT_WR)
    try:
        raw.recv(4096)
    finally:
        raw.close()
    assert _revision(instance.base, instance.token_a, instance.device_a) == revision
    assert _db_rows(
        instance.server_db, "SELECT COUNT(*) FROM attachments WHERE id=?", (truncated_id,)
    ) == [(0,)]
    full_request = urllib.request.Request(
        instance.base + truncated_path,
        data=truncated_bytes,
        headers={
            "Authorization": "Bearer " + instance.token_a,
            "x-orbit-device-id": instance.device_a,
            "Content-Type": "text/plain",
        },
        method="POST",
    )
    with urllib.request.urlopen(full_request, timeout=10) as response:
        receipt = json.loads(response.read())
    assert receipt["duplicate"] is False, receipt
    assert receipt["attachment"]["checksum"] == _sha256(truncated_bytes)
    assert _db_rows(
        instance.server_db, "SELECT COUNT(*) FROM attachments WHERE id=?", (truncated_id,)
    ) == [(1,)]
    assert _db_rows(
        instance.server_db,
        "SELECT COUNT(*) FROM activities WHERE id=?",
        (truncated_activity,),
    ) == [(1,)]
    _, content = _raw_content(instance, truncated_id)
    assert content == truncated_bytes
    print("PASS d2: truncated body rejected, retry with the same command ID accepted intact content")

    # e. No duplicate or corrupt content anywhere: one object per digest and
    # every listed row's stored checksum matches the bytes the server serves.
    final_rows = _server_attachments(instance)
    ids = [row["id"] for row in final_rows]
    assert len(ids) == len(set(ids)), ids
    for row in final_rows:
        _, body = _raw_content(instance, row["id"])
        assert _sha256(body) == row["checksum"] == _sha256(body)
        assert len(body) == row["size_bytes"]
        assert _db_rows(
            instance.server_db,
            "SELECT COUNT(*) FROM attachment_objects WHERE object_hash=?",
            (row["checksum"],),
        ) == [(1,)]
    assert _db_rows(
        instance.server_db,
        "SELECT COUNT(*) FROM attachments WHERE owner_kind='task' AND owner_id='task-scenario-d'",
    ) == [(len(final_rows),)]
    print(f"PASS e: {len(final_rows)} distinct rows, digests and byte counts all verified")


def _import_path(revision, command_id, attachment_id, activity_id, filename="test.txt"):
    return (
        "/v1/authoritative/attachment-import"
        f"?command_id={command_id}&expected_revision={revision}"
        f"&attachment_id={attachment_id}&activity_id={activity_id}"
        "&owner_kind=task&owner_id=task-scenario-d"
        f"&filename={urllib.parse.quote(filename)}&content_type=text/plain"
    )


def test_removal_replay_and_already_removed_are_idempotent(deployment):
    instance = deployment
    body = b"removal replay attachment\n" + secrets.token_bytes(32)
    attachment_id = "attachment-remove-replay"
    activity_id = "activity-remove-replay"
    import_command = "command-remove-replay-import"
    revision = _revision(instance.base, instance.token_a, instance.device_a)
    status, _, receipt = _raw_request(
        instance.base,
        _import_path(revision, import_command, attachment_id, activity_id),
        instance.token_a,
        instance.device_a,
        body,
    )
    assert status == 200, receipt
    assert receipt["attachment"]["checksum"] == _sha256(body)

    remove_command = "command-remove-replay"
    remove_activity = "activity-remove-replay-removal"
    revision = _revision(instance.base, instance.token_a, instance.device_a)
    remove_payload = {
        "command_id": remove_command,
        "expected_revision": revision,
        "attachment_id": attachment_id,
        "activity_id": remove_activity,
    }
    _, removed = _request(
        instance.base,
        "/v1/authoritative/attachment-remove",
        instance.token_a,
        instance.device_a,
        remove_payload,
    )
    assert removed["removed"] is True, removed
    assert removed["duplicate"] is False, removed
    assert removed["already_removed"] is False, removed
    assert removed["content_removed"] is True, removed
    assert _db_rows(
        instance.server_db, "SELECT COUNT(*) FROM attachments WHERE id=?", (attachment_id,)
    ) == [(0,)]
    assert _server_attachments(instance) == []

    # Replaying the same immutable command returns its original receipt and
    # mutates nothing, even though the attachment is now gone.
    _, replay = _request(
        instance.base,
        "/v1/authoritative/attachment-remove",
        instance.token_a,
        instance.device_a,
        remove_payload,
    )
    assert replay["removed"] is True, replay
    assert replay["duplicate"] is True, replay
    assert replay["already_removed"] is False, replay
    assert replay["content_removed"] is True, replay
    assert replay["revision"] == removed["revision"], (removed, replay)

    # A fresh removal command for the same absent attachment is a successful
    # no-op, not a permanent 404.
    revision = _revision(instance.base, instance.token_a, instance.device_a)
    _, already = _request(
        instance.base,
        "/v1/authoritative/attachment-remove",
        instance.token_a,
        instance.device_a,
        {
            "command_id": "command-remove-already-gone",
            "expected_revision": revision,
            "attachment_id": attachment_id,
            "activity_id": "activity-remove-already-gone",
        },
    )
    assert already["removed"] is True, already
    assert already["duplicate"] is False, already
    assert already["already_removed"] is True, already
    assert already["content_removed"] is False, already
    print("PASS: removal, duplicate replay and already-removed are idempotent")


def test_oversize_import_is_rejected_with_413(deployment):
    instance = deployment
    revision = _revision(instance.base, instance.token_a, instance.device_a)
    oversized = b"x" * (ATTACHMENT_LIMIT + 1024)
    status, _, payload = _raw_request(
        instance.base,
        _import_path(
            revision,
            "command-oversize",
            "attachment-oversize",
            "activity-oversize",
            "oversize.bin",
        ),
        instance.token_a,
        instance.device_a,
        oversized,
    )
    assert status == 413, (status, payload)
    assert _db_rows(
        instance.server_db,
        "SELECT COUNT(*) FROM attachments WHERE id=?",
        ("attachment-oversize",),
    ) == [(0,)]
    print(f"PASS: {len(oversized)}-byte body rejected with 413 under a {ATTACHMENT_LIMIT}-byte limit")


def test_unauthenticated_import_is_rejected_before_body(deployment):
    instance = deployment
    status, _, payload = _raw_request(
        instance.base,
        _import_path(
            0,
            "command-unauthenticated",
            "attachment-unauthenticated",
            "activity-unauthenticated",
        ),
        None,
        None,
        b"unauthenticated",
    )
    assert status == 401, (status, payload)
    assert _db_rows(
        instance.server_db,
        "SELECT COUNT(*) FROM attachments WHERE id=?",
        ("attachment-unauthenticated",),
    ) == [(0,)]
    print("PASS: unauthenticated import rejected with 401 before acceptance")


def test_download_sets_disposition_and_nosniff_headers(deployment):
    instance = deployment
    body = b"header check\n" + secrets.token_bytes(16)
    revision = _revision(instance.base, instance.token_a, instance.device_a)
    status, _, receipt = _raw_request(
        instance.base,
        _import_path(
            revision,
            "command-headers",
            "attachment-headers",
            "activity-headers",
            'report "final".txt',
        ),
        instance.token_a,
        instance.device_a,
        body,
    )
    assert status == 200, receipt
    headers, content = _raw_content(instance, "attachment-headers")
    assert content == body
    disposition = headers.get("Content-Disposition", "")
    assert disposition.startswith('attachment; filename="'), disposition
    assert "report" in disposition, disposition
    assert headers.get("X-Content-Type-Options") == "nosniff"
    assert headers.get("x-orbit-checksum") == _sha256(body)
    assert headers.get("x-orbit-size") == str(len(body))
    print("PASS: download carries Content-Disposition, nosniff and verification headers")


def test_ledger_payload_and_history_are_metadata_only(deployment):
    instance = deployment
    body = b"m" * (256 * 1024)
    attachment_id = "attachment-metadata-only"
    command_id = "command-metadata-only"
    revision = _revision(instance.base, instance.token_a, instance.device_a)
    status, _, receipt = _raw_request(
        instance.base,
        _import_path(revision, command_id, attachment_id, "activity-metadata-only", "large.bin"),
        instance.token_a,
        instance.device_a,
        body,
    )
    assert status == 200, receipt
    assert receipt["attachment"]["size_bytes"] == len(body)
    checksum = _sha256(body)
    assert _object_path(instance.server_db, checksum).is_file()

    rows = _db_rows(
        instance.server_db,
        "SELECT payload FROM authority_commands WHERE command_id=?",
        (command_id,),
    )
    assert len(rows) == 1, rows
    payload = rows[0][0]
    assert attachment_id in payload, payload[:200]
    assert checksum in payload, payload[:200]
    assert len(payload.encode("utf-8")) < 8 * 1024, len(payload)

    history = _db_rows(
        instance.server_db,
        "SELECT wire FROM authority_history WHERE wire LIKE ?",
        (f"%{attachment_id}%",),
    )
    assert history, "accepted attachment history row is missing"
    wire = history[-1][0]
    assert len(wire.encode("utf-8")) < 8 * 1024, len(wire)
    assert checksum in wire, wire[:200]
    print(
        "PASS: 256 KiB upload kept metadata-only ledger payload "
        f"({len(payload)} bytes) and history wire ({len(wire)} bytes)"
    )


def _raw_content(instance, attachment_id):
    request = urllib.request.Request(
        instance.base + f"/v1/authoritative/attachments/{attachment_id}/content",
        headers={
            "Authorization": "Bearer " + instance.token_a,
            "x-orbit-device-id": instance.device_a,
        },
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        return response.headers, response.read()
