#!/usr/bin/env python3
"""Fail when a public authoritative surface reintroduces SQL-body knowledge endpoints."""
from pathlib import Path
root=Path(__file__).resolve().parents[2]
server=(root/'crates/orbit-server/src/main.rs').read_text()
client=(root/'crates/orbit-client/src/unified.rs').read_text()
embedded=(root/'crates/orbit-client/src/embedded_authority.rs').read_text()
for old in ['/v1/authoritative/knowledge-commands','/v1/authoritative/knowledge-updates','/v1/authoritative/knowledge-search','/v1/authoritative/knowledge"']:
    assert old not in server,old
for old in ['pub fn knowledge_command(', 'pub fn knowledge_update(', 'pub fn knowledge_search(', 'pub fn knowledge(&self)']:
    assert old not in client,old
    assert old not in embedded,old
for new in ['markdown-by-id','markdown-search','markdown-relations','markdown-commands','knowledge-tree']:
    assert f'/v1/authoritative/{new}' in server,new
for new in ['pub fn markdown_command(', 'pub fn markdown_by_id(', 'pub fn markdown_search(', 'pub fn markdown_relations(']:
    assert new in client,new
print('Public canonical Markdown path: source contract PASS')
