---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0027
status: blocked
---

# Milestone: GitHub distribution, first-run experience and user documentation

## Intended outcome

A new user can install a verified Orbit binary from a GitHub Release without Rust or Python, initialize and diagnose a local workspace, update safely, and follow accurate task-oriented documentation through real workflows.

## Why this milestone matters

Orbit is runnable and internally accepted, but ordinary installation currently requires the source tree or a locally built archive. There is no single first-run command, no user-facing diagnostic outside a shell helper, no safe GitHub-managed update path and no coherent end-user documentation set. Those gaps block internal testers from treating the package as a product.

## Scope

- Gated GitHub release workflow for Linux and macOS targets, canonical asset names, checksums, build metadata and provenance attestations.
- Root GitHub-hosted POSIX installer that resolves a stable release, verifies it, and reuses the M26 transactional archive installer.
- Versioned unified configuration with `orbit init`, `orbit doctor`, and safe `orbit update`/`--check`.
- End-user documentation entry point, installation, first-project, everyday-workflow, agent, connected-mode, MCP and reference guides.
- Automated release, installer, initialization, diagnostics, update and documentation-drift checks.

## Non-goals

- Publishing an official stable GitHub Release without explicit maintainer authorization.
- Windows support, cross-compilation, Homebrew/Nix/mise, signing-key infrastructure, a hosted documentation service or a plugin marketplace.
- Completing deferred M25 capability, connected-MCP parity, blueprint-upgrade or habit-extraction work.
- TLS ingress certification, remote-machine Herdr execution or real installed-version downgrade support.

## Deliverables

- `.github/workflows/release.yml` and `scripts/release/validate-release-version.sh`.
- Root `install.sh`; portability updates to `scripts/install.sh`, `scripts/verify-release.sh`, `scripts/package-release.sh`.
- `crates/orbit-client/src/config.rs`, unified deployment resolution, `crates/orbit-cli/src/init_command.rs`, `doctor_command.rs`, `update_command.rs`.
- `tests/packaging/test_release_workflow.py`, `tests/packaging/test_github_installer.py`, `tests/acceptance/m27_init_doctor.py`, `tests/acceptance/m27_update.py`, `tests/documentation/test_m27_docs.py`.
- `docs/README.md`, root `README.md`, getting-started, guides, reference pages and release guidance refreshed against the implemented CLI.

## Dependencies

- MILESTONE-0015 executable Linux acceptance baseline.
- MILESTONE-0023 optional local agent execution.
- MILESTONE-0026 packaging, transactional archive installer and documentation test harness.

## Risks

- A public installer can overwrite unrelated binaries; mitigated by reusing the M26 managed manifest, staging and rollback.
- Same-origin checksums prove integrity, not publisher identity; provenance attestations are recorded separately and signing remains a release gate.
- Configuration precedence can drift between CLI, TUI and MCP; mitigated by one `orbit-client` resolver with pure precedence tests.
- Updates can migrate a database irreversibly; mitigated by a read-only backup before binary replacement and refusal to continue if the backup fails.
- Documentation can describe unbuilt commands; mitigated by source-backed and compiled-CLI checks.

## Ordered implementation sequence

1. I1: release automation, version validation, canonical artifact naming and release gate tests.
2. I2: root GitHub-hosted installer and install-safety acceptance.
3. I3: unified configuration, `orbit init` and `orbit doctor` with local acceptance.
4. I4: safe `orbit update` and update acceptance.
5. I5: end-user documentation entry point, tutorials, everyday workflow guides and references.
6. I6: reference consistency, documentation drift checks, packaging integration, full regression and records reconciliation.

## Exit criteria

