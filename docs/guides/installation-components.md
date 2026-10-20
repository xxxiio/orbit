# Choosing Orbit installation components

Orbit is local-first. Start with the CLI; add components only for workflows you need.

| Component | Installer selection | When to use | Additional requirements |
| --- | --- | --- | --- |
| Main CLI | `cli` (default) | Projects, blueprints, planning and agent job management | None at runtime beyond the platform's binary dependencies |
| MCP adapter | `mcp` | Allow MCP clients to interact with Orbit authority | An MCP client with appropriate permissions |
| Terminal UI | `tui` | Interactive terminal navigation | Compatible terminal |
| Self-hosted server | `server` | Connected authoritative mode | Deployment configuration, persistent storage and secured network access |
| Machine worker | Separate `scripts/install-worker.sh` | Execute assigned Herdr jobs on a machine | Python and independently configured Herdr/runtime |

## Install from source

From the source root, run `./scripts/install.sh` for the CLI, or `./scripts/install.sh --components cli,mcp,tui` for additional Rust binaries. Source builds require Cargo. The installer never downloads a binary on its own.

For an offline target without Rust, build elsewhere and copy the appropriate binaries into a directory, then use `./scripts/install.sh --from-dir ./release-bin --components cli,mcp`. Confirm with `./scripts/install.sh --components cli,mcp --check`.

## Connected deployments

Use `scripts/setup-server.sh` for optional self-hosting, then consult [self-hosting](self-hosting.md) for reverse-proxy TLS and credential setup. Installing the server does not enroll machines or start Herdr. Install the worker separately and keep its runtime journal distinct from Orbit's client command outbox.

## Uninstall and data retention

`scripts/uninstall.sh` removes installer-owned binaries and preserves databases, user configuration and worker journals. It validates the entire install manifest before removing executables. Back up any user-managed database before unrelated maintenance or upgrades.

## Verification boundary

The installer has fixture-level transactional checks. Real release binaries, operating-system deployment and live agent integrations are validated in M15; do not infer production readiness from a shell check alone.
