#!/usr/bin/env python3
"""Guard vault-derived discovery surfaces against SQL-backed regressions."""
from pathlib import Path
root = Path(__file__).resolve().parents[2]
cli = (root/'crates/orbit-cli/src/markdown_cli.rs').read_text()
index = (root/'crates/orbit-cli/src/markdown_discovery.rs').read_text()
assets = (root/'crates/orbit-cli/src/markdown_assets.rs').read_text()
for name in ['ReferenceBacklinks','AssetBacklinks','AssetAudit']:
    assert name in cli, name
for fragment in ['markdown_knowledge_index()', 'markdown_by_id(', 'linked_assets(', 'validate_asset_file(', 'orphaned', 'missing', 'corrupt']:
    assert fragment in index, fragment
for forbidden in ['SqliteStore','knowledge_resources','knowledge_items','DELETE FROM', 'remove_file(', 'create_dir(']:
    assert forbidden not in index, forbidden
for fragment in ['pub(super) fn linked_assets', 'pub(super) fn is_content_name']:
    assert fragment in assets, fragment
print('M19 vault-derived discovery boundary: passed')
