"""Executable tests for machine-scoped evidence and durable reporting."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('worker', Path(__file__).with_name('agent-machine-worker.py'))
w = importlib.util.module_from_spec(spec)
spec.loader.exec_module(w)


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True, stderr=subprocess.DEVNULL).strip()

class MachineEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root/'repo'
        self.repo.mkdir()
        git(self.repo, 'init', '-q')
        git(self.repo, 'config', 'user.email', 'robot@example.invalid')
        git(self.repo, 'config', 'user.name', 'Robot')
        (self.repo/'README.md').write_text('initial\n')
        git(self.repo, 'add', '.')
        git(self.repo, 'commit', '-qm', 'initial')
        self.job = {'id': 'job-one', 'repository_ref': str(self.repo)}
        self.claim = 'launch-token'

    def test_unisolated_snapshot_has_real_git_evidence(self):
        (self.repo/'new.txt').write_text('new\n')
        evidence = w.build_evidence(self.job, {'worktree': False}, self.claim)
        self.assertIn('new.txt', evidence['changed_files'])
        self.assertEqual(evidence['git']['branch'], git(self.repo, 'branch', '--show-current'))
        self.assertFalse(evidence['agent_authorship_verified'])
        self.assertEqual(evidence['validation']['status'], 'not_reported')

    def test_isolation_only_reads_actual_job_worktree(self):
        name = w.safe_name(self.claim)
        isolated = self.root/'isolated'
        git(self.repo, 'worktree', 'add', '-qb', 'orbit/'+name, str(isolated))
        (isolated/'isolated.txt').write_text('work\n')
        (self.repo/'main.txt').write_text('unrelated\n')
        evidence = w.build_evidence(self.job, {'worktree': {'enabled':True}}, self.claim)
        self.assertEqual(evidence['git']['worktree'], str(isolated))
        self.assertIn('isolated.txt', evidence['changed_files'])
        self.assertNotIn('main.txt', evidence['changed_files'])

    def test_missing_isolated_worktree_fails_closed(self):
        with self.assertRaisesRegex(ValueError, 'not uniquely identifiable'):
            w.build_evidence(self.job, {'worktree':True}, self.claim)

    def test_journaled_handoff_retries_identical_payload(self):
        path = self.root/'journal.json'
        entry = {'job_id':'job-one','claim_token':self.claim,'session_ref':'session','phase':'reported'}
        w.journal_write(path, entry)
        context = {'worktree': False}
        calls = []
        def failed_api(*args):
            calls.append(args[-1])
            if len(calls)==1: raise OSError('connection failed after commit')
            return []
        with patch.object(w,'api',side_effect=failed_api):
            with self.assertRaises(OSError):
                w.report_completion_evidence('https://host','token','device',path,entry,self.job,context)
            pending=json.loads(path.read_text())
            self.assertIn('evidence_pending', pending)
            w.report_completion_evidence('https://host','token','device',path,pending,self.job,context)
            recorded=json.loads(path.read_text())
            self.assertEqual(calls[0],calls[1])
            self.assertEqual(recorded['evidence_receipt'],calls[0]['handoff_id'])
            self.assertEqual(recorded['evidence_pending'],None)
            w.report_completion_evidence('https://host','token','device',path,recorded,self.job,context)
            self.assertEqual(len(calls),2)

    def test_authoritative_machine_handoff_is_not_task_approval(self):
        payload=w.build_evidence(self.job,{'worktree':False},self.claim)
        self.assertNotIn('approved',json.dumps(payload))
        self.assertNotIn('task_completed',json.dumps(payload))

if __name__=='__main__': unittest.main()
