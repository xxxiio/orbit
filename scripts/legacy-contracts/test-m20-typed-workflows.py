#!/usr/bin/env python3
"""Static regression guard; does not substitute for Cargo testing."""
from pathlib import Path
root = Path(__file__).resolve().parents[2]
code = (root/'crates/orbit-client/src/unified.rs').read_text()
for symbol in ['pub enum DomainWorkflowStep', 'pub struct WorkflowFailure',
               'pub fn execute_workflow(', 'failed_index: usize',
               'preflight_duplicate_ids_has_no_side_effects',
               'embedded_workflow_commits_in_order',
               'remote_workflow_queues_without_projecting_pending_writes',
               'failure_reports_accepted_prefix_without_rollback']:
    assert symbol in code, symbol
assert code.index('let mut ids = std::collections::HashSet::new();') < code.index('let mut completed = Vec::with_capacity(steps.len());')
assert 'DomainWorkflowStep::Evidence(cmd) => self.evidence_command(cmd)' in code
print('M20 typed workflow source guard: PASS (Rust not executed)')
