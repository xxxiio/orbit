#!/usr/bin/env python3
"""Guard shared rich use cases against regression into client-owned SQL mutations."""
from pathlib import Path
root = Path(__file__).resolve().parents[2]
service = (root / 'crates/orbit-runtime/src/application_services.rs').read_text()
client = (root / 'crates/orbit-client/src/lib.rs').read_text()
host = (root / 'crates/orbit-runtime/src/lib.rs').read_text()
methods = ('create_project','transition_project','create_task','transition_task','set_task_planning',
           'configure_project_strategy','set_project_criterion_status','create_habit',
           'set_habit_active','record_habit_occurrence','create_schedule_block',
           'change_project_evidence','set_metric_target','record_metric_observation','apply_task_plan','apply_habit_plan')
assert 'pub(crate) fn application_services(' in host
for method in methods:
    assert (f'pub(crate) fn {method}(' in service if method in ('apply_task_plan','apply_habit_plan') else f'pub(crate) fn {method}(' in service), method
    assert f'self.runtime.{method}_accepted(' in client, method
assert 'rich_project_and_task_use_cases_share_host_store_and_activity_transaction' in service
print(f'Host-owned rich application service: {len(methods)} use cases wired (source guard)')
