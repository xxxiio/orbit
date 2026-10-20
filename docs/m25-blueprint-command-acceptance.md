# Blueprint command acceptance (M25)

Connected clients stage blueprint mutations in the canonical SQLite outbox. The
client sends `{command_id, route, body}` to
`POST /v1/authoritative/blueprints/commands`, authenticated as the enrolled
device. The server derives the device identity from authentication, and writes
both the mutation and its immutable receipt under one SQLite savepoint.

A byte-for-byte equivalent serialized payload with the same command ID and
device returns the original result on replay. A different payload, route, or
device conflicts. Invalid mutations roll back without leaving a receipt.
Clients verify the returned command ID before acknowledging their outbox row.
The original blueprint endpoints remain for direct online CLI/MCP operations;
the new endpoint is the canonical path for offline replay.

External side effects do not occur in these handlers. Blueprint actions need
separate explicit authorization and durable execution fencing.

## M15 acceptance boundary

M15 must compile the Rust workspace, exercise replay after lost acknowledgments
against the real authenticated server, and check rollback, concurrency, and
restart durability. The feature-level source tests added here are not a claim
that these end-to-end checks have run.
