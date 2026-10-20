# Orbit 110 — canonical approved-proposal executor

The optional `deploy/server/agent-worker.py` is now a pure HTTP adapter. It claims an admin-approved proposal with an enrolled device token, reads the authoritative snapshot revision, converts allowlisted intents to canonical task or evidence command envelopes, and POSTs through the same server methods as REST/MCP. It then settles the proposal. It never opens the local-only SQLite database or invokes `orbit-cli`.

Supported intentions: `task-create` (target is canonical project ID), `task-update` (target task ID; status strings use canonical underscore form), `habit-record` (target habit ID; `at_ms`), and `metric-observe` (target metric ID; `at_ms`, integer `value`). Stable proposal-derived command IDs and occurrence/task IDs prevent accidental use of random IDs; the server remains responsible for validation.

**Limitations / acceptance obligations**: proposal settlement and domain mutation currently are not one atomic transaction. If a worker commits but crashes before settlement, the proposal remains leased. Expired leases are not reclaimed automatically. The server idempotency receipt matches the entire submitted command including `expected_revision`; recreating the command with a later revision will not count as the same retry. Inspect server receipts/history and proposal audit before manual recovery. No claim of exactly-once execution or automatic recovery is made. Native Rust compilation and live server acceptance are pending.

Example: `ORBIT_SYNC_HTTP_TOKEN=<enrolled-worker-token> python3 deploy/server/agent-worker.py --server https://orbit.example.com --device agent-worker-1 --once`.
