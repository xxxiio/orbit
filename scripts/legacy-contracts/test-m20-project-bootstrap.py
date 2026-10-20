#!/usr/bin/env python3
"""Static contract smoke: compiled Rust tests are the behavioral authority."""
from pathlib import Path
s=(Path(__file__).resolve().parents[2]/'crates/orbit-client/src/unified.rs').read_text()
assert 'pub struct ProjectBootstrap' in s
assert 'pub fn bootstrap_project(' in s
assert 'self.execute_workflow(steps)' in s
assert 'self.base_revision + 1 + index as i64' in s
assert 'project_id: Some(self.project_id.clone())' in s
assert 'invalid or duplicated bootstrap task' in s
assert 'project_launch_commits_complete_project_in_revision_order' in s
assert 'remote_launch_is_ordered_but_not_accepted' in s
print('M20 bootstrap source-contract checks passed (not Rust runtime acceptance)')
