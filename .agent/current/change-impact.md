---
template: change-impact
template_version: 1.0.0
template_digest: "8618a859250c7346e16cb5e20990e6c6d496e6b238ab8f44824028eba527e134"
project_owned: true
record_contract: change-impact
record_schema_version: 1
status: current
task_id: TASK-0009
---

# Change Impact Declaration

## Task

TASK-0008 — M30 I1: TUI workspace simplification and trusted Blueprint capability actions.

## Impact classification

| Impact | Value | Reason | Activated gate |
|---|---|---|---|
| public_api | yes | New local `OrbitClient` capability methods, TUI backend wrappers and CLI subcommands. | Compatibility gate: client/CLI tests and acceptance workflow. |
| persisted_data | yes | New durable capability installation, grant, receipt and reference-note tables; request/claim/result settlement extended. | Migration gate: storage/runtime matrices and restart/replay acceptance. |
| configuration | no | No new configuration keys or defaults. | none |
| dependencies | no | No new crate dependency; existing runtime/storage/client stack reused. | none |
| security_boundary | yes | First public path that can execute a trusted capability; must fail closed and never inherit authority from a manifest. | Security gate: runtime availability/rejection matrix, connected refusal, no HTTP route. |
| concurrency | yes | Single-claim fencing and atomic settlement of `claimed`/`uncertain` must stay race-safe. | Concurrency gate: claim/settlement tests. |
| performance_sensitive | no | Inspector builds on input paths only; render purity preserved and test-enforced. | Render-purity tests. |
| user_interface | yes | Default layout changes to two regions and gains inspector and capability-action surfaces. | UI gate: feature tests at three sizes, PTY scenarios, captures. |
| deployment | no | No installer, release workflow or packaging change. | none |

## Assumptions and unknowns

- Capability execution remains local-authority only in this iteration; connected
  parity is M25 EC-004 and is asserted as a refusal, not simulated.
- The reference capability's effect is internal and deterministic; no external
  service, Python, Herdr or network access is required.
- Existing databases gain the new tables through the same additive schema-init
  path used by earlier blueprint tables; no migration version bump was needed
  because no existing column changed.
- `uncertain` cannot be produced by the reference capability, so the runtime test
  matrix supplies a synthetic executor for that outcome and the TUI covers it at
  the modal/formatting layer.

## Required outputs

- Two-region TUI shell with explicit inspector and preserved context.
- Durable capability installation, grants, receipts and reconciliation.
- Public composition API plus CLI programmatic path.
- TUI capability action discovery, input form, confirmation and dispositions.
- Tests, acceptance workflow, documentation and records.

## Test tier

- Tier 4: full workspace tests, clippy across affected crates, contract and
  documentation suites, TUI PTY suite, isolated end-to-end capability acceptance
  and `getdone-validate-project`. Release qualification remains M15/M27.
