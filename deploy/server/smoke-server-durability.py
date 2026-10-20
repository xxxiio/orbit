#!/usr/bin/env python3
"""Isolated server durability acceptance for the canonical authority.

Replaces the retired append-only relay smoke. Proves two-device convergence,
offline reconnect, token rotation, device revocation, SQLite backup/restore
durability, and fail-closed handling of a restored (rewound) server history.
Uses only temporary databases and a loopback server.
"""
import argparse
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def request(base, path, token=None, device=None, payload=None, method=None):
    headers = {}
    if token:
        headers['Authorization'] = 'Bearer ' + token
    if device:
        headers['x-orbit-device-id'] = device
    body = None if payload is None else json.dumps(payload).encode()
    if body is not None:
        headers['Content-Type'] = 'application/json'
    req = urllib.request.Request(base + path, data=body, headers=headers,
                                 method=method or ('POST' if body is not None else 'GET'))
    with urllib.request.urlopen(req, timeout=8) as response:
        raw = response.read()
        return json.loads(raw) if raw else None


def expect_status(fn, status):
    try:
        fn()
    except urllib.error.HTTPError as exc:
        assert exc.code == status, f'expected HTTP {status}, got {exc.code}'
        return
    raise AssertionError(f'expected HTTP {status}, request succeeded')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--orbit', default='target/debug/orbit')
    parser.add_argument('--server', default='target/debug/orbit-server')
    args = parser.parse_args()
    orbit = str(Path(args.orbit).resolve())
    server = str(Path(args.server).resolve())
    for binary in (orbit, server):
        if not Path(binary).is_file():
            parser.error(f'missing {binary}; build orbit-cli and orbit-server first')

    with tempfile.TemporaryDirectory(prefix='orbit-durability-acceptance-') as directory:
        root = Path(directory)
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        base = f'http://127.0.0.1:{port}'
        admin = secrets.token_hex(32)
        current_db = root / 'server.db'
        env = {**os.environ, 'ORBIT_SERVER_DB': str(current_db),
               'ORBIT_SERVER_BIND': f'127.0.0.1:{port}',
               'ORBIT_SERVER_ADMIN_KEY': admin, 'NO_COLOR': '1'}
        process = None
        log = (root / 'server.log').open('w')

        def start(database=current_db):
            nonlocal process, log
            server_env = {**env, 'ORBIT_SERVER_DB': str(database)}
            process = subprocess.Popen([server], env=server_env, stdout=log, stderr=subprocess.STDOUT)
            for _ in range(100):
                if process.poll() is not None:
                    raise AssertionError('server exited:\n' + (root / 'server.log').read_text()[-3000:])
                try:
                    request(base, '/health')
                    return
                except (urllib.error.URLError, TimeoutError):
                    time.sleep(.1)
            raise AssertionError('server readiness timeout')

        def stop():
            nonlocal process
            if process is not None and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            process = None

        devices = {}
        for name in ('durability-a', 'durability-b', 'durability-c'):
            devices[name] = secrets.token_hex(32)

        def enroll():
            for name, token in devices.items():
                request(base, '/v1/devices', token=admin,
                        payload={'device_id': name, 'token': token})

        def cli(replica, *command, name=None, token=None, fail=False):
            replica_env = dict(env)
            if token:
                replica_env['ORBIT_SYNC_HTTP_TOKEN'] = token
            if name:
                replica_env['ORBIT_AUTHORITY_DEVICE'] = name
            result = subprocess.run(
                [orbit, '--json', 'sync', 'replica', '--db', str(replica),
                 '--server', base, *command],
                text=True, capture_output=True, env=replica_env, timeout=90)
            if fail:
                assert result.returncode != 0, (
                    f'{command!r} unexpectedly succeeded\n{result.stdout[-2500:]}')
                return result
            if result.returncode:
                raise AssertionError(f'{command!r} exited {result.returncode}\n'
                                     f'{result.stdout[-2500:]}\n{result.stderr[-2500:]}')
            parsed = json.loads(result.stdout)
            return parsed.get('result', parsed)

        try:
            start()
            enroll()
            replica_a = root / 'durability-a.db'
            replica_b = root / 'durability-b.db'
            for replica, name in ((replica_a, 'durability-a'), (replica_b, 'durability-b')):
                cli(replica, 'queue', '--command-id', f'{name}-project', '--kind', 'project',
                    '--action-json', json.dumps({'project_id': name, 'name': name}))
                cli(replica, 'flush', name=name, token=devices[name])
                cli(replica, 'refresh', name=name, token=devices[name])

            # Two-device convergence after offline reconnect.
            stop()
            cli(replica_b, 'queue', '--command-id', 'b-offline-task', '--kind', 'task',
                '--action-json', json.dumps({'type': 'create', 'task_id': 'b-offline-task',
                                             'title': 'Offline work', 'project_id': 'durability-b'}))
            assert cli(replica_b, 'status')['pending_or_conflicted'] == 1
            start()
            cli(replica_b, 'reconnect', name='durability-b', token=devices['durability-b'])
            cli(replica_a, 'refresh', name='durability-a', token=devices['durability-a'])
            accepted = cli(replica_a, 'accepted', '--family', 'tasks')
            assert accepted['revision'] == 3
            assert [task['id'] for task in accepted['records']] == ['b-offline-task']

            # Token rotation invalidates the old credential while retaining identity.
            rotated = secrets.token_hex(32)
            request(base, '/v1/devices/durability-b/token', token=admin,
                    payload={'token': rotated})
            expect_status(lambda: request(base, '/v1/authoritative/snapshot',
                                          token=devices['durability-b'],
                                          device='durability-b'), 401)
            devices['durability-b'] = rotated
            request(base, '/v1/authoritative/snapshot', token=rotated,
                    device='durability-b')

            # Revocation removes the device's authority entirely.
            request(base, '/v1/devices/durability-c', token=admin, method='DELETE')
            expect_status(lambda: request(base, '/v1/authoritative/snapshot',
                                          token=devices['durability-c'],
                                          device='durability-c'), 401)

            # Durable SQLite backup, then advance the live history beyond it.
            backup = root / 'snapshot.db'
            backup_result = subprocess.run(
                [server, 'backup', str(backup)],
                text=True, capture_output=True,
                env={**env, 'ORBIT_SERVER_DB': str(current_db)}, timeout=60)
            assert backup_result.returncode == 0, backup_result.stderr
            assert backup.is_file()
            cli(replica_a, 'queue', '--command-id', 'a-after-backup', '--kind', 'task',
                '--action-json', json.dumps({'type': 'create', 'task_id': 'a-after-backup',
                                             'title': 'After backup', 'project_id': 'durability-a'}))
            cli(replica_a, 'flush', name='durability-a', token=devices['durability-a'])
            cli(replica_a, 'refresh', name='durability-a', token=devices['durability-a'])
            assert cli(replica_a, 'status')['accepted_revision'] == 4

            # Restore the older snapshot into a NEW path while the server is stopped.
            stop()
            rewound_db = root / 'rewound.db'
            restore = subprocess.run(
                [server, 'restore', str(backup)], text=True, capture_output=True,
                env={**env, 'ORBIT_SERVER_DB': str(rewound_db)}, timeout=60)
            assert restore.returncode == 0, restore.stderr
            assert rewound_db.is_file()
            start(rewound_db)
            rewound = request(base, '/v1/authoritative/snapshot',
                              token=devices['durability-a'], device='durability-a')
            assert rewound['revision'] == 3
            assert [task['id'] for task in rewound['tasks']] == ['b-offline-task']

            # A replica that observed the newer history must fail closed rather
            # than silently accepting the rewound server state.
            conflict = cli(replica_a, 'refresh', name='durability-a',
                           token=devices['durability-a'], fail=True)
            combined = (conflict.stdout + conflict.stderr).lower()
            assert any(word in combined for word in
                           ('revision', 'conflict', 'history', 'recovery', 'snapshot')), combined
            # A fresh replica accepts the restored server state as authoritative.
            fresh = root / 'fresh-replica.db'
            cli(fresh, 'refresh', name='durability-a', token=devices['durability-a'])
            assert cli(fresh, 'status')['accepted_revision'] == 3

            # Restart the restored server: accepted state and receipts are durable.
            stop()
            start(rewound_db)
            assert request(base, '/v1/authoritative/snapshot',
                           token=devices['durability-a'], device='durability-a')['revision'] == 3
            print('PASS server durability: two-device convergence, offline reconnect, '
                  'token rotation, revocation, backup/restore, fail-closed rewind, restart')
        except Exception:
            log.flush()
            print((root / 'server.log').read_text()[-4000:], file=sys.stderr)
            raise
        finally:
            stop()
            log.close()


if __name__ == '__main__':
    main()
