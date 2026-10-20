# Building and verifying offline binary releases

Orbit is not yet distributed through a public package registry. The supported source build uses the root Cargo workspace. These scripts create **unsigned** archives of prebuilt executables. An archive's SHA-256 digest proves file consistency with its checksum file, **not publisher authenticity**. Obtain both from an authenticated trusted channel.

## Build a release bundle

With Rust and the target toolchain installed:

```sh
./scripts/package-release.sh --build --components cli,mcp,tui,server
```

By default the script selects the local Rust host target. Cross-compiled builds require a configured cross toolchain, appropriate system libraries, and `--target RUST_TARGET`; this script does not install any cross-compilation dependencies. You can also bundle previously built binaries, for example from a trusted CI build:

```sh
./scripts/package-release.sh \
  --from-dir ./target/release \
  --target x86_64-unknown-linux-gnu \
  --components cli,mcp,tui,server \
  --output ./dist
```

The `--target` label is supplied by the operator and **is not validated against binary architecture** when using `--from-dir`. Never relabel binaries to claim another target. Bundles are named `orbit-VERSION-TARGET.tar.gz` and accompanied by `orbit-VERSION-TARGET.tar.gz.sha256`.

Only requested components are packaged. The `orbit-cli` package produces the source binary `orbit`, which is packaged as `bin/orbit`; the others keep their names. The archive contains `RELEASE.txt` and checksums for all included files. It does not contain databases, configuration, credentials, Herdr, Python workers, Docker images, or platform installers.

## Verify before use

```sh
./scripts/verify-release.sh dist/orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

Change the example version and target to match your artifact. This checks the companion checksum, restricts archive entry names and types, then verifies internal file checksums. It does not establish the publisher's identity or that the binaries work on the target OS.

## Install verified binaries

The supported archive flow is `scripts/install.sh --from-archive ARCHIVE.tar.gz`.
The installer verifies both checksum layers and stages selected binaries using its
managed rollback protection. `--from-dir` instead takes source-named binaries
(`orbit`, `orbit-mcp`, etc.) and is **not** interchangeable with the public
`bin/orbit` archive layout. See [platform prerequisites](platforms-and-components.md)
and [release checklist](release-checklist.md).

## Release acceptance

The automated gate now includes a **real** build/package/verify/install smoke for
the CLI, and a four-component archive (`cli,mcp,tui,server`) was additionally
verified manually through build, verify, install and uninstall. Linux
local/loopback/local-Herdr M15 acceptance has passed at 37/37 checks (38/38 with opt-in live Herdr) with 1235
passing Rust tests.

Still unverified: the optional MCP/TUI/server binaries are packaged but not
launch-smoked, and signing is absent entirely. Cross-platform targets,
TLS/self-hosted ingress, remote-machine agent execution, and real
version-to-version upgrades remain separate open gates.

## Install directly from a verified release archive

After receiving both the archive and its matching `.sha256` file, install all
bundled components without Cargo or Python:

```bash
./scripts/install.sh --from-archive dist/orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

To install only selected bundled components, pass `--components cli,mcp`.
The installer verifies the archive and internal file checksums **before**
extracting binaries, refuses missing requested components, and uses the same
managed-installation rollback protections as `--from-dir`. No network access
is performed. Always obtain release archives and checksum files from a trusted
source: checksums alone do not authenticate publishers.

Use `./scripts/orbit-env.sh --check` or
`./scripts/install.sh --components cli,mcp --check` to inspect an installation.
A release archive is platform-specific and must match the target machine.
