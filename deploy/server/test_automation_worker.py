import importlib.util
import unittest
from types import SimpleNamespace
from unittest.mock import patch
from pathlib import Path
spec = importlib.util.spec_from_file_location('worker', Path(__file__).with_name('automation-worker.py'))
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


def sample():
    return {'revision': 7, 'projects': [{'id': 'p'}],
            'tasks': [{'id': 't', 'project_id': 'p'}], 'habits': [{'id': 'h'}],
            'habit_occurrences': [], 'metrics': [], 'metric_observations': []}


class WorkerTests(unittest.TestCase):
    def test_reject_unknown_action(self):
        with self.assertRaises(ValueError):
            worker.execute('run-command', sample())

    def test_project_health_counts_canonical_tasks(self):
        self.assertEqual(worker.execute('project-health-refresh', sample()),
                         {'revision': 7, 'project_health': [{'project_id': 'p', 'task_count': 1}]})

    def test_daily_briefing_uses_accepted_snapshot(self):
        output = worker.execute('daily-briefing', sample())
        self.assertEqual(output['revision'], 7)
        self.assertEqual(output['tasks'][0]['id'], 't')

    def test_habit_report_uses_accepted_snapshot(self):
        self.assertEqual(worker.execute('habit-summary', sample())['habits'], [{'id': 'h'}])

    def test_reject_incomplete_snapshot(self):
        args = SimpleNamespace(server='http://localhost:8787', authority_token='device-token', device='worker')
        with patch.object(worker, 'request', return_value={'revision': 1, 'projects': []}):
            with self.assertRaises(RuntimeError):
                worker.read_snapshot(args)

    def test_claim_and_settle_uses_canonical_data(self):
        args = SimpleNamespace(server='http://localhost:8787', token='admin', authority_token='device', device='worker')
        calls = []
        def fake_request(base, token, path, method='GET', data=None, device=None):
            calls.append((token, path, method, device, data))
            if path.endswith('/claim'): return {'id': 3, 'action': 'habit-summary'}
            if path.endswith('/snapshot'): return sample()
            return None
        with patch.object(worker, 'request', side_effect=fake_request):
            self.assertTrue(worker.run_once(args))
        self.assertEqual(calls[1][0:4], ('device', '/v1/authoritative/snapshot', 'GET', 'worker'))
        self.assertEqual(calls[2][1], '/v1/automation-runs/3/settle')
        self.assertTrue(calls[2][4]['succeeded'])

if __name__ == '__main__':
    unittest.main()
