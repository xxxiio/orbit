#!/usr/bin/env python3
"""Server-authoritative attachment smoke test.

Starts a disposable `orbit-server`, enrolls a device, creates one project and
one task, then exercises the real HTTP attachment routes:

  * import a small file and verify the returned metadata/checksum
  * reject an unauthenticated import with 401 before any body buffering
  * retry the same command_id and verify duplicate=true with no second row,
    activity, or content object
  * list, download, and verify the raw bytes and integrity/download headers
  * remove and verify the shared content object is deleted
  * retry the import after removal and verify removed=true with null metadata
  * download after removal and require 404
  * retry the removal command id and remove again with a fresh command id,
    requiring 200 with duplicate/already_removed semantics

Only the Python standard library is used. The temporary database and object
store are deleted afterwards; no user workspace is touched.
"""
import hashlib
import json
import os
from pathlib import Path
import secrets
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request


def pick_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


class Server:
    def __init__(self, base_url, admin):
        self.base_url = base_url
        self.admin = admin

    def call(self, path, *, token=None, device=None, body=None,
             content_type='application/json', method=None):
        """Return (status, headers, raw_body); never raises for 4xx/5xx."""
        headers = {}
        if token:
            headers['Authorization'] = 'Bearer ' + token
        if device:
            headers['x-orbit-device-id'] = device
        data = None
        if body is not None:
            data = body if isinstance(body, bytes) else json.dumps(body).encode()
            headers['Content-Type'] = content_type
        request = urllib.request.Request(
            self.base_url + path, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=10) as response:
                return (response.status,
                        {name.lower(): value for name, value in response.headers.items()},
                        response.read())
        except urllib.error.HTTPError as error:
            return (error.code,
                    {name.lower(): value for name, value in error.headers.items()},
                    error.read())

    def json(self, path, **options):
        status, headers, body = self.call(path, **options)
        parsed = json.loads(body) if body else None
        return status, parsed, headers


def content_object_count(root):
    objects = root / 'attachments' / 'objects'
    if not objects.is_dir():
        return 0
    return sum(1 for path in objects.rglob('*') if path.is_file())


def attachment_count(db_path):
    with sqlite3.connect(f'file:{db_path}?mode=ro', uri=True) as conn:
        return conn.execute('SELECT COUNT(*) FROM attachments').fetchone()[0]


