# Supported build targets and component requirements

This page describes **packaging interfaces**, not a claim that every target has passed M15 runtime acceptance. A target label supplied to `package-release.sh --from-dir` is not independently checked against the executable architecture. Only ship artifacts built and tested on the stated target.

## Component selection

| Component | Installed command | Built package | External requirements |
| --- | --- | --- | --- |
| `cli` (default) | `orbit` | `orbit-cli` | No server, Python, or Herdr for local SQLite mode |
| `mcp` | `orbit-mcp` | `orbit-mcp` | MCP host/client configuration; server only for connected mode |
| `tui` | `orbit-tui` | `orbit-tui` | Interactive terminal |
| `server` | `orbit-server` | `orbit-server` | Persistent server SQLite storage and server configuration |

The optional Python machine worker is **not** included in these Rust binary archives. Install it separately through `scripts/install-worker.sh`; its Python and Herdr dependencies are only needed for that execution mode. Docker is optional for self-hosted deployment and unnecessary for local-only use.

## Target and acceptance policy

`package-release.sh --build` uses the local Rust host target unless given `--target`. Cross-compilation requires the correct Rust target, linker, native libraries and deployment testing; the packaging script does not set these up. The bundle name contains the target triple, but its contents are not architecture-probed. The `--from-dir` packaging mode trusts the supplier's asserted target.

| Environment | Packaging workflow | Release readiness |
| --- | --- | --- |
| Linux x86_64 (native Rust target) | Source build or prebuilt archive | Only target with current M15 executable evidence: full profile 37/37, 1235 Rust tests, real archive build/verify/install smoke |
| macOS (native Rust target) | Source build or prebuilt archive | Unverified — no current M15 executable evidence |
| Windows | Cargo workspace build may be evaluated separately | Unverified; POSIX `*.sh` release/install utilities are **not** a native Windows installer |
| Cross-compiled target | Prebuilt archive with explicit target | Unverified — requires independent build provenance and target-machine smoke tests |

Only Linux x86_64 has current M15 executable evidence. macOS and Windows are
unverified for M15 acceptance. No platform is considered certified based only on
passing fixture-based shell packaging tests. Use `rustc -vV` to inspect the build
host and check the actual deployed executable against target requirements. Do
not relabel binaries from one target as another.

## Distribution and trust

Each archive ships with its adjacent `.sha256` file and internal `SHA256SUMS`. The verifier rejects unknown paths, symlinks and checksum mismatches. Checksums protect against accidental corruption **only when the checksum is acquired through a trusted channel**. Signing and authenticated distribution are separate release concerns.

Recommended offline flow:

```sh
./scripts/verify-release.sh ./dist/orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz
./scripts/install.sh --from-archive ./dist/orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz
./scripts/orbit-env.sh --check
```

Replace the example version and target with the actual release. Installed binary inspection is not a substitute for application smoke tests. See [installation](../getting-started/installation.md), [upgrade recovery](upgrades-and-recovery.md), and [release distribution](binary-distribution.md).
