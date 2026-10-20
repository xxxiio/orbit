#!/usr/bin/env python3
"""Opt-in M18 offline command transport. Does not replace a local Orbit replica.

Canonical project, task, habit and metric commands remain durable until the
authority explicitly acknowledges their command IDs.
"""
import argparse
import datetime
import json
import os
import sqlite3
import sys
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path

SCHEMA = '''PRAGMA journal_mode=WAL;
CREATE TABLE IF NOT EXISTS pending_commands (
 command_id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('project','task','evidence')),
 payload TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'pending'
 CHECK(state IN ('pending','accepted','conflict')),
 result TEXT, expected_revision INTEGER, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS authority_connection (singleton INTEGER PRIMARY KEY CHECK(singleton=1), server_url TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS accepted_snapshots (kind TEXT PRIMARY KEY CHECK(kind IN ('project','task')), revision INTEGER NOT NULL, payload TEXT NOT NULL, fetched_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS accepted_evidence_snapshots (kind TEXT PRIMARY KEY, revision INTEGER NOT NULL, payload TEXT NOT NULL, fetched_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS accepted_history (singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL, fingerprint TEXT NOT NULL);'''

def open_db(path):
    path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(path, timeout=10)
    conn.executescript(SCHEMA)
    return conn

def add(conn, kind, action, command_id=None):
    cid = command_id or str(uuid.uuid4())
    if not cid.strip() or len(cid) > 128:
        raise ValueError('invalid command ID')
    # Stable intent, but expected_revision is resolved at submission time.
    payload = json.dumps(action, separators=(',', ':'), sort_keys=True)
    with conn:
        conn.execute('INSERT INTO pending_commands(command_id,kind,payload) VALUES(?,?,?)', (cid,kind,payload))
    return cid

def request(base, token, method, route, obj=None):
    uri = urllib.parse.urlsplit(base)
    if uri.scheme != 'https' and not (uri.scheme == 'http' and uri.hostname in ('localhost','127.0.0.1','::1')):
        raise ValueError('HTTPS is required for non-loopback servers')
    if uri.username or uri.password or uri.query or uri.fragment:
        raise ValueError('credentials, queries and fragments are not allowed in server URL')
    data = json.dumps(obj).encode() if obj is not None else None
    req = urllib.request.Request(base.rstrip('/')+route, data=data, method=method,
        headers={'Authorization':'Bearer '+token, 'Content-Type':'application/json'})
    # Do not forward bearer tokens to a redirect destination.
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            return None
    opener = urllib.request.build_opener(NoRedirect)
    try:
        with opener.open(req, timeout=15) as response:
            return response.status, json.loads(response.read(2_000_000))
    except urllib.error.HTTPError as exc:
        return exc.code, {'error':exc.read(4096).decode(errors='replace')}

def normalized_server(base):
    parsed = urllib.parse.urlsplit(base)
    if parsed.scheme not in ('http', 'https') or not parsed.hostname:
        raise ValueError('server URL must be absolute HTTP(S)')
    if parsed.scheme != 'https' and parsed.hostname not in ('localhost', '127.0.0.1', '::1'):
        raise ValueError('HTTPS is required for non-loopback servers')
    if parsed.username or parsed.password or parsed.query or parsed.fragment or parsed.path not in ('', '/'):
        raise ValueError('server URL must not contain credentials, path, query or fragment')
    return base.rstrip('/')

def bind_server(conn, base):
    """Never send pending commands to a different authority accidentally."""
    server = normalized_server(base)
    with conn:
        existing = conn.execute('SELECT server_url FROM authority_connection WHERE singleton=1').fetchone()
        if existing and existing[0] != server:
            raise ValueError('this database belongs to a different authority; use a separate --db')
        if not existing:
            conn.execute('INSERT INTO authority_connection(singleton,server_url) VALUES(1,?)', (server,))
    return server

