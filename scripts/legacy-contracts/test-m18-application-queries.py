#!/usr/bin/env python3
"""Ensure embedded and MCP core reads use host-owned application queries."""
from pathlib import Path
root=Path(__file__).resolve().parents[2]
runtime=(root/'crates/orbit-runtime/src/lib.rs').read_text()
services=(root/'crates/orbit-runtime/src/application_services.rs').read_text()
client=(root/'crates/orbit-client/src/lib.rs').read_text()
mcp=(root/'crates/orbit-mcp/src/main.rs').read_text()
assert 'pub fn application_queries(&self)' in runtime
for op in ('projects','tasks','habits','routines','project_milestones'):
    assert f'pub fn {op}(&self' in services,op
for op in ('projects','tasks','habits','routines'):
    assert f'self.runtime.application_queries().{op}()' in client,op
for op in ('projects','tasks','habits','project_milestones'):
    assert f'self.authority.application_queries().{op}(' in mcp,op
for text in (client,mcp):
    for collection in ('projects','tasks','habits'):
        assert f'list_{collection}(self.' not in text,collection
print('M18 host-owned core read contract: PASS (source-only)')
