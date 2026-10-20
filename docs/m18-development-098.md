# M18 development 098 — Native Rust replica CLI

## Highest-priority selection
M18's native connected client is not yet available in everyday CLI workflows. Before M15 release validation or further backend hardening, expose the already implemented accepted-snapshot cache and pending-command outbox via the ordinary CLI. No higher-priority unfinished M18 category is displaced.

## Implementation
- `orbit-cli sync replica` now uses `orbit-client::authority` directly, with a dedicated SQLite cache/outbox database. The local-only Orbit database and legacy Git history remain untouched.
- `snapshot`, `status`, `queue`, `refresh`, and `flush` are supported; network operations use `ORBIT_SYNC_HTTP_TOKEN`. Offline reads and queued writes do not require a token.
- Project command envelopes are flat as required by the authoritative server API. Task/evidence commands use nested `action` envelopes. A focused regression covers both.
- Pending commands are not confused with server-accepted snapshots. Revision conflicts stop command replay, and the retry envelope retains its initial expected revision.

## Example
```sh
orbit-cli --json sync replica --db ~/.local/share/orbit/authority.sqlite --server https://orbit.example.com status
orbit-cli --json sync replica --db ~/.local/share/orbit/authority.sqlite --server https://orbit.example.com queue --command-id create-1 --kind project --action-json '{"project_id":"p1","name":"My Project"}'
ORBIT_SYNC_HTTP_TOKEN=... orbit-cli --json sync replica --db ~/.local/share/orbit/authority.sqlite --server https://orbit.example.com flush
ORBIT_SYNC_HTTP_TOKEN=... orbit-cli --json sync replica --db ~/.local/share/orbit/authority.sqlite --server https://orbit.example.com refresh
```

## Unfinished work
This is CLI integration, not general offline editing in the TUI/Flutter clients. The replica still lacks automated conflict rebase and a server-led migration/import path; browser/agent canonical cutover and built-in Git retirement remain pending. Compilation, Clippy and live integration are unverified in this environment and remain M15 checks.

## Progress
M18 source implementation estimate: 86% (from 83%); all other milestone estimates unchanged. Project is **not ready for internal acceptance** without a compiled development build and core smoke workflows.
