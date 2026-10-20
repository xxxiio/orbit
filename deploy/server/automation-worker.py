#!/usr/bin/env python3
"""Read-only scheduled reports over Orbit's canonical server snapshot.

The scheduler/lease API is separate from the domain read API. This worker never
opens a local-only Orbit database, executes shell commands, or submits domain writes.
"""
import argparse
import json
import os
import time
import urllib.error
import urllib.request
from collections import Counter
from datetime import datetime, timezone

MAX_RESULT = 60_000
MAX_RESPONSE = 120_000


from worker_transport import json_request, validate_server


def request(base, token, path, method='GET', data=None, device=None):
    return json_request(base, token, path, method=method, payload=data,
                        device=device, max_response=MAX_RESPONSE)

def read_snapshot(args):
    snapshot = request(args.server, args.authority_token,
                       '/v1/authoritative/snapshot', device=args.device)
    if not isinstance(snapshot, dict) or type(snapshot.get('revision')) is not int or snapshot['revision'] < 0:
        raise RuntimeError('invalid authoritative revision')
    for key in ('projects', 'tasks', 'habits', 'habit_occurrences', 'metrics', 'metric_observations'):
        if not isinstance(snapshot.get(key), list):
            raise RuntimeError('invalid authoritative snapshot: ' + key)
    return snapshot


def execute(action, snapshot):
    """Reports include revision provenance; no local-only derived health model."""
    revision = snapshot['revision']
    if action == 'daily-briefing':
        return {'revision': revision, 'projects': snapshot['projects'],
                'tasks': snapshot['tasks'], 'generated_at': datetime.now(timezone.utc).isoformat()}
    if action == 'habit-summary':
        return {'revision': revision, 'habits': snapshot['habits'],
                'habit_occurrences': snapshot['habit_occurrences']}
    if action == 'project-health-refresh':
        counts = Counter(t.get('project_id') for t in snapshot['tasks'] if isinstance(t, dict))
        return {'revision': revision, 'project_health': [
            {'project_id': project['id'], 'task_count': counts.get(project['id'], 0)}
            for project in snapshot['projects']
            if isinstance(project, dict) and isinstance(project.get('id'), str)]}
    raise ValueError('unknown action; refusing execution')


def run_once(args):
    run = request(args.server, args.token, '/v1/automation-runs/claim', 'POST')
    if run is None:
        return False
    try:
        if run.get('action') not in ('daily-briefing', 'habit-summary', 'project-health-refresh'):
            raise ValueError('unknown action; refusing execution')
        output = execute(run['action'], read_snapshot(args))
        result = json.dumps({'output': output}, separators=(',', ':'))
        if len(result.encode()) > MAX_RESULT:
            raise RuntimeError('report exceeds 60 KiB; not uploaded')
        ok, record = True, result
    except (ValueError, KeyError, TypeError, RuntimeError, urllib.error.URLError, json.JSONDecodeError) as exc:
        ok, record = False, json.dumps({'error': type(exc).__name__, 'detail': str(exc)[:200]})
    request(args.server, args.token, '/v1/automation-runs/%s/settle' % int(run['id']),
            'POST', {'succeeded': ok, 'result_json': record})
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server', required=True)
    parser.add_argument('--device', required=True, help='enrolled device identity for canonical reads')
    parser.add_argument('--interval', type=int, default=30)
    parser.add_argument('--once', action='store_true')
    args = parser.parse_args()
    args.token = os.environ.get('ORBIT_SYNC_HTTP_TOKEN', '')
    args.authority_token = os.environ.get('ORBIT_AUTHORITY_TOKEN', '')
    if not args.token or not args.authority_token:
        parser.error('ORBIT_SYNC_HTTP_TOKEN (scheduler) and ORBIT_AUTHORITY_TOKEN (device) are required')
    try: validate_server(args.server)
    except ValueError as error: parser.error(str(error))
    if args.interval < 5:
        parser.error('interval must be >= 5 seconds')
    while True:
        try:
            run_once(args)
        except (OSError, urllib.error.URLError, RuntimeError) as exc:
            print('automation worker: %s' % str(exc)[:200], flush=True)
        if args.once:
            break
        time.sleep(args.interval)


if __name__ == '__main__':
    main()
