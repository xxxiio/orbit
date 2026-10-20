# Orbit 072 — Pre-M15 source integration review

## Decision and scope

Next milestone: **M15 release readiness**. M9/M14/M16/M17 source percentages are provisional; no release acceptance is claimed. The server is optional and local Orbit clients keep their own SQLite and Git-compatible history. No cloud AI or multi-user service is required.

## Changes in Orbit 072

1. Knowledge search now uses the repository's optional **complete** lexical candidate set before hydration. SQLite prefilters ASCII needles with `instr(lower(...))` and includes *all* rows with non-ASCII text or identifiers for subsequent Rust canonical matching. Non-ASCII queries use complete repository scanning. Crucially, an incomplete FTS5 token list can no longer affect the result set or sorting. The full candidate list is unbounded to preserve top-N ranking correctness; memory/latency benchmarking belongs in M15.
2. Source milestone M16 frontmatter is reconciled to its existing 95% source estimate, and the program roadmap phase and M17 scope reflect 071/072 rather than obsolete 046/057 estimates.
3. Existing local resource metadata and optional semantic adapter are reflected in M9's checklist, without claiming those features have passed runtime acceptance.

## Architectural and correctness audit boundaries

### Semantic sync / restore

Existing code provides immutable Git-compatible change and resolution wire artifacts, durable outbox, HTTP relay, replay and per-device checkpoints. Review identified no new *confirmed* defect beyond the previously recorded runtime uncertainty. **Not proven:** two real clients concurrently editing, choosing resolutions in the TUI, swapping Git/HTTP transports, relaying a resolution out of order, then restoring a server backup and reconnecting. Execute the prepared `deploy/server/smoke-two-clients.py` and additional conflict-resolution scenario during M15. Preserve existing sequence identity and stop on any mismatched immutable record.

### Agent proposal and execution safety

Existing source separates proposal, administrator approval, authenticated worker claim, lease and result audit. Allowlisted worker actions are task status/create, habit occurrence and metric observation; arbitrary shell execution is not an accepted feature. A lost acknowledgement after a mutation remains an **ambiguous outcome** requiring reconciliation; do not enable blind automatic retries for expired mutation leases. Verify admin decisions cannot be bypassed, worker ownership checks, stable task/occurrence identities, and metric timestamp/value idempotency on real databases during M15.

### Knowledge retrieval

The complete candidate filter is correctness-preserving for ASCII query substrings including IDs and Unicode-folding cases. The legacy full scan remains for non-ASCII search; this is intentional. `knowledge_term_locations` provides Unicode scalar columns, not grapheme cluster display columns, and TUI navigation is an in-app line-centered viewer rather than an editor cursor. Benchmark large corpora, FTS5 update triggers, and compare indexed vs unindexed search results during M15.

### Scope of the optional server

- **Implemented in source:** single-user relay, authenticated device sync, backup/restore commands, schedulers and reporting worker, read-only automation MCP, administrator console, and approval-gated limited agent mutations.
- **Not implemented as complete clients:** live canonical browser CRUD parity, general AI-agent planner/executor, automatic cloud AI, multi-tenant users, hosted synchronization, and full browser offline operation.
- **External dependencies:** worker must be opted into and have authorized access to a local Orbit database; on-demand browser reports may be delayed and are not direct live database reads.

## M15 acceptance checklist

- [ ] Cargo fmt, check, clippy and complete workspace tests with a provisioned Rust toolchain.
- [ ] SQLite migrations from representative earlier version databases, including v9–v12, legacy metric backfill and FTS index repair.
- [ ] Two-client conflict and resolution smoke with Git/HTTP switching, offline reconnect, duplicate pulls and server rewind/recovery.
- [ ] Server device enrollment, token rotation, revocation, TLS ingress, backup/restore and durable-volume smoke.
- [ ] Approved-agent proposal/claim/settle with worker crash or acknowledgement loss; verify idempotent reconciliation, audit and no unapproved mutations.
- [ ] Real remote MCP protocol client and browser deployment, with scoped permissions and no stale-success presentation.
- [ ] Knowledge lexical equivalence tests on ASCII, Unicode, filtered, large-scale and reference-navigation datasets.
- [ ] Privacy/secret scanning, permission boundaries, packaging and local-first offline smoke.

All unmet runtime checks remain **not run**, not passing. No manual user verification is required before M15 begins.
