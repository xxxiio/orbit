#!/usr/bin/env python3
"""M18 isolated canonical task workflow acceptance harness.
Requires compiled orbit-cli and orbit-server; never touches user data.
"""
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


def command(*args, env=None, success=True):
    result = subprocess.run(args, text=True, capture_output=True, timeout=40, env=env)
    if success and result.returncode:
        raise AssertionError(f'{args!r}: {result.stderr[-1500:]}')
    return result


def request(url, path, token=None, device=None, data=None):
    headers = {}
    if token:
        headers['Authorization'] = 'Bearer ' + token
    if device:
        headers['x-orbit-device-id'] = device
    raw = None if data is None else json.dumps(data).encode()
    if raw is not None:
        headers['Content-Type'] = 'application/json'
    with urllib.request.urlopen(urllib.request.Request(url + path, data=raw, headers=headers), timeout=4) as response:
        body = response.read()
        return json.loads(body) if body else None


def main():
    cli = str(Path(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/orbit').resolve())
    server = str(Path(sys.argv[2] if len(sys.argv) > 2 else 'target/debug/orbit-server').resolve())
    assert Path(cli).is_file() and Path(server).is_file(), 'build orbit-cli and orbit-server first'
    with tempfile.TemporaryDirectory(prefix='orbit-m18-authority-') as directory:
        root = Path(directory)
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        url = f'http://127.0.0.1:{port}'
        admin = secrets.token_hex(32)
        env = {**os.environ, 'ORBIT_SERVER_DB': str(root/'server.db'),
               'ORBIT_SERVER_BIND': f'127.0.0.1:{port}', 'ORBIT_SERVER_ADMIN_KEY': admin}
        with (root/'server.log').open('w') as log:
            process = subprocess.Popen([server], env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                for _ in range(60):
                    assert process.poll() is None, 'server exited unexpectedly'
                    try:
                        request(url, '/health')
                        break
                    except (urllib.error.URLError, TimeoutError):
                        time.sleep(.1)
                else:
                    raise AssertionError('server not ready')
                devices = []
                for i in (1, 2):
                    name, token = f'authority-device-{i}', secrets.token_hex(32)
                    request(url, '/v1/devices', token=admin, data={'device_id':name,'token':token})
                    devices.append((name, token))
                def call(index, *options, success=True):
                    name, token = devices[index]
                    result = command(cli, '--json', 'sync', 'authority', '--server', url,
                                     '--device', name, '--token', token, *options, success=success)
                    if not success:
                        return result
                    parsed = json.loads(result.stdout)
                    return parsed.get('result', parsed)
                initial = call(0, 'tasks')
                assert initial['revision'] == 0 and initial['tasks'] == []
                created = call(0, 'create', '--command-id','a-create', '--revision','0',
                               '--id','task-1','--title','Canonical task')
                assert created['revision'] == 1
                assert call(0, 'create', '--command-id','a-create','--revision','0',
                            '--id','task-1','--title','Canonical task')['duplicate']
                assert call(1, 'tasks')['tasks'][0]['title'] == 'Canonical task'
                assert call(1, 'create', '--command-id','stale','--revision','0',
                            '--id','task-2','--title','Stale', success=False).returncode != 0
                updated = call(1, 'status', '--command-id','b-ready', '--revision','1',
                               '--id','task-1','--status','ready')
                assert updated['revision'] == 2
                assert call(0, 'tasks')['tasks'][0]['status'] == 'ready'
                assert call(0, 'status', '--command-id','invalid', '--revision','2',
                            '--id','task-1','--status','done', success=False).returncode != 0
                assert call(0, 'tasks')['revision'] == 2
                state = request(url, '/v1/authoritative/snapshot', token=devices[0][1], device=devices[0][0])
                assert state['revision'] == 2, 'server canonical snapshot revision mismatch'
                # Legacy relay must not accept a write once authority is active.
                with urllib.request.urlopen(urllib.request.Request(
                        url+'/v1/authoritative/tasks', headers={'Authorization':'Bearer '+devices[1][1],
                        'x-orbit-device-id':devices[1][0]}), timeout=4) as response:
                    assert json.loads(response.read())['revision'] == 2
                print('PASS M18 two authenticated clients, canonical writes, stale revisions, retries, transitions')
            except Exception:
                print((root/'server.log').read_text()[-2000:], file=sys.stderr)
                raise
            finally:
                process.terminate()
                try: process.wait(timeout=5)
                except subprocess.TimeoutExpired: process.kill(); process.wait()

if __name__ == '__main__':
    main()
