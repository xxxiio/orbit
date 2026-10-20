# Using Orbit with an MCP agent

Orbit includes an optional MCP executable, `orbit-mcp`. The human CLI remains
`orbit`; `orbit-mcp` is an integration process that speaks JSON-RPC over stdio.
Installing it does not install Herdr, Python or a server.

## 1. Install and verify

```sh
./scripts/install.sh --components cli,mcp
command -v orbit-mcp
orbit --help
```

For a prebuilt installation, select the `mcp` component from a verified archive
or directory. The binary is normally installed next to `orbit` in the same
`bin` directory.

## 2. Configure the client

Use the **absolute installed path** and put non-secret settings in the client
configuration. Keep tokens out of committed files.

```json
{
  "mcpServers": {
    "orbit": {
      "command": "/home/you/.local/bin/orbit-mcp",
      "args": [],
      "env": {
        "ORBIT_DB": "/home/you/.orbit/orbit.db",
        "ORBIT_MCP_MODE": "read_only",
        "ORBIT_MCP_PROJECTS": "project-1792480192043-1054584-0",
        "ORBIT_MCP_AGENT_ID": "my-editor-agent",
        "ORBIT_MCP_SESSION_ID": "default"
      }
    }
  }
}
```

In connected mode, add `ORBIT_DEVICE_ID` and let your client's secret facility
provide `ORBIT_DEVICE_TOKEN`. Do not put a real token in this JSON, shell
history, a repository, or a bug report.

| Variable | Meaning |
| --- | --- |
| `ORBIT_MCP_MODE` | `read_only` (default) or `developer` |
| `ORBIT_MCP_PROJECTS` | Comma-separated allowed project IDs; empty means project selection is unrestricted |
| `ORBIT_MCP_AGENT_ID` | Agent identity reported to Orbit (default `external-agent`) |
| `ORBIT_MCP_SESSION_ID` | Session identity (default `default`) |
| `ORBIT_DEVICE_ID` | Connected device identity |
| `ORBIT_DEVICE_TOKEN` | Connected device secret; never commit it |
| `ORBIT_MCP_ALLOW_UNSCOPED_EXECUTION` | Explicit exception for unscoped external execution |
| `ORBIT_MCP_ALLOW_UNSCOPED_KNOWLEDGE` | Explicit exception for unscoped knowledge access |

Grant least privilege. Start read-only, then enable `developer` only for the
specific client and session that needs writes.

## 3. Discover tools

Always call `tools/list` after connecting. Names and JSON schemas vary by build
and by mode. The [MCP reference](../reference/mcp.md) lists explicit
registrations, but dynamically registered capability tools must still be
discovered.

A client session typically sends:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"my-client","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
```

Some clients perform initialization automatically. Do not assume a tool exists
because an older document names it; check the negotiated inventory.

## 4. Authorization requirements

- **Read-only mode** exposes inspection and list operations. Mutations are
  denied.
- **Developer mode** permits scoped writes. Project restrictions still apply.
- Connected writes go through the authenticated authority; a replica is not
  canonical and pending commands are not accepted.
- Do not write directly to a replica SQLite file or bypass the MCP server.
- External execution flags are explicit exceptions, not defaults.

A denial can mean wrong mode, project restriction, stale revision, offline
state or insufficient authority. Read the tool error and inspect effective mode
before retrying.

## 5. Blueprint management

The MCP surface can inspect and manage published blueprints, subject to mode:

- list installed versions and show a definition;
- bind a project to an immutable version;
- create a typed work item;
- read an item and update properties;
- transition an item along a permitted edge;
- install a bundled blueprint or publish a manifest;
- apply a template.

A manifest cannot self-authorize capabilities. Declaring an action in a
blueprint does not grant shell, agent or network access. Capability actions are
only partially implemented in this build and have no executable CLI/MCP surface
for arbitrary providers.

Blueprint operations are user-directed mutations. Review IDs, versions and
revisions before writing, just as you would with the CLI.

## 6. Extension and trusted-provider boundaries

- Only trusted, separately registered providers may implement capabilities.
- Installing a blueprint does not install code or authorize execution.
- Capability actions remain bounded by the runtime's authorization model.
- Do not expose MCP to untrusted local processes or share a database with them.
- In connected mode, do not share device tokens or replica files across
  unrelated agents.

See [capability and provider authoring](../developers/extensions-and-capabilities.md)
and the [trusted provider example](../developers/capability-provider-example.md).

## 7. Custom adapters

If you build a custom MCP adapter:

1. Reuse the Orbit MCP protocol and authorization model; do not add a second
   writable database path.
2. Discover tools and validate schemas defensively.
3. Keep credentials in the host environment or a secret manager.
4. Surface denials and pending states accurately; do not translate "pending"
   into "accepted".
5. Test against a disposable workspace and both read-only and developer modes.

## 8. Testing an integration

1. Confirm the CLI and MCP binary run and report versions.
2. Connect with `read_only` and verify `tools/list` works.
3. Read a project and task; confirm IDs match the CLI.
4. Attempt a mutation in `read_only` and confirm it is denied.
5. Switch to `developer` for a disposable workspace and perform one scoped
   create or update.
6. In connected mode, verify pending commands are visible but not accepted until
   flush/reconnect succeeds.
7. Disconnect and confirm the client reports failure rather than inventing
   success.

## Example workflows

**Plan development:** retrieve project and task context, inspect prerequisites,
prepare a handoff, then use `orbit agent start` only after human approval.

**Customize a workflow:** list blueprints, inspect a pinned version, create
items with validated properties, and request a permitted transition. A
blueprint declaration does not grant capability execution rights.

**Review work:** inspect persisted handoffs and evidence, review the changes,
and separately complete the underlying task after acceptance.

For exact schemas, use tool discovery. See [MCP tools reference](../reference/mcp.md),
[offline operation](offline-operation.md), and
[troubleshooting](troubleshooting.md).
