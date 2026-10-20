# Orbit 162 — M19 portable assets and typed relationships

## Scope implemented

- `orbit note asset add <note-id> <file> [--label ...]`: check regular non-symlink input, cap to 64 MiB, derive SHA-256 identity plus extension, publish immutable content under `.orbit-assets`, and append a portable Markdown reference via durable note update commands.
- `orbit note asset list|verify|unlink`: list note references, independently verify referenced content digests, and detach from a note without deleting a blob that other notes might reference.
- `orbit note reference|unreference <note-id> project:<id>|task:<id>|habit:<id>`: portable typed entity links written to the note body through durable note mutation commands, without restoring SQL note bodies.
- `note export-dir`: include verified referenced `.orbit-assets` data alongside Markdown.
- `note import-dir`: import the supplied `.orbit-assets` files, verifying SHA-256 names; existing immutable assets are reusable.

## Known limitations

- Imported notes and assets are a resumable multi-command batch, **not** one atomic transaction. Failed note mutation may leave an unreferenced immutable asset. Reference-preserving garbage collection is not yet implemented.
- Typed links are physical Markdown link records; further typed backlink indexing/UI parity remains.
- Legacy SQL-backed `knowledge` CLI and resource APIs still exist. M19 cannot be considered complete until their replacement and removal.
- Rust toolchain unavailable in this environment. Rust behavioral tests were authored but not run; full validation remains assigned to M15 as instructed.

## Next M19 objectives (completion target by Orbit 166)

1. Remove old SQL-backed `knowledge` command and its mutable repository escape hatch after replacing relevant knowledge retrieval workflows.
2. Complete typed entity backlinks, resource discovery, and integrity checks across all note operations.
3. Harden filesystem note/asset recovery and ensure imports cannot expose broken references.
4. Finish the canonical Markdown M19 feature surface and documentation for client integration.
