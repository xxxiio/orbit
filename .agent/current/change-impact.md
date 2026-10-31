---
template: change-impact
template_version: 1.0.0
template_digest: "8618a859250c7346e16cb5e20990e6c6d496e6b238ab8f44824028eba527e134"
project_owned: true
record_contract: change-impact
record_schema_version: 1
status: current
task_id: TASK-0020
---

# Change Impact Declaration

## Task

TASK-0020 — M34 final acceptance (I5): MCP/Herdr record coverage, failure
injection, real recovery rehearsal, documentation and honest milestone acceptance.
This declaration covers the complete M34 work delivered by I1–I5: the shared
`orbit-backup` service and `.orbitbak` format, restore engine, server/Web GUI
management, automatic lifecycle and the rehearsal/acceptance evidence.

## Impact classification

| Impact | Value | Reason | Activated gate |
|---|---|---|---|
| public_api | yes | New CLI commands (`orbit backup create`, `list`, `inspect`, `verify`, later `restore`) and new `.orbitbak` archive/manifest format contracts; later slices also add server HTTP endpoints. No existing CLI or HTTP contract is changed. | Compatibility gate: CLI contract tests, archive format-version and manifest compatibility tests, server HTTP tests in I3. |
| persisted_data | no | No authority database schema change. The work adds archive artefacts (`.orbitbak` files) and later backup configuration, but no migrated table or changed stored record shape. | none |
| configuration | yes | New backup configuration in I4: mode (disabled/daily/weekly/pre-upgrade), destination and retention policy, validated at startup with safe defaults. | Configuration gate: configuration parsing/validation tests and fail-closed startup cases. |
| dependencies | no | No new external crate, service or cloud dependency is expected; the archive and snapshot work uses the existing SQLite API and standard library. | none |
| security_boundary | yes | Restore is destructive and administrative: it must require explicit authorization and a maintenance boundary, reject path traversal, and exclude secrets from the archive by an allow-list. The Web GUI must not accept arbitrary browser-supplied server paths. | Security gate: restore authorization tests, traversal/version rejection tests, secret-exclusion tests, local-trusted privilege checks. |
| concurrency | yes | A bounded writer fence defines the snapshot window across the database, vault and attachment references; creation and restore must be safe against concurrent writers and a target-active restore. | Concurrency gate: writer-fence and snapshot-window tests, interrupted-create and target-active restore tests. |
| performance_sensitive | yes | Archiving and verification stream a potentially large vault and attachment store; bounded memory and archive streaming are required. | Performance gate: streaming archive creation/verification checks and a large-attachment fixture. |
| user_interface | yes | I3 adds a Backup & Restore settings section (overview, history, verification, schedule, restore preflight and separated controlled recovery). No accepted M33 surface is redesigned. | UI gate: web unit tests, Playwright backup spec, light/dark desktop and phone captures. |
| deployment | yes | Operators gain a restore/maintenance path and a pre-upgrade recovery point; `orbit update` must fail closed when it cannot create one. | Deployment gate: maintenance/restore runbook, `orbit update` pre-upgrade recovery tests and documentation. |

## Assumptions and unknowns

- The bundled SQLite build exposes the supported online backup API; if not, the
  slice stops rather than copying a live main file.
- Referenced attachment objects can be enumerated from authoritative records inside
  the writer fence; the exact store abstraction is confirmed in I1.
- Normalized configuration is rebuilt from an explicit non-secret allow-list, not
  copied raw.
- Connected-client outbox and staged uploads are server-backup exclusions and must
  be reported and reconciled; they are not silently guaranteed.
- The host currently has no automated backup/restore test suite; I1 establishes the
  first one.
- Whether `orbit update` already exposes a hook for a pre-upgrade recovery point is
  an open implementation question for I4.
- No public-release, M15 or M27 claim is made or implied by this milestone.

## Required outputs

- Shared Rust backup crate/service and `.orbitbak` v1 format, manifest and
  component inventory.
- Fenced online SQLite snapshot and failure-safe archive creation.
- CLI `orbit backup create|list|inspect|verify` (and `restore` in I2).
- Archive-format documentation and path/version/secret rules.
- Unit, integration and failure tests with recorded evidence.
- Later slices: restore engine, server endpoints and Web GUI section, schedules and
  retention, `orbit update` recovery point, rehearsal and documentation.
- MILESTONE-0034 evidence updates, TASK-0016 acceptance/evidence, and journal
  entries as each slice lands.

## Test tier

- Tier 4 for the milestone overall: new backup/restore tests plus
  `cargo test --workspace --offline`, clippy `-D warnings`, fmt, the web typecheck,
  unit and build gates, the full Chromium suite when I3 lands, the documentation
  suites, and `getdone-validate-project`.
- Tier 3 for I1 alone: the new backup crate tests plus the affected workspace Rust
  gates.
