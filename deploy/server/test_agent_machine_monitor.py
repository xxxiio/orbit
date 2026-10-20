import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('worker',Path(__file__).with_name('agent-machine-worker.py'))
w=importlib.util.module_from_spec(spec);spec.loader.exec_module(w)

class MonitorTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.path=Path(self.temp.name)/'job.json'
        self.entry={'job_id':'j1','claim_token':'tok','phase':'reported','session_ref':'orbit-1'}
        w.journal_write(self.path,self.entry)
        self.calls=[]
    def api(self,server,token,device,endpoint,payload=None):
        self.calls.append((endpoint,payload))
        if endpoint.startswith('brief?'):return {'job':{'id':'j1','machine_ref':'machine','revision':3,'updated_at_ms':1}}
        if endpoint=='observe':return {'state':'waiting_input','revision':4}
        raise AssertionError(endpoint)
    def test_idle_is_reported_without_task_completion(self):
        with patch.object(w,'api',self.api),patch.object(w,'herdr',return_value={'result':{'agent':{'status':'done'}}}):
            w.reconcile_one('https://s','t','machine',self.path)
        self.assertEqual(self.calls[-1][1]['activity'],'idle')
        self.assertTrue(all('launch' not in call[0] for call in self.calls))
    def test_unreachable_is_not_absent(self):
        with patch.object(w,'api',self.api),patch.object(w,'herdr',side_effect=RuntimeError('offline')):
            w.reconcile_one('https://s','t','machine',self.path)
        self.assertEqual(self.calls[-1][1]['activity'],'unreachable')
    def test_uncertain_launch_never_inspected_or_relaunched(self):
        w.journal_write(self.path,{**self.entry,'phase':'dispatch_unknown'})
        with patch.object(w,'api',self.api),patch.object(w,'herdr') as runtime:
            self.assertEqual(w.reconcile_one('https://s','t','machine',self.path),'operator_reconciliation_required')
            runtime.assert_not_called()
    def test_unknown_runtime_state_fails_closed(self):
        with patch.object(w,'api',self.api),patch.object(w,'herdr',return_value={'result':{'status':'mystery'}}):
            w.reconcile_one('https://s','t','machine',self.path)
        self.assertEqual(self.calls[-1][1]['activity'],'unreachable')
    def test_machine_identity_verified(self):
        def api(*a):return {'job':{'id':'j1','machine_ref':'other','revision':3,'updated_at_ms':1}}
        with patch.object(w,'api',api),patch.object(w,'herdr') as runtime:
            with self.assertRaises(ValueError):w.reconcile_one('https://s','t','machine',self.path)
            runtime.assert_not_called()
    def test_batch_failure_is_isolated(self):
        (self.path.parent/'bad.json').write_text('{bad')
        with patch.object(w,'api',self.api),patch.object(w,'herdr',return_value={'result':{'status':'working'}}):
            results=w.reconcile('https://s','t','machine',self.path.parent)
        self.assertEqual(len(results),2)
        self.assertEqual(sum(x['result']=='error' for x in results),1)

if __name__=='__main__':unittest.main()
