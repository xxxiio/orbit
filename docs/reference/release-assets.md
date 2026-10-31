# Release assets reference

This page defines the canonical artifact names and layout produced by the
release workflow. It does **not** claim that a public GitHub Release has been
published. The workflow creates a **draft** on a tag push; publishing requires
explicit manual authorization.

## Archive names

```text
orbit-VERSION-TARGET.tar.gz
orbit-VERSION-TARGET.tar.gz.sha256
install.sh                       # public installer entry point
orbit-install-core.sh            # transactional binary installer reused by install.sh
orbit-verify-release.sh          # archive verifier reused by the installer
```

The installer helper assets are attached to the same release, so the public
entry point does not use raw source hosting:
`https://github.com/xxxiio/orbit/releases/latest/download/install.sh`.

`VERSION` has no leading `v` in the asset name (for example `0.1.0`), while the
Git tag is normally `v0.1.0`. `TARGET` is one of the declared Rust target
triples:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

The archive is platform-specific. Do not relabel a binary from one target as
another.

## Archive layout

```text
orbit-VERSION-TARGET.tar.gz
├── bin/
│   ├── orbit                 # required
│   ├── orbit-mcp             # optional component
│   ├── orbit-tui             # optional component
│   └── orbit-server          # optional component
├── RELEASE.txt               # version, target and selected components
└── SHA256SUMS                # checksums of included files
```

Only selected components are packaged. The CLI archive contains `bin/orbit`.
The Python machine worker, Herdr, Docker images, databases, configuration,
credentials and native installers are never included.

## Integrity and provenance

Two checksum layers are used:

1. the adjacent `.sha256` file for the archive;
2. `SHA256SUMS` inside the archive for its files.

The verifier restricts entry names and types, rejects unknown paths and
symlinks, and checks digests. A matching checksum proves **file consistency**,
not publisher authenticity. Same-origin checksums catch corruption in transit;
obtain releases through a channel you trust.

The GitHub workflow also attests build provenance for the archive artifacts. Attestation
answers "which workflow built this" only when verified through the supported
tooling; it is not a substitute for reviewing release authorization.

## Install and update

Verify and install offline:

```sh
./scripts/verify-release.sh orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz
./scripts/install.sh --from-archive orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

The public installer downloads and verifies the same assets, plus the release's
installer helper assets:

```sh
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh
```

Updates use:

```sh
orbit update --check
orbit update
```

Update verifies checksums, refuses installations without the M26 manifest,
creates a complete, verified workspace recovery point (database, Markdown vault,
referenced attachments and normalized non-secret configuration) under
`~/.orbit/backups/`, fails closed before replacing binaries if it cannot, and
preserves the prior binary/data on failure.

## Supported and verified matrix

| Target | Declared | Runtime-verified in this build | Notes |
| --- | --- | --- | --- |
| `x86_64-unknown-linux-gnu` | Yes | **Yes** | Local, loopback-connected and local-Herdr workflows |
| `aarch64-unknown-linux-gnu` | Yes | No | Packaged target; not runtime-probed here |
| `x86_64-apple-darwin` | Yes | **No** | macOS not runtime-verified |
| `aarch64-apple-darwin` | Yes | **No** | macOS not runtime-verified |
| Windows | No | No | Unsupported; no native Windows installer |

"Declared" means the installer recognizes the triple and the workflow can build
it on a native runner. It does not mean the artifact passed runtime acceptance
on that platform. The only current runtime evidence is Linux x86_64.

## Draft-to-publish boundary

On a tag push the GitHub workflow:

1. validates that the tag and package version agree;
2. builds on native runners for the declared targets;
3. packages archives and verifies their contents;
4. attests provenance;
5. creates a **draft** release.

A human must review evidence and explicitly authorize publishing a non-draft
release. Do not describe a draft as published, and do not infer a published
release from the presence of the workflow.

## Related

- [Binary distribution](../releasing/binary-distribution.md)
- [Release checklist](../releasing/release-checklist.md)
- [Platforms and components](../releasing/platforms-and-components.md)
- [Installation](../getting-started/installation.md)
