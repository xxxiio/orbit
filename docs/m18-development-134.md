# Orbit Development 134 — File-backed knowledge retrieval slice

## Why this work now
M18's primary blocker remains the competing SQL-body and Markdown-file knowledge paths. Rather than releasing another single primitive, this iteration delivers a cohesive, file-backed read experience: stable-ID retrieval, content search, forward links/backlinks, and authenticated HTTP endpoints based on the same host-side service. This is groundwork, **not** the canonical write cutover. No less-complete lower-priority UI or compatibility work should precede it.

## Implemented
- KnowledgeService::read_by_id: resolves stable `orbit_id` identity from actual Markdown files after validating the complete vault identity set; catches identity change during read.
- KnowledgeService::search: searches filesystem-backed content and filename titles, stable-ID ordered and bounded to 200 results. This is not an FTS engine.
- KnowledgeService::relations: returns outgoing `[[id]]` references, backlinks, and unresolved targets; derives its answer from actual .md files, not the legacy SQL table.
- All three operations are exposed through AuthoritativeHost, and through authenticated server endpoints:
  - GET `/v1/authoritative/markdown-by-id?id=...`
  - POST `/v1/authoritative/markdown-search` with `{ "query": "...", "limit": 20 }`
  - GET `/v1/authoritative/markdown-relations?id=...`
- Rust regression test added for search, cross-document links, missing links, and stable-ID resolution after a folder move.

## Known gaps and risks
- Legacy `authority_knowledge_items` remains active and authoritative for the old knowledge command endpoints. It is not yet safe to retire.
- No SQLite/Markdown joint commit protocol or command receipts for file-backed changes; no claim of durable cross-resource acceptance.
- Remote native client, MCP, and remote offline replica are not integrated with these new file-backed read APIs.
- Link parsing is intentionally narrow, does not skip fenced code or Markdown frontmatter. Search is linear and case folding is basic Unicode lowercasing.
- Rust compilation, Rust tests, and HTTP integration are not verified in this environment. Tests are *written*, not passed.

## Validation
- `python scripts/test-authority-offline.py` passed (17 tests).
- `python -m unittest discover -s deploy/server -p 'test_*.py'` passed (12 tests).
- `python -m compileall -q deploy scripts` passed.
- Source-wiring checks passed.
- Archive integrity checked.

## Next bundled iteration
Deliver the real cross-resource command lifecycle (intent, publish, SQLite acceptance/receipts, recovery), switch old knowledge APIs to read accepted Markdown state, and remove the SQL-body store after executable acceptance checks. Prefer a single larger vertical slice instead of spreading those tasks into many small numbered releases.

## Internal-testing readiness
Not ready. Build and executable smoke workflows have not been proven, and blocking crash-recovery/data-consistency issues remain. Flutter is deferred to Atomic App.
