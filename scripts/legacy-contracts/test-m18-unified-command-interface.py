#!/usr/bin/env python3
"""Static guard: all file-backed commands use the same embedded/remote API.

Not a replacement for Rust compilation or executable HTTP/SQLite tests.
"""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
CLIENT = (ROOT / 'crates/orbit-client/src/unified.rs').read_text()
QUEUE = (ROOT / 'crates/orbit-client/src/authority.rs').read_text()

class UnifiedCommandInterface(unittest.TestCase):
    def test_single_public_file_write_semantics(self):
        for name, queue in [('markdown_command','queue_markdown'), ('folder_create','queue_folder_create'), ('folder_move','queue_folder_move')]:
            start = CLIENT.index(f'    pub fn {name}(')
            end = CLIENT.index('\n    pub fn ', start + 12)
            body = CLIENT[start:end]
            self.assertIn('CommandDisposition', body)
            self.assertIn('CommandDisposition::Pending', body)
            self.assertIn(f'replica.{queue}(', body)
            self.assertNotIn('transport.post(', body)
        self.assertIn('fn remote_file_commands_use_one_ordered_outbox_and_never_commit_offline', CLIENT)

    def test_remote_receipts_are_checked_before_acceptance(self):
        self.assertIn('accepted_revision != expected_revision + 1', QUEUE)
        self.assertIn('invalid authoritative command receipt', QUEUE)
        self.assertIn('global queue order changed', QUEUE)
        self.assertIn('command conflict requires explicit reconciliation', QUEUE)

if __name__ == '__main__':
    unittest.main()
