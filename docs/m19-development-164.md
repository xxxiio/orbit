# Orbit 164 — Vault-derived reverse discovery

## Scope

Source implementation of three new `orbit note` commands:

- `reference-backlinks project:<id>` (also supports `task:` and `habit:`) reads actual Markdown and returns the notes carrying an exact typed reference.
- `asset-backlinks <sha256>.<ext>` returns Markdown notes referring to a particular content-addressed attachment.
- `asset-audit` enumerates all notes' Markdown attachment references and compares them with the physical `.orbit-assets` directory, returning referenced, orphaned, missing and corrupt file lists.

No SQLite knowledge-body or relation tables are added. No orphaned files are deleted: interrupted publication can leave unreferenced immutable bytes.

## Deferred M19 work

- Remove obsolete knowledge domain, repository, and semantic-materialization dependencies without breaking project and task APIs.
- Coordinate interrupted asset publication and import repair more tightly with durable Markdown intents and receipts.
- Complete the M19 knowledge implementation handoff by Orbit 166; full executable Rust acceptance remains assigned to M15 at user request.

## Verification

Source guard `scripts/legacy-contracts/test-m19-knowledge-discovery.py`; M19 existing Python guards; 17 offline synchronization tests and 12 Python worker tests plus 10 subtests. `cargo`/`rustc` not available, so executable Rust verification not claimed.
