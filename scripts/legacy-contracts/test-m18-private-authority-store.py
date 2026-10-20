#!/usr/bin/env python3
"""Check that transports cannot open an independent mutable authority store.

This is a static API-surface regression check, not executable Rust acceptance.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2] / 'crates'

class PrivateAuthorityStore(unittest.TestCase):
    def test_runtime_store_module_private(self):
        source = (ROOT/'orbit-runtime/src/lib.rs').read_text()
        self.assertRegex(source, r'(?m)^mod store;')
        self.assertNotRegex(source, r'(?m)^pub mod store;')
        self.assertIn('pub use store::{AuthorityError, AuthorityEvent};', source)

    def test_server_backup_is_host_operation(self):
        source = (ROOT/'orbit-server/src/main.rs').read_text()
        self.assertIn('AuthoritativeHost::validate_backup(', source)
        self.assertNotIn('AuthorityStore::', source)
        self.assertNotIn('pub use orbit_runtime::store::*', source)

    def test_external_crates_cannot_access_store_type(self):
        for package in ROOT.iterdir():
            if not package.is_dir() or package.name == 'orbit-runtime':
                continue
            for path in (package/'src').rglob('*.rs'):
                text = path.read_text()
                with self.subTest(path=str(path)):
                    self.assertNotIn('orbit_runtime::store::', text)
                    self.assertNotIn('AuthorityStore::open(', text)

    def test_knowledge_storage_helpers_are_crate_private(self):
        for name in ('markdown_commands.rs', 'folder_commands.rs'):
            text = (ROOT/'orbit-runtime/src'/name).read_text()
            for method in ('schema', 'pending', 'enqueue', 'accept'):
                if re.search(rf'\bfn {method}\(', text):
                    with self.subTest(file=name, method=method):
                        self.assertNotRegex(text, rf'(?m)^pub fn {method}\(')

    def test_legacy_knowledge_escape_removed(self):
        text = (ROOT/'orbit-runtime/src/lib.rs').read_text()
        self.assertNotIn('with_legacy_knowledge_cli', text)
        self.assertNotIn('pub fn application_store_mut(', text)

if __name__ == '__main__':
    unittest.main()
