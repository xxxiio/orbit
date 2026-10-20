#!/usr/bin/env python3
"""Source-contract assertions for one transactional command engine.

These checks enforce module ownership and test presence, not Rust execution.
The Rust tests in command_engine.rs must be run by the M18 acceptance script.
"""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
CORE = ROOT / 'crates/orbit-runtime/src'

class CommandEngineContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.engine = (CORE / 'command_engine.rs').read_text()
        cls.store = (CORE / 'store.rs').read_text()
        cls.evidence = (CORE / 'authority_evidence.rs').read_text()
        cls.lib = (CORE / 'lib.rs').read_text()

    def test_common_implementation_is_private_runtime_module(self):
        self.assertIn('mod command_engine;', self.lib)
        self.assertNotIn('pub mod command_engine;', self.lib)

    def test_all_three_domain_commands_use_same_admission_and_commit(self):
        self.assertEqual(self.store.count('pending.check(&tx)?'), 2)
        self.assertEqual(self.store.count('pending.commit(&tx,'), 2)
        self.assertEqual(self.evidence.count('pending.check(&tx)?'), 1)
        self.assertEqual(self.evidence.count('pending.commit(&tx,'), 1)

    def test_single_ledger_receipt_projection_writer_for_production_commands(self):
        for sql in [
            'INSERT INTO authority_history(device_id,sequence,kind,wire)',
            'INSERT INTO authority_commands(command_id,actor,payload,result_json)',
            'INSERT INTO authority_projection(id,authority_revision,view_json)',
        ]:
            self.assertIn(sql, self.engine)
        self.assertNotIn('INSERT INTO authority_commands', self.evidence)
        # Store still retains explicitly legacy/test-only sync paths; the
        # production authority handlers are bounded by their function ranges.
        block = self.store.split('pub fn authority_project_command',1)[1].split('pub fn authority_command_count',1)[0]
        self.assertNotIn('INSERT INTO authority_commands', block)
        block = self.store.split('pub fn authority_task_command',1)[1].split('pub fn event_count',1)[0]
        self.assertNotIn('INSERT INTO authority_commands', block)

    def test_receipt_duplicate_checked_before_revision(self):
        prior = self.engine.index('let previous: Option<(String, String, String)>')
        revision = self.engine.index('if tip != self.expected_revision')
        self.assertLess(prior, revision)

    def test_replay_and_receipt_within_same_transaction(self):
        self.assertIn('pub fn commit(', self.engine)
        self.assertIn('crate::canonical::project(&history)', self.engine)
        self.assertIn('tx: &Connection', self.engine)
        self.assertIn('unresolved_conflicts != 0', self.engine)

    def test_required_mixed_domain_rust_regressions_present(self):
        for name in [
            'mixed_domains_have_one_monotonic_revision_and_one_atomic_receipt_log',
            'cross_domain_command_id_collision_is_rejected_without_mutation',
            'retry_uses_original_receipt_regardless_of_current_revision',
            'stale_and_failed_domain_updates_do_not_advance_shared_history',
            'an_immutable_receipt_is_bound_to_its_original_actor_and_payload',
        ]:
            self.assertIn(f'fn {name}()', self.engine)

if __name__ == '__main__':
    unittest.main()
