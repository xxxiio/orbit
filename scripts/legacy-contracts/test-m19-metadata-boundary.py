#!/usr/bin/env python3
"""Source contract for file-authoritative knowledge metadata edits."""
from pathlib import Path
root=Path(__file__).resolve().parents[2]
cli=(root/'crates/orbit-cli/src/markdown_cli.rs').read_text()
impl=(root/'crates/orbit-cli/src/markdown_metadata.rs').read_text()
for command in ('Kind','Status','Link','Unlink'):
    assert f'NoteSubcommand::{command}' in cli, command
for required in ('MarkdownCommand::Update','execute_markdown_command','orbit_kind','orbit_status','read_scalar','remove_links','markdown_by_id'):
    assert required in impl, required
for forbidden in ('SqliteStore','with_legacy_knowledge_cli','update_knowledge_content_with_activity','store.conn'):
    assert forbidden not in impl, forbidden
print('M19 Markdown metadata command boundary: passed')
