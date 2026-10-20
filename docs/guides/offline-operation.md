# Local authority and connected offline operation

## Local-only mode

Local SQLite is authoritative; no server or synchronization service is necessary. Set `ORBIT_DB` or use the default under `~/.orbit/`. Do not manually edit SQLite rows.

## Connected mode

The self-hosted server's SQLite is the canonical authority. Client SQLite is an offline-capable accepted-state replica and a durable pending-command store. Configure the server endpoint and replica DB together (see root README). Do not mix local and connected databases under one path.

A client may enqueue an offline-capable blueprint or domain mutation while disconnected. The request preserves its command identity and remains pending until the server accepts or rejects it. On reconnect, Orbit submits pending commands in order. A lost acknowledgment is not permission to generate a new command identity.

Use `orbit blueprint pending` to examine outstanding blueprint operations and `orbit blueprint flush` to attempt synchronization. Other domain operations may have separate `orbit sync` commands: check `orbit sync --help` for the installed version.

## External effects

Agent launch requests, continuation messages and other external actions are never executed locally by a disconnected connected-mode client. The server authorizes them and machine workers fence execution. Treat unreachable and uncertain states as requiring reconciliation, not automatic retry.
