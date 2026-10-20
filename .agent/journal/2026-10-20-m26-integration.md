---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-20"
task_id: TASK-0001
---

# Journal Entry: 2026-10-20 — M26 integration and verified internal-testing baseline

## Context

The project had no GetDone records and no version-control metadata. An incoming `orbit_m26_iteration_14.tar.gz` claimed 100% M26 source implementation, while the local tree contained M15 compilation fixes and executable acceptance work. This entry reconstructs the integration record from current evidence; it does not claim historical GetDone state.

## Work completed

- Created and verified a complete timestamped backup outside the project.
- Compared the incoming archive with the local tree by relative path and content.
- Preserved the repaired local Rust source; the archive's Rust source was older, unformatted and non-compiling.
- Integrated M26 packaging scripts, release verification, installation diagnostics, documentation trees, packaging tests and documentation tests.
- Reorganized contract tests into `tests/contracts/`.
- Merged M26 packaging/documentation checks into the M15 runner while preserving local/connected/offline/durability/blueprint/agent/live-Herdr checks.
- Fixed the real packaging defect where scripts expected `orbit-cli` but Cargo builds `orbit`.
- Added `tests/packaging/test_real_release_archive.sh` for real build/package/verify/install/uninstall coverage.
- Moved 54 historical source-shape scripts to `scripts/legacy-contracts/`.
- Added an MIT `LICENSE`, corrected stale release/install/milestone documentation, and rewrote the roadmap with verified status.
- Initialized GetDone and reconstructed milestone records from available evidence.

## Decisions made

- Local Rust source is authoritative for M15 fixes; M26 source was not merged blindly.
- M15 readiness is scoped to Linux local/loopback/local-Herdr, not public release.
- M25 is materially partial despite implemented receipts: capability actions are unreachable.
- Historical scripts are preserved but excluded from the active gate.

## Files or components changed

- `Cargo.toml`, Rust workspace source (local M15 source preserved).
- `scripts/` (M26 installers/release tools plus M15 runner).
- `tests/` (contracts, acceptance, packaging, documentation).
- `docs/` (M26 guides/reference/release docs, corrected milestone and roadmap pages).
- `.agent/` (GetDone records and evidence).

## Validation performed

- `cargo fmt --all -- --check` — pass.
- `cargo test --workspace --locked` — 1,235 passed, 0 failed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — pass.
- `./scripts/m15-full-automated.sh --profile full --live-herdr` — 38 passed, 0 failed, 0 not_run.
- Live Herdr acceptance — pass.
- `python3 -m pytest -q tests` — 51 passed.
- `python3 -m unittest discover -s tests/documentation -v` — 14 passed.

## Problems encountered

- The archive's source did not compile and used a stale module layout; resolved by preserving the local repaired source.
- Packaging scripts assumed a binary named `orbit-cli`; resolved by standardizing on the actual `orbit` binary and adding a real-archive smoke.
- The M26 local CLI acceptance directly completed an inbox task; the local domain correctly rejects that transition, so the test was corrected to start the task first.
- No Docker, signing key, second machine or older release fixture is available; those checks remain not-run.

## Remaining work

- M25-I3 capability installation/action request/execute.
- M25-I5 connected MCP outbox parity, lost-ack replay and blueprint upgrade.
- M25-I4 habit recurrence extraction.
- Separate public-release gates: TLS ingress, remote-machine execution, signing, cross-platform acceptance and installed-version upgrade.

## New TODOs

- None beyond the M25 and release-gate work recorded in `.agent/tracking/todos.md` and the roadmap.

## Risks or blockers

- M25 capability-requiring blueprints fail closed at publication until I3 is wired.
- Public release is not certified; only Linux local/loopback/local-Herdr internal testing is ready.
