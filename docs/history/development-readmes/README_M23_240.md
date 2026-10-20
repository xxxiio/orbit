> **Historical checkpoint only.** Not current CLI or deployment instructions. See the [root README](../../../README.md) and [current architecture](../../architecture.md).

# Development 240 — unified agent CLI and public-surface audit

All local agent operator commands now use the existing `orbit` binary and its clap parser, `--db`/`--json` flags, error handling, and fail-closed connected-mode guard.

Examples:

```sh
orbit --db ./orbit.sqlite agent status JOB_ID
orbit --db ./orbit.sqlite agent attention
orbit --db ./orbit.sqlite agent launch JOB_ID --revision 1 --claim-token TOKEN --at-ms 1792300000000
orbit --db ./orbit.sqlite agent inspect JOB_ID --revision 2 --at-ms 1792300000000
```

Removed the separate `orbit-agent-executor` binary, which bypassed canonical CLI conventions. Runtime execution remains an internal service via `orbit-runtime::agent_dispatch` and `HerdrRuntime`. **Do not invoke `agent launch` on a connected replica:** the existing CLI deployment guard rejects it. Connected executor leases/remote agent commands are still outstanding; the CLI does not claim to provide them. Preparation and reservation still require prior authority operations; no one-click `agent start` is claimed.

## Related findings

- Legacy M23_238/M23_239 notes documented the standalone binary; these are superseded by this document, retained as historical records.
- Server `/v1/authoritative/agent-jobs/...` endpoints are transport-specific and correctly internal to HTTP; they should not become end-user command names.
- `AgentRuntime`, `HerdrRuntime`, dispatch and reconciliation modules are infrastructure ports/adapters, not alternate user CLIs.
- No public `orbit agent start`, `resume`, `review` or MCP equivalents exist yet. Do not document them as working commands.
- The CLI currently constructs a second SqliteStore next to AuthoritativeHost; this is local-only and must be refactored into host methods before connected-mode command acceptance.

## Validation
Static source/audit checks only; Rust toolchain not available in this container. Actual `cargo test -p orbit-cli` and smoke calls remain required.

## Milestone progress
M23: 26% (unchanged; this is API correction, not new acceptance scope). M15 executable acceptance pending; M22 source scope 100%. Internal testing readiness not yet established.
