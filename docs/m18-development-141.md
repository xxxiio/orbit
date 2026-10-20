# Orbit Development 141 — Public knowledge API cutover

## Goal and priority
M18 U2 remains the highest-priority unfinished development category: remove competing SQL-bodied knowledge interfaces and converge the embedded and server consumers on file-backed Markdown. Work deliberately excludes Flutter (Atomic App). This iteration is an **API cutover**, not a claim of complete repository or runtime consolidation.

## Implemented
- Removed all four old authoritative server endpoints that read or mutate `authority_knowledge_items` (`knowledge`, `knowledge-search`, `knowledge-commands`, and `knowledge-updates`). The supported HTTP surface for document content is now the Markdown-file family.
- Removed old SQL knowledge operations from the native `AuthorityConnection` and `EmbeddedAuthorityClient` public interfaces instead of retaining two ways to create, edit, list and search knowledge. This is an intentional greenfield breaking change.
- Connected Markdown stable-ID reads, search, and backlink/relationship lookup to BOTH unified-client transports. Embedded delegates to the same AuthoritativeHost used by the server, and remote delegates to the authenticated Markdown endpoints.
- Preserved existing durable Markdown remote command outbox; remote offline search remains explicitly unsupported rather than returning unaccepted content.
- Added a source contract test to forbid accidental reintroduction of the obsolete routes and native API methods.

## Verification
- Python offline-replica suite: 17 tests passed.
- `scripts/legacy-contracts/test-m18-markdown-cutover.py`: passed.
- No Rust toolchain on this runner: cargo build/test not run.

## Important remaining work — NOT M18 complete
- `orbit-client::OrbitClient` and `orbit-storage::SqliteStore` still contain feature-rich legacy local application behavior. Consolidating this with AuthoritativeHost is required before M18 completion.
- Legacy SQL knowledge tables, schema and internal Rust methods have not been deleted; older CLI/MCP code paths and pending legacy outbox records must be audited and fully transitioned.
- Actual Rust compilation, authenticated HTTP/MCP parity, crash-recovery, local/remote workflow and zero-data-loss smoke tests remain unverified.
- Folder management still lacks a fully revisioned accepted-command workflow. Remote offline note caches are incomplete.

## Next priority
Complete application-service repository consolidation and remove the remaining internal SQL-body knowledge write paths. Run Rust compilation and executable contract/smoke tests before declaring internal-testing readiness. Internal testing: **not ready**.
