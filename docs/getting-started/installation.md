# Installing Orbit

Orbit is a local-first Rust application. **The default local CLI requires no Python worker, Herdr runtime, server, or cloud account.** The self-hosted server and agent execution are optional.

## Install from source

Requirements: a Rust toolchain compatible with the workspace's Rust 2024 edition, Cargo, and network access to retrieve crates (or an already populated Cargo cache). Git is not required to build or install Orbit from this source tree.

```sh
./scripts/install.sh
export PATH="$HOME/.local/bin:$PATH"
orbit --help
```

The installer runs `cargo build --locked --release` for the `orbit-cli` package, then installs the executable as `~/.local/bin/orbit`. It does not use `sudo`, alter shell startup files, or initialize a database on your behalf.

To build and install optional executables:

```sh
./scripts/install.sh --components cli,mcp,tui,server
```

For a custom user-owned prefix, supply `--prefix "$HOME/tools/orbit"`. Use `--debug` to install debug-build outputs. To inspect the operation without installing anything, use `--dry-run`.

## Install prebuilt binaries

Once you have binaries from a trusted build, no Rust toolchain is required on the target machine:

```sh
./scripts/install.sh --from-dir /path/to/prebuilt/bin --components cli
```

The expected source filenames are `orbit`, `orbit-mcp`, `orbit-tui`, and `orbit-server` (the workspace package is `orbit-cli`, but its executable is `orbit`); the CLI is installed as `orbit`. This script **does not download or verify upstream release signatures**. Verify the source of prebuilt binaries independently before installation.

For offline release bundles and checksum verification, see [binary distribution](../releasing/binary-distribution.md).

## Your first local workspace

```sh
export ORBIT_DB="$HOME/.orbit/first-project.db"
orbit status
orbit --help
orbit project --help
orbit blueprint --help
```

The default local database location is `$HOME/.orbit/orbit.db` when `ORBIT_DB` is not supplied. See [First project](first-project.md) for the project workflow. Do not point `ORBIT_DB` at an existing production database to experiment with an unverified build.

## Uninstall

```sh
./scripts/uninstall.sh
# or: ./scripts/uninstall.sh --prefix "$HOME/tools/orbit"
```

The uninstaller removes **only the binaries recorded by the installer manifest**. It intentionally leaves SQLite databases, configuration, and pending offline commands untouched. Back these up before manual deletion. The installer refuses to overwrite binaries it does not own.

## Connected deployments

The optional server is built with `--components server` and needs separate authentication and TLS configuration. Consult [Self-hosting](../guides/self-hosting.md) and `deploy/server/`. For cloud-connected clients, server SQLite is authoritative; clients cannot accept edits locally as canonical state.

## Acceptance status

These instructions document a source installer. Linux local/loopback/local-Herdr
M15 acceptance has passed (37/37 checks, 1235 Rust tests), including a real
release-archive build/verify/install smoke. Cross-platform installation
(macOS/Windows), TLS/self-hosted ingress, remote-machine agent execution,
artifact signing and real upgrade testing remain separate unverified gates.
This is not yet a published `cargo install orbit` package.

## Optional agent-machine worker

For Herdr execution on an enrolled machine, use `./scripts/install-worker.sh`; see [worker deployment](../guides/worker-deployment.md). Ordinary CLI users do not need this component or Python.

### Check an existing installation

Use `./scripts/install.sh --components cli --check` to verify the CLI is recorded and executable at the selected prefix. The installer checks all requested binaries before copying any of them and restores previously installed binaries if staging/commit fails. It does not remove user databases.

## Prepare and inspect a local installation

The installer installs executables but does not create the Orbit SQLite database.
You can prepare private XDG-style directories and inspect installed components without starting any services:

```sh
./scripts/orbit-env.sh --init-local
./scripts/orbit-env.sh --status
./scripts/orbit-env.sh --check
```

`--init-local` prepares `~/.local/share/orbit`, `~/.config/orbit`, and `~/.local/state/orbit` (or the corresponding `XDG_*_HOME` locations). These are *directory preparation only*; they do **not** change the CLI's default SQLite location (`~/.orbit/orbit.db`). Use `ORBIT_DB` explicitly when selecting a database path.

`--check` validates the installation manifest and installed executables. It does not call Herdr, Docker, or the network. For an alternate installation use `--prefix /path/to/prefix`; for isolated user directory preparation use `--data-home`, `--config-home`, and `--state-home`.

For an upgrade, run `./scripts/install.sh` with the same prefix and selected components; the installer stages the binaries and has a rollback path. Back up any existing Orbit SQLite databases before running a new version: binary rollback is **not** database schema rollback. A real version-to-version upgrade has not been verified; that remains an open gate outside the Linux local acceptance pass.

## Install a prebuilt release archive

Download a trusted platform-specific Orbit `.tar.gz` archive and the matching
`.sha256` file into the same directory, then run:

```bash
./scripts/install.sh --from-archive /path/to/orbit-VERSION-TARGET.tar.gz
```

All components in the archive are discovered automatically. Supply
`--components cli,mcp` to install a subset. Archive verification precedes
installation; a missing component or checksum failure aborts without
replacing installed binaries. The release bundle does not install Herdr,
Python, or a self-hosted server automatically.

## Diagnose a local installation

Run `./scripts/orbit-env.sh --doctor` before reporting installation failures. This read-only diagnostic checks the managed installation manifest, binary presence, PATH selection, the configured `ORBIT_DB` path, and whether optional Herdr, Docker and Python executables are discoverable. Optional executables are **not** required for local-only usage. The diagnostic does not execute Orbit or its optional runtimes, initialize SQLite, or start a server.

If PATH selects a different Orbit executable, inspect your shell PATH and prefer the known installation prefix before modifying anything. If the database parent does not exist, create it explicitly or choose `ORBIT_DB` in an existing writable directory. Back up existing database content before upgrades; `--doctor` cannot establish SQLite integrity or migration compatibility.
