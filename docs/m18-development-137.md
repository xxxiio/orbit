# Orbit Development 137 — File-authoritative command transport integration

## Scope
- Connect the already-implemented SQLite/Markdown accept-and-replay command path to authenticated server HTTP.
- Expose the exact same host command via the embedded Rust client and the unified connection API.
- Return `MarkdownReceipt` only on server/embedded acceptance; reject unsupported offline Markdown writes (no fake `Pending`).
- Add a Rust regression scenario exercising unified embedded creation, duplicate receipt, editing and file-backed reads.

## Implementation
- `POST /v1/authoritative/markdown-commands` accepts `{command_id, command}` with authenticated device actor, returning `{command_id,note_id,revision}`.
- `AuthorityConnection::markdown_command` uses the host in embedded mode and authenticated HTTP in remote mode.
- Legacy `/knowledge-commands` and `/knowledge-updates` continue using SQL bodies and must be retired; this release is **not** a complete storage cutover.

## Limitations and next highest priority
- Rust toolchain is not available here; compile and execute before considering the path verified.
- Legacy body-bearing SQL read/write commands, MCP adapter, existing remote knowledge outbox and its `knowledge_create`/`knowledge_update` payloads have **not** been cut over. The old and new knowledge APIs must not be used interchangeably.
- File/SQLite external-edit atomicity and crash recovery require executable fault-injection testing.
- Complete the cutover in the next integrated iteration: update callers, remove legacy knowledge bodies, implement a durable Markdown offline outbox, and test cross-transport parity.
- Flutter remains out of scope (Atomic App).