def main():
    server_binary = Path(
        sys.argv[1] if len(sys.argv) > 1 else 'target/debug/orbit-server').resolve()
    if not server_binary.is_file():
        raise SystemExit(f'build orbit-server first: missing {server_binary}')

    with tempfile.TemporaryDirectory(prefix='orbit-attachments-smoke-') as directory:
        root = Path(directory)
        db_path = root / 'server.db'
        port = pick_port()
        base_url = f'http://127.0.0.1:{port}'
        admin = secrets.token_hex(32)
        device_id = 'attachment-smoke-device'
        device_token = secrets.token_hex(32)
        max_bytes = 1_048_576
        env = {
            **os.environ,
            'ORBIT_SERVER_DB': str(db_path),
            'ORBIT_SERVER_BIND': f'127.0.0.1:{port}',
            'ORBIT_SERVER_ADMIN_KEY': admin,
            'ORBIT_ATTACHMENT_MAX_BYTES': str(max_bytes),
        }
        log_path = root / 'server.log'
        with log_path.open('w') as log:
            process = subprocess.Popen(
                [str(server_binary)], env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                server = Server(base_url, admin)
                for _ in range(100):
                    if process.poll() is not None:
                        raise AssertionError('orbit-server exited during startup')
                    try:
                        status, _, _ = server.call('/health')
                        if status == 200:
                            break
                    except (urllib.error.URLError, TimeoutError, ConnectionError):
                        pass
                    time.sleep(0.1)
                else:
                    raise AssertionError('orbit-server did not become healthy')

                status, body, _ = server.json(
                    '/v1/devices', token=admin,
                    body={'device_id': device_id, 'token': device_token})
                assert status in (200, 201), (status, body)
                print('PASS enrolled a disposable device')

                # Project (Basic default workflow) and one task to own files.
                status, project, _ = server.json(
                    '/v1/authoritative/project-commands',
                    token=device_token, device=device_id,
                    body={
                        'command_id': 'smoke-project-create',
                        'expected_revision': 0,
                        'project_id': 'project-smoke',
                        'name': 'Attachment smoke project',
                    })
                assert status == 200, (status, project)
                assert project['revision'] == 1, project
                assert project['project']['name'] == 'Attachment smoke project'
                print('PASS created project (Basic default workflow) at revision 1')

                status, task, _ = server.json(
                    '/v1/authoritative/task-commands',
                    token=device_token, device=device_id,
                    body={
                        'command_id': 'smoke-task-create',
                        'expected_revision': 1,
                        'action': {
                            'type': 'create',
                            'task_id': 'task-smoke',
                            'title': 'Attachment smoke task',
                            'project_id': 'project-smoke',
                        },
                    })
                assert status == 200, (status, task)
                assert task['revision'] == 2, task
                print('PASS created task owner at revision 2')

                # Unknown owner kinds and unknown owners are rejected before
                # any bytes or metadata are written.
                status, body, _ = server.json(
                    '/v1/authoritative/attachments'
                    '?owner_kind=bogus&owner_id=task-smoke',
                    token=device_token, device=device_id)
                assert status == 400, (status, body)
                assert 'task, project, work_item, knowledge, or agent_job' in body['error']
                print('PASS unknown owner_kind rejected with 400 and allowed values')

                payload = b'Orbit attachment smoke payload.\n'
                checksum = hashlib.sha256(payload).hexdigest()
                query = urllib.parse.urlencode({
                    'command_id': 'smoke-import-1',
                    'expected_revision': 2,
                    'attachment_id': 'attachment-smoke',
                    'activity_id': 'activity-smoke-import',
                    'owner_kind': 'task',
                    'owner_id': 'task-smoke',
                    'filename': 'smoke note.txt',
                    'content_type': 'text/plain',
                    'description': 'Smoke evidence',
                })
                import_path = '/v1/authoritative/attachment-import?' + query

                status, missing, _ = server.json(
                    '/v1/authoritative/attachment-import?' + urllib.parse.urlencode({
                        'command_id': 'smoke-import-missing',
                        'expected_revision': 2,
                        'attachment_id': 'attachment-missing',
                        'activity_id': 'activity-missing',
                        'owner_kind': 'task',
                        'owner_id': 'task-missing',
                        'filename': 'missing.txt',
                    }), token=device_token, device=device_id, body=payload,
                    content_type='application/octet-stream')
                assert status == 404, (status, missing)
                assert 'task-missing' in missing['error'] and 'does not exist' in missing['error']
                print('PASS missing owner rejected with actionable 404')

                # Authentication is checked before the body is buffered, so an
                # unauthenticated caller cannot force a large allocation.
                status, unauth, _ = server.json(
                    import_path, body=payload,
                    content_type='application/octet-stream')
                assert status == 401, (status, unauth)
                print('PASS unauthenticated import rejected with 401')

                status, imported, _ = server.json(
                    import_path, token=device_token, device=device_id,
                    body=payload, content_type='application/octet-stream')
                assert status == 200, (status, imported)
                assert imported['revision'] == 3, imported
                assert imported['duplicate'] is False, imported
                assert imported['removed'] is False, imported
                attachment = imported['attachment']
                assert attachment == {
                    'id': 'attachment-smoke',
                    'owner_kind': 'task',
                    'owner_id': 'task-smoke',
                    'filename': 'smoke note.txt',
                    'content_type': 'text/plain',
                    'size_bytes': len(payload),
                    'checksum': checksum,
                    'created_at_ms': attachment['created_at_ms'],
                    'description': 'Smoke evidence',
                    'is_image': False,
                }, attachment
                assert attachment_count(db_path) == 1
                assert content_object_count(root) == 1
                print('PASS imported attachment at revision 3 (metadata + one content object)')

                # Idempotent retry: same command_id, same bytes, stale revision.
                status, retried, _ = server.json(
                    import_path, token=device_token, device=device_id,
                    body=payload, content_type='application/octet-stream')
                assert status == 200, (status, retried)
                assert retried['duplicate'] is True, retried
                assert retried['revision'] == 3, retried
                assert retried['removed'] is False, retried
                assert retried['attachment'] == attachment, retried
                assert attachment_count(db_path) == 1
                assert content_object_count(root) == 1
                with sqlite3.connect(f'file:{db_path}?mode=ro', uri=True) as conn:
                    activities = conn.execute(
                        "SELECT subject_kind, subject_id, activity_kind FROM activities"
                    ).fetchall()
                assert activities == [
                    ('task', 'task-smoke', 'created'),
                    ('task', 'task-smoke', 'attachment_added'),
                ], activities
                print('PASS duplicate import returned the stored receipt with one row/activity')

                status, listed, _ = server.json(
                    '/v1/authoritative/attachments?'
                    + urllib.parse.urlencode({'owner_kind': 'task', 'owner_id': 'task-smoke'}),
                    token=device_token, device=device_id)
                assert status == 200, (status, listed)
                assert listed == {'attachments': [attachment]}, listed
                print('PASS listed one attachment for the task owner')

                status, headers, content, = server.call(
                    '/v1/authoritative/attachments/attachment-smoke/content',
                    token=device_token, device=device_id)
                assert status == 200, status
                assert content == payload, content
                assert headers.get('content-type', '').startswith('text/plain')
                assert headers.get('x-orbit-checksum') == checksum, headers
                assert headers.get('x-orbit-size') == str(len(payload)), headers
                assert headers.get('content-disposition') == (
                    'attachment; filename="smoke note.txt"'), headers
                assert headers.get('x-content-type-options') == 'nosniff', headers
                print('PASS downloaded verified bytes with checksum and size headers')

                status, removed, _ = server.json(
                    '/v1/authoritative/attachment-remove',
                    token=device_token, device=device_id,
                    body={
                        'command_id': 'smoke-remove-1',
                        'expected_revision': 3,
                        'attachment_id': 'attachment-smoke',
                        'activity_id': 'activity-smoke-remove',
                    })
                assert status == 200, (status, removed)
                assert removed == {
                    'revision': 4, 'removed': True, 'duplicate': False,
                    'content_removed': True, 'already_removed': False}, removed
                assert attachment_count(db_path) == 0
                assert content_object_count(root) == 0
                print('PASS removal deleted the unreferenced content object at revision 4')

                # A lost-ack import retry after the attachment was removed
                # reports the original acceptance without inventing metadata.
                status, late_retry, _ = server.json(
                    import_path, token=device_token, device=device_id,
                    body=payload, content_type='application/octet-stream')
                assert status == 200, (status, late_retry)
                assert late_retry == {
                    'revision': 3, 'duplicate': True, 'removed': True,
                    'attachment': None}, late_retry
                print('PASS duplicate import after removal reports removed metadata')

                status, _, _ = server.call(
                    '/v1/authoritative/attachments/attachment-smoke/content',
                    token=device_token, device=device_id)
                assert status == 404, status
                print('PASS download after removal returns 404')

                # The removal receipt is also idempotent.
                status, again, _ = server.json(
                    '/v1/authoritative/attachment-remove',
                    token=device_token, device=device_id,
                    body={
                        'command_id': 'smoke-remove-1',
                        'expected_revision': 3,
                        'attachment_id': 'attachment-smoke',
                        'activity_id': 'activity-smoke-remove',
                    })
                assert status == 200, (status, again)
                assert again['duplicate'] is True, again
                assert again['revision'] == removed['revision'], again
                assert again['removed'] is True, again
                assert again['content_removed'] == removed['content_removed'], again
                assert again['already_removed'] is False, again
                print('PASS duplicate removal returned the stored receipt')

                # A fresh command id for an already-removed attachment is a
                # successful no-op rather than a 404.
                status, already, _ = server.json(
                    '/v1/authoritative/attachment-remove',
                    token=device_token, device=device_id,
                    body={
                        'command_id': 'smoke-remove-2',
                        'expected_revision': 4,
                        'attachment_id': 'attachment-smoke',
                        'activity_id': 'activity-smoke-remove-2',
                    })
                assert status == 200, (status, already)
                assert already == {
                    'revision': 4, 'removed': True, 'duplicate': False,
                    'content_removed': False, 'already_removed': True}, already
                print('PASS removal of an already-removed attachment is idempotent')

                print('PASS all attachment smoke checks passed')
            except Exception:
                print(log_path.read_text()[-4000:], file=sys.stderr)
                raise
            finally:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


if __name__ == '__main__':
    main()
