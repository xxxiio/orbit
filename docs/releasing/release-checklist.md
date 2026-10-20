# Release candidate checklist

Orbit's source implementation and its executable release acceptance are separate. Record the package version, target triple, artifact hashes and test evidence for **each** release candidate. This project is not Git-managed, so identify the source by an immutable artifact instead: a source archive with its SHA-256 checksum, or an equivalently recorded source identity. GetDone owns milestone progress; do not infer M15 completion from this checklist alone.

Current baseline: Linux local/loopback/local-Herdr M15 acceptance has passed (37/37 checks (38/38 with opt-in live Herdr), 1235 Rust tests), including a real release-archive build/verify/install smoke. Cross-platform targets, TLS/self-hosted ingress, remote-machine agent execution, signing and real upgrade testing remain separate unverified gates.

## Before packaging

- [ ] Source identity is recorded immutably (source archive plus checksum, or an equivalent recorded source identity); any local modifications are explicitly recorded.
- [ ] Cargo lockfile is preserved; the build uses `--locked`.
- [ ] The intended set of CLI, MCP, TUI and server components is explicit.
- [ ] Optional Python workers, Docker assets and Herdr requirements are documented separately.
- [ ] The native build target, linker and runtime prerequisites are identified.

## Package and validate

- [ ] Run the maintained Rust and Python test suites using `scripts/m15-full-automated.sh --profile full`.
- [ ] Produce binary archives with `scripts/package-release.sh --build` or a trusted prebuilt source.
- [ ] Verify every archive using `scripts/verify-release.sh` and a trusted checksum.
- [ ] Install the verified bundle into a disposable prefix using `scripts/install.sh --from-archive`.
- [ ] Inspect the managed installation using `scripts/orbit-env.sh --check`.
- [ ] Run compiled local SQLite workflows: create project, create task, restart process, inspect persisted data.
- [ ] When shipping the server, run connected client, offline-outbox and recovery smoke tests against temporary storage.
- [ ] When shipping agent execution, validate deterministic launch fencing, reviews and the configured optional Herdr workflow.
- [ ] Record all failed and skipped tests explicitly; do not silently mark unsupported environments as passed.

## Upgrade and rollback

- [ ] Back up authoritative SQLite state with a transactionally consistent backup method.
- [ ] Preserve connected-client pending commands and verify replay/conflict behavior.
- [ ] Verify upgrading over a managed installation; test binary rollback separately from database restore.
- [ ] Confirm schema-compatibility limitations and restore procedures in [upgrades and recovery](upgrades-and-recovery.md).
- [ ] Do not test destructive restore or migration procedures against live user databases.

## Distribution decision

- [ ] Publish target-specific support status and known limitations from [platforms and components](platforms-and-components.md).
- [ ] Identify the origin/authenticity channel. SHA-256 alone is not publisher authentication.
- [ ] Link the actual M15 evidence ZIP or equivalent immutable evidence artifacts.
- [ ] Declare internal-testing readiness only with runnable package, successful core smoke workflows and no known blocking data-loss issue.
