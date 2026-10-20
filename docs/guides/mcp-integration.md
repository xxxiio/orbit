# Using Orbit with an MCP agent

Orbit includes an optional MCP executable, `orbit-mcp`. Install it with `./scripts/install.sh --components cli,mcp` or install prebuilt binaries with `--from-dir`. The primary human CLI remains `orbit`; `orbit-mcp` is an integration process, not a second user CLI.

## Set up

1. Confirm the CLI works: `orbit --help`.
2. Confirm the MCP executable is installed: `command -v orbit-mcp`.
3. Read `docs/reference/mcp.md` for the supported tool inventory and any required connection configuration.
4. Configure your MCP client to start the **installed** `orbit-mcp` executable. Use the client's normal environment and credential configuration; never place server tokens in committed JSON files.
5. Start with read-only project discovery, then request mutation privileges only for workflows that need them.

## Example workflows

**Plan development:** retrieve project and task context, inspect prerequisites, prepare a handoff, then use `orbit agent start` when human-approved execution is appropriate.

**Customize a workflow:** list blueprints, inspect a pinned blueprint version, create work items with validated properties, and request a permitted transition. A blueprint declaration does not itself grant capability execution rights.

**Review work:** inspect persisted agent handoffs and evidence, review the changes, and separately complete the underlying task after acceptance.

## Boundaries

In local-authoritative mode, MCP uses the local authority. In connected mode, mutations go through the authenticated server and must not write directly to replicas. Offline requests may remain pending until accepted by the authority. Installing `orbit-mcp` does not install Herdr or launch a coding agent.

For the precise tool names and arguments in your installed build, inspect the MCP tool discovery response rather than copying historical milestone examples.
