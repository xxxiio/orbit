---
template: project-status
template_version: 2.0.0
template_digest: "bae979b223043d02aa7be0cacebed8dd05241d5cc9f0d330796a7c919fbbe019"
project_owned: true
record_contract: project-status
record_schema_version: 1
status: current
---

# Project Status

## Current state

Orbit builds and passes the full Linux local/loopback/local-Herdr acceptance profile. M22, M23, M24, M26 and M15 source/integration work is recorded as complete for its declared scope. M25 blueprint I1/I2 and normal receipts are implemented, but capability actions, connected MCP parity, upgrade and domain extraction remain.

## Current milestone

MILESTONE-0025 — M25 Project Blueprint Framework, status active. Next unmet criterion: EC-003, a trusted capability installs through a public composition surface and an offered action can be requested, claimed, executed and reconciled end to end.

## Recently completed

- MILESTONE-0026 packaging/installation/documentation integration and real-archive verification; evidence in `.agent/evidence/m15-final.zip`.
- MILESTONE-0015 Linux local/loopback/local-Herdr acceptance: 38/38 checks with live Herdr, 1,235 Rust tests, fmt/clippy clean; evidence in `.agent/evidence/`.
- M26 packaging defects fixed: Cargo builds `orbit`, not `orbit-cli`; real archive build/install smoke added.

## In progress

- TASK-0002 / NEXT-0001: make M25-I3 capability installation and action request/execute reachable with a deterministic fake-runtime acceptance test.

## Blocked

- None. Public-release gates are deferred, not blocking local internal testing.

## Key decisions

- Preserve the repaired local M15 Rust source over the older non-compiling M26 source snapshot.
- Keep M26 packaging/docs/tests while correcting stale binary-name assumptions.
- Treat M15 acceptance as Linux local/loopback/local-Herdr scope, not public-release certification.
- Keep historical M18-M22 scripts under `scripts/legacy-contracts/` rather than deleting evidence.

## Project health

- Build: pass (`cargo check`, `cargo build --locked`).
- Tests: 1,235 Rust tests pass; 51 maintained Python tests pass.
- Format/lint: pass.
- Acceptance: 38/38 with live Herdr.
- Packaging: real Linux archive build/verify/install/uninstall pass.
- Data integrity: local/offline/durability smokes pass; no known blocking data-loss issue.
- Deployment: loopback HTTP verified; TLS ingress unverified; no signing.

## Risks

- M25-I3 is unreachable; capability-requiring blueprints fail closed at publication.
- Connected MCP bypasses the outbox/receipts.
- Public release lacks signing, TLS, cross-platform and upgrade evidence.
- Connected server is single-user trust; no role split.

## Remaining milestones

1. MILESTONE-0025 — active.

## Next deterministic step

NEXT-0001 — expose trusted capability installation and an explicit action request/execute path, then prove it with a deterministic fake runtime.
