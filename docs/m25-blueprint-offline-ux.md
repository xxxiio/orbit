# Blueprint CLI: automatic authority submission and offline pending commands

Connected deployments use the same command spelling as local-authoritative installations:

```sh
orbit blueprint install development
orbit blueprint bind PROJECT-ID development 1
```

Mutating commands in connected mode first persist a frozen, uniquely identified request in the existing SQLite authority outbox. Orbit then attempts to flush the **global** outbox in order, so an earlier queued domain update cannot be bypassed. If the server is unavailable or its response is uncertain, the command remains pending with its identifier; it is **not** executed against the local replica. Run `orbit blueprint pending` to see pending work or `orbit blueprint flush` to retry. A blocked earlier command is reported as blocked, rather than represented as success.

For explicit control:

```sh
orbit blueprint --offline install development
orbit blueprint --require-online install development
```

`--offline` deliberately records the command without network access; `--require-online` uses the immediate HTTP path, returning failure without creating a new pending request when the authority is unreachable. Neither flag applies to read-only commands, and `--offline` is unavailable in fully local-authoritative mode.

The legacy `orbit blueprint queue` command was removed. Ordinary mutations automatically use the canonical outbox in connected mode.

**Acceptance evidence:** Server-side command receipts are persisted atomically with the actual blueprint mutation, and ordinary offline replay/reconciliation is exercised by the blueprint smoke test. Lost-acknowledgement replay is now covered: `orbit-mcp` unit tests simulate a lost response and a `pending`-to-`accepted` replay, and `tests/acceptance/mcp/test_m30_blueprint_outbox.py` proves against a real `orbit-server` that a command applied without a visible acknowledgement is replayed from the same identity, returns the original immutable receipt and applies exactly once. Cross-platform and broad end-to-end release testing belongs to M15.

As of Development 294, the deprecated `queue` syntax is removed from the CLI parser; use `--offline` on normal mutation commands.
