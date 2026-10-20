#!/usr/bin/env python3
"""RETIRED legacy relay smoke (not part of M15 acceptance).

This script targets the append-only `/v1/events` relay and the removed
`orbit sync init|http|push|pull` commands. The relay architecture was retired in
M24; current acceptance lives in `smoke-server-durability.py` (canonical server
backup/restore, rotation, revocation) and `smoke-offline-authority.py`
(canonical replica outbox replay). Kept only as a historical reference.

Opt-in M16 HTTP two-client smoke; requires built orbit and orbit-server binaries.
Runs only against a temporary loopback relay and isolated databases.
No external network, accounts, or developer data touched.
"""
import argparse
import json
import shutil
import os
import secrets
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path


def invoke(*args, env=None):
    result = subprocess.run(args, text=True, capture_output=True, env=env, timeout=40)
    if result.returncode:
        raise RuntimeError(f"command failed: {args[0]} {args[1:]}\n{result.stderr[-2500:]}")
    return result.stdout


def api(url, path, *, token=None, device=None, payload=None, method=None):
    headers = {}
    if token:
        headers['Authorization'] = f'Bearer {token}'
    if device:
        headers['x-orbit-device-id'] = device
    data = None
    if payload is not None:
        headers['Content-Type'] = 'application/json'
        data = json.dumps(payload).encode()
    req = urllib.request.Request(url + path, data=data, headers=headers,
                                 method=method or ('POST' if data is not None else 'GET'))
    with urllib.request.urlopen(req, timeout=4) as response:
        data = response.read()
        return json.loads(data) if data else None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--orbit', default='target/debug/orbit')
    parser.add_argument('--server', default='target/debug/orbit-server')
    args = parser.parse_args()
    orbit, server = (str(Path(x).resolve()) for x in (args.orbit, args.server))
    if shutil.which('git') is None:
        parser.error('git is required for mixed-transport smoke')
    for binary in (orbit, server):
        if not Path(binary).is_file():
            parser.error(f'missing {binary}; first run cargo build -p orbit-cli -p orbit-server')
    with tempfile.TemporaryDirectory(prefix='orbit-m16-smoke-') as directory:
        tmp = Path(directory)
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        url = f'http://127.0.0.1:{port}'
        admin = secrets.token_hex(32)
        env = {**os.environ, 'ORBIT_SERVER_ADMIN_KEY': admin,
               'ORBIT_SERVER_DB': str(tmp/'relay.db'), 'ORBIT_SERVER_BIND': f'127.0.0.1:{port}'}
        proc = subprocess.Popen([server], env=env, stdout=subprocess.DEVNULL,
                                stderr=(tmp/'server.log').open('w'))
        try:
            for attempt in range(60):
                if proc.poll() is not None:
                    raise RuntimeError('server stopped before health check')
                try:
                    api(url, '/health')
                    break
                except (urllib.error.URLError, TimeoutError):
                    time.sleep(.1)
            else:
                raise RuntimeError('server health timeout')
            clients = []
            for index in (1, 2):
                name = f'm16-smoke-device-{index}'
                token = secrets.token_hex(32)
                db, history = tmp/f'{name}.db', tmp/f'{name}-history'
                api(url, '/v1/devices', token=admin, payload={'device_id': name, 'token': token})
                invoke(orbit, '--db', str(db), 'sync', 'init', '--root', str(history), '--device', name)
                branch = invoke('git', '-C', str(history), 'symbolic-ref', '--short', 'HEAD').strip()
                assert branch == 'main', f'{name}: sync init selected {branch!r}, expected main'
                clients.append((name, token, db, history))

            def sync(client):
                name, token, db, history = client
                invoke(orbit, '--db', str(db), 'sync', 'http', '--root', str(history),
                       '--server', url, env={**env, 'ORBIT_SYNC_HTTP_TOKEN': token})

            def tasks(client):
                return json.loads(invoke(orbit, '--json', '--db', str(client[2]), 'task', 'list'))

            first, second = clients
            invoke(orbit, '--db', str(first[2]), 'task', 'add', 'M16 smoke alpha')
            sync(first)
            sync(second)
            assert 'M16 smoke alpha' in json.dumps(tasks(second)), 'A -> B transport failed'
            # Device B creates a local change while not communicating with the relay.
            invoke(orbit, '--db', str(second[2]), 'task', 'add', 'M16 smoke offline beta')
            sync(second)
            sync(first)
            assert 'M16 smoke offline beta' in json.dumps(tasks(first)), 'B -> A replay failed'
            # Mix transports using one bare repository. Device A publishes via
            # Git; device B imports via Git, then both use HTTP without changing
            # original (device, sequence) identities.
            git_remote = tmp/'sync-remote.git'
            invoke('git', 'init', '--bare', '--initial-branch=main', str(git_remote))
            for client in clients:
                history = client[3]
                invoke('git', '-C', str(history), 'config', 'user.name', 'Orbit Smoke')
                invoke('git', '-C', str(history), 'config', 'user.email', 'orbit-smoke@example.invalid')
                invoke('git', '-C', str(history), 'remote', 'add', 'origin', str(git_remote))
            invoke(orbit, '--db', str(first[2]), 'task', 'add', 'M16 Git-only gamma')
            invoke(orbit, '--db', str(first[2]), 'sync', 'push', '--root', str(first[3]))
            # The second device must fetch the Git history before it publishes
            # its own branch; it has a separate local Git repository.
            invoke(orbit, '--db', str(second[2]), 'sync', 'pull', '--root', str(second[3]))
            assert 'M16 Git-only gamma' in json.dumps(tasks(second)), 'Git A -> B replay failed'
            # Conversely, a change originating through HTTP may subsequently be
            # committed and shared with a Git-only client; Git's semantic artifacts
            # must be identical, not regenerated under the importing device ID.
            invoke(orbit, '--db', str(second[2]), 'task', 'add', 'M16 HTTP-to-Git delta')
            sync(second)
            sync(first)
            assert 'M16 HTTP-to-Git delta' in json.dumps(tasks(first))
            invoke(orbit, '--db', str(first[2]), 'sync', 'push', '--root', str(first[3]))
            invoke(orbit, '--db', str(second[2]), 'sync', 'pull', '--root', str(second[3]))
            assert 'M16 HTTP-to-Git delta' in json.dumps(tasks(second))
            # HTTP retrieval of the same Git artifact must not duplicate relay
            # history; only authors are permitted to publish their own records.
            sync(first); sync(second)
            mixed_before = api(url, '/v1/events?after=0&limit=1000', token=first[1], device=first[0])
            sync(first); sync(second)
            mixed_after = api(url, '/v1/events?after=0&limit=1000', token=first[1], device=first[0])
            assert mixed_before['tip'] == mixed_after['tip'], 'mixed transports duplicated relay history'
            # Credential rotation through the live HTTP API: retained device
            # identity must continue syncing, while the old token is rejected.
            rotated = secrets.token_hex(32)
            api(url, f'/v1/devices/{second[0]}/token', token=admin,
                payload={'token': rotated})
            try:
                api(url, '/v1/events?after=0&limit=1', token=second[1], device=second[0])
            except urllib.error.HTTPError as exc:
                assert exc.code == 401, f'expected rejected old token, got {exc.code}'
            else:
                raise AssertionError('rotated old device token still works')
            second = (second[0], rotated, second[2], second[3])
            clients[1] = second
            sync(second)
            # Attempting to reuse an existing sequence with altered bytes must
            # fail, without changing the append-only relay tip.
            before_conflict = api(url, '/v1/events?after=0&limit=1000',
                                  token=first[1], device=first[0])
            own = next(e for e in before_conflict['events']
                       if e['device_id'] == first[0] and e['kind'] == 'change')
            try:
                api(url, '/v1/events', token=first[1], device=first[0],
                    payload={'kind': 'change', 'wire': own['wire'] + ' '})
            except urllib.error.HTTPError as exc:
                assert exc.code in (400, 409), f'unexpected immutable-event rejection: {exc.code}'
            else:
                raise AssertionError('altered event was accepted')
            assert api(url, '/v1/events?after=0&limit=1000',
                       token=first[1], device=first[0])['tip'] == before_conflict['tip']
            # Snapshot the relay before the following new event. This is a real
            # SQLite backup, not a filesystem copy of a potentially live WAL.
            backup = tmp/'relay-backup.db'
            invoke(server, 'backup', str(backup), env=env)
            assert backup.is_file(), 'server backup was not created'
            # Publish an event that will be absent when the old snapshot is restored.
            invoke(orbit, '--db', str(first[2]), 'task', 'add', 'M16 after snapshot')
            sync(first)
            sync(second)
            assert 'M16 after snapshot' in json.dumps(tasks(second))
            # Restart with the same durable relay database, then repeat exchanges.
            proc.terminate()
            proc.wait(timeout=10)
            proc = subprocess.Popen([server], env=env, stdout=subprocess.DEVNULL,
                                    stderr=(tmp/'server-restart.log').open('w'))
            for _ in range(60):
                try:
                    api(url, '/health')
                    break
                except (urllib.error.URLError, TimeoutError):
                    time.sleep(.1)
            else:
                raise RuntimeError('restart health timeout')
            sync(first)
            sync(second)
            assert 'M16 smoke alpha' in json.dumps(tasks(second))
            assert 'M16 smoke offline beta' in json.dumps(tasks(first))
            # Repeated exchange must not append duplicate server history.
            before = api(url, '/v1/events?after=0&limit=1000', token=first[1], device=first[0])
            sync(first); sync(second)
            after = api(url, '/v1/events?after=0&limit=1000', token=first[1], device=first[0])
            assert before['tip'] == after['tip'], 'idempotent sync appended duplicate history'
            # Restore the older backup into a NEW database path while stopped.
            # The server preserves generation but its tip goes backwards; clients
            # must invalidate the advanced cursor and originating devices must
            # republish immutable artifacts missing from the restored relay.
            proc.terminate()
            proc.wait(timeout=10)
            restored = tmp/'restored-relay.db'
            restore_env = {**env, 'ORBIT_SERVER_DB': str(restored)}
            invoke(server, 'restore', str(backup), env=restore_env)
            proc = subprocess.Popen([server], env=restore_env, stdout=subprocess.DEVNULL,
                                    stderr=(tmp/'server-restored.log').open('w'))
            for _ in range(60):
                if proc.poll() is not None:
                    raise RuntimeError('restored server stopped before health check')
                try:
                    api(url, '/health')
                    break
                except (urllib.error.URLError, TimeoutError):
                    time.sleep(.1)
            else:
                raise RuntimeError('restored server health timeout')
            # Local immutable authorship history is still present on client A.
            sync(first)
            sync(second)
            assert 'M16 after snapshot' in json.dumps(tasks(second)), 'restore lost local state'
            restored_page = api(url, '/v1/events?after=0&limit=1000', token=first[1], device=first[0])
            assert restored_page['tip'] >= before['tip'], 'originating device did not republish history'
            print('PASS: bidirectional sync, offline reconnect, mixed Git/HTTP, credential rotation, immutable replay, restart, backup/restore')
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    proc.kill(); proc.wait()


if __name__ == '__main__':
    main()
