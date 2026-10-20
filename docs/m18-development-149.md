# Orbit 149: Host-owned routine execution

## Scope
Moved the complete CLI routine command family (create, list, add habit, remove habit) from the mutable legacy SQLite bridge to the same host-owned application service/query ports used by other local clients.

## Evidence
- The newly added CLI routine host-boundary assertions pass.
- Existing application-services, application-queries, and CLI task/project source guards pass.
- Offline synchronization Python suite: 17 tests passed.
- Rust compilation and runtime integration: not tested (cargo/rustc unavailable).

## Remaining M18 requirements
- Remove the CLI mutable-store bridge for schedule, knowledge, search, status, today and other specialized handlers.
- Consolidate command validation, revision and receipt semantics; eliminate remaining relay-era machinery.
- Prove local/remote behavior by compiling and running executable Rust workflows.

M18 remains incomplete. M19 (Markdown knowledge/folders), M20 (transport parity), M21 (remote offline), and M22 (integrated acceptance) retain separate milestone boundaries. Flutter remains in Atomic App.
