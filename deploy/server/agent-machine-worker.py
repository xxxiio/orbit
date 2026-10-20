#!/usr/bin/env python3
"""Optional machine-side Herdr worker for Orbit's authenticated authority API.

The worker runs only on the enrolled execution machine. Orbit's server never
runs commands. A local write-ahead journal fences ambiguous launch attempts.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.error
import urllib.request
from urllib.parse import urlsplit, quote

BASE = '/v1/authoritative/agent-machines/'
MAX_BODY = 1 << 20

from worker_transport import json_request, validate_server
from agent_worker_journal import journal_write, exclusive_worker, journal_health, journal_path
from agent_worker_evidence import safe_name, git_output, evidence_repository, build_evidence


def api(server, token, device, endpoint, payload=None):
    return json_request(server, token, BASE + endpoint,
        method="GET" if payload is None else "POST", payload=payload,
        device=device, max_response=MAX_BODY)

def timestamp():
    return int(time.time() * 1000)

def herdr(args, *, executable='herdr'):
    result = subprocess.run([executable, '--json', *args], capture_output=True, timeout=90, check=False)
    if result.returncode:
        raise RuntimeError('Herdr command unsuccessful: ' + result.stderr.decode(errors='replace')[:250])
    return json.loads(result.stdout)

def launch_runtime(job, brief, token, executable):
    if job['runtime_kind'] != 'herdr' or job['agent_kind'] not in ('codex', 'opencode'):
        raise ValueError('unsupported runtime or agent')
    repository = job['repository_ref']
    if not isinstance(repository, str) or not os.path.isabs(repository):
        raise ValueError('machine repository must be an absolute path')
    if not Path(repository).is_dir():
        raise ValueError('repository is not present on execution machine')
    name = safe_name(token)
    policy = brief.get('worktree')
    # Accept only the known persisted worktree choice.
    worktree = policy is True or (isinstance(policy, dict) and policy.get('enabled') is True)
    if worktree:
        setup = ['worktree', 'create', '--cwd', repository, '--branch', 'orbit/' + name, '--label', name, '--no-focus']
    else:
        setup = ['workspace', 'create', '--cwd', repository, '--label', name, '--no-focus']
    created = herdr(setup, executable=executable)
    pane = created.get('result', {}).get('root_pane', {}).get('pane_id')
    if not isinstance(pane, str) or not pane: raise RuntimeError('workspace created but pane is unknown')
    herdr(['agent', 'start', name, '--kind', job['agent_kind'], '--pane', pane], executable=executable)
    context = json.dumps(brief['brief'], sort_keys=True)
    herdr(['agent', 'prompt', name, f'Orbit job {job["id"]}. Implement task brief; report tests and changes. Do not complete the Orbit task automatically.\n{context}'], executable=executable)
    return name

def execute_one(server, token, device, folder, job, *, executable='herdr'):
    jid = job['id']
    path = journal_path(folder, jid)
    # An existing journal is an unresolved side effect, never a fresh launch.
    if path.exists(): return 'reconcile_required'
    context = api(server, token, device, 'brief?id=' + quote(jid, safe=''))
    if context['job']['id'] != jid or context['job']['machine_ref'] != device:
        raise ValueError('authority brief does not match assigned job')
    claim_token = hashlib.sha256(('orbit-machine-v1:' + device + ':' + jid).encode()).hexdigest()
    # Capture a pre-launch baseline to distinguish prior history from job-era changes.
    # If the Git repository is unreadable, fail before claiming a launch.
    try:
        baseline = git_output(job['repository_ref'], 'rev-parse', 'HEAD')
    except (RuntimeError, OSError, subprocess.TimeoutExpired):
        baseline = None  # Preserve legacy launch behavior; no attributable commit range.
    if baseline is not None and len(baseline) not in (40, 64):
        baseline = None
    base_entry = {'job_id': jid, 'claim_token': claim_token, 'baseline_head': baseline}
    # Fence before the network claim. Lost acknowledgments must not trigger a new launch.
    journal_write(path, {**base_entry, 'phase': 'claim_unknown'})
    receipt = api(server, token, device, 'claim', {
        'job_id': jid, 'expected_revision': job['revision'],
        'claim_token': claim_token, 'at_ms': max(timestamp(), job['updated_at_ms']),
    })
    if not receipt['claimed']:
        journal_write(path, {**base_entry, 'phase': 'reconcile_required'})
        return 'reconcile_required'
    journal_write(path, {**base_entry, 'phase': 'dispatch_unknown'})
    try:
        session = launch_runtime(job, context, claim_token, executable)
    except Exception:
        # A Herdr error can occur after a workspace or session was created.
        # Keep the ambiguity in the journal even if the authority is offline.
        journal_write(path, {**base_entry, 'phase': 'ambiguous_report_pending'})
        try:
            api(server, token, device, 'launch-result', {'job_id': jid, 'claim_token': claim_token, 'session_ref': None, 'at_ms': timestamp()})
        except Exception:
            return 'ambiguous_report_pending'
        journal_write(path, {**base_entry, 'phase': 'ambiguous_reported'})
        return 'ambiguous'
    journal_write(path, {**base_entry, 'phase': 'report_pending', 'session_ref': session})
    api(server, token, device, 'launch-result', {'job_id': jid, 'claim_token': claim_token, 'session_ref': session, 'at_ms': timestamp()})
    journal_write(path, {**base_entry, 'phase': 'reported', 'session_ref': session})
    return 'launched'

def report_completion_evidence(server, token, device, path, entry, job, context):
    """Journal an immutable snapshot before HTTP; retry identical payload after failure."""
    if 'evidence_receipt' in entry:
        return 'already_reported'
    if not entry.get('evidence_pending'):
        payload = build_evidence(job, context, entry['claim_token'], entry.get('baseline_head'))
        fingerprint = hashlib.sha256(json.dumps(payload, sort_keys=True).encode()).hexdigest()
        pending = {'job_id': entry['job_id'], 'claim_token': entry['claim_token'],
                   'session_ref': entry['session_ref'], 'handoff_id': 'machine-' + hashlib.sha256((entry['job_id'] + ':' + fingerprint).encode()).hexdigest(),
                   'payload': payload, 'at_ms': timestamp()}
        entry = {**entry, 'evidence_pending': pending}
        journal_write(path, entry)
    api(server, token, device, 'handoff', entry['evidence_pending'])
    journal_write(path, {**entry, 'evidence_receipt': entry['evidence_pending']['handoff_id'],
                         'evidence_pending': None})
    return 'reported'

def reconcile_one(server, token, device, path, *, executable='herdr'):
    """Inspect only an already journaled session; never create an agent."""
    entry = json.loads(path.read_text())
    if entry.get('phase') not in ('reported', 'report_pending', 'dispatch_unknown', 'reconcile_required', 'claim_unknown', 'ambiguous_report_pending', 'ambiguous_reported'):
        return 'invalid_journal'
    jid = entry['job_id']
    context = api(server, token, device, 'brief?id=' + quote(jid, safe=''))
    job = context['job']
    if job['machine_ref'] != device or job['id'] != jid:
        raise ValueError('journal machine or job mismatch')
    claim = entry['claim_token']
    if entry['phase'] == 'ambiguous_report_pending':
        # Idempotent report only; never restart a potentially created session.
        api(server, token, device, 'launch-result', {'job_id': jid, 'claim_token': claim, 'session_ref': None, 'at_ms': timestamp()})
        journal_write(path, {**entry, 'phase': 'ambiguous_reported'})
        return 'operator_reconciliation_required'
    if entry['phase'] in ('claim_unknown', 'dispatch_unknown', 'reconcile_required', 'ambiguous_reported'):
        # Cannot prove whether the side effect occurred. Never retry the launch.
        return 'operator_reconciliation_required'
    session = entry['session_ref']
    if entry['phase'] == 'report_pending':
        # A lost launch-result acknowledgment must be replayed before observation.
        # The server's claim token and bound session fence duplicate attempts.
        api(server, token, device, 'launch-result', {'job_id': jid, 'claim_token': claim, 'session_ref': session, 'at_ms': timestamp()})
        journal_write(path, {**entry, 'phase':'reported'})
    try:
        response = herdr(['agent', 'get', session], executable=executable)
        status = response.get('result', {}).get('agent', {}).get('status')
        if status is None: status = response.get('result', {}).get('status')
        if status is None: status = response.get('result', {}).get('agent', {}).get('state')
        activity = {'working':'working', 'running':'working', 'active':'working',
                    'blocked':'blocked', 'waiting_input':'waiting_input',
                    'waiting-for-input':'waiting_input', 'idle':'idle',
                    'done':'idle', 'completed':'idle'}.get(status, 'unreachable')
    except (RuntimeError, subprocess.TimeoutExpired, OSError, ValueError):
        activity = 'unreachable'
    payload = {'job_id': jid, 'claim_token': claim, 'expected_revision': job['revision'],
               'session_ref': session if activity != 'unreachable' else None,
               'activity': activity, 'at_ms': max(timestamp(), job['updated_at_ms'])}
    result = api(server, token, device, 'observe', payload)
    # Reporting failure does not discard an already journaled Git snapshot.
    if activity == 'idle':
        try:
            report_completion_evidence(server, token, device, path, entry, job, context)
        except (RuntimeError, OSError, ValueError, KeyError, urllib.error.URLError):
            return 'evidence_pending_review'
    elif entry.get('evidence_pending'):
        try:
            report_completion_evidence(server, token, device, path, entry, job, context)
        except (RuntimeError, OSError, ValueError, KeyError, urllib.error.URLError):
            return 'evidence_pending_review'
    return result.get('state', activity)


def deliver_prompts(server, token, device, folder, *, executable='herdr'):
    """Fenced prompt delivery to existing sessions; never starts a new agent.

    Claim is committed at the authority before the subprocess side effect. If
    an acknowledgment disappears, the request remains claimed and is not replayed.
    """
    results = []
    for command in api(server, token, device, 'prompts'):
        identifier = command['command_id']
        try:
            receipt = api(server, token, device, 'prompt-claim',
                {'command_id':identifier,'at_ms':timestamp()})
            if not receipt.get('claimed'):
                results.append({'command_id':identifier,'state':'already_claimed'})
                continue
            # Persist an additional machine-local trace; an uncertain command is
            # not automatically retried even when the worker restarts.
            trace = folder / ('prompt-' + hashlib.sha256(identifier.encode()).hexdigest() + '.prompt-journal')
            journal_write(trace, {'command_id':identifier,'job_id':command['job_id'],
                'session_ref':command['session_ref'],'state':'dispatched'})
            try:
                herdr(['agent','prompt',command['session_ref'],command['prompt']],executable=executable)
                delivered = True
            except (OSError,RuntimeError,ValueError,subprocess.TimeoutExpired):
                delivered = False
            journal_write(trace, {'command_id':identifier,'job_id':command['job_id'],
                'session_ref':command['session_ref'],
                'state':'confirmed' if delivered else 'uncertain'})
            try:
                api(server, token, device, 'prompt-result',
                    {'command_id':identifier,'delivered':delivered,'at_ms':timestamp()})
            except (OSError,urllib.error.URLError,RuntimeError):
                pass  # Durable authority claim prevents duplicate prompt delivery.
            results.append({'command_id':identifier,'state':'confirmed' if delivered else 'uncertain'})
        except (OSError,ValueError,KeyError,RuntimeError,urllib.error.URLError) as error:
            results.append({'command_id':identifier,'state':'error','error':str(error)[:120]})
    return results

def reconcile(server, token, device, folder, *, executable='herdr'):
    results=[]
    for path in sorted(folder.glob('*.json')) if folder.exists() else []:
        try:
            results.append({'journal':path.name, 'result':reconcile_one(server,token,device,path,executable=executable)})
        except Exception as error:
            results.append({'journal':path.name, 'result':'error', 'error':str(error)[:180]})
    return results

def poll(server, token, device, folder, *, executable='herdr'):
    results = []
    for job in api(server, token, device, 'inbox'):
        try:
            results.append({'job_id': job['id'], 'result': execute_one(server, token, device, folder, job, executable=executable)})
        except Exception as error:
            results.append({'job_id': job.get('id'), 'result': 'error', 'error': str(error)[:180]})
    return results

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server', required=True)
    parser.add_argument('--device', required=True, help='enrolled Orbit machine device ID')
    parser.add_argument('--journal-dir', required=True, type=Path)
    parser.add_argument('--interval', type=int, default=30)
    parser.add_argument('--once', action='store_true')
    parser.add_argument('--mode', choices=('all','launch','monitor','health'), default='all')
    args = parser.parse_args()
    try:
        validate_server(args.server)
    except ValueError as error:
        parser.error(str(error))
    if args.interval < 5:
        parser.error('polling interval must be >= 5')
    token = os.environ.get('ORBIT_SYNC_HTTP_TOKEN')
    if not token: parser.error('ORBIT_SYNC_HTTP_TOKEN required')
    with exclusive_worker(args.journal_dir):
      while True:
        try:
            result = {}
            if args.mode == 'health':
                result['health'] = journal_health(args.journal_dir)
            if args.mode in ('all','launch'):
                result['launches'] = poll(args.server, token, args.device, args.journal_dir)
            if args.mode in ('all','monitor'):
                result['observations'] = reconcile(args.server, token, args.device, args.journal_dir)
                result['prompts'] = deliver_prompts(args.server, token, args.device, args.journal_dir)
            print(json.dumps(result))
        except Exception as exc:
            print(json.dumps({'error': str(exc)[:180]}))
            if args.once: raise
        if args.once or args.mode == 'health': break
        time.sleep(args.interval)

if __name__ == '__main__': main()