def refresh(conn, base, token):
    """Stage both authoritative lists before replacing either cached snapshot."""
    base = bind_server(conn, base)
    try:
        status, response = request(base, token, 'GET', '/v1/authoritative/snapshot')
    except (urllib.error.URLError, TimeoutError, OSError) as exc:
        print(f'Unable to refresh authority: {exc}; cached data retained.', file=sys.stderr)
        return 2
    if (status != 200 or not isinstance(response, dict)
            or type(response.get('revision')) is not int or response['revision'] < 0
            or not isinstance(response.get('projects'), list)
            or not isinstance(response.get('tasks'), list)
            or any(not isinstance(response.get(field), list) for field in ('habits','habit_occurrences','metrics','metric_observations'))):
        print('Invalid atomic authority snapshot; cached data retained.', file=sys.stderr)
        return 2
    fingerprint = response.get('history_fingerprint')
    if not isinstance(fingerprint, str) or len(fingerprint) != 64 or any(c not in '0123456789abcdef' for c in fingerprint):
        print('Authority snapshot lacks a valid history fingerprint; cached data retained.', file=sys.stderr)
        return 2
    revision = response['revision']
    # A rewound revision could indicate a restored server. Do not overwrite a
    # newer accepted cache until the operator explicitly reconciles its history.
    existing = conn.execute('SELECT revision FROM accepted_snapshots UNION SELECT revision FROM accepted_evidence_snapshots').fetchall()
    if existing and (len(existing) != 1 or revision < existing[0][0]):
        print('Authority revision moved backwards or local cache is inconsistent; cached data retained.', file=sys.stderr)
        return 3
    previous = conn.execute('SELECT revision,fingerprint FROM accepted_history WHERE singleton=1').fetchone()
    if previous and (revision < previous[0] or (revision == previous[0] and fingerprint != previous[1])):
        print('Server history differs from the accepted cache at the same or older revision; explicit recovery required.',file=sys.stderr)
        return 3
    stamp = datetime.datetime.now(datetime.timezone.utc).isoformat()
    with conn:
        for kind, key in (('project','projects'),('task','tasks'),('habit','habits'),('habit_occurrence','habit_occurrences'),('metric','metrics'),('metric_observation','metric_observations')):
            payload = json.dumps(response[key], separators=(',', ':'), sort_keys=True)
            table = 'accepted_snapshots' if kind in ('project', 'task') else 'accepted_evidence_snapshots'
            conn.execute(f'INSERT INTO {table}(kind,revision,payload,fetched_at) VALUES(?,?,?,?) ON CONFLICT(kind) DO UPDATE SET revision=excluded.revision,payload=excluded.payload,fetched_at=excluded.fetched_at', (kind,revision,payload,stamp))
        conn.execute('INSERT INTO accepted_history(singleton,revision,fingerprint) VALUES(1,?,?) ON CONFLICT(singleton) DO UPDATE SET revision=excluded.revision,fingerprint=excluded.fingerprint', (revision,fingerprint))
    return 0

def cached(conn, kind):
    table = 'accepted_snapshots' if kind in ('project', 'task','evidence') else 'accepted_evidence_snapshots'
    if kind not in ('project','task','evidence','habit','habit_occurrence','metric','metric_observation'):
        raise ValueError('unknown cache kind')
    row = conn.execute(f'SELECT revision,payload,fetched_at FROM {table} WHERE kind=?',(kind,)).fetchone()
    return {'revision':row[0], 'items':json.loads(row[1]), 'fetched_at':row[2]} if row else None

def sync(conn, base, token):
    base = bind_server(conn, base)
    # Sequential replay is intentional. Never skip over an unaccepted command.
    rows = conn.execute("SELECT command_id,kind,payload,expected_revision FROM pending_commands WHERE state!='accepted' ORDER BY rowid").fetchall()
    accepted = 0
    for cid, kind, payload, stored_revision in rows:
        route = '/v1/authoritative/'+ ('projects' if kind=='project' else 'tasks' if kind=='task' else 'evidence')
        try:
            status, state = request(base, token, 'GET', route)
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            print(f'Network error reading authority: {exc}; queue retained.',file=sys.stderr)
            return 2
        if status != 200:
            print(f'Unable to read authoritative revision: {status}', file=sys.stderr)
            return 2
        # Once submitted, the revision is part of the immutable idempotency key.
        # Never change it on retry after a lost acknowledgement.
        revision = stored_revision if stored_revision is not None else state['revision']
        if stored_revision is None:
            with conn:
                conn.execute('UPDATE pending_commands SET expected_revision=? WHERE command_id=? AND expected_revision IS NULL', (revision,cid))
        command = json.loads(payload)
        command.update(command_id=cid, expected_revision=revision)
        target = '/v1/authoritative/'+('project-commands' if kind=='project' else 'task-commands' if kind=='task' else 'evidence-commands')
        try:
            code, result = request(base, token, 'POST', target, command)
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            print(f'Network error submitting {cid}: {exc}; queue retained.',file=sys.stderr)
            return 2
        if code in (200, 201) and isinstance(result,dict) and 'revision' in result:
            with conn:
                conn.execute("UPDATE pending_commands SET state='accepted',result=? WHERE command_id=?", (json.dumps(result),cid))
            accepted += 1
        elif code in (409, 412, 422):
            with conn:
                conn.execute("UPDATE pending_commands SET state='conflict',result=? WHERE command_id=?",(json.dumps(result),cid))
            print(f'Conflict on {cid}; no later commands submitted. Review and reconcile explicitly.',file=sys.stderr)
            return 3
        else:
            print(f'Submission failed ({code}) for {cid}; pending work retained.',file=sys.stderr)
            return 2
    print(f'Accepted {accepted} command(s).')
    return 0

