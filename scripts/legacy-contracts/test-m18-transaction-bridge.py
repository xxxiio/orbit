"""Source guard for the M18 ambient-transaction / savepoint bridge.

These checks supplement, but never replace, cargo test.
"""
from pathlib import Path
import unittest
ROOT = Path(__file__).resolve().parents[2]
STORAGE = (ROOT / 'crates/orbit-storage/src/lib.rs').read_text()
HOST = (ROOT / 'crates/orbit-runtime/src/lib.rs').read_text()
SERVICES = (ROOT / 'crates/orbit-runtime/src/application_services.rs').read_text()

class TransactionBridge(unittest.TestCase):
    def test_application_unit_of_work_uses_savepoints(self):
        body = STORAGE.split('impl UnitOfWork for SqliteStore {', 1)[1].split('#[cfg(test)]\nmod authority_transaction_tests', 1)[0]
        self.assertIn('SAVEPOINT orbit_application_uow', body)
        self.assertIn('ROLLBACK TO SAVEPOINT orbit_application_uow', body)
        self.assertIn('RELEASE SAVEPOINT orbit_application_uow', body)
        self.assertNotIn('BEGIN IMMEDIATE TRANSACTION', body)
        self.assertNotIn('"COMMIT"', body)

    def test_host_controls_outer_commit_and_rollback(self):
        body = HOST.split('pub(crate) fn transact_application<T>(', 1)[1].split('    /// Replay durable empty-folder', 1)[0]
        for needle in ('BEGIN IMMEDIATE', 'COMMIT', 'ROLLBACK', 'operation(&mut self.application_services())'):
            self.assertIn(needle, body)

    def test_rich_application_command_uses_canonical_receipt_engine(self):
        body = HOST.split('pub(crate) fn execute_application_command<T>(', 1)[1].split('    /// Replay durable empty-folder', 1)[0]
        for needle in ('PendingCommand', 'pending.check(', 'pending.commit(', 'BEGIN IMMEDIATE',
                       'COMMIT', 'ROLLBACK', 'application_services()',
                       'CommandAdmission::Duplicate', 'CommandAdmission::New'):
            self.assertIn(needle, body)
        engine = (ROOT / 'crates/orbit-runtime/src/command_engine.rs').read_text()
        self.assertIn("tx: &Connection", engine)

    def test_rich_command_has_executable_behavioral_tests(self):
        for name in ('rich_command_receipt_and_activity_commit_with_canonical_history',
                     'rich_command_application_error_preserves_revision_and_releases_transaction',
                     'rich_command_rejects_cross_domain_id_collision_without_mutation'):
            self.assertIn(f'fn {name}(', SERVICES)

    def test_rich_domains_have_typed_accepted_commands(self):
        accepted = (ROOT / 'crates/orbit-runtime/src/accepted_application.rs').read_text()
        for method in ('create_project_accepted', 'create_task_accepted', 'create_habit_accepted', 'create_routine_accepted', 'transition_project_accepted', 'transition_task_accepted', 'start_task_accepted'):
            self.assertIn(f'pub fn {method}(', accepted)
        self.assertGreaterEqual(accepted.count('self.execute_application_command('), 8)
        cli = (ROOT / 'crates/orbit-cli/src/lib.rs').read_text()
        for method in ('create_project_accepted', 'create_task_accepted', 'create_habit_accepted', 'create_routine_accepted', 'transition_project_accepted', 'transition_task_accepted', 'start_task_accepted'):
            self.assertIn(f'host.{method}(', cli)
        for case in ('rich_project_task_habit_share_revision_history_and_activity_commit',
                     'rich_command_retries_return_original_receipts_without_duplicate_activities',
                     'failed_rich_task_does_not_advance_global_revision_or_activity_history',
                     'immutable_rich_payload_includes_activity_identity'):
            self.assertIn(f'fn {case}(', accepted)

    def test_all_cli_lifecycle_commands_use_accepted_envelopes(self):
        cli = (ROOT / 'crates/orbit-cli/src/lib.rs').read_text()
        for accepted in ('create_project', 'transition_project', 'create_task',
                         'start_task', 'transition_task', 'create_habit', 'create_routine'):
            self.assertIn(f'host.{accepted}_accepted(', cli)
        # Plain mutable application services remain available for local-only
        # workflows, but canonical CLI lifecycle operations must never bypass
        # the revision/receipt admission engine.
        for bypass in ('host.application_services().create_project(',
                       'host.application_services().transition_project(',
                       'host.application_services().create_task(',
                       'host.application_services().start_task(',
                       'host.application_services().transition_task(',
                       'host.application_services().create_habit(',
                       'host.application_services().create_routine('):
            self.assertNotIn(bypass, cli)

    def test_generic_semantic_admission_is_not_public_to_external_adapters(self):
        self.assertIn('pub(crate) fn execute_application_command<T>(', HOST)
        self.assertNotIn('pub fn execute_application_command<T>(', HOST)

    def test_storage_proves_nested_transaction_semantics(self):
        for case in ('standalone_application_transaction_commits', 'standalone_application_failure_rolls_back',
                     'rich_application_success_is_not_committed_before_authority_commit',
                     'failing_rich_application_rolls_back_only_its_savepoint',
                     'deeply_nested_application_transactions_are_isolated',
                     'outer_failure_rolls_back_inner_success'):
            self.assertIn(f'fn {case}(', STORAGE)

    def test_host_proves_cross_use_case_rollback(self):
        self.assertIn('fn authority_batch_commits_all_project_and_activity_records(', SERVICES)
        self.assertIn('fn authority_batch_rejects_partial_project_and_activity_commit(', SERVICES)

if __name__ == '__main__': unittest.main()
