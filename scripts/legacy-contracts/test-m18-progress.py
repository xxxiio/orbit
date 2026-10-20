#!/usr/bin/env python3
"""Tests that progress reporting cannot silently mark unresolved M18 complete."""
import importlib.util
from pathlib import Path
import unittest
spec=importlib.util.spec_from_file_location('m18progress',Path(__file__).with_name('m18-progress.py'))
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
class Evidence(unittest.TestCase):
    def test_weights_and_no_false_completion(self):
        report=module.assess()
        self.assertEqual(sum(g['weight'] for g in report['gates']),100)
        self.assertFalse(report['m18_complete'])
        self.assertEqual(report['acceptance'],'NOT_ESTABLISHED')
        self.assertLessEqual(report['source_evidence_score'],60)
    def test_single_owner_has_source_evidence_but_not_acceptance(self):
        report=module.assess()
        ids={g['id']:g for g in report['gates']}
        self.assertFalse(any(b['still_present'] for b in ids['single_authority']['blocking_patterns']))
        self.assertGreater(ids['single_authority']['source_credit'],0)
        self.assertFalse(ids['single_authority']['complete'])
    def test_no_source_acceptance_credit(self):
        ids={g['id']:g for g in module.assess()['gates']}
        self.assertEqual(ids['executable_acceptance']['source_credit'],0)
        self.assertEqual(ids['internal_testing']['source_credit'],0)
if __name__=='__main__': unittest.main()
