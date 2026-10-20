# Orbit Development 127 — Recoverable Markdown create intents

## Priority decision
M18's highest-priority unfinished work remains completing the Markdown-authoritative knowledge command and transaction cutover. No other unfinished priority should supersede it in this iteration. The existing SQL `authority_knowledge_items.body` is still authoritative for canonical knowledge commands; this patch **does not** complete the cutover.

## Implementation
- Added `orbit-runtime::knowledge_operations::KnowledgeOperations` with durable, private `.orbit-operations/*.json` creation intents containing the path, full intended Markdown text and SHA-256 digest. These are temporary recovery records, not canonical SQL bodies.
- Intents are written using create-new semantics and synchronized to disk, then Markdown is published with the existing no-clobber file primitive. Intents are removed only after publication is verified. Reopening the runtime replays outstanding intents; a mismatched external file fails closed without overwriting the file or discarding the intent.
- `AuthoritativeHost::open` now constructs the recoverable operation component and runs pending-intent recovery before opening the authority store. Existing knowledge tree behavior is preserved through its vault reference.
- New Rust regression tests cover normal creation, interrupted publication and conflict-preserving recovery.

## Validation
- Python offline tests: 17 passed (`python scripts/test-authority-offline.py -q`).
- Python syntax: passed.
- Source assertions: passed.
- Rust toolchain unavailable: compilation and Rust unit tests NOT executed.
- No live integration or cross-resource crash-fault injection tests executed.

## Known limitations / next priority
- Knowledge create/update commands still write Markdown bodies into SQL; this journal is not connected to canonical command acceptance or receipts.
- This filesystem-only journal does NOT make a SQLite transaction atomic with file publication. Duplicate operation IDs after intent cleanup are not backed by permanent domain receipts.
- Recovery requires folder hierarchy to exist and external filesystem modifications to be serialized/coordinated. Untrusted external writes remain a threat outside this primitive.
- Next: connect a durable **SQLite-owned** command-intent/receipt workflow to the Markdown file service, remove SQL bodies from authoritative projections, and implement recoverable update semantics. Follow with folder domain commands and embedded/HTTP/MCP parity.
- Flutter remains deferred to Atomic App.
- Internal testing readiness: NOT established. No successful Rust build or core executable smoke suite and no evidence yet ruling out blocking data-loss risks.

## Milestone estimates
M1 94%, M2 96%, M3 92%, M4 94%, M5 94%, M6 94%, M7 92% (retired historical), M8 94%, M9 97%, M10 94%, M11 94%, M12 95%, M13 94%, M14 97%, M15 8%, M16 95%, M17 98%, M18 89%. Overall equal-weight implementation estimate 89.5%. These are estimates, not acceptance results. M18 remains unchanged because the cutover is incomplete.

## Exit priority check
Markdown-authoritative command cutover still outranks transport expansion, broad hardening, and Flutter integration. This iteration adds a recoverable filesystem operation but not final architecture convergence.
