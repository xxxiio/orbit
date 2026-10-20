#!/usr/bin/env python3
"""Source contract for the shared accepted read projections (not executable Rust acceptance)."""
from pathlib import Path
root = Path(__file__).resolve().parents[2]
unified = (root/'crates/orbit-client/src/unified.rs').read_text()
connected = (root/'crates/orbit-client/src/connected.rs').read_text()
embedded = (root/'crates/orbit-client/src/embedded_authority.rs').read_text()
for method in ('accepted_snapshot', 'projects', 'tasks', 'habits', 'habit_occurrences',
               'metrics', 'metric_observations', 'project', 'task', 'habit', 'habit_occurrence',
               'metric', 'metric_observations_for', 'history_prefix'):
    assert f'pub fn {method}(' in unified, method
assert '"history_fingerprint": workspace.history_fingerprint' in unified
assert 'if workspace.revision.is_none() { return Ok(None); }' in unified
assert 'pub history_fingerprint: Option<String>' in connected
assert 'store::AuthorityError' not in embedded
assert 'queued_writes_are_never_exposed_as_accepted_reads' in unified
assert 'accepted_read_contract_is_the_same_for_embedded_and_refreshed_remote' in unified
print('M20 unified accepted read source contract: PASS')
