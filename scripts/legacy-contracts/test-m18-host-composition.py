#!/usr/bin/env python3
"""Source-level guard: native adapters must not independently open local stores.

This is only a topology check, NOT a Rust build or a replacement for tests.
"""
from pathlib import Path
root=Path(__file__).resolve().parents[2]
def file(relative):return (root/relative).read_text()
runtime=file('crates/orbit-runtime/src/lib.rs')
assert 'pub struct OrbitRuntime' not in runtime
assert 'application: SqliteStore' not in runtime
assert 'pub fn application_store(&self)' not in runtime
assert 'authority: store::AuthorityStore' in runtime
assert 'projection_bridge::' not in runtime
assert 'pub fn application_store_mut(&mut self)' not in runtime
assert 'pub(crate) fn application_services(&mut self)' in runtime
for name,location in {
    'GUI/FFI': 'crates/orbit-client/src/lib.rs',
    'CLI': 'crates/orbit-cli/src/lib.rs',
    'TUI': 'crates/orbit-tui/src/main.rs',
    'local MCP': 'crates/orbit-mcp/src/main.rs',
}.items():
    content=file(location)
    assert 'SqliteStore::open(' not in content, name
    assert 'OrbitRuntime::open(' not in content, name
    assert 'AuthoritativeHost::open(' in content or 'orbit_runtime::AuthoritativeHost::open(' in content, name
print('Host-owned native client composition: SOURCE GUARD PASS (Rust acceptance pending)')

# All supported MCP mutation paths must use the host-owned application service.
mcp=file('crates/orbit-mcp/src/main.rs')
assert 'application_store_mut(' not in mcp
for operation in ('change_project_evidence', 'set_metric_target',
                  'record_metric_observation', 'create_task', 'transition_task',
                  'transition_project', 'configure_project_strategy',
                  'set_project_criterion_status', 'transition_milestone',
                  'set_habit_active'):
    assert operation + '_accepted(' in mcp, operation
assert 'application_store_mut(' not in file('crates/orbit-cli/src/lib.rs')
