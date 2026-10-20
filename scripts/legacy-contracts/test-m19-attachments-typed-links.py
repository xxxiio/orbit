#!/usr/bin/env python3
"""Source boundary tests for the M19 portable asset and typed-reference slice.

These checks do not stand in for Rust compilation (scheduled for M15).
"""
from pathlib import Path
import unittest
ROOT = Path(__file__).resolve().parents[2]
ASSETS=(ROOT/'crates/orbit-cli/src/markdown_assets.rs').read_text()
NOTES=(ROOT/'crates/orbit-cli/src/markdown_cli.rs').read_text()
META=(ROOT/'crates/orbit-cli/src/markdown_metadata.rs').read_text()
CLI=(ROOT/'crates/orbit-cli/src/lib.rs').read_text()
class M19AttachmentContract(unittest.TestCase):
    def test_note_has_attachment_commands(self):
        for action in ('AssetCommand::Add','AssetCommand::List','AssetCommand::Verify','AssetCommand::Unlink'):
            self.assertIn(action,ASSETS)
    def test_hash_and_bytes_verification(self):
        self.assertIn('Sha256::digest(&bytes)',ASSETS)
        self.assertIn('MAX_ASSET_BYTES',ASSETS)
        self.assertIn('fs::symlink_metadata',ASSETS)
        self.assertIn('output.sync_all()',ASSETS)
    def test_assets_do_not_add_sql_content(self):
        self.assertNotIn('SqliteStore',ASSETS)
        self.assertIn('MarkdownCommand::Update',ASSETS)
    def test_import_export_include_assets(self):
        self.assertIn('preflight_import_assets(source)',NOTES)
        self.assertIn('preflight_import_references(db,&import_bodies,&incoming_assets)',NOTES)
        self.assertIn('publish_import_assets(db,incoming_assets)',NOTES)
        self.assertIn('export_asset_directory(host,db,&staged)',NOTES)
        self.assertIn('preflight_export_references', NOTES)
    def test_typed_note_references_are_file_authoritative(self):
        self.assertIn('NoteMetadataOperation::AddTypedReference',META)
        self.assertIn('NoteMetadataOperation::RemoveTypedReference',META)
        self.assertIn('orbit://',META)
        self.assertIn('"project"|"task"|"habit"',META)
    def test_host_wiring_keeps_workspace_path(self):
        self.assertIn('run_note_host(&mut host,&db_path,note.command)',CLI)
    def test_rust_scenarios_include_export_and_verify(self):
        self.assertIn('typed_references_and_immutable_attachments_survive_reopen',NOTES)
        self.assertIn('imported_attachments_verify_hashes',NOTES)
if __name__ == '__main__':
    unittest.main()
