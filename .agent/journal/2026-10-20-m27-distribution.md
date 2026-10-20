---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-20"
task_id: TASK-0003
---

# Journal Entry: 2026-10-20 — M27 distribution, first-run and user documentation

## Context

M27 was requested after the M26/M15 baseline established a runnable Linux
build but no GitHub-hosted installation, first-run configuration, diagnostic,
update path or coherent end-user documentation. The repository's M25 record
still showed unmet capability/upgrade criteria despite earlier "source complete"
reports; that discrepancy was recorded rather than silently resolved by
declaring M25 complete.

## Work completed

- **I1 release automation:** added `.github/workflows/release.yml`,
  `scripts/release/validate-release-version.sh` and 20 workflow/validator tests.
  The workflow validates tag/version agreement, gates on fmt/Clippy/tests/
  documentation/packaging, builds the four declared native targets, verifies
  canonical archives, attests provenance, and creates a draft on tag push.
- **I2 installer:** added root POSIX `install.sh` with platform detection,
  stable/explicit release resolution, checksum verification, PATH guidance and
  reuse of the M26 transactional archive installer. The public entry point is
  attached to each release as `install.sh`; the transactional core and verifier
  are release assets (`orbit-install-core.sh`, `orbit-verify-release.sh`), so
  the documented URL stays on `github.com` and does not use
  `raw.githubusercontent.com`. Made the M26 scripts work with `shasum` and bash
  3.2 (removed associative arrays) so macOS is not blocked by GNU-only
  assumptions. Added 10 fixture installer tests.
- **I3 configuration/first run:** added `orbit-client::config`, unified
  deployment resolution, `orbit init`, read-only `orbit doctor`, read-only
  SQLite inspection, global `--config`, agent provider fallback, and 14
  init/doctor acceptance tests.
- **I4 update:** added `orbit update`/`--check`, stable/prerelease and explicit
  version selection, checksum and `RELEASE.txt` verification, managed-manifest
  enforcement, read-only SQLite backup, embedded M26 installer reuse, binary
  rollback and 12 fixture update tests.
- **I5 documentation:** rewrote root `README.md` and `docs/README.md`; added
  initialization, configuration, release-assets, everyday workflow, backup,
  workspace, development/learning and troubleshooting guidance; rewrote the
  ten-step agent tutorial and MCP integration guide; fixed the stale CLI/server
  contracts; added 24 documentation tests.
- **I6 integration:** added M27 checks to `scripts/collect-release-evidence.py`,
  isolated acceptance commands from a developer's real config, fixed the
  deployment-boundary contract test for the unified resolver, and ran the full
  M15 profile.
- Created and reconciled M27/TASK-0003/NEXT records, risk/TODO/dependency
  registers and evidence files.

## Decisions made

- Keep the existing canonical asset naming `orbit-VERSION-TARGET.tar.gz`; tags
  are `vVERSION`. Do not introduce `orbit-v...` asset names.
- Put configuration resolution in `orbit-client` rather than a new crate so
  CLI, TUI and MCP share one precedence/authority path.
- `orbit init` writes `$HOME/.orbit` runtime directories even when `--config`
  points elsewhere; custom config moves only the file.
- `orbit update` uses the installed prefix binary's version when present, not
  only the compiled package version, so `--prefix` and up-to-date checks are
  coherent.
- A real database backup is mandatory before binary replacement; the update
  command refuses an unreadable/foreign/newer-schema database rather than
  guessing.
- M27 can complete source/fixture implementation while EC-001 and real-release
  installation remain unverified. A draft/manual publication gate is required.

## Files or components changed

- `.github/workflows/release.yml`, `scripts/release/`, root `install.sh`.
- `scripts/install.sh`, `scripts/verify-release.sh`, `scripts/package-release.sh`.
- `crates/orbit-client/src/{config.rs,deployment.rs,lib.rs,Cargo.toml}`.
- `crates/orbit-storage/src/lib.rs`, `crates/orbit-storage/Cargo.toml`.
- `crates/orbit-cli/src/{commands.rs,lib.rs,main.rs,init_command.rs,doctor_command.rs,update_command.rs,agent_commands.rs,connected_agent.rs}`.
- `README.md`, `docs/` user-facing pages, `docs/reference/`, `docs/releasing/`.
- `tests/packaging/test_{release_workflow,github_installer}.py`,
  `tests/acceptance/m27_{init_doctor,update}.py`,
  `tests/documentation/test_m27_{docs,cli_help}.py`, packaging smoke script.
- `.agent/` milestone, current, register, evidence and journal records.

## Validation performed

- `cargo fmt --all -- --check` — pass.
- `cargo test --workspace --locked` — 1,266 passed, 0 failed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — pass.
- `python3 -m pytest -q tests` — 91 passed.
- `python3 -m unittest discover -s tests/documentation -v` — 24 passed.
- `tests/packaging/test_github_installer.py` — 10 passed.
- `tests/packaging/test_release_workflow.py` — 20 passed.
- `tests/acceptance/m27_init_doctor.py` — 14 passed.
- `tests/acceptance/m27_update.py` — 12 passed.
- `tests/packaging/test_real_release_archive.sh` — pass, including installed
  `orbit init`/`orbit doctor` and local project write.
- `./scripts/m15-full-automated.sh --profile full` — 42 passed, 0 failed,
  0 not_run; evidence `.agent/evidence/m27-m15-final.zip`.

## Problems encountered

- The first M15 profile failed `blueprint-workflow-smoke`: connected mode passed
  both `ORBIT_DB` and `--db` (replica), and the new resolver compared the
  replica against the CLI override, producing a false `SameDatabase` error.
  Fixed by separating the local-authority candidate from the CLI override and
  adding a regression test; the full profile then passed 42/42.
- Documentation tests initially caught a relative link, line-wrapped required
  phrases, and a macOS/release claim line; all were corrected.
- The release-evidence runner now sets an isolated `ORBIT_CONFIG` so a
  developer's real `~/.orbit/config.toml` cannot change acceptance deployment.
- The repository's authenticated account has read-only permission and no
  GitHub Release exists, so a real workflow/install verification is impossible
  without external authorization.

## Remaining work

- An authorized maintainer must run the release workflow and verify the real
  GitHub-hosted installer/update path (NEXT-0004).
- macOS/aarch64 runtime behavior, signing, TLS ingress, remote-machine Herdr
  execution and real version-to-version upgrades remain release gates.
- M25 EC-003 through EC-006 remain deferred, not completed.

## New TODOs

- TODO-0007 (blocked): authorized real release/install verification.
- TODO-0008 (blocked): macOS installer/update portability execution.

## Risks or blockers

- No real GitHub Release has been published; fixture tests prove the code path,
  not the public distribution channel.
- Release runners (`ubuntu-24.04-arm`, `macos-13`, `macos-14`) and attestation
  permissions have not executed.
- The workspace remains single-user trust in connected mode.
