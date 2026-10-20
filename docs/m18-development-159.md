# Orbit 159 — M18 authority API closure

## Implementation

The mutable `AuthorityStore` module is now private to `orbit-runtime`. Consumers retain the publicly re-exported `AuthorityError` / `AuthorityEvent` data contracts but cannot construct or operate a separate authoritative repository through `orbit_runtime::store`. The server backup validation operation moves to `AuthoritativeHost::validate_backup`, maintaining the existing functionality without exposing the store. The markdown/folder low-level SQL helpers are crate-private; callers use the host knowledge API instead. Public paths in `orbit-client` and `orbit-server` now use the stable error export.

This closes a concrete M18 storage-ownership bypass. The only remaining public raw-store closure is the explicitly marked legacy SQL-knowledge adapter used solely by CLI knowledge operations. This is owned by M19's Markdown knowledge cutover and is **not** evidence that the general application runtime accepts arbitrary SQL mutations.

## Validation disposition

Per user direction, complete build and integrated acceptance belong to M15. Development-stage static guards and the offline Python suite are retained as cheap safety checks. Rust compilation is not available in this environment and no runtime Rust acceptance is claimed.

## Scope and status

M18 architectural implementation is substantially complete after the authority API closure; M19 owns the knowledge-specific compatibility removal, and M15 owns executable acceptance. Do not reopen M18 for M19 knowledge transport rewrites or M15 integration verification unless a concrete M18 architecture bug is identified.
