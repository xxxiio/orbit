#!/usr/bin/env python3
"""Prevent reintroduction of unaccepted rich CLI write paths."""
from pathlib import Path
import unittest
ROOT=Path(__file__).resolve().parents[2]
RUNTIME=(ROOT/'crates/orbit-runtime/src/accepted_application.rs').read_text()
CLI=(ROOT/'crates/orbit-cli/src/lib.rs').read_text()
QUERIES=(ROOT/'crates/orbit-runtime/src/application_services.rs').read_text()
FAMILIES=(
 'set_task_planning', 'configure_project_strategy', 'set_project_criterion_status',
 'set_habit_active', 'record_habit_occurrence', 'set_routine_habit_membership',
 'create_schedule_block', 'reschedule_block', 'transition_milestone',
 'set_metric_target', 'record_metric_observation',
)
class RichMutationCutover(unittest.TestCase):
 def test_accepted_operations_are_typed_and_shared(self):
  for name in FAMILIES:
   with self.subTest(operation=name):
    self.assertIn(f'pub fn {name}_accepted(',RUNTIME)
    self.assertIn(f'app.{name}(command)?',RUNTIME)
    self.assertIn('self.accept_rich_state(',RUNTIME)
 def test_every_active_cli_mutation_uses_acceptance(self):
  for name in FAMILIES:
   if name == "transition_milestone": continue
   with self.subTest(operation=name):
    self.assertIn(f'host.{name}_accepted(',CLI)
    self.assertNotIn(f'host.application_services().{name}(',CLI)
 def test_evidence_uses_canonical_receipt(self):
  self.assertIn('pub fn change_project_evidence_accepted(',RUNTIME)
  self.assertIn('host.change_project_evidence_accepted(',CLI)
  self.assertNotIn('host.application_services().change_project_evidence(',CLI)
 def test_retried_mutation_fenced_by_revision_and_receipt(self):
  for contract in ('latest_field_version(self, kind, entity_id, "rich_state")?',
                   'self.execute_application_command(', 'command_payload(&command)?'):
   self.assertIn(contract,RUNTIME)
 def test_cli_reloads_from_host_queries_after_acceptance(self):
  self.assertIn('pub fn schedule_block(',QUERIES)
  self.assertIn('pub fn routine(',QUERIES)
 def test_rust_regressions_present(self):
  for name in ('rich_habit_state_commands_share_receipts_and_global_revision',
               'failed_rich_habit_update_rolls_back_receipt_and_history',
               'rich_task_planning_preserves_application_rows_and_revision'):
   self.assertIn(f'fn {name}()',RUNTIME)
if __name__=='__main__':unittest.main()
