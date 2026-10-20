# Orbit 167 — M19 import/export publication integrity

## Implementation

- Portable import now publishes all preflight-verified immutable blobs before any Markdown file or folder is created. A crash may strand a recoverable orphan blob, but must not create an accepted note pointing to an attachment that import has not installed yet.
- Portable export now gathers Markdown and checks each stable identity and referenced asset before creating any destination.
- Export writes notes and assets into a private sibling staging directory, syncs files, then renames the completed package to its requested destination. Errors before publication remove the staging directory rather than leaving a misleading partial export.
- Two Rust regression scenarios assert that missing or corrupt attachment references do not create an export destination. Existing M19 source assertions were updated for the staged export call.

## Remaining M19 implementation

**Not complete.** Legacy SQL-backed knowledge tables (`knowledge_items`, `knowledge_relations`, `knowledge_resources`, `knowledge_fts`) and associated `KnowledgeRepository` / materialization implementations still exist. A safe removal must remove the legacy API and semantic replay dependencies together; the current pass does not pretend that this architectural retirement is finished. Physical Markdown remains authoritative for the new CLI note surface.

## Validation performed

All six M19 source guards, M18 public mutation guard, 17 offline authority Python tests and 12 Python worker tests passed. Cargo / rustc unavailable; the new Rust tests were not executed. Full acceptance stays in M15 per the project plan.

## Progress

M19 estimated implementation **97%** (previously 96%); this change closes an import/export durability criterion but does not complete legacy repository retirement. M18 remains 99%; M20 65%; M21 65%; M22 10%; M15 8%. These are implementation estimates, not verified acceptance.
