# Choosing Orbit installation components

Orbit is local-first. Start with the CLI; add components only for workflows you
need. A normal local installation requires no server, Python, Herdr or Docker.

| Component | Selection | When to use | Additional requirements |
| --- | --- | --- | --- |
| Main CLI | `cli` (default) | Projects, tasks, notes, planning and agent job management | None beyond the platform binary |
| MCP adapter | `mcp` | Let an MCP client interact with Orbit | An MCP client and explicit permissions |
| Terminal UI | `tui` | Interactive terminal navigation | A compatible terminal |
| Self-hosted server | `server` | Connected authoritative mode | Persistent storage, credentials and TLS ingress |
| Machine worker | Separate `scripts/install-worker.sh` | Execute assigned Herdr jobs | Python and a configured Herdr runtime |

## Install the CLI

The public installer installs only the CLI by default:

```sh
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh
```

From a source checkout, install selected Rust components:

```sh
./scripts/install.sh
./scripts/install.sh --components cli,mcp,tui,server
```

Source builds require Cargo. Prebuilt archive installs do not. To install a
subset from a verified archive:

```sh
./scripts/install.sh --from-archive orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  --components cli,mcp
```

## Update the CLI

```sh
orbit update --check
orbit update
```

`orbit update` targets the installed CLI, verifies checksums, backs up the
database and preserves the prior installation on failure. It does not install
Herdr, Python, a worker or a server. Reinstall optional components with the
installer when you need a matching version.

## Connected deployments

Use `scripts/setup-server.sh` for optional self-hosting, then follow
[self-hosting](self-hosting.md) for TLS and credentials. Installing the server
does not enroll devices, start Herdr or configure a service. Install machine
workers separately and keep their journals distinct from the client outbox.

## Uninstall and data retention

`scripts/uninstall.sh --prefix PREFIX` removes installer-owned binaries and
preserves databases, configuration, the Markdown vault and worker journals. It
validates the installation manifest before removing executables. Back up
authoritative data before unrelated maintenance.

## Verification boundary

Packaging and installer checks exist in the repository. Only Linux x86_64 local,
loopback-connected and local-Herdr workflows are runtime-verified here. The
optional MCP, TUI and server binaries are packaged but not all are
launch-smoked. macOS is not runtime-verified; Windows is unsupported.

See [installation](../getting-started/installation.md),
[platforms and components](../releasing/platforms-and-components.md), and
[binary distribution](../releasing/binary-distribution.md).
