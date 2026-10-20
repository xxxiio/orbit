# Orbit Development 266 — connected authority module consolidation

Baseline: Development 265. M24 implementation scope: 38% -> 45% (+7 pp).

## Source changes

- Extracted the existing device-authenticated HTTP transport into
  `crates/orbit-client/src/authority/http.rs`, retaining the public
  `orbit_client::authority::AuthorityHttp` path.
- Extracted accepted Markdown mirror synchronization, cache integrity and
  publication operations into `crates/orbit-client/src/authority/markdown_mirror.rs`.
- Kept the original `AuthorityReplica` facade, accepted server snapshots,
  and outbox ownership in `authority.rs`; no schema or route changes.

## Scope boundaries

This is I2 module decomposition. It does not merge agent-specific pending commands
with Orbit's domain outbox (I3) or modify Rust runtime behavior intentionally.
M15 owns broad compilation, executable integration and validation-driven fixes.

## Remaining M24 priorities

1. Storage facade decomposition (remaining I2 scope).
2. Review and consolidate pending-command ownership, including agent intentions (I3).
3. Finalize worker/runtime capabilities, obsolete-code retirement, docs (I4).

Internal-testing readiness is not asserted until M15 executable acceptance.
