#!/usr/bin/env python3
"""Reject adapter-level SQL reads and the duplicate CLI project implementation.

This is a source contract, not proof of Rust compilation or functional parity.
"""
from pathlib import Path

root = Path(__file__).resolve().parents[2]
cli = (root / 'crates/orbit-cli/src/lib.rs').read_text()
client = (root / 'crates/orbit-client/src/lib.rs').read_text()
mcp = (root / 'crates/orbit-mcp/src/main.rs').read_text()
service = (root / 'crates/orbit-runtime/src/application_services.rs').read_text()
assert 'fn run_project(store: &mut SqliteStore' not in cli, 'obsolete duplicate project handler survived'
assert 'other => host.application_services().with_legacy_cli(|store| run_project' not in cli
assert 'host.application_queries().project_evidence_report' in cli
assert 'host.application_queries().mode_aware_project_health' in cli
assert 'today_host(&host)?' in cli
assert 'orbit_application::get_task(self.authority.application_store()' not in mcp
assert 'self.authority.application_store()' not in mcp
assert 'self.runtime.application_store()' not in client.split('#[cfg(test)]')[0], 'embedded production client bypasses host query facade'
for name in ('project', 'task', 'milestone', 'project_evidence',
             'project_evidence_report', 'mode_aware_project_health',
             'project_health', 'activities', 'schedule_between'):
    assert f'pub fn {name}(' in service, name
print('M18 cross-adapter read and project command boundary: PASS (source only)')
