# M21 Development 207 — Reconnection orchestration

Implemented an explicit online reconnect operation through the Rust replica, connected client,
unified `AuthorityConnection`, and `orbit sync replica reconnect` CLI command.

The operation replays pending global commands in order, refreshes the accepted snapshot, then
attempts a full Markdown mirror refresh. On a domain conflict it still refreshes accepted state
so users can inspect server changes before explicitly replacing a rejected command. The
Markdown mirror refresh is independent: a failure preserves the previous mirror and is included
in the reconnect report instead of being silently presented as successful. A failed authoritative
snapshot refresh remains an error; it is never represented as a successful reconnect.

The returned report separates:
- revision before and after the accepted snapshot refresh
- the count of server-accepted commands
- the number of unresolved local commands remaining
- the first blocked command and reason, if any
- the newly downloaded Markdown count, or its mirror failure reason

The local SQLite outbox and its stable command IDs are preserved. Pending operations are never
projected into the accepted snapshot. Domain command execution on the server remains sequential,
not transactional across an entire workflow.

## CLI

`orbit sync replica --db ./replica.sqlite --server http://localhost:3000 --device DEVICE reconnect`

Provide the server bearer token in `ORBIT_SYNC_HTTP_TOKEN`.

## Scope and validation

This is a source-level change; compilation and broad release acceptance are deferred to M15.
It does not establish comprehensive two-client convergence or mark M21 complete. The existing
source-only implementation progress estimate should not increase without those acceptance gates.
