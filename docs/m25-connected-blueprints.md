# Connected Blueprint Commands

The public `orbit blueprint` CLI now targets the authenticated authoritative server when Orbit is configured in connected mode. It does not mutate the local replica or fall back to a local SQLite authority.

All commands supported in local mode (`list`, `show`, `install`, `import`, `export`, `bind`, `binding`, `create-item`, `item`, `transition`, `set-properties`, `apply-template`) have an HTTP transport mapping to `/v1/authoritative/blueprints/*`. The server owns blueprint validation, state transitions, and template transactions.

Use `ORBIT_SYNC_HTTP_TOKEN` and `ORBIT_AUTHORITY_DEVICE` for authenticated requests. HTTPS is required except for loopback test servers. Manifest upload is limited to 1 MiB and work-item property uploads to 256 KiB.

Connected operation requires an online authority for immediate acceptance; durable offline blueprint intents are staged in the canonical outbox (see [offline blueprint mutations](m25-offline-blueprints.md)) and MCP management is documented separately. Server-side acceptance receipts are atomic, and offline replay/reconciliation is exercised by the blueprint smoke test and the connected MCP acceptance suite. Connected MCP blueprint writes use the same canonical queue-first path and receipt read-back (see [blueprint MCP management](m25-mcp-blueprint-management.md)); a lost acknowledgement is recovered by replaying the frozen identity, which returns the original receipt exactly once. Remaining gaps: capability actions are not reachable from the CLI/MCP/HTTP, and no blueprint upgrade path exists. Broad compilation and integration acceptance belong to M15.
