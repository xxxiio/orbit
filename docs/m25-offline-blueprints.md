# Offline blueprint mutations (canonical outbox)

In connected mode, regular `orbit blueprint install`, `import`, `bind`, `create-item`, `transition`, `set-properties` and `apply-template` operations persist a durable request in the canonical SQLite authority outbox before attempting server acceptance. If the server is unavailable, the request remains pending.

`orbit blueprint --offline install development` explicitly stores a request without contacting the server. `orbit blueprint --require-online install development` requires immediate authority acceptance.

`orbit blueprint pending` inspects waiting commands and `orbit blueprint flush` replays the **global** canonical outbox in order (not only blueprint commands). There is no separate `queue` subcommand.

External plugin effects are never launched directly by offline blueprint mutations. Server-side acceptance receipts are atomic, and normal offline replay/reconciliation is implemented and exercised by the blueprint smoke test. Connected MCP blueprint writes use this same queue-first path and are covered by the MCP acceptance suite, including lost-acknowledgement replay. Remaining gaps: capability action request/execute surfaces are unreachable from the CLI/MCP/HTTP, and there is no blueprint upgrade path.
