---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0015
status: completed
---

# Milestone: M15 release acceptance for the Linux local/loopback/local-Herdr scope

## Intended outcome

A runnable Orbit build and packaged CLI are accepted for internal testing on Linux x86_64, with local, loopback-connected, offline, blueprint and local-Herdr workflows passing reproducible checks.

## Why this milestone matters

Source implementation is not runtime acceptance. This milestone establishes that the integrated tree builds, runs real SQLite workflows, survives restarts, and has no known blocking data-loss defect before internal testers use it.

## Scope

- Rust format, check, test and Clippy gates.
- Python worker and maintained contract suites.
- Local-authoritative CLI workflows with process restart and SQLite integrity checks.
- Loopback connected-authority, offline replica, durability and blueprint workflows.
- Deterministic agent-execution contract and opt-in live local Herdr acceptance.
- Installation/release-archive checks and a real build/package/verify/install/uninstall smoke.

## Non-goals

- TLS/Caddy ingress, browser deployment and public distribution signing.
- Remote-machine Herdr execution against the real Rust server.
- macOS, Windows and cross-compiled target acceptance.
- Real version-to-version installed-database upgrade drills.

## Deliverables

- `scripts/collect-release-evidence.py` and `scripts/m15-full-automated.sh`.
- `tests/acceptance/local_cli_workflows.py` and maintained contract/packaging suites.
- Disposable loopback smokes under `deploy/server/`.
- Durable evidence bundle under `.agent/evidence/`.

## Dependencies

- MILESTONE-0022, MILESTONE-0023, MILESTONE-0024, MILESTONE-0025 and MILESTONE-0026 source work.

## Risks

- Acceptance on one platform does not prove cross-platform behavior; mitigated by explicit scope statements and release gates.
- Live agent tests require credentials and a real Herdr installation; mitigated by keeping them opt-in and using a disposable workspace.

## Ordered implementation sequence

1. Repair compilation and integration defects found by the Rust workspace gates.
2. Run local-authoritative and persistence acceptance against the real binary.
3. Run loopback connected, offline, durability and blueprint acceptance.
4. Run deterministic agent contracts and opt-in live Herdr acceptance.
5. Build and install a real release archive into a disposable prefix.
6. Archive the evidence bundle and record the verified scope.

## Exit criteria

- [x] EC-001: `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
- [x] EC-002: `cargo test --workspace --locked` passes with 1,235 tests and 0 failures.
- [x] EC-003: `./scripts/m15-full-automated.sh --profile full` passes 37 of 37 checks (38 of 38 with opt-in live Herdr) with 0 failed and 0 not-run.
- [x] EC-004: Live local Herdr acceptance passes with a real `opencode` agent in a disposable workspace.
- [x] EC-005: A real release archive is built, verified, installed into a disposable prefix, exercised and uninstalled.
- [x] EC-006: No known blocking data-loss defect remains in the verified scope.

## Evidence

- EC-001: `.agent/evidence/final-fmt.log`, `.agent/evidence/final-clippy.log`.
- EC-002: `.agent/evidence/final-test.log` (1,235 passed, 0 failed).
- EC-003: `.agent/evidence/m15-final.zip` (38 passed with live Herdr, 0 failed, 0 not_run).
- EC-004: `.agent/evidence/herdr-live.log` (live Herdr PASS).
- EC-005: `.agent/evidence/m15-final.zip` contains `real-release-archive-smoke`; independently repeated four-component archive verification in this integration review.
- EC-006: M15 local/connected/offline/durability/blueprint/agent checks pass; no blocking data-loss defect was observed.

## Remaining work

- None inside the declared Linux local/loopback/local-Herdr acceptance scope.
- Public-release gates are tracked as deferred work in `.agent/roadmap.md`.

## Next milestone

MILESTONE-0025
