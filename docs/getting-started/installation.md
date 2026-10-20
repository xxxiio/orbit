# Installing Orbit

Orbit ships as a prebuilt CLI for Linux and macOS targets. Installing a
prebuilt binary does **not** require Rust, Python, Docker, Herdr or a server.
The optional self-hosted server and machine workers are separate choices.

## One-command install (recommended)

```sh
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh
```

The public entry point is a release asset (`install.sh`), so it stays on
`github.com` and does not depend on `raw.githubusercontent.com`. The installer
fetches its transactional core and verifier from the same release namespace.

The root `install.sh`:

1. Detects Linux or macOS and `x86_64`/`aarch64`.
2. Resolves the latest stable GitHub release unless you pin a version.
3. Downloads `orbit-VERSION-TARGET.tar.gz` and its `.sha256` file.
4. Rejects a checksum file that does not name the expected archive and verifies
   the SHA-256 digest before extracting anything.
5. Reuses the transactional M26 installer to install **only the `orbit` CLI**
   into `~/.local/bin`.
6. Leaves an existing installation untouched if verification or installation
   fails.

It does not use `sudo`, edit shell startup files, install a server, or run
`orbit init`. Checksums fetched from the same release are an integrity check,
**not proof of publisher authenticity**. Obtain releases through a channel you
trust.

Expected output ends with a line similar to:

```text
Checksum verified. Note: same-origin checksums catch corruption in transit but are not publisher authenticity.
Orbit 0.1.0 installed into /home/you/.local/bin
```

