#!/usr/bin/env python3
"""Prevent resurrection of the removed canonical SQL-authoritative note store."""
from pathlib import Path
r=Path(__file__).resolve().parents[2]/'crates/orbit-runtime/src'
store=(r/'store.rs').read_text()
lib=(r/'lib.rs').read_text()
contracts=(r/'domain_commands.rs').read_text()
assert 'CREATE TABLE IF NOT EXISTS authority_knowledge_items' not in store
for symbol in ('pub fn authority_knowledge_command(', 'pub fn authority_knowledge_update(',
               'pub fn authority_knowledge(', 'pub fn authority_knowledge_search('):
    assert symbol not in store, symbol
for symbol in ('pub fn knowledge_command(', 'pub fn knowledge_update(',
               'pub fn knowledge_search(', 'pub fn knowledge(&self)'):
    assert symbol not in lib, symbol
assert 'pub struct KnowledgeRecord' not in contracts
assert 'retired_table' in store and 'MigrationConflict' in store
assert 'pub fn execute_markdown_command(' in lib
print('Retired canonical SQL-body store and route guard: PASS')
