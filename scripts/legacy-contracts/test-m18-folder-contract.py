#!/usr/bin/env python3
"""Dependency-free structural guard; does not replace Rust execution tests."""
from pathlib import Path
root=Path(__file__).resolve().parents[2]
files={
 'runtime': (root/'crates/orbit-runtime/src/lib.rs').read_text(),
 'folder':(root/'crates/orbit-runtime/src/folder_commands.rs').read_text(),
 'server':(root/'crates/orbit-server/src/main.rs').read_text(),
 'embedded':(root/'crates/orbit-client/src/embedded_authority.rs').read_text(),
 'unified':(root/'crates/orbit-client/src/unified.rs').read_text(),
}
for name, required in {
 'runtime':['recover_folder_commands','execute_folder_move','folder_commands::schema','recover_markdown_commands'],
 'folder':['markdown_folder_commands','TransactionBehavior::Immediate','pending(', 'publish(', 'accept(', 'folder_exists'],
 'server':['/v1/authoritative/folder-moves','authoritative_folder_move','device(&state,&headers)'],
 'embedded':['pub fn folder_move','execute_folder_move'],
 'unified':['pub fn folder_move','replica.queue_folder_move(command_id, &command)','CommandDisposition::Pending'],
}.items():
 for match in required:
  assert match in files[name], (name,match)
print('M18 folder command contracts: PASS (source-level only)')
