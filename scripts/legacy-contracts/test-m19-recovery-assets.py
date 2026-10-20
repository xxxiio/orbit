#!/usr/bin/env python3
"""Regression guard for file-journal and immutable asset publication (source only)."""
from pathlib import Path
base=Path(__file__).resolve().parents[2]
ops=(base/'crates/orbit-runtime/src/knowledge_operations.rs').read_text()
assets=(base/'crates/orbit-cli/src/markdown_assets.rs').read_text()
assert 'fn preflight(&self)' in ops
assert 'let entries = self.preflight()?;' in ops
assert 'if !paths.insert(target.clone())' in ops
assert 'fs::hard_link(&stage,&dest)' in assets
assert 'f.write_all(bytes).and_then(|_|f.sync_all())' in assets
assert 'if !name.starts_with(&digest)' in assets
assert 'preflight_import_assets' in assets
assert 'preflight_import_references' in assets
assert 'for (name,bytes) in verified { publish_asset' in assets
print('M19 recovery and immutable asset source checks: passed')
