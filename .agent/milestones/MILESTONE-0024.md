---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0024
status: completed
---

# Milestone: One authoritative runtime architecture

## Intended outcome

Orbit has one authoritative host and a clear module ownership model across application, storage, runtime, client, server and worker boundaries.

## Why this milestone matters

M22 planning and M23 agent work depended on removing duplicate authority implementations and transport-owned business rules. M25 blueprints reuse the same command, revision and receipt path.

## Scope

- One `AuthoritativeHost` ownership model and shared application services.
- Split agent authority modules and worker transport.
- Client outbox versus external machine journal separation.
- CLI/client/persistence module boundaries.
- Retirement of the legacy append-only relay architecture.

## Non-goals

- New user-facing features.
- Public plugin isolation or a marketplace.

## Deliverables

- Consolidated runtime, storage, client and server modules.
- `docs/m24-*` ownership and consolidation records.
- Historical source-shape scripts preserved under `scripts/legacy-contracts/`.

## Dependencies

- None beyond the existing M18-M22 source base.

## Risks

- Large structural changes can break compilation; mitigated by full workspace gates and M15 acceptance.

## Ordered implementation sequence

1. Establish one authoritative host and accepted-command path.
2. Move business rules into shared application services.
3. Separate client outbox and machine-launch journal responsibilities.
4. Retire legacy relay entry points and stale scripts.
5. Compile, test and document module ownership.

## Exit criteria

- [x] EC-001: The workspace compiles with one authoritative host path.
- [x] EC-002: No transport-owned domain mutation replaces the shared application path.
- [x] EC-003: Full Rust tests, fmt and Clippy pass.
- [x] EC-004: Historical source-shape scripts are moved out of the active gate and documented as unmaintained.

## Evidence

- EC-001: `cargo check --workspace --all-targets --locked` passed.
- EC-002: `tests/contracts/test_deployment_boundaries.py` and authority-entrypoint contracts pass.
- EC-003: `.agent/evidence/final-test.log`, `.agent/evidence/final-fmt.log`, `.agent/evidence/final-clippy.log`.
- EC-004: `scripts/legacy-contracts/README.md`; 54 scripts moved, active `scripts/` contains only maintained tools.

## Remaining work

- None in source consolidation. Preserve the boundary in future M25 capability work.

## Next milestone

MILESTONE-0025
