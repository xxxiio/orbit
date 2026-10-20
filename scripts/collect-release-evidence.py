#!/usr/bin/env python3
"""Noninteractive, isolated M15 acceptance runner.

Runs all selected checks (including after failures), creates a redacted evidence ZIP,
and returns 0 only when every required check executes successfully. Never installs
software, contacts a production server, or uses a user's Orbit database.
"""
import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SECRET = [
    re.compile(r'(?i)(authorization\s*:\s*bearer\s+)\S+'),
    re.compile(r'(?i)((?:ORBIT_[A-Z_]*(?:TOKEN|KEY|SECRET|PASSWORD)|password|api[_-]?key)\s*[=:]\s*)\S+'),
]


def redact(message):
    for rule in SECRET:
        message = rule.sub(r'\1[REDACTED]', message)
    return message


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--profile', choices=('quick', 'full'), default='full')
    p.add_argument('--output', type=Path, help='Destination for evidence ZIP')
    p.add_argument('--offline', action='store_true', help='Run Cargo with cached dependencies only')
    p.add_argument('--timeout', type=int, default=1200, help='Timeout in seconds per check')
    p.add_argument('--no-setup-prompt', action='store_true', help='Deprecated: all runs are noninteractive')
    p.add_argument('--require-herdr', action='store_true', help='Require installed Herdr CLI; checks presence/version only, not live credentials')
    p.add_argument('--live-herdr', action='store_true', help='Opt in to the live Herdr acceptance: starts a real agent in a disposable temporary repository and Orbit workspace, then closes the Herdr workspace')
    args = p.parse_args(argv)
    if args.timeout < 1:
        p.error('--timeout must be positive')
    stamp = dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    output = (args.output or (ROOT / f'orbit-m15-evidence-{stamp}.zip')).expanduser().resolve()
    if output.suffix.lower() != '.zip':
        p.error('--output must be a .zip')
    output.parent.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env['PATH'] = str(Path.home() / '.cargo' / 'bin') + os.pathsep + env.get('PATH', '')
    env['CARGO_TERM_COLOR'] = 'never'
    env['NO_COLOR'] = '1'
    env['PYTHONDONTWRITEBYTECODE'] = '1'
    if args.offline:
        env['CARGO_NET_OFFLINE'] = 'true'
    with tempfile.TemporaryDirectory(prefix='orbit-m15-') as temp:
        evidence = Path(temp)
        results = []

        def check(name, command, *, needs=(), after=(), timeout=None):
            missing = [tool for tool in needs if shutil.which(tool, path=env['PATH']) is None]
            blocked = [item for item in after if next((r['status'] for r in results if r['name'] == item), None) != 'passed']
            rec = {'name': name, 'command': command, 'status': 'not_run', 'exit_code': None,
                   'duration_seconds': 0, 'log': f'{len(results) + 1:02d}-{name}.log'}
            results.append(rec)
            if missing or blocked:
                rec['reason'] = ('missing executable(s): ' + ', '.join(missing)) if missing else ('prerequisite checks not passed: ' + ', '.join(blocked))
                (evidence / rec['log']).write_text(rec['reason'] + '\n', encoding='utf-8')
                print(f'SKIP {name}: {rec["reason"]}', flush=True)
                return
            print(f'RUN  {name}', flush=True)
            start = time.monotonic()
            try:
                completed = subprocess.run(command, cwd=ROOT, env=env, stdout=subprocess.PIPE,
                                           stderr=subprocess.STDOUT, timeout=timeout or args.timeout,
                                           text=True, errors='replace', stdin=subprocess.DEVNULL)
                log = completed.stdout
                rec['exit_code'] = completed.returncode
                rec['status'] = 'passed' if completed.returncode == 0 else 'failed'
            except subprocess.TimeoutExpired as exc:
                log = exc.stdout or b''
                if isinstance(log, bytes):
                    log = log.decode('utf-8', 'replace')
                log += '\n[RUNNER] Check timed out.\n'
                rec['status'] = 'timeout'
            except OSError as exc:
                log = str(exc)
                rec['status'] = 'error'
            rec['duration_seconds'] = round(time.monotonic() - start, 2)
            (evidence / rec['log']).write_text(redact(log), encoding='utf-8')
            print(f'{rec["status"].upper()} {name} ({rec["duration_seconds"]}s)', flush=True)

        check('python-version', [sys.executable, '--version'])
        check('user-documentation-contract', [sys.executable, '-m', 'unittest', 'discover', '-s', 'tests/documentation', '-v'])
        check('installer-contract', ['bash', 'tests/packaging/test_install.sh'], needs=('bash',))
        check('installer-transaction-contract', ['bash', 'tests/packaging/test_install_transaction.sh'], needs=('bash',))
        check('installation-environment-contract', ['bash', 'tests/packaging/test_orbit_env.sh'], needs=('bash',))
        check('installation-manifest-safety', ['bash', 'tests/packaging/test_manifest_safety.sh'], needs=('bash',))
        check('release-archive-contract', ['bash', 'tests/packaging/test_release_archive.sh'], needs=('bash', 'tar', 'sha256sum'))
        check('release-installation-contract', ['bash', 'tests/packaging/test_install_release_archive.sh'], needs=('bash', 'tar', 'sha256sum'))
        check('optional-worker-installer-contract', ['bash', 'scripts/test-worker-install.sh'], needs=('bash', 'python3'))
        check('server-setup-contract', [sys.executable, '-m', 'unittest', 'discover', '-s', 'tests/packaging', '-p', 'test_server_setup.py', '-v'])
        check('rust-toolchain', ['rustc', '--version'], needs=('rustc',))
        check('cargo-version', ['cargo', '--version'], needs=('cargo',))
        check('cargo-metadata', ['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'], needs=('cargo',))
        check('cargo-format', ['cargo', 'fmt', '--all', '--', '--check'], needs=('cargo', 'rustfmt'))
        check('cargo-check', ['cargo', 'check', '--workspace', '--all-targets', '--locked'], needs=('cargo',))
        check('cargo-test', ['cargo', 'test', '--workspace', '--all-targets', '--locked'], needs=('cargo',))
        check('python-worker-tests', [sys.executable, '-m', 'unittest', 'discover', '-s', 'deploy/server', '-p', 'test_*.py', '-v'])
        check('python-contract-tests', [sys.executable, '-m', 'pytest', '-q', 'tests'])
        check('m25-blueprint-contracts', [sys.executable, 'scripts/check-m25-blueprint-contracts.py'])
        check('m25-blueprint-crate-tests', ['cargo', 'test', '-p', 'orbit-blueprint', '--locked'], needs=('cargo',))
        check('m25-blueprint-cli-tests', ['cargo', 'test', '-p', 'orbit-cli', 'blueprint', '--locked'], needs=('cargo',))
        if args.profile == 'full':
            check('cargo-clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings'], needs=('cargo', 'cargo-clippy'))
            check('cargo-build', ['cargo', 'build', '-p', 'orbit-cli', '-p', 'orbit-server', '--locked'], needs=('cargo',))
            check('local-cli-persistence-smoke', [sys.executable, 'tests/acceptance/local_cli_workflows.py', '--binary', 'target/debug/orbit'], after=('cargo-build',))
            check('real-release-archive-smoke', ['bash', 'tests/packaging/test_real_release_archive.sh'], needs=('cargo', 'bash', 'tar', 'sha256sum'), after=('cargo-build',))
            check('m25-blueprint-cli-help', ['target/debug/orbit', 'blueprint', '--help'], after=('cargo-build',))
            for builtin in ('development', 'learning', 'habits', 'research'):
                check('m25-validate-' + builtin,
                      ['target/debug/orbit', 'blueprint', 'validate', f'blueprints/{builtin}.json'],
                      after=('cargo-build',))
            check('server-task-smoke', [sys.executable, 'deploy/server/smoke-authoritative-tasks.py', 'target/debug/orbit', 'target/debug/orbit-server'], after=('cargo-build',))
            check('server-domain-smoke', [sys.executable, 'deploy/server/smoke-authoritative-domains.py', 'target/debug/orbit-server'], after=('cargo-build',))
            check('local-authority-smoke', [sys.executable, 'deploy/server/smoke-local-authority.py', '--orbit', 'target/debug/orbit'], after=('cargo-build',))
            check('server-durability-smoke', [sys.executable, 'deploy/server/smoke-server-durability.py', '--orbit', 'target/debug/orbit', '--server', 'target/debug/orbit-server'], after=('cargo-build',))
            check('offline-authority-smoke', [sys.executable, 'deploy/server/smoke-offline-authority.py', '--orbit', 'target/debug/orbit', '--server', 'target/debug/orbit-server'], after=('cargo-build',))
            check('blueprint-workflow-smoke', [sys.executable, 'deploy/server/smoke-blueprint-workflows.py', '--orbit', 'target/debug/orbit', '--server', 'target/debug/orbit-server'], after=('cargo-build',))
            check('agent-execution-contract', ['cargo', 'test', '-p', 'orbit-runtime', '--test', 'agent_execution_contract', '--locked'], needs=('cargo',), after=('cargo-build',))
            if args.require_herdr:
                check('herdr-available', ['herdr', '--version'], needs=('herdr',))
            if args.live_herdr:
                check('herdr-live-acceptance',
                      [sys.executable, 'deploy/server/smoke-herdr-live.py',
                       '--orbit', 'target/debug/orbit', '--confirm-live'],
                      needs=('herdr',), after=('cargo-build',))
        counts = {s: sum(r['status'] == s for r in results) for s in ('passed', 'failed', 'timeout', 'error', 'not_run')}
        manifest = {'schema_version': 2, 'created_at_utc': dt.datetime.now(dt.timezone.utc).isoformat(),
                    'profile': args.profile, 'offline': args.offline, 'platform': platform.system(),
                    'architecture': platform.machine(), 'python_version': platform.python_version(),
                    'repository_manifest_sha256': hashlib.sha256((ROOT / 'Cargo.toml').read_bytes()).hexdigest(),
                    'counts': counts, 'results': results,
                    'scope': 'Compiles and tests the Rust workspace, runs Python worker/contract suites and user-documentation contracts, exercises packaging/installation and release-archive installation contracts, verifies compiled local CLI persistence, and executes isolated local-authority, connected-authority, offline-replica, server-durability, blueprint-workflow and agent-execution acceptance. Live Herdr acceptance is opt-in via --live-herdr; TLS ingress, browser deployment and production calls are never attempted.',
                    'privacy': 'Generated logs may contain paths and unexpected identifiers; review before sharing.'}
        (evidence / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
        summary = ['# Orbit M15 automated acceptance', '', f'Profile: {args.profile}', '', '| Check | Status |', '|---|---|']
        summary += [f'| {r["name"]} | {r["status"]} |' for r in results]
        summary += ['', f'Counts: {json.dumps(counts)}', '', 'Exit 0: complete pass; 1: failure; 2: incomplete (required check skipped).',
                    'Live Herdr, production deployment, and destructive migration drills are not automatically run.',
                    'Blueprint manifest inventory checks do not substitute for compiled Rust blueprint tests.',
                    'Documentation and packaging checks validate repository artifacts only; they do not install software system-wide.',
                    'The local CLI persistence smoke uses the compiled target/debug/orbit binary against disposable local data.',
                    'All HTTP smoke tests target temporary loopback servers and disposable databases.']
        (evidence / 'SUMMARY.md').write_text('\n'.join(summary) + '\n', encoding='utf-8')
        with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED) as archive:
            for item in sorted(evidence.iterdir()):
                archive.write(item, arcname=item.name)
    print(f'\nEvidence: {output}\nCounts: {counts}', flush=True)
    return 1 if sum(counts[s] for s in ('failed', 'timeout', 'error')) else (2 if counts['not_run'] else 0)


if __name__ == '__main__':
    sys.exit(main())
