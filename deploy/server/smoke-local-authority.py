#!/usr/bin/env python3
"""Isolated local-authoritative acceptance workflow.

Exercises the real `orbit` CLI against one temporary SQLite workspace:
projects, tasks, dependencies, planning, schedule, restart persistence, and
rejection of an invalid lifecycle command. Never opens a user database.
"""
import argparse
import json
import sqlite3
import subprocess
import sys
import tempfile
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--orbit', default='target/debug/orbit')
    args = parser.parse_args()
    orbit = str(Path(args.orbit).resolve())
    if not Path(orbit).is_file():
        parser.error(f'missing {orbit}; run cargo build -p orbit first')

    now = 1_770_000_000_000
    with tempfile.TemporaryDirectory(prefix='orbit-local-acceptance-') as directory:
        root = Path(directory)
        db = root / 'orbit.db'

        def invoke(*command, fail=False):
            result = subprocess.run(
                [orbit, '--json', '--db', str(db), *command],
                text=True, capture_output=True, timeout=60)
            if fail:
                assert result.returncode != 0, (
                    f'{command!r} unexpectedly succeeded\n{result.stdout[-2000:]}')
                return result
            if result.returncode:
                raise AssertionError(
                    f'{command!r} exited {result.returncode}\n'
                    f'stdout={result.stdout[-2000:]}\nstderr={result.stderr[-2000:]}')
            return json.loads(result.stdout)

        status = invoke('status')
        assert status['type'] == 'workspace_status'
        assert status['healthy'] is True
        assert status['schema_version'] == 19, status
        assert (status['projects'], status['tasks']) == (0, 0)

        project = invoke('project', 'add', 'Local acceptance')['project']
        project_id = project['id']
        assert project['status'] == 'proposed'
        active = invoke('project', 'status', project_id, 'active')['project']
        assert active['status'] == 'active'
        configured = invoke(
            'project', 'configure', project_id, '--mode', 'standard',
            '--objective', 'Prove local authority',
            '--criterion', 'complete=All acceptance steps run')['project']
        assert configured['mode'] == 'standard'
        assert configured['success_criteria'][0]['key'] == 'complete'

        first = invoke(
            'task', 'add', 'Foundation', '--id', 'task-foundation',
            '--project', project_id)['task']
        second = invoke(
            'task', 'add', 'Dependent work', '--id', 'task-dependent',
            '--project', project_id)['task']
        assert (first['status'], second['status']) == ('inbox', 'inbox')
        planned = invoke(
            'task', 'planning', 'task-foundation',
            '--estimate-minutes', '45', '--deadline-ms', str(now + 86_400_000))['task']
        assert planned['estimated_minutes'] == 45
        assert planned['deadline_ms'] == now + 86_400_000

        invoke('task', 'depends-on', 'task-dependent', 'task-foundation')
        graph = invoke('task', 'graph', '--now-ms', str(now))
        assert graph['type'] == 'authority'
        graph_value = graph['result']
        assert 'task-foundation' in json.dumps(graph_value)
        ready = invoke('task', 'ready', '--now-ms', str(now), '--all')
        assert ready['type'] == 'authority'
        assert 'task-foundation' in json.dumps(ready['result'])

        started = invoke('task', 'start', 'task-foundation')['task']
        assert started['status'] == 'in_progress'
        completed = invoke('task', 'complete', 'task-foundation')['task']
        assert completed['status'] == 'done'

        ready_after = invoke('task', 'ready', '--now-ms', str(now + 1_000), '--all')
        assert 'task-dependent' in json.dumps(ready_after['result'])
        plan = invoke(
            'task', 'day-plan', '--now-ms', str(now + 2_000),
            '--available-minutes', '120', '--include-unsized')
        assert plan['type'] == 'authority'

        block = invoke(
            'schedule', 'add', 'Acceptance focus', '--start-ms', str(now + 3_600_000),
            '--duration-minutes', '45', '--task', 'task-dependent')['block']
        blocks = invoke(
            'schedule', 'list', '--from-ms', str(now),
            '--to-ms', str(now + 7_200_000))['blocks']
        assert any(item['id'] == block['id'] for item in blocks)

        # Invalid transitions must fail without corrupting accepted state.
        rejected = invoke('task', 'status', 'task-dependent', 'done', fail=True)
        assert 'error' in rejected.stdout.lower() or rejected.stderr
        unchanged = invoke('task', 'list', '--query', 'Dependent work')['tasks']
        assert unchanged[0]['status'] == 'inbox'

        status = invoke('status')
        assert status['projects'] == 1 and status['tasks'] == 2
        assert status['habits'] == 0

        # A fresh process must see the same committed state.
        restarted = invoke('task', 'list')
        by_id = {task['id']: task for task in restarted['tasks']}
        assert by_id['task-foundation']['status'] == 'done'
        assert by_id['task-dependent']['status'] == 'inbox'
        assert by_id['task-dependent']['project_id'] == project_id

        with sqlite3.connect(db) as connection:
            assert connection.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
            assert connection.execute(
                'SELECT COUNT(*) FROM tasks').fetchone()[0] == 2

        print('PASS local authority: project/task lifecycle, dependency, planning, '
              'schedule, invalid transition rejection, restart persistence')


if __name__ == '__main__':
    main()
