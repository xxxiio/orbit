#!/usr/bin/env python3
"""Static contract checks for the M19 native, file-authoritative CLI seam.

Executable Rust tests are in crates/orbit-cli/src/markdown_cli.rs and are NOT
substituted by these checks; CI/M15 must compile and run them.
"""
from pathlib import Path
import unittest
ROOT = Path(__file__).resolve().parents[2]
CLI = (ROOT/'crates/orbit-cli/src/lib.rs').read_text()
ARGS = (ROOT/'crates/orbit-cli/src/commands.rs').read_text()
NATIVE = (ROOT/'crates/orbit-cli/src/markdown_cli.rs').read_text()
RUNTIME = (ROOT/'crates/orbit-runtime/src/lib.rs').read_text()
class NativeKnowledgeContract(unittest.TestCase):
    def test_separate_note_and_folder_cli(self):
        self.assertIn('Note(NoteCommand)',ARGS)
        self.assertIn('Folder(FolderCommand)',ARGS)
        self.assertIn('run_note_host(&mut host,&db_path,note.command)',CLI)
        self.assertIn('run_folder_host(&mut host,folder.command)',CLI)
    def test_writes_are_accepted(self):
        self.assertIn('execute_markdown_command',NATIVE)
        self.assertIn('execute_folder_create',NATIVE)
        self.assertIn('execute_folder_move',NATIVE)
        self.assertNotIn('SqliteStore',NATIVE)
        self.assertNotIn('with_legacy_knowledge_cli',NATIVE)
    def test_reads_are_file_authoritative(self):
        for method in ['markdown_by_id','markdown_knowledge_index','markdown_search','markdown_relations','markdown_document','knowledge_tree']:
            with self.subTest(method=method): self.assertIn(method,NATIVE)
    def test_contracts_expose_conflict_retry_inputs(self):
        self.assertIn('expected_file: Option<PathBuf>',NATIVE)
        self.assertIn('command_id: Option<String>',NATIVE)
        self.assertIn('ImmutableConflict', (ROOT/'crates/orbit-runtime/src/markdown_commands.rs').read_text())
    def test_directory_import_export_is_physical_and_replayable(self):
        self.assertIn('ImportDir { source: PathBuf',NATIVE)
        self.assertIn('ExportDir { destination: PathBuf',NATIVE)
        self.assertIn('existing_id(&bytes)?',NATIVE)
        self.assertIn('import-folder-',NATIVE)
        self.assertIn('fs::OpenOptions::new().write(true).create_new(true)',NATIVE)
    def test_rust_integration_scenarios_present(self):
        for scenario in ['notes_and_folders_survive_process_reopens','updates_use_explicit_expected_bytes','note_search_and_relations','import_export_round_trip','import_rejects_duplicate_ids']:
            with self.subTest(scenario=scenario): self.assertIn(scenario,NATIVE)
    def test_legacy_sql_bridge_retired(self):
        self.assertNotIn('with_legacy_knowledge_cli',RUNTIME)
        self.assertNotIn('Command::Knowledge(',CLI)
        self.assertNotIn('fn run_knowledge(',CLI)
        self.assertNotIn('Knowledge(KnowledgeCommand)',ARGS)
if __name__=='__main__': unittest.main()
