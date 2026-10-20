# Orbit 101 — Explicit server-authoritative TUI mode

Orbit TUI now has two **mutually exclusive** entry paths. Without `ORBIT_AUTHORITY_DB`, it preserves the existing fully local SQLite application and controls. When `ORBIT_AUTHORITY_DB` is set, it starts the native authority-replica interface and **does not open the local-only OrbitClient or Git transport**. This is an intentional breaking-mode boundary; there is no implicit mixing of local and server canonical state.

Connected mode reads six-family server-accepted cached snapshots, renders projects/tasks/habits/metrics, shows the last accepted revision and unresolved queue count, and supports offline task creation through the native Rust outbox. Keys: `c` create/queue task, `f` flush queued intentions, `r` refresh accepted snapshot, `q` quit. Conflicts stop submission and are handled explicitly with the `orbit-cli sync replica pending` and `replace-conflict` workflow. The TUI does not claim that queued commands are accepted.

```bash
ORBIT_AUTHORITY_DB="$HOME/.local/share/orbit/authority.sqlite" \
ORBIT_AUTHORITY_SERVER="https://orbit.example.com" \
ORBIT_AUTHORITY_TOKEN="<device bearer token>" orbit-tui
```

For **offline-only cached use**, omit `ORBIT_AUTHORITY_TOKEN`; specify the same server URL to retain replica isolation. Connected mode is currently a deliberately minimal native TUI. Knowledge, schedule, rich planning/agent actions, and habit/metric mutations have **not** been cut over to server-authoritative operations. They remain available in local-only mode and cannot be safely invoked against disconnected local state in connected mode. No Flutter binding is claimed.

Testing: added a Rust test asserting queued TUI tasks are not reflected in accepted snapshots. Full Rust build, native TUI interaction and end-to-end two-device smoke require an environment with Cargo; unavailable in this sandbox. Source-only validation is not runtime acceptance.
