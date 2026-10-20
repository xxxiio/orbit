#!/usr/bin/env python3
"""Isolated M18 two-device integration: projects, tasks, habits and metrics.

Requires built orbit-server. Never opens or modifies a user's Orbit database.
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


def request(base, path, token=None, device=None, payload=None):
    headers = {}
    if token:
        headers['Authorization'] = 'Bearer ' + token
    if device:
        headers['x-orbit-device-id'] = device
    body = None if payload is None else json.dumps(payload).encode()
    if body is not None:
        headers['Content-Type'] = 'application/json'
    with urllib.request.urlopen(urllib.request.Request(base + path, data=body, headers=headers), timeout=5) as result:
        raw = result.read()
        return json.loads(raw) if raw else None


def expect_rejection(fn, statuses=(400, 409, 422)):
    try:
        fn()
    except urllib.error.HTTPError as exc:
        assert exc.code in statuses, f'unexpected HTTP status {exc.code}'
    else:
        raise AssertionError('unsafe command unexpectedly accepted')


def main():
    binary = Path(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/orbit-server').resolve()
    assert binary.is_file(), 'build orbit-server first'
    with tempfile.TemporaryDirectory(prefix='orbit-m18-four-domains-') as directory:
        root = Path(directory)
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        base = f'http://127.0.0.1:{port}'
        admin = secrets.token_hex(32)
        env = {**os.environ, 'ORBIT_SERVER_DB': str(root / 'server.sqlite'),
               'ORBIT_SERVER_BIND': f'127.0.0.1:{port}', 'ORBIT_SERVER_ADMIN_KEY': admin}
        with (root / 'server.log').open('w') as log:
            process = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                for _ in range(80):
                    assert process.poll() is None, 'server exited unexpectedly'
                    try:
                        request(base, '/health')
                        break
                    except (urllib.error.URLError, TimeoutError):
                        time.sleep(.1)
                else:
                    raise AssertionError('server readiness timeout')
                devices = []
                for i in (1, 2):
                    device, token = f'm18-client-{i}', secrets.token_hex(32)
                    request(base, '/v1/devices', token=admin, payload={'device_id':device,'token':token})
                    devices.append((device, token))
                def api(who, path, payload=None):
                    device, token = devices[who]
                    return request(base, path, token, device, payload)
                snap = api(0, '/v1/authoritative/snapshot')
                assert snap['revision'] == 0
                assert all(not snap[key] for key in ('projects','tasks','habits','habit_occurrences','metrics','metric_observations'))
                genesis = snap['history_fingerprint']
                def commit(who, route, command_id, revision, **kwargs):
                    return api(who, '/v1/authoritative/' + route + '-commands',
                               {'command_id':command_id,'expected_revision':revision,**kwargs})
                project = {'project_id':'project-a','name':'Canonical project'}
                result = commit(0,'project','project-create',0,**project)
                assert result['revision'] == 1
                assert commit(0,'project','project-create',0,**project)['duplicate']
                expect_rejection(lambda: commit(1,'project','project-stale',0,project_id='stale',name='Stale'))
                result = commit(1,'task','task-create',1,action={'type':'create','task_id':'task-a','title':'Shared task','project_id':'project-a'})
                assert result['revision'] == 2
                assert api(0, '/v1/authoritative/snapshot')['tasks'][0]['project_id'] == 'project-a'
                result = commit(0,'evidence','habit-create',2,action={'type':'create_habit','habit_id':'habit-a','name':'Exercise','cadence':'daily'})
                assert result['revision'] == 3
                result = commit(1,'evidence','habit-record',3,action={'type':'record_habit','occurrence_id':'occ-a','habit_id':'habit-a','occurred_at_ms':1000})
                assert result['revision'] == 4
                result = commit(0,'evidence','metric-create',4,action={'type':'set_metric_target','metric_id':'metric-a','baseline':1,'target':10,'direction':'increasing'})
                assert result['revision'] == 5
                result = commit(1,'evidence','metric-record',5,action={'type':'record_metric','metric_id':'metric-a','observed_at_ms':2000,'value':3})
                assert result['revision'] == 6
                expect_rejection(lambda: commit(0,'evidence','orphan',6,action={'type':'record_habit','occurrence_id':'orphan','habit_id':'missing','occurred_at_ms':3000}))
                # MCP and REST must use the same canonical store and revision.
                def mcp(who, name, arguments=None):
                    response = api(who, '/mcp', {'jsonrpc':'2.0','id':99,'method':'tools/call',
                                                  'params':{'name':name,'arguments':arguments or {}}})
                    assert response.get('error') is None, response
                    return response['result']
                before = mcp(0,'orbit_get_workspace')['structuredContent']
                assert before['revision'] == 6 and len(before['tasks']) == 1
                changed = mcp(1,'orbit_task_command',
                              {'command_id':'mcp-task-status','expected_revision':6,
                               'action':{'type':'change_status','task_id':'task-a','status':'planned'}})
                assert changed['isError'] is False and changed['structuredContent']['revision'] == 7
                retry = mcp(1,'orbit_task_command',
                            {'command_id':'mcp-task-status','expected_revision':6,
                             'action':{'type':'change_status','task_id':'task-a','status':'planned'}})
                assert retry['structuredContent']['duplicate'] is True
                conflict = mcp(0,'orbit_task_command',
                               {'command_id':'mcp-stale','expected_revision':6,
                                'action':{'type':'change_status','task_id':'task-a','status':'done'}})
                assert conflict['isError'] is True
                assert api(0,'/v1/authoritative/snapshot')['tasks'][0]['status'] == 'planned'
                final_a = api(0, '/v1/authoritative/snapshot')
                final_b = api(1, '/v1/authoritative/snapshot')
                assert final_a == final_b and final_a['revision'] == 7
                assert [len(final_a[k]) for k in ('projects','tasks','habits','habit_occurrences','metrics','metric_observations')] == [1]*6
                prefix = api(0, '/v1/authoritative/history-prefix?revision=0')
                assert prefix['history_fingerprint'] == genesis
                prefix = api(1, '/v1/authoritative/history-prefix?revision=7')
                assert prefix['history_fingerprint'] == final_a['history_fingerprint']
                # An approved remote agent may only mutate via the canonical API.
                # Use an enrolled worker device and ensure the state is visible
                # to another device; the worker must never open a local Orbit DB.
                worker_device, worker_token = 'm18-approved-worker', secrets.token_hex(32)
                request(base, '/v1/devices', token=admin,
                        payload={'device_id':worker_device,'token':worker_token})
                approved = api(0, '/v1/agent-proposals',
                               {'intent':'task-create','target':'project-a',
                                'payload':{'title':'Agent-approved task'}})
                decision = urllib.request.Request(
                    base+'/v1/agent-proposals/%d/decision' % approved['id'],
                    data=json.dumps({'approve':True}).encode(),
                    headers={'Authorization':'Bearer '+admin,'Content-Type':'application/json'})
                with urllib.request.urlopen(decision,timeout=5) as response:
                    assert response.status == 204
                worker_env = {**os.environ, 'ORBIT_SYNC_HTTP_TOKEN':worker_token}
                worker = subprocess.run(
                    [sys.executable, str(Path(__file__).with_name('agent-worker.py')),
                     '--server', base, '--device', worker_device, '--once'],
                    env=worker_env, capture_output=True, text=True, timeout=20)
                assert worker.returncode == 0, worker.stderr
                accepted = api(1, '/v1/authoritative/snapshot')
                assert accepted['revision'] == 8
                assert any(t['id']=='remote-proposal-%d'%approved['id'] and
                           t['project_id']=='project-a' for t in accepted['tasks'])
                with urllib.request.urlopen(urllib.request.Request(
                    base+'/v1/agent-proposals/%d/audit'%approved['id'],
                    headers={'Authorization':'Bearer '+admin}),timeout=5) as response:
                    audit = json.load(response)
                assert any(event['action']=='succeeded' for event in audit), audit
                print('PASS approved agent proposal -> canonical server command -> second device snapshot')
                print('PASS M18 two devices: canonical projects/tasks/habits/metrics, MCP/REST parity, retry, stale writes, referential integrity, history')
            except Exception:
                print((root / 'server.log').read_text()[-2500:], file=sys.stderr)
                raise
            finally:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()

if __name__ == '__main__':
    main()
