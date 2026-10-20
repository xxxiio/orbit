# Supported targets and component requirements

This page describes packaging and support interfaces, not a claim that every
target passed runtime acceptance. A target label supplied to
`package-release.sh --from-dir` is not independently checked against executable
architecture.

## Components

| Component | Installed command | Built package | External requirements |
| --- | --- | --- | --- |
| `cli` (default) | `orbit` | `orbit-cli` | No server, Python or Herdr for local SQLite mode |
| `mcp` | `orbit-mcp` | `orbit-mcp` | An MCP host/client; server only for connected mode |
| `tui` | `orbit-tui` | `orbit-tui` | Interactive terminal |
| `server` | `orbit-server` | `orbit-server` | Persistent server SQLite and deployment configuration |

The optional Python machine worker is **not** in these Rust archives. Install it
separately with `scripts/install-worker.sh`; Python and Herdr are required only
for that execution mode. Docker is optional for self-hosting and unnecessary
for local-only use.

## Targets

`package-release.sh --build` uses the local Rust host target unless given
`--target`. Cross-compilation requires the correct target, linker, native
libraries and deployment testing; the packaging script does not set these up.
The bundle name contains the target triple, but its contents are **not architecture-probed**. The `--from-dir` mode trusts the supplier's asserted
target.

| Environment | Packaging workflow | Runtime status |
| --- | --- | --- |
| Linux x86_64 (native Rust target) | Source build or prebuilt archive | Only runtime-verified target: local, loopback connected and local-Herdr workflows |
| Linux aarch64 (native or cross) | Prebuilt archive with explicit target | Declared but not runtime-verified here |
| macOS x86_64 / arm64 | Native Rust target | **Unverified** — macOS is not runtime-verified |
| Windows | Not a supported release target | Unsupported; the POSIX `*.sh` utilities are **not a native Windows installer** |
| Cross-compiled target | Prebuilt archive with explicit target | Unverified; requires independent build provenance and target-machine smoke tests |

Declared archive triples are `x86_64-unknown-linux-gnu`,
`aarch64-unknown-linux-gnu`, `x86_64-apple-darwin` and
`aarch64-apple-darwin`. Only Linux x86_64 has current runtime evidence in this
repository. macOS and Windows are not runtime-verified.

M15 executable acceptance is scoped to Linux x86_64. Passing fixture-based
packaging tests does not certify a platform. Use `rustc -vV` to inspect the
build host and verify the deployed executable against target requirements. Do
not relabel binaries from one target as another.

## Distribution and trust

Each archive ships with an adjacent `.sha256` file and an internal `SHA256SUMS`.
The verifier rejects unknown paths, symlinks and checksum mismatches. Checksums
protect against accidental corruption **only when acquired through a trusted
channel**; they are not publisher authenticity. The GitHub workflow also attests
provenance, but authorization to publish remains a human decision.

Recommended offline flow:

```sh
./scripts/verify-release.sh ./dist/orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz
./scripts/install.sh --from-archive ./dist/orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz
./scripts/orbit-env.sh --check
```

The public one-command install uses the same assets:

```sh
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh
```

Replace example versions and targets with the actual release. Installed binary
inspection is not a substitute for application smoke tests.

## Update and support policy

```sh
orbit update --check
orbit update
```

`orbit update` targets a managed CLI installation, verifies checksums, backs up
the database and refuses unsupported downgrades. Do not infer that an update
makes an unverified platform supported. macOS users must validate the artifact
independently; Windows is unsupported.

## Related

- [Binary distribution](binary-distribution.md)
- [Release assets reference](../reference/release-assets.md)
- [Release checklist](release-checklist.md)
- [Installation](../getting-started/installation.md)
- [Upgrades and recovery](upgrades-and-recovery.md)
