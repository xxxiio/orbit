# Building, distributing and verifying binaries

Orbit distributes prebuilt archives through the GitHub release workflow. The
workflow creates a **draft** on a tag push; a human must authorize publishing a
non-draft release. This page describes artifact creation and verification, not a
claim that a public release is live.

Archive SHA-256 digests prove consistency with a checksum file, **not publisher authenticity**. Obtain artifacts through a trusted channel.

## Public install flow

```sh
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh
```

The root `install.sh` is itself a release asset, so the public entry point
stays on `github.com` rather than raw source hosting. It resolves a release
(stable by default, or `--version`), detects the declared target, downloads the
archive and `.sha256`, verifies the digest, fetches the release's
`orbit-install-core.sh` and `orbit-verify-release.sh` helper assets, then
reuses the transactional installer to install the CLI. It does not use `sudo`,
edit startup files, or install a server.

## Build a release bundle

With Rust and the target toolchain installed:

```sh
./scripts/package-release.sh --build --components cli,mcp,tui,server
```

By default the script selects the local Rust host target. Cross-compiled builds
require a configured toolchain, appropriate system libraries and
`--target RUST_TARGET`. The script does not install cross-compilation
dependencies.

You can bundle previously built binaries:

```sh
./scripts/package-release.sh \
  --from-dir ./target/release \
  --target x86_64-unknown-linux-gnu \
  --components cli,mcp,tui,server \
  --output ./dist
```

The `--target` label supplied with `--from-dir` is **not validated against
binary architecture**. Never relabel binaries to claim another target.

Canonical names:

```text
orbit-VERSION-TARGET.tar.gz
orbit-VERSION-TARGET.tar.gz.sha256
```

Inside the archive:

```text
bin/orbit
bin/orbit-mcp        # optional
bin/orbit-tui        # optional
bin/orbit-server     # optional
RELEASE.txt
SHA256SUMS
```

Only requested components are packaged. The archive contains no databases,
configuration, credentials, Herdr, Python workers, Docker images or platform
installers.

## Verify before use

```sh
./scripts/verify-release.sh dist/orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

Change the example version and target to match the artifact. The verifier checks
the companion checksum, restricts archive entry names/types, and verifies
internal file checksums. It does not establish publisher identity or prove the
binary runs on the target OS.

## Install a verified archive

```sh
./scripts/install.sh --from-archive dist/orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

The installer verifies both checksum layers and stages selected binaries with
managed rollback protection. `--from-dir` takes source-named binaries (`orbit`,
`orbit-mcp`, etc.) and is **not** interchangeable with the public `bin/` archive
layout. Always obtain archives and checksum files from a trusted source:
checksums alone do not authenticate publishers.

Inspect the result:

```sh
./scripts/orbit-env.sh --check
```

## GitHub release workflow

On a version tag push, the workflow:

1. validates that the tag and package version agree;
2. builds on native runners for the declared targets;
3. packages the canonical archives and verifies them;
4. attests build provenance for the archives;
5. stages `install.sh` and its helper assets from the tagged source;
6. creates a **draft** release with the archives and installer assets.

Publishing a non-draft release requires explicit manual authorization. Draft
status is not publication, and the existence of the workflow is not evidence
that a release was published. macOS artifacts are declared and can be built, but
macOS runtime behavior is **not verified** in this repository.

## Upgrades

```sh
orbit update --check
orbit update
```

`orbit update` checks the stable channel by default, verifies checksums before
replacement, refuses installations without the M26 manifest, and backs up the
SQLite database under `~/.orbit/backups/` before replacing the binary. It
refuses downgrades unless `--allow-downgrade`, and preserves the prior
binary/data on failure. `--check` never mutates.

## Release acceptance

The automated gate includes a real build/package/verify/install smoke for the
CLI. The packaging/verification contract also covers a four-component layout
(`cli,mcp,tui,server`) using stub executables. Linux local/loopback/local-Herdr
acceptance is the only runtime-verified scope.

Still unverified: launch smoke for every optional binary, signing and
authenticated distribution, macOS and aarch64 runtime behavior, Windows, TLS
ingress, remote-machine agent execution, and real version-to-version upgrades.
Attestations and checksums are integrity/provenance controls, not proof of a
working release on an untested platform.

## Related

- [Release assets reference](../reference/release-assets.md)
- [Release checklist](release-checklist.md)
- [Platforms and components](platforms-and-components.md)
- [Upgrades and recovery](upgrades-and-recovery.md)
- [Installation](../getting-started/installation.md)
