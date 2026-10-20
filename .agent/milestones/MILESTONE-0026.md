---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0026
status: completed
---

# Milestone: Packaging, installation and task-oriented documentation

## Intended outcome

A user can install Orbit from source or a verified prebuilt archive into a disposable prefix, inspect or diagnose the installation, remove only managed binaries, and follow task-oriented documentation to a first project.

## Why this milestone matters

A runnable tree is not a usable product without a safe install/upgrade/removal path, diagnosable environment state and documentation that matches the implemented CLI.

## Scope

- Local source installer, prebuilt-archive installer and uninstaller.
- Release packaging, checksum manifest, archive verification and release evidence.
- Installation environment inspection and doctor.
- Optional worker and server setup helpers.
- Task-oriented user, release, CLI and MCP documentation.
- Packaging and documentation regression tests.

## Non-goals

- Distribution signing or publisher authentication.
- A native Windows installer.
- TLS ingress automation or a hosted service.
- Cross-platform build certification.

## Deliverables

- `scripts/install.sh`, `scripts/uninstall.sh`, `scripts/package-release.sh`, `scripts/verify-release.sh`, `scripts/orbit-env.sh`, `scripts/install-worker.sh`, `scripts/setup-server.sh`.
- `tests/packaging/` and `tests/documentation/`.
- `docs/getting-started/`, `docs/guides/`, `docs/releasing/`, `docs/reference/`, `docs/developers/`, `docs/contributing/`.
- `docs/README.md` navigation index.

## Dependencies

- MILESTONE-0015 local build and acceptance.
- MILESTONE-0023 optional worker.
- MILESTONE-0025 blueprint documentation surfaces.

## Risks

- Installers can overwrite user files; mitigated by a managed manifest, staging/rollback and unmanaged-file refusal tests.
- Release archives can be tampered with; checksums detect corruption but not publisher identity.
- Documentation can drift from the CLI; mitigated by source-backed documentation contracts.

## Ordered implementation sequence

1. Implement managed install/uninstall with manifest and rollback safety.
2. Implement release packaging and archive verification.
3. Implement installation diagnostics and environment preparation.
4. Add optional worker/server setup helpers.
5. Write task-oriented user and release documentation.
6. Add packaging/documentation regression tests.
7. Build, package, verify and install a real archive into a disposable prefix.

## Exit criteria

- [x] EC-001: Install/uninstall and manifest-safety tests pass.
- [x] EC-002: Release archive creation and verification tests pass, including corruption and extra-member rejection.
- [x] EC-003: A real Rust release archive is built, verified, installed, exercised and uninstalled in a disposable prefix.
- [x] EC-004: `orbit-env.sh` check/doctor behavior and optional worker/server setup contracts pass.
- [x] EC-005: Documentation link, command-inventory and release-guidance contracts pass.
- [x] EC-006: The packaged CLI creates and reopens a local SQLite workspace.

## Evidence

- EC-001: `.agent/evidence/m15-final.zip` checks `installer-contract`, `installer-transaction-contract` and `installation-manifest-safety`.
- EC-002: `.agent/evidence/m15-final.zip` checks `release-archive-contract` and `release-installation-contract`.
- EC-003: `.agent/evidence/m15-final.zip` check `real-release-archive-smoke`.
- EC-004: `.agent/evidence/m15-final.zip` checks `installation-environment-contract`, `optional-worker-installer-contract` and `server-setup-contract`.
- EC-005: `.agent/evidence/m15-final.zip` check `user-documentation-contract`.
- EC-006: `tests/packaging/test_real_release_archive.sh` creates a project through the installed binary and verifies the database.

## Remaining work

- None in the unsigned Linux packaging/documentation scope.
- Signing/authenticated distribution, cross-platform packaging, optional-binary launch smoke and installed-version upgrade evidence remain deferred release gates.

## Next milestone

MILESTONE-0015
