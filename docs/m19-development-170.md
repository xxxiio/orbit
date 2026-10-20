# Orbit Development 170 — retire legacy knowledge application API

## Scope and priority

M19 remained the highest priority because the schema and CLI were already file-authoritative, but the application crate still publicly exported SQL-era knowledge commands, repository ports, retrieval APIs and corresponding dead CLI DTOs. This iteration retires that overlapping public architecture in one cross-crate change, rather than introducing another knowledge abstraction. The next feature priority after this M19 implementation handoff is M20 transport parity. Full compilation and integrated validation are assigned to M15 as requested.

## Production changes

- Remove six retired application modules: knowledge commands, queries, resource commands, resource queries, SQL-coupled retrieval and semantic retrieval. They had no active `SqliteStore` implementations after previous M19 cutovers.
- Remove `KnowledgeRepository`, resource repository ports and the deprecated legacy knowledge unit-of-work contract.
- Remove SQL-era knowledge semantic outbox entity kinds and obsolete materialization branches; these must not be confused with the durable Markdown command history.
- Remove unused legacy CLI `Knowledge*` JSON response variants, portable SQL knowledge import document parser, obsolete conversion DTOs and parser tests targeting the retired `orbit knowledge` command.
- Remove stale legacy application imports from the embedded client while retaining its file-backed `create_knowledge`, `list_knowledge` and Markdown APIs for current consumers.
- Update the M19 application-boundary guard to prohibit restored obsolete interfaces. Add dedicated regression guard for the application/CLI removal while preserving file-backed client pathways.

## Current boundary

Markdown files and content-addressed assets remain authoritative for knowledge; SQLite stores the intended identities/index/intent metadata. No automatic migration of old SQL knowledge bodies is introduced. The domain's knowledge value types and the client's compatibility DTO names remain where they serve current file-backed clients; this change removes the retired **persistence/application** APIs, not necessary user-visible file workflows.

## Verification, limitations and milestone handoff

- Nine M19 Python source guards: passed.
- Offline authority Python tests: passed (17 tests).
- Python agent/automation workers: passed (12 tests plus 10 subtests).
- Rust workspace compilation and executable CLI/client integration: **not run** (Rust toolchain unavailable); M15 responsibility.
- M19 implementation scope is handed off as **100% source-implemented / executable acceptance unverified**. This is not a claim that this Rust archive compiles.
- M18 remains 99% estimated; M20 65%, M21 65%, M22 10%, M15 8%, pending their work.

## Measured change

See `docs/orbit170-diff.json` for all per-file additions, deletions and the 169→170 baseline. Test/documentation changes are separate from production Rust changes in reporting.
