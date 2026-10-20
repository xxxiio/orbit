#!/usr/bin/env python3
"""Prevent retired SQL knowledge interfaces rejoining canonical application transactions."""
from pathlib import Path
root = Path(__file__).resolve().parents[2]
ports=(root/'crates/orbit-application/src/ports.rs').read_text()
assert 'pub trait UnitOfWork:' in ports
for retired in ('KnowledgeRepository', 'KnowledgeResourceRepository',
                'KnowledgeResourceLocatorRepository', 'LegacyKnowledgeUnitOfWork'):
    assert retired not in ports, f'retired application port survives: {retired}'
for module in ('knowledge_commands', 'knowledge_queries', 'knowledge_resource_commands',
               'knowledge_resource_queries', 'knowledge_retrieval', 'semantic_retrieval'):
    assert not (root/'crates/orbit-application/src'/f'{module}.rs').exists(), module
    assert f'mod {module};' not in (root/'crates/orbit-application/src/lib.rs').read_text(), module
store=(root/'crates/orbit-storage/src/lib.rs').read_text()
for old in ('fn load_knowledge_relations(', 'fn knowledge_item_base_from_row(', 'fn knowledge_resource_from_row(',
            'fn knowledge_fts_migration_indexes_inserts_updates_and_deletes('):
    assert old not in store, f'obsolete SQL body path: {old}'
for required in ('fn existing_legacy_sql_knowledge_is_rejected_not_discarded(', 'fn fresh_database_creates_no_sql_knowledge_body_or_asset_tables('):
    assert required in store
assert 'impl KnowledgeRepository for SqliteStore' not in (root/'crates/orbit-storage/src/repositories/entities.rs').read_text()
assert 'impl KnowledgeResourceRepository for SqliteStore' not in (root/'crates/orbit-storage/src/repositories/resources.rs').read_text()
print('M19 application transaction boundary: passed')
