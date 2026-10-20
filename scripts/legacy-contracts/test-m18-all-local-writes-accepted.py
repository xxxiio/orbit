#!/usr/bin/env python3
"""Host boundary acceptance for local rich mutations (source evidence only)."""
from pathlib import Path
import unittest
ROOT=Path(__file__).resolve().parents[2]
CLIENT=(ROOT/'crates/orbit-client/src/lib.rs').read_text()
MCP=(ROOT/'crates/orbit-mcp/src/main.rs').read_text()
CLI=(ROOT/'crates/orbit-cli/src/lib.rs').read_text()
RUNTIME=(ROOT/'crates/orbit-runtime/src/accepted_application.rs').read_text()
MUTATIONS=(
 'create_project','transition_project','create_task','transition_task',
 'set_task_planning','configure_project_strategy','set_project_criterion_status',
 'create_habit','set_habit_active','record_habit_occurrence',
 'create_schedule_block','set_metric_target','record_metric_observation',
 'change_project_evidence',
)
MCP_OPERATIONS=(
 'create_task','transition_task','transition_project',
 'configure_project_strategy','set_project_criterion_status',
 'set_habit_active','transition_milestone','change_project_evidence',
 'set_metric_target','record_metric_observation',
)
class AcceptedBoundary(unittest.TestCase):
 def test_every_embedded_mutation_routes_through_command_admission(self):
  for method in MUTATIONS:
   with self.subTest(operation=method):
    self.assertIn(f'self.runtime.{method}_accepted(', CLIENT)
    self.assertNotIn(f'self.runtime.application_services().{method}(', CLIENT)
 def test_every_mcp_mutation_uses_canonical_receipts(self):
  for method in MCP_OPERATIONS:
   with self.subTest(operation=method):
    self.assertIn(f'self.authority.{method}_accepted(', MCP)
    self.assertNotIn(f'self.authority.application_services().{method}(', MCP)
 def test_plan_methods_use_canonical_admission(self):
  for kind in ("task", "habit"):
   self.assertIn(f"self.runtime.apply_{kind}_plan_accepted(", CLIENT)
   self.assertNotIn(f"self.runtime.application_services().apply_{kind}_plan(", CLIENT)
   self.assertIn(f"pub fn apply_{kind}_plan_accepted(", RUNTIME)
 def test_mcp_command_identity_is_stable_for_retries(self):
  for marker in ('fn mcp_command_id(', 'fn mcp_entity_id(', 'fn mcp_activity_id(',
                 'mcp_retry_does_not_repeat_task_or_activity_and_rejects_payload_change'):
   self.assertIn(marker, MCP)
 def test_executable_client_acceptance_is_defined(self):
  self.assertIn('fn embedded_public_client_writes_join_one_accepted_revision_history()',CLIENT)
 def test_runtime_implements_every_consumed_accepted_path(self):
  for method in set(MUTATIONS)|set(MCP_OPERATIONS):
   with self.subTest(operation=method):
    self.assertIn(f'pub fn {method}_accepted(', RUNTIME)
if __name__ == '__main__': unittest.main()
