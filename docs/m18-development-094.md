# M18 development 094 — Verified canonical snapshot and recovery boundary

## Source changes

- `GET /v1/authoritative/snapshot` now includes a SHA-256 `history_fingerprint` computed from a length-delimited, ordered semantic history and the server's persistent relay generation. Revision, fingerprint, projects and tasks are read in the **same SQLite snapshot**.
- Server refuses to advertise a canonical snapshot if relay event count and accepted command receipts diverge, rather than presenting legacy relay history as authoritative.
- Opt-in offline client's cache commits project/task collections and their history fingerprint atomically, rejects malformed or absent fingerprints, revision rewinds, and **different history at the same revision**, without discarding pending commands or prior accepted data.
- Rust tests authored for stable fingerprints, command retries, advancing hashes, and fail-closed legacy history. Python regression tests cover same-revision divergence, malformed metadata, and atomic commit.

## Remaining architecture gates — do not claim complete server-authoritative sync

- **Prefix continuity is not proven at greater revisions** by a single head fingerprint; a new server with a divergent but longer history can still be accepted. Implement authoritative incremental revision receipts/prefix proofs before automatic, general-purpose replica resync.
- Old Python accepted caches lacking a stored fingerprint may be incompatible with the upgraded protocol; a deliberate refresh/import decision is needed rather than assuming lineage.
- Server-authoritative habit/metric business commands, native Rust/mobile replica, conflict reconciliation, legacy import/Git retirement, and server-backed browser/worker execution remain incomplete.
- Rust compile/test/Clippy, live two-client acceptance, server-restore acceptance remain unexecuted; collect in M15 after M18 source migration.

M18 estimated source progress: **70%** (was 68%); M15 remains 8%. All other milestones unchanged.