The one-command path depends on a published release asset. If no public release
is available yet, use [the checkout install](#install-from-this-checkout-before-a-release)
or [the local archive flow](#source-and-archive-alternatives).
The GitHub workflow creates a **draft** release on a tag push; publication
requires manual authorization, so the workflow's existence does not imply a
public release today.

## Install from this checkout before a release

Use this path while developing Orbit or before a release asset exists. It uses
the repository's transactional installer directly; it does **not** need a
GitHub Release, and it does not fetch the release-asset `install.sh`.

Requirements: Rust 1.99 with Cargo. Python is not required for the CLI.

```sh
git clone https://github.com/xxxiio/orbit.git
cd orbit
./scripts/install.sh --prefix "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"

orbit --version
orbit init
orbit doctor
```

If you already have a checkout, run the same `./scripts/install.sh` command from
its root. Use `--dry-run` to inspect the plan, `--debug` for a debug build, or
`--components cli,mcp,tui,server` to install optional binaries. The installer
records what it owns and refuses to overwrite an unmanaged binary.

To test a built release archive locally before publishing:

```sh
./scripts/package-release.sh --build --components cli --output ./dist
./scripts/install.sh \
  --from-archive ./dist/orbit-0.1.0-<target>.tar.gz \
  --prefix "$HOME/.local"
```

Replace `<target>` with the host triple printed by `rustc -vV` (for example
`x86_64-unknown-linux-gnu`). The archive path verifies checksums and internal
contents before replacing binaries.

To run the development binary without installing:

```sh
cargo build -p orbit-cli --locked
./target/debug/orbit --db /tmp/orbit-dev.db init
./target/debug/orbit --db /tmp/orbit-dev.db doctor
```

Use `--db` or `ORBIT_DB` for a disposable workspace so development checks do not
touch your normal `~/.orbit/orbit.db`. Uninstall a checkout installation with:

```sh
./scripts/uninstall.sh --prefix "$HOME/.local"
```

## Pin a version

Install a specific release with `--version`. Both `v0.1.0` and `0.1.0` are
accepted.

```sh
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh \
  | sh -s -- --version v0.1.0
```

An installer asset is also attached to each tagged release, for example
`https://github.com/xxxiio/orbit/releases/download/v0.1.0/install.sh`. Use the
`latest/download` URL unless you specifically need that tag's installer.

Prerelease-looking versions are installed only when explicitly requested. The
default path refuses a release that is marked draft or prerelease.

## Choose a prefix

```sh
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh \
  | sh -s -- --prefix "$HOME/tools/orbit"
```

The prefix must be an absolute path or is resolved against the current
directory. The CLI is installed at `<prefix>/bin/orbit`. The installer refuses
to install into `/`.

Inspect resolution without downloading or changing anything:

```sh
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh \
  | sh -s -- --version v0.1.0 --dry-run
```

## Save and inspect the script first

Security-conscious environments can download the script, inspect it, and run
it locally:

```sh
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh -o install.sh
sh ./install.sh --version v0.1.0
```

`sh ./install.sh --help` lists the supported options. Environment variables
are equivalent to flags where noted:

| Variable | Meaning |
| --- | --- |
| `ORBIT_VERSION` | Same as `--version` |
| `ORBIT_INSTALL_PREFIX` | Same as `--prefix` |
| `ORBIT_TARGET` | Override target detection; intended for installer tests and unsupported hosts, not normal use |

Supported declared targets are `x86_64-unknown-linux-gnu`,
`aarch64-unknown-linux-gnu`, `x86_64-apple-darwin` and
`aarch64-apple-darwin`. Windows is unsupported. Only Linux x86_64 is
runtime-verified in this repository; macOS archives are declared but **not
runtime-verified**.

## Put `orbit` on PATH

The default prefix is `~/.local`, so the binary is `~/.local/bin/orbit`. Add it
to your shell configuration yourself; the installer deliberately does not:

```sh
export PATH="$HOME/.local/bin:$PATH"
orbit --version
```

Check which binary your shell resolves:

```sh
command -v orbit
orbit doctor
```

If `doctor` reports that `orbit` is not on PATH, the installation is still
valid; use the absolute path or fix `PATH`.

## Initialize after install

```sh
orbit init
orbit doctor
```

`orbit init` creates the home layout, configuration and database. It is
idempotent. See [initialization](initialization.md) for the layout, config
schema and precedence rules.

## Update an installed CLI

The supported update command is:

```sh
orbit update
```

Options:

| Option | Effect |
| --- | --- |
| `--check` | Report whether an update is available; never mutate anything |
| `--version VERSION` | Install a specific version |
| `--channel stable\|prerelease` | Select the channel; stable is the default |
| `--prefix DIR` | Update an installation at a non-default prefix |
| `--allow-downgrade` | Permit installing an older version |
| `--force` | Reinstall even when the target version is already installed |

`orbit update` verifies release checksums before replacing the binary, refuses
an installation that lacks the M26 installation manifest, backs up the SQLite
database under `~/.orbit/backups/` before replacement, and preserves the prior
binary and data if the update fails. Downgrades are refused unless explicitly
allowed. `--check` is read-only.

Before updating, read [upgrades and recovery](../releasing/upgrades-and-recovery.md).
Binary rollback is not database schema rollback.

## Uninstall

The curl installer has no uninstall flag. From a source checkout, use the
same prefix you installed with:

```sh
./scripts/uninstall.sh --prefix "$HOME/.local"
```

The uninstaller removes only binaries recorded in the managed installation
manifest. It preserves databases, configuration, the Markdown vault and
pending work. If you no longer have a checkout, remove the installed binary
and manifest manually after backing up `~/.orbit/`:

```sh
rm -f "$HOME/.local/bin/orbit"
rm -f "$HOME/.local/share/orbit/installed-binaries.txt"
```

Do not delete `~/.orbit/` unless you intend to discard all local data.

## Source and archive alternatives

For maintainers or systems without a published release channel:

```sh
# Build and install from a source checkout; requires Cargo.
./scripts/install.sh

# Install additional Rust components from source.
./scripts/install.sh --components cli,mcp,tui,server
```

For a verified offline archive, no Rust toolchain is required:

```sh
./scripts/install.sh --from-archive /path/to/orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

The archive flow verifies the companion checksum and internal `SHA256SUMS`
before installing. For a directory of prebuilt binaries that does not use the
archive `bin/` layout:

```sh
./scripts/install.sh --from-dir /path/to/bin --components cli,mcp
```

These alternatives install binaries only. They do not start a server, install
Herdr or Python, enroll a machine, or initialize a database.

## Common installation failures

- **`orbit: command not found`** — add the prefix's `bin` directory to `PATH`.
- **Checksum failure** — download both the archive and `.sha256` again; do not
  bypass verification.
- **Unsupported target** — only the four declared targets are supported.
- **Refusing to overwrite** — the installer only manages its own manifest;
  remove or relocate an unrelated binary explicitly.
- **`doctor` reports a different PATH binary** — adjust `PATH` so the intended
  installation is first.

Next: [initialize and check](initialization.md) or go straight to the
[quickstart](quickstart.md).
