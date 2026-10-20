#!/usr/bin/env python3
"""Source-level regression guard. Full Rust execution is tracked in M15."""
from pathlib import Path
root = Path(__file__).resolve().parents[2]
storage = (root/'crates/orbit-storage/src/lib.rs').read_text()
entities = (root/'crates/orbit-storage/src/repositories/entities.rs').read_text()
resources = (root/'crates/orbit-storage/src/repositories/resources.rs').read_text()
runtime = (root/'crates/orbit-runtime/src/lib.rs').read_text()
tui = (root/'crates/orbit-tui/src/main.rs').read_text()
assert 'const SCHEMA_VERSION: i64 = 13;' in storage
assert 'CREATE TABLE IF NOT EXISTS knowledge_items' not in storage
assert 'CREATE TABLE IF NOT EXISTS knowledge_resources' not in storage
assert 'CREATE VIRTUAL TABLE knowledge_fts' not in storage
assert 'retired SQL knowledge schema detected' in storage
assert 'impl KnowledgeRepository for SqliteStore' not in entities
assert 'impl KnowledgeResourceRepository for SqliteStore' not in resources
assert 'impl KnowledgeResourceLocatorRepository for SqliteStore' not in resources
assert 'pub fn markdown_term_locations' in runtime
assert 'store.markdown_term_locations(&v, 25)' in tui
print('M19 SQL knowledge retirement source boundary: PASS')
