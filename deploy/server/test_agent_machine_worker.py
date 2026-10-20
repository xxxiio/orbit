"""Contract tests for the opt-in connected Herdr machine worker."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('machine_worker', Path(__file__).with_name('agent-machine-worker.py'))
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)

class WorkerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name)
        self.repo = self.folder / 'repo'
        self.repo.mkdir()
        self.job = dict(id='job-1', agent_kind='codex', runtime_kind='herdr', machine_ref='machine-1',
                        repository_ref=str(self.repo), revision=1, updated_at_ms=1)
        self.calls = []

    def api(self, server, token, device, endpoint, payload=None):
        self.calls.append((endpoint, payload))
        if endpoint.startswith('brief?'):
            return dict(job=self.job, brief={'title': 'Task'}, worktree=False)
        if endpoint == 'claim':
            return {'claimed': True, 'job': dict(self.job, state='launching', revision=2)}
        if endpoint == 'inbox':
            return [self.job]
        return dict(self.job)

    def test_launch_claim_persists_before_external_execution_and_never_relaunches(self):
        def runner(job, brief, token, executable):
            path = worker.journal_path(self.folder, job['id'])
            self.assertEqual('dispatch_unknown', __import__('json').loads(path.read_text())['phase'])
            return worker.safe_name(token)
        with patch.object(worker, 'api', self.api), patch.object(worker, 'launch_runtime', runner):
            self.assertEqual(worker.execute_one('https://orbit', 'token', 'machine-1', self.folder, self.job), 'launched')
            self.assertEqual(worker.execute_one('https://orbit', 'token', 'machine-1', self.folder, self.job), 'reconcile_required')
        self.assertEqual([v[0] for v in self.calls].count('claim'), 1)
        self.assertEqual([v[0] for v in self.calls].count('launch-result'), 1)

    def test_lost_claim_response_never_launches(self):
        def api(*args):
            if args[3] == 'claim': raise TimeoutError('lost claim acknowledgment')
            return self.api(*args)
        with patch.object(worker, 'api', api), patch.object(worker, 'launch_runtime') as run:
            with self.assertRaises(TimeoutError):
                worker.execute_one('https://orbit', 'token', 'machine-1', self.folder, self.job)
            self.assertEqual(worker.execute_one('https://orbit', 'token', 'machine-1', self.folder, self.job), 'reconcile_required')
            run.assert_not_called()

    def test_ambiguous_herdr_failure_is_recorded_not_retried(self):
        with patch.object(worker, 'api', self.api), patch.object(worker, 'launch_runtime', side_effect=RuntimeError('transport lost')):
            self.assertEqual(worker.execute_one('https://orbit', 'token', 'machine-1', self.folder, self.job), 'ambiguous')
        self.assertIsNone([x for x in self.calls if x[0] == 'launch-result'][0][1]['session_ref'])

    def test_rejects_cross_machine_brief(self):
        def api(server, token, device, endpoint, payload=None):
            value = self.api(server, token, device, endpoint, payload)
            if endpoint.startswith('brief?'):
                value['job'] = dict(self.job, machine_ref='another-machine')
            return value
        with patch.object(worker, 'api', api), patch.object(worker, 'launch_runtime') as run:
            with self.assertRaises(ValueError):
                worker.execute_one('https://orbit', 'token', 'machine-1', self.folder, self.job)
            run.assert_not_called()

    def test_worktree_command_keeps_local_execution(self):
        calls=[]
        def herdr(args, executable='herdr'):
            calls.append(args)
            if args[0] == 'worktree': return {'result': {'root_pane': {'pane_id': 'pane-1'}}}
            return {'result': {}}
        with patch.object(worker, 'herdr', herdr):
            name = worker.launch_runtime(self.job, {'brief': {'title':'Build'},'worktree':True}, 'launch-token', 'herdr')
        self.assertEqual(calls[0][0:2], ['worktree', 'create'])
        self.assertIn('orbit/' + name, calls[0])
        self.assertEqual(calls[1][0:2], ['agent','start'])
        self.assertEqual(calls[2][0:2], ['agent','prompt'])
        self.assertTrue(all('--machine' not in args for args in calls))

if __name__ == '__main__': unittest.main()