- [ ] EC-001: A tag-triggered release workflow validates version consistency, builds and verifies canonical archives for the declared Linux/macOS matrix, records source revision and target metadata, attests provenance, and creates only a gated draft release unless explicitly authorized.
- [x] EC-002: Root `install.sh` works when piped to `sh`, detects supported platforms, resolves the latest stable or an explicit version, verifies checksums before installation, reuses the transactional archive installer, and preserves an existing installation on failure.
- [x] EC-003: `orbit init` creates the documented `~/.orbit` layout, a versioned validated configuration and a local SQLite workspace; it is idempotent, preserves existing configuration and data, and honors CLI > environment > config-file > default precedence including `ORBIT_DB`.
- [x] EC-004: `orbit doctor` reports executable, configuration validity, effective database, authority mode, connected settings and optional dependencies without modifying data or printing secrets.
- [x] EC-005: `orbit update` and `orbit update --check` resolve stable/prerelease channels, verify artifacts, refuse unmanaged installations, back up the database before replacement, and leave the previous binary and data intact on failure.
- [x] EC-006: `docs/README.md`, root README, getting-started, everyday guides and references describe only implemented commands and cover installation, init, first project, projects/tasks/blueprints, notes/habits/routines/schedules/search, agent workflows, connected mode, MCP, backup/recovery and troubleshooting.
- [x] EC-007: Automated checks cover release gating, installer safety, init/doctor/update behavior, documentation links and command/config examples; the existing M15 acceptance suite still passes after integration.
- [x] EC-008: GetDone and the roadmap record actual implementation and separate local fixture acceptance from a real published GitHub Release installation.

## Evidence

- EC-001: `.github/workflows/release.yml` (draft release plus archive/installer assets), `scripts/release/validate-release-version.sh`, `tests/packaging/test_release_workflow.py` (20 tests pass); `.agent/evidence/m27-i1-release.txt`. A live GitHub Actions run is pending authorization.
- EC-002: Root release-asset `install.sh` with `orbit-install-core.sh`/`orbit-verify-release.sh` helper assets, portability updates in `scripts/{install,verify-release,package-release}.sh`, `tests/packaging/test_github_installer.py` (10 fixture tests pass), existing packaging/install tests and `test_real_release_archive.sh` pass; `.agent/evidence/m27-i2-installer.txt`. Real GitHub installation and macOS runtime verification remain pending.
- EC-003: `crates/orbit-client/src/config.rs`, `crates/orbit-cli/src/init_command.rs`, `tests/acceptance/m27_init_doctor.py` (14 tests pass), `cargo test -p orbit-client -p orbit-storage -p orbit-cli` 498 tests pass, fmt/clippy clean; `.agent/evidence/m27-i3-init-doctor.txt`.
- EC-004: `crates/orbit-cli/src/doctor_command.rs`, `crates/orbit-storage/src/lib.rs` read-only inspection, doctor acceptance tests (read-only missing DB, hidden credentials, strict exit 1, schema/authority checks); `.agent/evidence/m27-i3-init-doctor.txt`.
- EC-005: `crates/orbit-storage/src/lib.rs` read-only backup API, `crates/orbit-cli/src/update_command.rs`, `tests/acceptance/m27_update.py` (12 tests pass), focused Rust tests/fmt/clippy pass; `.agent/evidence/m27-i4-update.txt`.
- EC-006: `README.md`, `docs/README.md`, getting-started, everyday guides, `reference/{cli,configuration,release-assets,mcp}.md`, connected/MCP/releasing pages; `tests/documentation/test_m27_docs.py` and `test_m27_cli_help.py` 24 tests pass.
- EC-007: Focused M27 tests plus `./scripts/m15-full-automated.sh --profile full`: 42 passed, 0 failed, 0 not_run; `.agent/evidence/m27-m15-final.zip`.
- EC-008: Roadmap, milestone, task, status, evidence and journal records record fixture acceptance separately from the unexecuted real-release gate.

## Remaining work

- EC-001 remains unverified until an authorized maintainer runs the release workflow; the repository currently has no release and the local account has read-only permission.
- Real GitHub Release installation/update verification is not-run; fixture installer/update tests are not a substitute.
- macOS and aarch64 runtime verification, signing, TLS ingress and remote-machine agent execution remain separate release gates.
- M25 EC-003 through EC-006 remain deferred.

## Next milestone

MILESTONE-0028
