#!/usr/bin/env python3
"""M18 API-boundary contract; full executable acceptance is deferred to M15."""
from pathlib import Path
root = Path(__file__).resolve().parents[2]
host = (root/'crates/orbit-runtime/src/lib.rs').read_text()
services = (root/'crates/orbit-runtime/src/application_services.rs').read_text()
cli = (root/'crates/orbit-cli/src/lib.rs').read_text()
tui = (root/'crates/orbit-tui/src/main.rs').read_text()
client = (root/'crates/orbit-client/src/lib.rs').read_text()
assert 'pub(crate) fn application_services(&mut self)' in host
assert 'pub(crate) fn transact_application<T>' in host
assert 'pub fn application_store(' not in host
assert 'with_legacy_knowledge_cli' not in host
assert 'with_legacy_knowledge_cli' not in cli
assert 'fn run_knowledge(' not in cli
assert 'store.markdown_term_locations(&v, 25)' in tui
assert 'legacy_knowledge_rows_empty()' in client
for name in ('create_project','transition_project','create_task','transition_task',
             'create_habit','set_habit_active','record_habit_occurrence',
             'create_routine','set_routine_habit_membership','create_schedule_block',
             'reschedule_block','configure_project_strategy','set_task_planning',
             'change_project_evidence','set_metric_target','record_metric_observation'):
    assert f'pub(crate) fn {name}(' in services, name
    assert f'    pub fn {name}(' not in services, name
print('M18 public mutation boundary: PASS; M19 file-backed term navigation')
