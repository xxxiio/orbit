# Orbit Development 125 — Markdown note creation primitive

## Priority rationale
M18 knowledge ownership is inconsistent: existing canonical SQL commands continue to store Markdown bodies in SQL, while the directory explorer introduced in 124 reads real `.md` files. This iteration prioritizes a safe file-creation foundation over new facades or UI. There is no higher-priority M18 activity than finishing source-of-truth consolidation. Full domain-command cutover remains unfinished.

## Implemented
- `MarkdownVault::create_markdown(relative, content)` creates real Markdown documents under existing knowledge folders.
- Rejects non-`.md` destinations, absolute/traversal paths, missing or symlinked parents, and duplicate filenames.
- Creates an exclusive private staging file, writes and `sync_all`s contents, atomically publishes via no-clobber hard link, removes staging file and synchronizes parent directory. Hidden staging files do not appear in folder navigation.
- Rust regression tests cover nested note creation, duplicate prevention, traversal, extension rejection, hidden stages and symlink-parent rejection.

## Remaining gaps / limitations
- This is a **filesystem primitive only**. It is not wired to `AuthoritativeHost::knowledge_command` or HTTP and does **not** replace SQL knowledge body ownership. Cross-file/SQLite crash recovery, journaled domain commits, identity, durable receipts and revisions are needed before activating the canonical write path.
- The primitive assumes trusted, nonadversarial concurrent writers to its vault root. Filesystem path checks alone are not a hardened sandbox against concurrent symlink replacement, and hard-link availability is filesystem-dependent.
- Markdown note edits, deletion, moves, folder revisioned mutations, metadata indexing, replica caching, and legacy SqliteStore consolidation remain incomplete.
- Flutter UI is deferred to Atomic App.

## Validation
- `python -m pytest -q scripts/test-authority-offline.py`: 17 passed.
- `python -m compileall -q scripts`: passed.
- Six focused Rust source assertions: passed.
- Rust tests: authored, not executed (no Rust compiler available).
- Live embedded/HTTP/MCP parity and data-loss acceptance: not verified.

## Next priority
Build a recoverable journaled file+SQLite command boundary and make Markdown files authoritative for canonical knowledge commands. Then consolidate old local repository and expand revisioned folder operations across client transports. Do not declare internal-testing ready without successful Rust compilation, core executable smoke workflows and no known blocking data-loss issue.

## Provisional milestone estimates
M1 94%; M2 96%; M3 92%; M4 94%; M5 94%; M6 94%; M7 92% (retired); M8 94%; M9 97%; M10 94%; M11 94%; M12 95%; M13 94%; M14 97%; M15 8%; M16 95%; M17 98%; M18 89%. Overall equal-weight: 89.5% (rounded). M18 unchanged until authoritative cutover.
