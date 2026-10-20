# Orbit 097 — Native authoritative client boundary

## Why this work precedes more server endpoints

M18's highest unfinished daily workflow is a real native connected client: Python command bridges must not become the permanent synchronization architecture. This slice provides an explicit Rust authority/cache/outbox interface within `orbit-client` without changing the local-only store. No higher-priority unfinished M18 work should be displaced by standalone hardening.

## Design contract

- Server is the only authority in connected mode. The cache is a replaceable SQLite replica of acknowledged collections.
- The outbox stores intent separately; each command receives a frozen expected revision before network submission. A lost acknowledgement reuses the same envelope.
- One server is pinned to a cache. Another server requires another cache.
- Revisions cannot go backwards; equal revisions cannot switch history fingerprints.
- The native transport requires HTTPS except for loopback, refuses credential-bearing URLs, and does not follow redirects.
- Unauthorized, transport failures and conflict results must not silently discard pending commands.

## Limitations

This is a native Rust foundation, NOT complete Flutter/TUI integration or an accepted two-device offline synchronization capability. A newer revision with unrelated history still needs continuity proof. Native source compilation, HTTP integration and migration recovery remain unverified. Breaking changes are acceptable for the final cutover, but no old history is deleted.

## Progress

M18 estimated **83%**; prior 79%. All other milestones unchanged. M15 remains 8%.
