# Orbit Development 161 — Markdown knowledge metadata and relations

## Implemented

- Added Markdown-native note `kind` and `status` commands. These edit `orbit_kind` and `orbit_status` scalar YAML frontmatter fields, retaining the physical file and `orbit_id` as the authority.
- Added `note link` and `note unlink`, editing explicit `[[stable-id]]` references inside the Markdown file. Links remain discoverable through the existing filesystem-backed backlinks query.
- All mutations use the existing durable `AuthoritativeHost::execute_markdown_command` update receipts, rather than the SQL knowledge body or transitional repository store.
- Strict plain-scalar metadata parser rejects duplicate fields and malformed frontmatter; link target IDs must refer to existing notes. Existing labeled links may be removed by stable identity.
- Added Rust tests covering metadata edits, duplicate protection, labeled link handling, persisted relations, and invalid targets.

## Not implemented / assigned remaining M19

- Migrating or retiring the old `knowledge` CLI command family and its SQL-backed resource references.
- Attachment storage and secure external-file references.
- Fully typed relationship kinds and project/task context edges in Markdown metadata.
- Cross-document atomic transactions. Markdown mutation remains an individual durable command with optimistic read/modify/update semantics.

## Verification

Full cargo compilation and integrated acceptance stay in M15. No Rust execution was available in this environment. Existing source and Python tests are not substitutes for a compiled Rust build.
