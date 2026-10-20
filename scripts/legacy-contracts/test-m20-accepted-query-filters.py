#!/usr/bin/env python3
"""Source-level guards only; Rust behavior must still be executed in CI."""
from pathlib import Path
root=Path(__file__).resolve().parents[2]
src=(root/'crates/orbit-client/src/unified.rs').read_text()
for method in ('tasks_for_project', 'habit_occurrences_for', 'tasks_with_status'):
    assert f'pub fn {method}(' in src, method
assert 'task_filters_have_embedded_remote_parity_and_exclude_pending' in src
assert 'self.tasks()?.into_iter()' in src
assert 'self.habit_occurrences()?.into_iter()' in src
assert 'empty project identity' in src
assert 'empty habit identity' in src
assert 'empty task status' in src
print('M20 accepted query filter source guard: PASS (Rust behavioral test unexecuted)')
