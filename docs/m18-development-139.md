# Orbit Development 139 — Acceptance content verification / M18 blocker report

## Priority assessment
M18's highest-priority unfinished work is a single Markdown-authoritative knowledge system, removal of SQL-body legacy routes, consolidation of OrbitRuntime/SqliteStore with AuthoritativeHost, and executable verification. These take precedence over more features or unrelated hardening.

## This revision
The `markdown_commands::accept` operation now requires the caller to supply freshly read Markdown bytes and refuses an accepted SQLite receipt if those bytes do not match the command's exact intended content. Both normal submission and startup-replay paths read by stable ID before acceptance. This closes the existing scenario in which identity discovery succeeded but the actual content was externally altered before SQLite acceptance. Added a Rust regression test for the guard.

## M18 NOT COMPLETE
This is an interim safety fix, not the promised unified cutover. The legacy SQL-backed `authority_knowledge_items`, old HTTP knowledge routes, and older local persistence remain; embedded, remote and MCP feature parity is incomplete. The check is still subject to external-writer TOCTOU without coordinated locking, and cannot prove crash consistency. Do not announce internal-testing readiness.

## Verification
Rust toolchain is not available in the execution environment. Therefore Cargo build and Rust tests were not run; source-only checks do not establish implementation correctness. Use `cargo test --workspace` and exercise actual filesystem crash-recovery on a host with Rust installed before marking accepted.