def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--db', type=Path, default=Path.home()/'.orbit'/'authority-pending.sqlite3')
    commands = p.add_subparsers(dest='cmd', required=True)
    project = commands.add_parser('create-project')
    project.add_argument('project_id'); project.add_argument('name')
    task = commands.add_parser('create-task')
    task.add_argument('task_id'); task.add_argument('title'); task.add_argument('--project-id')
    status = commands.add_parser('task-status')
    status.add_argument('task_id'); status.add_argument('status')
    habit = commands.add_parser('create-habit')
    habit.add_argument('habit_id'); habit.add_argument('name'); habit.add_argument('--cadence',default='daily')
    occurrence = commands.add_parser('record-habit')
    occurrence.add_argument('occurrence_id'); occurrence.add_argument('habit_id'); occurrence.add_argument('occurred_at_ms', type=int)
    metric = commands.add_parser('set-metric-target')
    metric.add_argument('metric_id'); metric.add_argument('baseline',type=int); metric.add_argument('target',type=int)
    metric.add_argument('--direction',choices=['increasing','decreasing'],required=True)
    observation = commands.add_parser('record-metric')
    observation.add_argument('metric_id'); observation.add_argument('observed_at_ms',type=int); observation.add_argument('value',type=int)
    commands.add_parser('status')
    commands.add_parser('cached-projects')
    commands.add_parser('cached-tasks')
    commands.add_parser('cached-habits')
    commands.add_parser('cached-habit-occurrences')
    commands.add_parser('cached-metrics')
    commands.add_parser('cached-metric-observations')
    update = commands.add_parser('refresh'); update.add_argument('--server', required=True)
    flush = commands.add_parser('sync'); flush.add_argument('--server', required=True)
    a=p.parse_args(argv)
    conn=open_db(a.db)
    if a.cmd=='create-project':
        cid=add(conn,'project',dict(project_id=a.project_id,name=a.name))
    elif a.cmd=='create-task':
        cid=add(conn,'task',dict(action=dict(type='create',task_id=a.task_id,title=a.title,project_id=a.project_id)))
    elif a.cmd=='task-status':
        cid=add(conn,'task',dict(action=dict(type='change_status',task_id=a.task_id,status=a.status)))
    elif a.cmd=='create-habit':
        cid=add(conn,'evidence',dict(action=dict(type='create_habit',habit_id=a.habit_id,name=a.name,cadence=a.cadence)))
    elif a.cmd=='record-habit':
        cid=add(conn,'evidence',dict(action=dict(type='record_habit',occurrence_id=a.occurrence_id,habit_id=a.habit_id,occurred_at_ms=a.occurred_at_ms)))
    elif a.cmd=='set-metric-target':
        cid=add(conn,'evidence',dict(action=dict(type='set_metric_target',metric_id=a.metric_id,baseline=a.baseline,target=a.target,direction=a.direction)))
    elif a.cmd=='record-metric':
        cid=add(conn,'evidence',dict(action=dict(type='record_metric',metric_id=a.metric_id,observed_at_ms=a.observed_at_ms,value=a.value)))
    elif a.cmd=='status':
        for item in conn.execute('SELECT command_id,kind,state,created_at FROM pending_commands ORDER BY rowid'):
            print(' | '.join(item))
        return 0
    elif a.cmd in ('cached-projects','cached-tasks','cached-habits','cached-habit-occurrences','cached-metrics','cached-metric-observations'):
        kind={'cached-projects':'project','cached-tasks':'task','cached-habits':'habit','cached-habit-occurrences':'habit_occurrence','cached-metrics':'metric','cached-metric-observations':'metric_observation'}[a.cmd]
        print(json.dumps(cached(conn, kind), indent=2))
        return 0
    else:
        token=os.environ.get('ORBIT_SYNC_HTTP_TOKEN','')
        if not token:
            p.error('ORBIT_SYNC_HTTP_TOKEN must be set')
        if a.cmd == 'refresh': return refresh(conn,a.server,token)
        code = sync(conn,a.server,token)
        if code != 0: return code
        return refresh(conn,a.server,token)
    print('Queued offline:',cid)
    return 0
if __name__=='__main__':
    sys.exit(main())
