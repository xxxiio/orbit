# Orbit Development 261 — M24-I1 authority boundary

- Source-level architecture refactor: decouple agent dispatch from SQLite using
  an application-owned port implemented by the storage crate.
- Add mock authority and runtime tests for duplicate dispatch and ambiguity.
- Update current local-only and optional server-authoritative architecture.
- Broad Rust builds, system integration, and fixes from acceptance remain M15.
- M24 implementation progress: 10%; M23: 100% source; M15: pending.

No change to public `orbit` CLI surface or existing SQLite schema.
