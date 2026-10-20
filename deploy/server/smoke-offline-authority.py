#!/usr/bin/env python3
"""Isolated connected/offline acceptance for the native authoritative replica.

Proves: durable queueing while disconnected, ordered replay, server acceptance
only, idempotent retry, stale-revision conflict, reviewed resolution, device
ownership, and durable server restart. Uses only temporary loopback databases.
"""
import argparse
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


def expect_http(fn, statuses=(400, 409, 422)):
    try:
        fn()
    except urllib.error.HTTPError as exc:
        assert exc.code in statuses, f'unexpected HTTP {exc.code}'
        return exc.code
    raise AssertionError('unsafe request was accepted')


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

    with tempfile.TemporaryDirectory(prefix='orbit-offline-acceptance-') as directory:
        root = Path(directory)
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        base = f'http://127.0.0.1:{port}'
        admin = secrets.token_hex(32)
        server_db = root / 'server.db'
        env = {**os.environ, 'ORBIT_SERVER_DB': str(server_db),
               'ORBIT_SERVER_BIND': f'127.0.0.1:{port}',
               'ORBIT_SERVER_ADMIN_KEY': admin, 'NO_COLOR': '1'}
        process = None
        log = (root / 'server.log').open('w')

        def start_server():
            nonlocal process, log
            process = subprocess.Popen([server], env=env, stdout=log, stderr=subprocess.STDOUT)
            for _ in range(100):
                if process.poll() is not None:
                    raise AssertionError('server exited during startup:\n' + (root / 'server.log').read_text()[-3000:])
                try:
                    request(base, '/health')
                    return
                except (urllib.error.URLError, TimeoutError):
                    time.sleep(.1)
            raise AssertionError('server readiness timeout')

        def stop_server():
            nonlocal process
            if process is not None and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            process = None

        def cli(replica, *command, token=None, device=None, fail=False):
            replica_env = dict(env)
            if token:
                replica_env['ORBIT_SYNC_HTTP_TOKEN'] = token
            if device:
                replica_env['ORBIT_AUTHORITY_DEVICE'] = device
            result = subprocess.run(
                [orbit, '--json', 'sync', 'replica', '--db', str(replica),
                 '--server', base, *command],
                text=True, capture_output=True, env=replica_env, timeout=90)
            if fail:
                assert result.returncode != 0, (
                    f'{command!r} unexpectedly succeeded\n{result.stdout[-2500:]}')
                return result
            if result.returncode:
                raise AssertionError(
                    f'{command!r} exited {result.returncode}\n'
                    f'stdout={result.stdout[-2500:]}\nstderr={result.stderr[-2500:]}')
            parsed = json.loads(result.stdout)
            return parsed.get('result', parsed)

        try:
            start_server()
            devices = {}
            for name in ('offline-a', 'offline-b'):
                token = secrets.token_hex(32)
                request(base, '/v1/devices', token=admin,
                        payload={'device_id': name, 'token': token})
                devices[name] = token
            stop_server()

            replica_a = root / 'replica-a.db'
            cli(replica_a, 'queue', '--command-id', 'a-project', '--kind', 'project',
                '--action-json', json.dumps({'project_id': 'p-offline', 'name': 'Offline project'}))
            cli(replica_a, 'queue', '--command-id', 'a-task', '--kind', 'task',
                '--action-json', json.dumps({'type': 'create', 'task_id': 't-offline',
                                             'title': 'Offline task', 'project_id': 'p-offline'}))
            offline_status = cli(replica_a, 'status')
            assert offline_status['accepted_revision'] is None
            assert offline_status['pending_or_conflicted'] == 2
            snapshot = cli(replica_a, 'snapshot')
            assert snapshot['accepted'] is None, 'offline queue became locally accepted'
            pending = cli(replica_a, 'pending')
            assert len(pending['unresolved']) == 2
            audit = cli(replica_a, 'audit')
            assert audit['healthy'] is True
            assert audit['total_commands'] == 2
            assert audit['families']['project'] == 1
            assert audit['families']['task'] == 1

            start_server()
            flushed = cli(replica_a, 'flush', token=devices['offline-a'], device='offline-a')
            assert flushed['status'] == 'accepted' and flushed['accepted'] == 2, flushed
            refreshed = cli(replica_a, 'refresh', token=devices['offline-a'], device='offline-a')
            assert refreshed['accepted_revision'] == 2
            accepted = cli(replica_a, 'accepted', '--family', 'tasks')
            assert accepted['accepted'] is True and accepted['revision'] == 2
            assert [task['id'] for task in accepted['records']] == ['t-offline']
            for command_id in ('a-project', 'a-task'):
                receipt = cli(replica_a, 'receipt', '--command-id', command_id)
                assert receipt['command']['state'] == 'accepted', receipt
            audit = cli(replica_a, 'audit')
            assert audit['healthy'] is True and not audit['issues'], audit

            # Repeating the same flush must not append server history.
            duplicate = cli(replica_a, 'flush', token=devices['offline-a'], device='offline-a')
            assert duplicate['status'] == 'empty' and duplicate['accepted'] == 0

            # Lost acknowledgement: the server committed but the client never
            # recorded success. Replaying the same frozen identity must recover
            # the original receipt without appending a second history revision.
            cli(replica_a, 'queue', '--command-id', 'lost-ack', '--kind', 'task',
                '--action-json', json.dumps({'type': 'create', 'task_id': 'lost-ack-task',
                                             'title': 'Lost acknowledgement'}))
            lost = cli(replica_a, 'flush', token=devices['offline-a'], device='offline-a')
            assert lost['accepted'] == 1, lost
            cli(replica_a, 'refresh', token=devices['offline-a'], device='offline-a')
            assert cli(replica_a, 'status')['accepted_revision'] == 3
            with sqlite3.connect(replica_a) as connection:
                connection.execute(
                    "UPDATE authority_outbox SET state='pending', receipt=NULL "
                    "WHERE command_id='lost-ack'")
                connection.commit()
            recovered = cli(replica_a, 'flush', token=devices['offline-a'], device='offline-a')
            assert recovered['accepted'] == 1, recovered
            cli(replica_a, 'refresh', token=devices['offline-a'], device='offline-a')
            assert cli(replica_a, 'status')['accepted_revision'] == 3
            receipt = cli(replica_a, 'receipt', '--command-id', 'lost-ack')
            assert receipt['command']['state'] == 'accepted'

            # A second device freezes a now-stale revision while offline.
            replica_b = root / 'replica-b.db'
            cli(replica_b, 'refresh', token=devices['offline-b'], device='offline-b')
            cli(replica_b, 'queue', '--command-id', 'b-task', '--kind', 'task',
                '--action-json', json.dumps({'type': 'create', 'task_id': 'b-task',
                                             'title': 'Stale task'}))
            # Simulate a durable envelope frozen by an earlier submission attempt
            # that reached the server snapshot but whose command never arrived.
            with sqlite3.connect(replica_b) as connection:
                connection.execute(
                    "UPDATE authority_outbox SET expected_revision=3 "
                    "WHERE command_id='b-task'")
                connection.commit()
            cli(replica_a, 'queue', '--command-id', 'a-task-2', '--kind', 'task',
                '--action-json', json.dumps({'type': 'create', 'task_id': 'a-task-2',
                                             'title': 'Winning task'}))
            winner = cli(replica_a, 'flush', token=devices['offline-a'], device='offline-a')
            assert winner['accepted'] == 1, winner
            cli(replica_a, 'refresh', token=devices['offline-a'], device='offline-a')
            loser = cli(replica_b, 'flush', token=devices['offline-b'], device='offline-b')
            assert loser['status'] == 'conflict', loser
            assert loser['command_id'] == 'b-task'
            b_pending = cli(replica_b, 'pending')
            assert b_pending['unresolved'][0]['state'] == 'conflict'
            live = request(base, '/v1/authoritative/snapshot',
                           token=devices['offline-b'], device='offline-b')
            assert live['revision'] == 4
            assert [task['id'] for task in live['tasks']] == [
                'a-task-2', 'lost-ack-task', 't-offline'], live['tasks']

            # Review against the exact accepted history, then stage a replacement.
            review = cli(replica_b, 'review-conflict-online', '--command-id', 'b-task',
                         token=devices['offline-b'], device='offline-b')
            review_file = root / 'review.json'
            review_file.write_text(json.dumps(review), encoding='utf-8')
            assert review['rejected_id'] == 'b-task' and review['accepted_revision'] == 4
            resolution = cli(
                replica_b, 'resolve-reviewed', '--review-file', str(review_file),
                '--replacement-id', 'b-task-resolved', '--kind', 'task',
                '--action-json', json.dumps({'type': 'create', 'task_id': 'b-task-resolved',
                                             'title': 'Reviewed replacement'}),
                token=devices['offline-b'], device='offline-b')
            assert resolution['accepted'] is False and resolution['queued'] == 'b-task-resolved'
            resolved = cli(replica_b, 'flush', token=devices['offline-b'], device='offline-b')
            assert resolved['status'] == 'accepted' and resolved['accepted'] == 1, resolved
            cli(replica_b, 'refresh', token=devices['offline-b'], device='offline-b')
            accepted = cli(replica_b, 'accepted', '--family', 'tasks')
            assert accepted['revision'] == 5
            assert [task['id'] for task in accepted['records']] == [
                'a-task-2', 'b-task-resolved', 'lost-ack-task', 't-offline']

            # Command receipts are bound to the authenticated device: another
            # enrolled device cannot replay device A's command ID.
            expect_http(lambda: request(
                base, '/v1/authoritative/task-commands', token=devices['offline-b'],
                device='offline-b',
                payload={'command_id': 'a-task-2', 'expected_revision': 4,
                         'action': {'type': 'create', 'task_id': 'a-task-2',
                                    'title': 'Winning task'}}))
            # Server-side duplicate submission returns the original receipt and
            # does not advance the revision.
            retry = request(base, '/v1/authoritative/task-commands',
                            token=devices['offline-a'], device='offline-a',
                            payload={'command_id': 'a-task-2', 'expected_revision': 3,
                                     'action': {'type': 'create', 'task_id': 'a-task-2',
                                                'title': 'Winning task'}})
            assert retry.get('duplicate') is True, retry
            assert request(base, '/v1/authoritative/snapshot',
                           token=devices['offline-a'], device='offline-a')['revision'] == 5

            # Server restart must retain receipts and accepted state.
            stop_server()
            start_server()
            cli(replica_a, 'refresh', token=devices['offline-a'], device='offline-a')
            restarted = cli(replica_a, 'accepted', '--family', 'tasks')
            assert restarted['revision'] == 5
            assert len(restarted['records']) == 4
            receipt = cli(replica_a, 'receipt', '--command-id', 'a-task-2')
            assert receipt['command']['state'] == 'accepted'
            audit = cli(replica_a, 'audit')
            assert audit['healthy'] is True

            print('PASS offline authority: ordered durable replay, no local acceptance, '
                  'idempotent retry, stale conflict, reviewed resolution, device '
                  'ownership, restart durability')
        except Exception:
            log.flush()
            print((root / 'server.log').read_text()[-4000:], file=sys.stderr)
            raise
        finally:
            stop_server()
            log.close()


if __name__ == '__main__':
    main()
