# Release candidate checklist

Orbit's source implementation and its executable release acceptance are
separate. Record the package version, target triple, artifact hashes and test
evidence for **each** candidate. Do not infer release readiness from a green
source test run.

## Readiness versus publication

There are three distinct states:

1. **Source complete** — code builds and maintained tests pass.
2. **Release candidate ready** — archives are built, verified, installed and
   smoke-tested on the declared target, with evidence recorded.
3. **Published** — a human with authorization makes a non-draft GitHub Release
   public.

The GitHub workflow creates a **draft** on a tag push and requires manual
authorization to publish. A draft is not published. Never describe a release as
published merely because a tag exists or the workflow ran.

## Before packaging

- [ ] Source identity is recorded immutably (tag/commit plus artifact hash).
- [ ] `Cargo.lock` is preserved; the build uses `--locked`.
- [ ] The intended `cli`, `mcp`, `tui` and `server` component set is explicit.
- [ ] Optional Python workers, Docker assets and Herdr requirements are
      documented separately.
- [ ] The native build target, linker and runtime prerequisites are identified.
- [ ] The version in the tag and package metadata agree.

## Package and validate

- [ ] Run the maintained Rust and Python test suites.
- [ ] Build archives with `scripts/package-release.sh --build` or a trusted
      prebuilt source.
- [ ] Verify every archive with `scripts/verify-release.sh` and a trusted
      checksum.
- [ ] Install the verified bundle into a disposable prefix with
      `scripts/install.sh --from-archive`.
- [ ] Inspect the managed installation with
      `scripts/orbit-env.sh --check`.
- [ ] Exercise the public one-command install from a draft/private channel
      without claiming it is a public release.
- [ ] Test `orbit update --check` and a managed update on a disposable install.
- [ ] Run compiled local workflows: create a project, create a task, restart
      the process, inspect persisted data.
- [ ] When shipping the server, run connected client, offline-outbox and
      recovery smoke tests against temporary storage.
- [ ] When shipping agent execution, validate launch fencing, reviews and the
      configured optional Herdr workflow.
- [ ] Record failed and skipped tests explicitly. Do not mark an unsupported
      environment as passed.

## Artifact contract

- [ ] Canonical assets are `orbit-VERSION-TARGET.tar.gz` and
      `orbit-VERSION-TARGET.tar.gz.sha256`.
- [ ] `install.sh`, `orbit-install-core.sh` and `orbit-verify-release.sh` are
      attached to the same release; the public installer URL is
      `https://github.com/xxxiio/orbit/releases/latest/download/install.sh`.
- [ ] The archive contains `bin/orbit` (and only selected optional binaries),
      `RELEASE.txt` and `SHA256SUMS`.
- [ ] No database, configuration, credential, Herdr, Python worker or Docker
      image is bundled.
- [ ] Attestations were generated and are associated with the built artifacts.
- [ ] Checksums are described as integrity controls, not publisher
      authentication.

## Platform evidence

- [ ] Linux x86_64: runtime evidence recorded for local, loopback connected and
      local-Herdr scope.
- [ ] Linux aarch64: build evidence recorded; runtime status stated honestly.
- [ ] macOS: explicitly marked **not runtime-verified** unless independent
      target-machine evidence exists.
- [ ] Windows: stated as unsupported; no native Windows installer.
- [ ] Cross-compiled targets: independent provenance and target smoke tests
      recorded.

## Upgrade and rollback

- [ ] Back up authoritative SQLite state with a transactionally consistent
      method.
- [ ] Preserve connected-client pending commands and verify replay/conflict
      behavior.
- [ ] Verify upgrading over a managed installation; test binary rollback
      separately from database restore.
- [ ] Confirm schema-compatibility limitations and restore procedures in
      [upgrades and recovery](upgrades-and-recovery.md).
- [ ] Do not test destructive restore or migration procedures against live user
      databases.

## Authorization and publication

- [ ] Evidence is reviewed against the intended support matrix.
- [ ] The draft release's assets match the verified hashes.
- [ ] A named human authorizes publication of a non-draft release.
- [ ] Release notes state verified versus unverified platforms and known
      limitations from [platforms and components](platforms-and-components.md).
- [ ] No claim of macOS runtime verification or a published release is made
      without corresponding evidence.

## M15 boundary

Current M15 executable evidence is Linux x86_64 local, loopback-connected and
local-Herdr scope, including a real archive build/verify/install smoke. TLS
ingress, remote-machine agent execution, signing, macOS/Windows runtime behavior
and real version-to-version upgrades remain separate gates. Packaging scripts
and fixture-based checks do not close those gates.

See [binary distribution](binary-distribution.md),
[platforms and components](platforms-and-components.md) and the
[release assets reference](../reference/release-assets.md).
