#!/usr/bin/env python3
"""Source contract guard for accepted task rename/reassignment; not a Rust test."""
from pathlib import Path
root = Path(__file__).resolve().parents[2]
contracts = (root / 'crates/orbit-runtime/src/domain_commands.rs').read_text()
store = (root / 'crates/orbit-runtime/src/store.rs').read_text()
for action in ('Rename { task_id: String, title: String }',
               'AssignProject { task_id: String, project_id: Option<String> }'):
    assert action in contracts, action
for clause in ('TaskAction::Rename { task_id, title }',
               'TaskAction::AssignProject { task_id, project_id }',
               'latest_task_field_version(&tx, task_id, "title")',
               'latest_task_field_version(&tx, task_id, "project_id")',
               'task_rename_and_project_assignment_are_authoritative_and_retry_safe'):
    assert clause in store, clause
assert 'project does not exist' in store
print('M20 task-planning mutation source guard: PASS (Rust not compiled)')
