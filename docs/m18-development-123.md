# Orbit Development 123 — Canonical Knowledge Link Read Model

## Why this work is next
M18 U2 remains the highest-priority unfinished implementation category. Existing canonical knowledge create, update, and search lacked an explicit knowledge relationship read model. This is a bounded source-level consolidation slice; broader repository convergence (metadata, resources, and legacy store elimination) remains unfinished.

## Implementation
- Added `orbit_runtime::knowledge_relations` to derive outgoing links, incoming backlinks, and unresolved targets from committed canonical knowledge Markdown.
- Links use explicit `[[stable-item-id]]` or `[[stable-item-id|alias]]` syntax and do not guess based on titles.
- Deduplication and sorting are deterministic; dangling references are surfaced rather than silently discarded.
- Added `AuthoritativeHost::knowledge_relations`, which returns the authority revision alongside a derived read model. The model does not create an alternate write-side store or history.
- Added a Rust unit regression covering forward/back links, missing targets, alias deduplication, and changes after content editing (written, not executed).

## Known limitations
- The parser is intentionally narrow: it does not parse Markdown escaping or ignore fenced-code links; it is not a full Markdown parser.
- Relationship reads currently scan knowledge items; no separate index exists.
- No HTTP/MCP/client adapter for this capability yet.
- Rich metadata, resources and the older local store are not consolidated.
- No Rust toolchain is available in this environment, so Rust build/tests are not verified.

## Validation
- `python -m pytest scripts/test-authority-offline.py -q`: 17 passed.
- Manual source inspections were performed; Rust compilation and parity checks remain unverified.

## Internal testing
Not ready: requires a runnable Rust build, executed core smoke workflows, and no known blocking data-loss issue. Flutter integration remains outside Orbit (Atomic App).

## Next priority
Replace the split local/authority persistence implementations and preserve richer knowledge metadata and resource functionality. Avoid treating this small read model as full U2 completion.
