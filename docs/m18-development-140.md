# Orbit Development 140 — M18 acceptance blocker review

## Scope
This package is **not** M18 completion. A complete cutover cannot responsibly be claimed without consolidating the legacy SqliteStore and SQL-body knowledge commands, preserving rich local workflows, and executing the Rust integration suite.

## Change
The Markdown create acceptance transaction now also verifies that the discovered note path matches the command's requested path, in addition to verifying stable note ID and exact Markdown bytes. A regression test checks that an unexpected move cannot earn an accepted receipt and that the pending command remains replayable.

## Evidence and blockers
- `rustc` and `cargo` are absent in the execution environment. Rust tests have not run.
- The old `OrbitRuntime` still owns `orbit_storage::SqliteStore`.
- `RelayStore` still has SQL-body `authority_knowledge_items` and old knowledge handlers.
- Legacy `/v1/authoritative/knowledge*` HTTP endpoints remain, including SQL-body writes.
- Old and new remote outboxes are separate and have not been consolidated or runtime-tested.
- All core embedded, remote and MCP parity tests, crash fault-injection, and workflow smoke tests are required.

## Acceptance rule
Do not mark M18 complete until the above legacy paths are removed/replaced, the integrated workspace compiles, executable workflows and recovery checks pass, and no known blocking loss of acknowledged knowledge exists. Flutter stays with Atomic App and is not part of the M18 acceptance gate.
