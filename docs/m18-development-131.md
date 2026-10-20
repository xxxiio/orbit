# Orbit Development 131 — Real Markdown reads through unified clients

## Priority assessment
M18 U2 remains the highest-priority unfinished category. The canonical knowledge commands still store note bodies in SQL, and a full filesystem/SQLite crash-safe cutover would be unsafe without a coordinated acceptance and recovery design. This iteration implements an end-to-end **read** path for real Markdown files selected from the folder tree, without claiming a write cutover or introducing another SQL content store.

## Implemented
- `AuthoritativeHost::markdown_document` reads through `KnowledgeOperations` and `MarkdownVault`.
- Authenticated `GET /v1/authoritative/markdown-document?path=...` serves the actual Markdown file contents; server handlers never read filesystem paths directly.
- Embedded Rust client and `AuthorityConnection` expose the same `markdown_document(path)` operation in both embedded and remote modes.
- Query paths are percent-encoded on remote transport and are validated by the vault. Hidden operational paths are prohibited at the vault layer as well as the HTTP boundary.
- Added a vault-level regression test for physical content reads, directory rejection, private operational paths and traversal.

## Validation
- Offline synchronization: 17 Python tests passed.
- Python syntax checks: passed.
- Source-wiring assertions: passed.
- Rust compilation and added Rust regression tests: **not executed** (toolchain unavailable).
- Remote HTTP runtime parity: not executed.
- Archive integrity: verified.

## Gaps and next priority
**This is not the storage cutover.** Existing `/knowledge` SQL-backed reads/commands remain; this new endpoint is explicitly for filesystem-backed documents already visible in the tree. Do not expose this as accepted canonical note content to UI clients until write coordination is complete. Next: build the two-resource (SQLite + Markdown) acceptance protocol, durable command receipts, journal reconciliation, and fail-closed crash tests before switching knowledge creates/updates and indexes to Markdown authority. Then remove the SQL authoritative body path instead of maintaining both.

Internal testing readiness: **not established**. Need a runnable Rust build, core workflows, and absence of known blocking data loss. Flutter remains in Atomic App, not an M18 blocker.
