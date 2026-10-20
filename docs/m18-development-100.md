# Orbit 100 — Verified server ancestry and deterministic conflict replacement

This iteration strengthens the native connected-mode contract rather than introducing another transport. The authoritative server now supports authenticated `GET /v1/authoritative/history-prefix?revision=N`, returning the SHA-256 digest of the server's immutable event history up to revision N, including its persistent server generation. This digest uses the exact encoding of the six-domain canonical snapshot history fingerprint.

On `orbit-cli sync replica ... refresh`, the native Rust client verifies that the prefix at its previously accepted revision matches its persisted fingerprint before storing a newer snapshot. Divergent server histories must be reconciled explicitly: simply presenting a greater revision is no longer sufficient. The prefix endpoint validates the server's accepted command/event history first. The proof and snapshot come from separate reads; append-only event immutability under the authoritative command contract ensures the prior prefix stays valid.

The native conflict replacement workflow now retains the original command as superseded audit data but reuses its queue ordinal for the new, reviewed command. Later pending work cannot bypass the replaced intention. The superseded receipt moves to a new tail ordinal to satisfy the uniqueness constraint.

**Breaking compatibility:** Clients using the native authority replica require the new server endpoint before refreshing an already cached replica to a later revision. Fresh replicas can refresh against the existing snapshot endpoint. Legacy Git relay and Python bridge remain but are not target architecture.

**Remaining M18 work:** Native embedded/Flutter/TUI integration; complete browser/automation canonical cutover; legacy import and Git retirement; real two-client Rust runtime validation. Rust compiler and Cargo were not available here, so these tests were authored but not executed.
