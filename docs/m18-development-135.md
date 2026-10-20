# Orbit Development 135 — File-backed knowledge workspace workflow

## Priority rationale
M18 remains blocked by the SQL-body and Markdown-file dual write paths. This iteration groups the existing stable-ID service, recoverable filesystem creation/editing, folder move, metadata reconciliation, and retrieval into a single host-owned **pre-acceptance** workflow. It is not yet a canonical domain-command acceptance implementation. This functionality is prioritized before transport parity, Flutter, and release hardening; no higher-priority M18 feature is completed by adding a new facade.

## Implemented
- `KnowledgeService::stage_update_by_id`: resolves a note by `orbit_id`, checks expected and replacement frontmatter identity, checks expected bytes, and uses the existing recoverable filesystem-update journal.
- `KnowledgeService::move_by_id`: resolves the physical path through stable identity and moves a note while preserving its Markdown bytes. The underlying filesystem move is not crash-atomic.
- Host workflow methods `stage_markdown_create`, `stage_markdown_update`, and `stage_markdown_move` invoke the file-backed service and then reconcile the derived SQLite ID/path index.
- Regression scenario added for create → read → edit → stale-edit rejection → folder move → index update → search → reopen/reconcile.

## Explicitly not solved
- These `stage_*` methods MUST NOT be exposed as accepted canonical commands or substituted into the existing HTTP command endpoints: metadata-index reconciliation may fail after filesystem mutation; file replacement has an external-writer race; move lacks a durable recovery intent. No cross-resource SQLite receipt or revision is committed.
- The existing `authority_knowledge_items` SQLite body table and legacy read/write APIs remain. The SQL-backed path is not retired; there are two knowledge write paths.
- Cross-device offline command semantics, native client parity, MCP parity, executable Rust smoke testing, and crash fault-injection are outstanding.
- Host method errors currently collapse service errors to generic RelayError::Invalid; a later canonical command implementation must expose typed conflict outcomes.

## Validation
- Python authority offline regression: 17 passed.
- Python server agent/automation tests: 12 passed.
- Python compileall: passed.
- Rust tests added but not executed because `cargo`/`rustc` unavailable.
- No claim of runtime acceptance, consistency under crashes, or internal-testing readiness.

## Next bundled iteration
Implement the **single canonical command protocol**: durable SQLite intent keyed by command id; staged and recoverable filesystem publication; accepted metadata, semantic revision and idempotent receipt; startup recovery with crash-point fault tests; reads from accepted Markdown state; and retirement of SQL-body knowledge commands. Deliver the lifecycle as one integrated feature, not one helper per iteration.

## Readiness
M18 remains at provisional 89%; internal testing NOT ready. Build, smoke workflows and no blocking data-loss issues are still unverified. Flutter belongs to Atomic App.
