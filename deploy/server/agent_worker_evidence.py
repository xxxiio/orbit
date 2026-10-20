"""Read-only Git evidence and deterministic Herdr workspace naming.

Pure machine-local operations; this module cannot access Orbit authority or
submit handoffs. External side effects remain with the machine coordinator.
"""
import subprocess
from pathlib import Path

def safe_name(token):
    # Must match orbit-runtime/src/herdr_runtime.rs's FNV-1a naming.
    value = 0xcbf29ce484222325
    for byte in token.encode():
        value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return f'orbit{value:016x}'


def git_output(repository, *arguments):
    result = subprocess.run(['git', '-C', str(repository), *arguments], capture_output=True,
                            timeout=20, check=False)
    if result.returncode:
        raise RuntimeError('Git evidence unavailable: ' + result.stderr.decode(errors='replace')[:160])
    return result.stdout.decode('utf-8', errors='replace').strip()

def evidence_repository(job, context, claim):
    """Resolve exact job checkout; never attribute the main checkout to an isolated agent."""
    repository = Path(job['repository_ref'])
    if not repository.is_absolute() or not repository.is_dir():
        raise ValueError('repository unavailable on assigned machine')
    policy = context.get('worktree')
    isolated = policy is True or (isinstance(policy, dict) and policy.get('enabled') is True)
    if not isolated:
        git_output(repository, 'rev-parse', '--show-toplevel')
        return repository
    branch = 'refs/heads/orbit/' + safe_name(claim)
    records = git_output(repository, 'worktree', 'list', '--porcelain').splitlines()
    matches, location, recorded_branch = [], None, None
    def flush():
        if location and recorded_branch == branch:
            matches.append(Path(location))
    for line in records + ['']:
        if not line:
            flush()
            location, recorded_branch = None, None
        elif line.startswith('worktree '): location = line[len('worktree '):]
        elif line.startswith('branch '): recorded_branch = line[len('branch '):]
    if len(matches) != 1 or not matches[0].is_dir():
        raise ValueError('job worktree not uniquely identifiable; evidence withheld')
    if git_output(matches[0], 'symbolic-ref', 'HEAD') != branch:
        raise ValueError('job worktree branch mismatch; evidence withheld')
    return matches[0]

def build_evidence(job, context, claim, baseline_head=None):
    directory = evidence_repository(job, context, claim)
    head = git_output(directory, 'rev-parse', 'HEAD')
    branch = git_output(directory, 'symbolic-ref', '--short', 'HEAD')
    modified = git_output(directory, 'status', '--porcelain=v1', '--untracked-files=normal')
    if baseline_head:
        # Fail closed if a history rewrite makes the baseline unreachable.
        ancestor = subprocess.run(['git', '-C', str(directory), 'merge-base', '--is-ancestor', baseline_head, head],
                                  capture_output=True, timeout=20, check=False)
        if ancestor.returncode != 0:
            raise ValueError('launch baseline is not an ancestor of current HEAD; evidence withheld')
        commits = git_output(directory, 'log', '--format=%H', f'{baseline_head}..HEAD').splitlines()
        committed = git_output(directory, 'diff', '--name-only', baseline_head, 'HEAD').splitlines()
    else:
        commits = []  # Legacy journals cannot establish pre-launch commit attribution.
        committed = []
    changed = sorted(set(committed + [line[3:] for line in modified.splitlines()]))
    return {'summary': 'Agent session became idle; review Git snapshot and validation separately.',
            'validation': {'status': 'not_reported', 'note': 'Herdr idle is not passing tests or accepted task completion'},
            'next_steps': ['Review changed files and run acceptance checks'],
            'commits': commits, 'changed_files': changed,
            'git': {'head': head, 'launch_baseline': baseline_head, 'branch': branch, 'worktree': str(directory)},
            'evidence_source': 'machine_git_snapshot', 'agent_authorship_verified': False}

