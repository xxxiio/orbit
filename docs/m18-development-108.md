# Orbit 108 — clean-break Rust workspace

## Scope
- Remove `crates/orbit-git` entirely and the workspace/dependency references.
- Replace local-only TUI's obsolete Sync + Agent tab with read-only Agent inspection.
- Remove Git artifact conflict resolution, materialization and outbox-export code from the TUI.
- Preserve two authoritative modes: local SQLite or optional server SQLite with native offline replica.
- Preserve `orbit-sync` semantic event and revision primitives used by canonical server history.
- No compatibility, import, data migration or Flutter deliverables.

## Verification
Source scanning, TOML parsing, Python syntax and archive integrity can run in the source environment. Cargo build and live two-client smoke still require a Rust toolchain.

## Remaining cleanup
Legacy relay-named private server internals and unused semantic-outbox repository methods can be simplified later, after a successful Rust build confirms the required authoritative history paths. Canonical agent/automation write access and multi-client runtime acceptance remain necessary before M18 can close.
