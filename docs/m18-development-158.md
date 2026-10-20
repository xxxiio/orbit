# Orbit 158 — M18 public mutation boundary

## Implementation

- Converted the raw rich-service mutation entry point and its transaction helper to runtime-crate visibility, so external adapters must use typed accepted commands.
- Made rich ApplicationServices write methods `pub(crate)` to prohibit accidental direct-write adoption in another crate; the accepted-command engine still composes them internally.
- Removed `AuthoritativeHost::application_store()` to prevent client-side SQL access through the host API.
- Moved TUI term-reference read and embedded client SQL-knowledge-regression read behind `ApplicationQueries`.
- Isolated the remaining legacy CLI knowledge SQL operations behind the explicitly deprecated `with_legacy_knowledge_cli` host adapter. This belongs to M19's Markdown knowledge cutover, and does not authorize project/task/habit/metric writes.

## Milestone boundary

Per user direction, full Rust compilation, integrated system validation and release-readiness checks are deferred to **M15**. Do not reclassify those as an M18 implementation prerequisite. Retain cheap essential regression guards.

M18's primary production architecture is now substantially implemented. The remaining public M19-only legacy knowledge bridge is a transitional exception, **not** an accepted permanent application API. M18 should not be declared fully accepted or runnable without eventual Rust verification.

## Limitations

- Cargo/Rustc unavailable in this environment. New Rust call sites were not compiled.
- Legacy SQL knowledge commands must be migrated to Markdown in M19 before removing the last compatibility bridge.
- The historical M18 percentages are provisional implementation estimates, not passing executable-acceptance scores.
