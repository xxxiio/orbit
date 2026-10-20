# Orbit 099 — Explicit offline conflict reconciliation

Native authority replicas now **fail closed** when an outbox command has a server conflict. A repeated `flush` returns a blocked result without sending the conflicted command or any following command. Previously the next flush could resubmit that same conflicted envelope.

`orbit-cli --json sync replica --db <path> --server <url> pending` lists unresolved intentions, including the original frozen revision. After examining the latest accepted snapshot and choosing a new intention, the operator can use `replace-conflict --old-command-id <id> --new-command-id <new-id> --kind task --action-json '{...}'`. The replacement must have a new idempotency key, keeps the old command as a `superseded` audit record, and occupies its original queue position. Subsequent queued intentions remain ordered. This is **explicit manual reconciliation**, not automatic semantic merge or a desktop/mobile conflict UI.

Existing Orbit 098 authority SQLite files are upgraded transactionally to permit the superseded state without dropping receipts or frozen revisions. No change is made to local-only mode or user-managed Git backups.

**Remaining M18 blockers:** runtime validation in a Rust toolchain, desktop/mobile UI integration, automatic or guided domain-aware reconciliation, migration/import of populated histories, browser/agent canonical cutover, and built-in Git retirement. Source-level checks do not establish Rust correctness.
