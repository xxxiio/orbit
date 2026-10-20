import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('worker',Path(__file__).with_name('agent-machine-worker.py'))
w=importlib.util.module_from_spec(spec);spec.loader.exec_module(w)

class RecoveryTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.folder=Path(self.tmp.name)
    def test_competing_worker_rejected(self):
        with w.exclusive_worker(self.folder):
            with self.assertRaisesRegex(RuntimeError,'another Orbit machine worker'):
                with w.exclusive_worker(self.folder): pass
        with w.exclusive_worker(self.folder): pass
    def test_health_flags_uncertain_claim_and_malformed_journal(self):
        w.journal_write(self.folder/'a.json',{'job_id':'job','phase':'dispatch_unknown'})
        w.journal_write(self.folder/'b.json',{'job_id':'done','phase':'reported'})
        (self.folder/'c.json').write_text('{')
        result=w.journal_health(self.folder)
        self.assertEqual(result['journals_by_phase'],{'dispatch_unknown':1,'reported':1})
        self.assertEqual({x['phase'] for x in result['attention']},{'dispatch_unknown','invalid_journal'})
    def test_health_has_no_execution_side_effects(self):
        self.assertEqual(w.journal_health(self.folder),{'journals_by_phase':{},'attention':[]})
        self.assertFalse(list(self.folder.iterdir()))

if __name__=='__main__':unittest.main()
