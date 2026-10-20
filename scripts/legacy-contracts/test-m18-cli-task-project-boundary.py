#!/usr/bin/env python3
from pathlib import Path
root=Path(__file__).resolve().parents[2]
cli=(root/'crates/orbit-cli/src/lib.rs').read_text()
services=(root/'crates/orbit-runtime/src/application_services.rs').read_text()
assert 'run_task(&mut host, task.command)?' in cli
assert 'run_project_host(&mut host, project.command)?' in cli
for fragment in ['host.create_task_accepted(', 'host.start_task_accepted(', 'host.transition_task_accepted(', 'host.set_task_planning_accepted(', 'host.application_queries().tasks()', 'host.create_project_accepted(', 'host.transition_project_accepted(', 'host.application_queries().projects()']:
    assert fragment in cli, fragment
assert 'pub(crate) fn start_task(' in services
assert 'run_task(store, command.command)' not in cli
print('CLI project/task host-owned workflow source guard: PASS')
