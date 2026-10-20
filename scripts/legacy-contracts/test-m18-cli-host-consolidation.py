#!/usr/bin/env python3
"""Source-level regression: core CLI commands must use the shared host API."""
from pathlib import Path
root=Path(__file__).resolve().parents[2]
cli=(root/'crates/orbit-cli/src/lib.rs').read_text()
services=(root/'crates/orbit-runtime/src/application_services.rs').read_text()
for item in ('run_task(&mut host, task.command)', 'run_project_host(&mut host, project.command)',
             'run_habit_host(&mut host, habit.command)',
             'host.configure_project_strategy_accepted(',
             'host.set_project_criterion_status_accepted(',
             'host.set_metric_target_accepted(',
             'host.record_metric_observation_accepted(',
             'host.record_habit_occurrence_accepted(',
             'host.set_habit_active_accepted(',
             'host.application_queries().habit_occurrences(',
             'host.application_queries().habits()'):
    assert item in cli, f'missing shared service wiring: {item}'
assert 'Command::Habit(command) => run_habit(store, command.command)?' not in cli
assert 'pub fn habit_occurrences(' in services
print('CLI multi-domain host consolidation guard: PASS')
