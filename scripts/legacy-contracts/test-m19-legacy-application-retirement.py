"""Source boundary check for removed SQL knowledge application contracts.

Full cargo integration remains in M15; this guard only asserts the APIs are gone.
"""
from pathlib import Path
root = Path(__file__).resolve().parents[2] / 'crates'
application = root / 'orbit-application' / 'src'
for module in ('knowledge_commands', 'knowledge_queries', 'knowledge_resource_commands',
               'knowledge_resource_queries', 'knowledge_retrieval', 'semantic_retrieval'):
    assert not (application / f'{module}.rs').exists(), module
    assert f'mod {module};' not in (application / 'lib.rs').read_text(), module
for forbidden in ('KnowledgeRepository', 'KnowledgeResourceRepository',
                  'LegacyKnowledgeUnitOfWork', 'KnowledgeResourceLocatorRepository'):
    assert forbidden not in (application / 'ports.rs').read_text(), forbidden
    assert forbidden not in (application / 'lib.rs').read_text(), forbidden
for forbidden in ('KnowledgeItemView', 'KnowledgePortableDocument',
                  'KnowledgeSearchHitView', 'KnowledgeBacklinkView'):
    assert forbidden not in (root / 'orbit-cli' / 'src/lib.rs').read_text(), forbidden
client = (root / 'orbit-client/src/lib.rs').read_text()
assert 'markdown_knowledge_index()' in client
assert 'execute_markdown_command' in client
assert 'pub fn create_knowledge' in client
assert 'KnowledgeRepository' not in client
sync = (application / 'sync_outbox.rs').read_text()
assert 'KnowledgeItem,' not in sync and 'KnowledgeResource,' not in sync
print('M19 retired knowledge API boundary checks: passed')
