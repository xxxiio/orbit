#!/usr/bin/env python3
"""Source-contract checks for plan application crossing authoritative acceptance."""
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]

class PlanAcceptance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.client = (ROOT/'crates/orbit-client/src/lib.rs').read_text()
        cls.runtime = (ROOT/'crates/orbit-runtime/src/accepted_application.rs').read_text()
        cls.services = (ROOT/'crates/orbit-runtime/src/application_services.rs').read_text()
        cls.sync = (ROOT/'crates/orbit-sync/src/lib.rs').read_text()

    def test_both_adapters_use_accepted_commands(self):
        for kind in ('task', 'habit'):
            body = self.client.split(f'pub fn apply_{kind}_plan(')[1].split('\n    pub fn ')[0]
            self.assertIn(f'apply_{kind}_plan_accepted(', body)
            self.assertNotIn(f'application_services().apply_{kind}_plan(', body)
            self.assertIn('self.runtime.snapshot()', body)

    def test_plan_history_preserves_all_block_ids(self):
        self.assertIn('PlanApplication,',self.sync)
        for kind in ('task','habit'):
            body = self.runtime.split(f'pub fn apply_{kind}_plan_accepted(')[1].split('\n    pub fn ')[0]
            self.assertIn('schedule_block_ids',body)
            self.assertIn('command.identities.iter()',body)
            self.assertIn('ids.iter().collect::<std::collections::BTreeSet<_>>()',body)
            self.assertIn('self.execute_application_command(',body)

    def test_plain_plan_services_are_not_public_api(self):
        for kind in ('task','habit'):
            self.assertIn(f'pub(crate) fn apply_{kind}_plan(',self.services)

    def test_duplicate_query_methods_removed(self):
        self.assertEqual(self.services.count('pub fn schedule_block(&self,'),1)
        self.assertEqual(self.services.count('pub fn routine(&self,'),1)

    def test_restart_assertion_for_accepted_plan(self):
        self.assertIn('durable accepted history',self.client)
        self.assertIn('before_apply + 1',self.client)

if __name__ == '__main__':
    unittest.main()
