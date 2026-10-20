#!/usr/bin/env python3
"""Opt-in live Herdr acceptance against a disposable repository and Orbit DB.

This is the only M15 harness that starts a real coding agent. It requires an
explicit --confirm-live flag, uses only a temporary Git repository, a temporary
Orbit workspace, and a Herdr workspace it closes on exit. Never point this at a
user repository or an already running Orbit workspace.

Covered: authority-captured brief -> durable reservation -> single Herdr
workspace/pane/agent launch -> prompt -> real session monitoring -> read-only
Git evidence capture -> durable handoff -> review (without task completion) ->
Herdr workspace cleanup.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


def run(*command, env=None, timeout=120, expect=0):
    result = subprocess.run(command, text=True, capture_output=True,
                            env=env, timeout=timeout)
    if expect == 0 and result.returncode:
        raise AssertionError(f'{command!r} exited {result.returncode}\n'
                             f'stdout={result.stdout[-2500:]}\nstderr={result.stderr[-2500:]}')
    if expect != 0:
        assert result.returncode != 0, f'{command!r} unexpectedly succeeded'
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--orbit', default='target/debug/orbit')
    parser.add_argument('--confirm-live', action='store_true',
                        help='Required acknowledgement that a real agent will be launched')
    parser.add_argument('--timeout', type=int, default=180,
                        help='Seconds to wait for the live agent to become idle')
    args = parser.parse_args()
    if not args.confirm_live:
        parser.error('live Herdr acceptance requires --confirm-live')
    orbit = str(Path(args.orbit).resolve())
    if not Path(orbit).is_file():
        parser.error(f'missing {orbit}; build orbit first')
    if shutil.which('herdr') is None:
        parser.error('herdr is not installed')

    def herdr(*command):
        return json.loads(run('herdr', *command, timeout=60).stdout)

    def workspace_labels():
        listing = herdr('workspace', 'list')
        return {item['label']: item['workspace_id']
                for item in listing['result']['workspaces']}

    before = workspace_labels()
    with tempfile.TemporaryDirectory(prefix='orbit-herdr-live-') as directory:
        root = Path(directory)
        repo = root / 'repository'
        repo.mkdir()
        run('git', '-C', str(repo), 'init', '-q')
        run('git', '-C', str(repo), 'config', 'user.email', 'live@example.invalid')
        run('git', '-C', str(repo), 'config', 'user.name', 'Orbit Live Acceptance')
        (repo / 'README.md').write_text('# disposable live acceptance repository\n')
        run('git', '-C', str(repo), 'add', 'README.md')
        run('git', '-C', str(repo), 'commit', '-q', '-m', 'initial')
        db = root / 'workspace.db'

        def orbit_json(*command, fail=False):
            result = subprocess.run(
                [orbit, '--json', '--db', str(db), *command],
                text=True, capture_output=True, timeout=90)
            if fail:
                assert result.returncode != 0, (
                    f'{command!r} unexpectedly succeeded\n{result.stdout[-2000:]}')
                return result
            if result.returncode:
                raise AssertionError(f'{command!r} exited {result.returncode}\n'
                                     f'{result.stdout[-2500:]}\n{result.stderr[-2500:]}')
            parsed = json.loads(result.stdout)
            return parsed.get('result', parsed)

        project = orbit_json('project', 'add', 'Live Herdr acceptance')['project']
        task = orbit_json(
            'task', 'add',
            'Live acceptance only: reply with the single word OK. Do not modify files or run commands.',
            '--id', 'live-herdr-task', '--project', project['id'])['task']
        orbit_json('task', 'status', 'live-herdr-task', 'ready')

        session = None
        workspace = None
        try:
            launched = orbit_json(
                'agent', 'start', 'live-herdr-task', '--agent', 'opencode',
                '--machine', 'local', '--repository', str(repo))
            assert launched['outcome'] == 'confirmed', launched
            job_id = launched['job_id']
            session = launched['session']

            # The launch created exactly one new Herdr workspace labeled after
            # the Orbit-managed session.
            after = workspace_labels()
            new_labels = {label: wid for label, wid in after.items()
                          if before.get(label) != wid}
            assert session in new_labels, (
                f'expected Herdr workspace label {session!r}, saw {sorted(new_labels)}')
            workspace = new_labels[session]

            run('herdr', 'agent', 'wait', session, '--until', 'idle',
                '--timeout', str(args.timeout * 1000), timeout=args.timeout + 30)

            status = orbit_json('agent', 'status', job_id)
            assert status['state'] in ('launching', 'running', 'waiting_input'), status
            observed = orbit_json('agent', 'inspect', job_id,
                                  '--revision', str(status['revision']),
                                  '--at-ms', str(int(time.time() * 1000)))
            assert observed['session'] == session, observed

            captured = orbit_json('agent', 'capture', job_id, '--max-commits', '10')
            evidence = captured['evidence']
            assert len(evidence['head']) >= 40, evidence
            assert evidence['recent_commits'], evidence
            handoffs = orbit_json('agent', 'handoffs', job_id)
            assert any(item['handoff_id'] == captured['handoff_id'] for item in handoffs), handoffs

            review = orbit_json('agent', 'review', job_id,
                                '--handoff-id', captured['handoff_id'],
                                '--reviewer', 'live-acceptance',
                                '--decision', 'approved')
            assert review['task_completed'] is False, review
            recorded = orbit_json('agent', 'review-status', job_id)
            assert recorded['decision'] == 'approved', recorded
            tasks = orbit_json('task', 'list', '--query', 'Live acceptance')['tasks']
            assert tasks[0]['status'] == 'ready', 'agent review must not complete the Orbit task'

            print('PASS live Herdr: workspace create, opencode agent launch, prompt, '
                  'real session monitoring, Git evidence, handoff, review, no task completion')
        finally:
            # Close every workspace created during this acceptance run, including
            # an ambiguous launch that failed before returning a session name.
            after = workspace_labels()
            for label, wid in after.items():
                if before.get(label) != wid:
                    run('herdr', 'workspace', 'close', wid, timeout=30)
            remaining = workspace_labels()
            leaked = [label for label in remaining if before.get(label) != remaining[label]]
            assert not leaked, f'Herdr workspaces leaked: {leaked}'


if __name__ == '__main__':
    main()
