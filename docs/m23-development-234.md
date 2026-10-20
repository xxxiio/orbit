# Development 234 — M23 dependency-aware launch gate

## Implementation
- Added an authority-schema-aware prerequisite gate to SQLite agent reservation, accepted prepare commands, and exclusive launch claims.
- The gate checks dependency status atomically within the same write transaction as each job mutation and rejects dangling/unfinished prerequisites.
- Added a regression test covering a prerequisite becoming incomplete between reservation, prepare and launch.
- Preserved an optional schema check for independent `SqliteStore` tests without the `AuthorityStore` table; production integration must require an authoritative schema.

## Validation and limits
- Rust compilation/tests not executed because `cargo` and `rustc` are unavailable.
- Connected-mode accepted-command routes, authenticated launch actors, task revision tokens, and a Herdr adapter are still not implemented. This is NOT completed I1 acceptance.
- Provisional milestone progress held at 21%, because source-only changes without compilation do not warrant an additional verified >=5 percentage points. I1 remains open.
