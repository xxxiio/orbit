# Orbit 163 — Retire SQL-backed knowledge command entry points

## Scope

- Removed `orbit knowledge ...` from Clap's public command model. `orbit note ...` and `orbit folder ...` are the supported file-authoritative commands.
- Removed the SQL-backed `run_knowledge`, resource mutation, and relationship command handlers from the CLI.
- Removed `AuthoritativeHost::with_legacy_knowledge_cli`, the last public escape hatch permitting a CLI closure to mutate `SqliteStore` directly.
- Updated source contract tests to reject restoration of the legacy command and bridge.

## Deliberate scope boundaries

- Existing `orbit_application`/`orbit_domain` legacy knowledge models and serialized response definitions have **not** all been removed in this pass. They may still be used by tests and by historical application boundaries; remove safely while finalizing M19.
- File-backed asset and note operations implemented in Orbit 160–162 remain intact.
- No migration is provided from legacy SQL knowledge bodies: this is a greenfield breaking change, requiring Markdown file authority.
- Compile/integration validation remain deferred to M15 per project direction; source-contract and relevant Python smoke checks were run.

## Next M19 implementation priorities

Typed backlink/resource discovery, legacy model/reference removal, physical file integrity and interruption recovery, and complete knowledge surface parity.
