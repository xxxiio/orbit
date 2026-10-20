"""Machine-local crash journal for irreversible agent side effects.

This is not the client's canonical offline-command queue. The worker keeps
launch fencing on the execution machine even if the authority is unreachable.
"""
import fcntl
import hashlib
import json
import os
import tempfile
from pathlib import Path
from contextlib import contextmanager

def journal_write(path, value):
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=path.parent, prefix='.orbit-claim-')
    try:
        with os.fdopen(fd, 'w') as stream:
            os.fchmod(stream.fileno(), 0o600)
            json.dump(value, stream)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY)
        try: os.fsync(directory)
        finally: os.close(directory)
    finally:
        if os.path.exists(temporary): os.unlink(temporary)

@contextmanager
def exclusive_worker(folder):
    """Prevent concurrent worker processes using the same machine journal.

    The advisory lock is held across claim, dispatch, reporting and inspection.
    It is released by the OS on abnormal process termination.
    """
    folder.mkdir(mode=0o700, parents=True, exist_ok=True)
    lock_path = folder / '.worker.lock'
    fd = os.open(lock_path, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise RuntimeError('another Orbit machine worker owns this journal directory') from error
        yield
    finally:
        os.close(fd)


def journal_health(folder):
    """Offline diagnostics: no Herdr or authority side effects."""
    counts = {}
    issues = []
    for path in sorted(folder.glob('*.json')) if folder.exists() else []:
        try:
            value = json.loads(path.read_text())
            phase = value['phase']
            if not isinstance(phase,str) or not isinstance(value['job_id'],str):
                raise ValueError('invalid journal fields')
            counts[phase] = counts.get(phase,0)+1
            if phase not in ('reported',):
                issues.append({'journal':path.name,'job_id':value['job_id'],'phase':phase})
        except (OSError,ValueError,KeyError,TypeError):
            issues.append({'journal':path.name,'phase':'invalid_journal'})
    return {'journals_by_phase':counts,'attention':issues}

def journal_path(folder, job_id):
    return folder / (hashlib.sha256(job_id.encode()).hexdigest() + '.json')

