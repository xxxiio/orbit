#!/usr/bin/env python3
"""Isolated blueprint acceptance: local and authenticated connected modes.

Covers bundled publication, immutable version pinning, JSON/YAML authoring,
typed properties, declarative guards, workflow transitions, templates,
offline durable replay, command receipts, device ownership, and restart
durability. Uses only temporary databases and a loopback server.
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

ROOT = Path(__file__).resolve().parents[2]

CUSTOM = {
    'id': 'acceptance', 'version': 1, 'name': 'Acceptance',
    'description': 'Guard and template acceptance fixture',
    'required_capabilities': [],
    'work_items': [{
        'id': 'gated', 'name': 'Gated', 'initial_state': 'draft',
        'states': ['draft', 'approved', 'done'],
        'fields': [{'id': 'approved', 'kind': {'type': 'boolean'},
                    'required': False, 'default': False}],
        'transitions': [
            {'from': 'draft', 'to': 'approved',
             'guards': [{'type': 'property_equals', 'field': 'approved', 'value': True}]},
            {'from': 'approved', 'to': 'done'},
        ],
    }],
    'templates': [{
        'id': 'starter', 'name': 'Starter',
        'items': [
            {'key': 'one', 'kind': 'gated', 'properties': {'approved': False}},
            {'key': 'two', 'kind': 'gated', 'properties': {'approved': False},
             'prerequisites': ['one']},
        ],
    }],
}


def request(base, path, token=None, device=None, payload=None):
    headers = {}
    if token:
        headers['Authorization'] = 'Bearer ' + token
    if device:
        headers['x-orbit-device-id'] = device
    body = None if payload is None else json.dumps(payload).encode()
    if body is not None:
        headers['Content-Type'] = 'application/json'
    req = urllib.request.Request(base + path, data=body, headers=headers,
                                 method='POST' if body is not None else 'GET')
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

    with tempfile.TemporaryDirectory(prefix='orbit-blueprint-acceptance-') as directory:
        root = Path(directory)
        custom = root / 'acceptance.json'
        custom.write_text(json.dumps(CUSTOM), encoding='utf-8')
        guarded = json.loads(json.dumps(CUSTOM))
        guarded['required_capabilities'] = ['agent.launch']
        guarded['work_items'][0]['transitions'][0]['actions'] = [{
            'id': 'launch', 'capability': 'agent.launch', 'description': 'offer only'}]
        guarded_file = root / 'unauthorized.json'
        guarded_file.write_text(json.dumps(guarded), encoding='utf-8')

        def local(*command, fail=False):
            result = subprocess.run(
                [orbit, '--json', '--db', str(root / 'local.db'), *command],
                text=True, capture_output=True, timeout=60)
            if fail:
                assert result.returncode != 0, (
                    f'{command!r} unexpectedly succeeded\n{result.stdout[-2500:]}')
                return result
            if result.returncode:
                raise AssertionError(f'{command!r} exited {result.returncode}\n'
                                     f'{result.stdout[-2500:]}\n{result.stderr[-2500:]}')
            return json.loads(result.stdout)

        # Local authoring, guards, templates and restart durability.
        assert 'development' in local('blueprint', 'bundled')['result']
        installed = local('blueprint', 'install', 'development')['result']
        development_version = installed['version']
        assert installed['id'] == 'development' and development_version >= 1
        versions = local('blueprint', 'list')['result']
        assert ['development', development_version] in versions
        learning_installed = local('blueprint', 'install', 'learning')['result']
        learning_version = learning_installed['version']
        published = local('blueprint', 'import', str(custom))['result']
        assert published['id'] == 'acceptance'

        # A manifest cannot grant itself an external capability.
        rejected = local('blueprint', 'import', str(guarded_file), fail=True)
        assert rejected.stdout or rejected.stderr
        invalid = json.loads(json.dumps(CUSTOM))
        invalid['work_items'][0]['transitions'][0]['to'] = 'missing'
        invalid_file = root / 'invalid.json'
        invalid_file.write_text(json.dumps(invalid), encoding='utf-8')
        local('blueprint', 'validate', str(invalid_file), fail=True)
        local('blueprint', 'validate', str(custom))

        v2 = json.loads(json.dumps(CUSTOM))
        v2['version'] = 2
        v2['description'] = 'Newer immutable version'
        v2_file = root / 'acceptance-v2.json'
        v2_file.write_text(json.dumps(v2), encoding='utf-8')
        local('blueprint', 'import', str(v2_file))
        exported_yaml = root / 'acceptance.yaml'
        local('blueprint', 'export', 'acceptance', '1', '--file', str(exported_yaml))
        assert 'version: 1' in exported_yaml.read_text(encoding='utf-8')
        local('blueprint', 'validate', str(exported_yaml))
        local('blueprint', 'import', str(exported_yaml))  # byte-stable replay

        project = local('project', 'add', 'Blueprint acceptance')['project']
        project_id = project['id']
        binding = local('blueprint', 'bind', project_id, 'acceptance', '1')['result']
        assert binding['version'] == 1
        assert local('blueprint', 'binding', project_id)['result']['version'] == 1

        properties = root / 'properties.json'
        properties.write_text(json.dumps({'approved': False}), encoding='utf-8')
        item = local('blueprint', 'create-item', project_id, 'item-one', 'gated',
                     '--properties', str(properties))['result']
        assert item['state'] == 'draft' and item['revision'] == 0
        assert item['properties']['approved'] is False

        # Guarded transition fails until the typed property is updated.
        local('blueprint', 'transition', 'item-one', 'approved', '--revision', '0', fail=True)
        local('blueprint', 'set-properties', 'item-one', '--revision', '0',
              '--file', str(properties))  # same value: still revision 1
        assert local('blueprint', 'item', 'item-one')['result']['revision'] == 1
        truth = root / 'truth.json'
        truth.write_text(json.dumps({'approved': True}), encoding='utf-8')
        local('blueprint', 'set-properties', 'item-one', '--revision', '1',
              '--file', str(truth))
        transitioned = local('blueprint', 'transition', 'item-one', 'approved',
                             '--revision', '2')['result']
        assert transitioned['state'] == 'approved' and transitioned['revision'] == 3
        local('blueprint', 'transition', 'item-one', 'draft', '--revision', '3', fail=True)
        local('blueprint', 'transition', 'item-one', 'done', '--revision', '3')

        bad = root / 'bad-properties.json'
        bad.write_text(json.dumps({'approved': 'yes'}), encoding='utf-8')
        local('blueprint', 'create-item', project_id, 'item-bad', 'gated',
              '--properties', str(bad), fail=True)

        learning_project = local('project', 'add', 'Template acceptance')['project']['id']
        local('blueprint', 'bind', learning_project, 'learning', str(learning_version))
        template = local('blueprint', 'apply-template', learning_project,
                         'first_course', 'course')['result']
        assert len(template['items']) == 3
        assert len(template['prerequisites']) == 2

        # Local restart: pinned version, guard state and template survive.
        assert local('blueprint', 'item', 'item-one')['result']['state'] == 'done'
        assert local('blueprint', 'binding', project_id)['result']['version'] == 1
        assert any(row == ['acceptance', 2] for row in local('blueprint', 'list')['result'])

        # Connected mode: authenticated authority, durable offline replay.
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        base = f'http://127.0.0.1:{port}'
        admin = secrets.token_hex(32)
        device, token = 'blueprint-device', secrets.token_hex(32)
        server_env = {**os.environ, 'ORBIT_SERVER_DB': str(root / 'server.db'),
                      'ORBIT_SERVER_BIND': f'127.0.0.1:{port}',
                      'ORBIT_SERVER_ADMIN_KEY': admin, 'NO_COLOR': '1'}
        process = None
        log = (root / 'server.log').open('w')

        def start():
            nonlocal process
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

        replica = root / 'replica.db'
        connected_env = {
            **os.environ,
            'ORBIT_DB': str(root / 'unused-local.db'),
            'ORBIT_AUTHORITY_DB': str(replica),
            'ORBIT_AUTHORITY_SERVER': base,
            'ORBIT_SYNC_HTTP_TOKEN': token,
            'ORBIT_AUTHORITY_DEVICE': device,
            'NO_COLOR': '1',
        }

        def connected(*command, fail=False):
            result = subprocess.run(
                [orbit, '--json', '--db', str(replica), 'blueprint', *command],
                text=True, capture_output=True, env=connected_env, timeout=90)
            if fail:
                assert result.returncode != 0, (
                    f'{command!r} unexpectedly succeeded\n{result.stdout[-2500:]}')
                return result
            if result.returncode:
                raise AssertionError(f'{command!r} exited {result.returncode}\n'
                                     f'{result.stdout[-2500:]}\n{result.stderr[-2500:]}')
            return json.loads(result.stdout)

        try:
            start()
            request(base, '/v1/devices', token=admin,
                    payload={'device_id': device, 'token': token})
            direct = subprocess.run(
                [orbit, '--json', 'sync', 'authority', '--server', base,
                 '--device', device, '--token', token, 'project-create',
                 '--command-id', 'connected-project', '--revision', '0',
                 '--id', 'connected-project', '--name', 'Connected project'],
                text=True, capture_output=True, timeout=60)
            assert direct.returncode == 0, direct.stderr
            assert json.loads(direct.stdout)['result']['revision'] == 1

            installed = connected('install', 'development')
            assert installed['accepted'] is True, installed
            versions = connected('list')
            development_version = next(
                version for name, version in versions if name == 'development')
            assert ['development', development_version] in versions
            binding = connected('bind', 'connected-project', 'development', str(development_version))
            assert binding['accepted'] is True, binding
            assert connected('binding', 'connected-project')['version'] == development_version
            item = connected('create-item', 'connected-project', 'connected-item', 'feature')
            assert item['accepted'] is True, item
            item = connected('item', 'connected-item')
            assert item['state'] == 'backlog'

            # Take the authority offline: durable mutation remains pending and
            # is not presented as accepted.
            stop()
            queued = connected('--offline', 'install', 'learning')
            assert queued['status'] == 'pending' and queued['accepted'] is False
            queued_id = queued['command_id']
            pending = connected('pending')
            assert pending['queued'] == 1
            connected('list', fail=True)  # reads require a live authority

            start()
            flushed = connected('flush')
            assert flushed['status'] == 'flushed' and flushed['accepted_count'] == 1
            assert ['learning', learning_version] in connected('list')
            # Idempotent replay: no new command is sent and history is stable.
            assert connected('flush')['status'] == 'empty'

            # A second device cannot claim the first device's command receipt.
            other = 'blueprint-device-2'
            other_token = secrets.token_hex(32)
            request(base, '/v1/devices', token=admin,
                    payload={'device_id': other, 'token': other_token})
            expect_http(lambda: request(
                base, '/v1/authoritative/blueprints/commands', token=other_token,
                device=other,
                payload={'command_id': queued_id, 'route': 'blueprints/install',
                         'body': {'id': 'learning'}}))

            stop()
            start()
            assert ['learning', learning_version] in connected('list')
            assert connected('binding', 'connected-project')['version'] == development_version
            assert connected('item', 'connected-item')['state'] == 'backlog'

            print('PASS blueprints: bundled install, immutable versions, JSON/YAML, '
                  'typed properties, guards, templates, capability refusal, connected '
                  'offline replay, receipts, device ownership, restart durability')
        except Exception:
            log.flush()
            print((root / 'server.log').read_text()[-4000:], file=sys.stderr)
            raise
        finally:
            stop()
            log.close()


if __name__ == '__main__':
    main()
