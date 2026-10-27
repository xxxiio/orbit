---
template: evidence-manifest
template_version: 1.0.0
template_digest: "2787d570257f4ffbf8add20f62aa21227a4aea1a6723583e8928b5e71e09f79b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: final
task_id: TASK-0013
---

# Evidence Manifest

## Task

TASK-0013 — M32 I4: connected Workflow Capability execution.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001: Install, grant, publish | pass | `capability_server.rs` install/authorize/revoke + `blueprints.rs` publication against the durable installed set; server HTTP tests; `capability-execution.spec.ts` spec 2. |
| AC-002: Browser offers, inputs, states, receipt | pass | `web/src/features/workflows/{capability-panel,capability-request-form,capability-request-history}.tsx` + `capability-model.ts`; unit tests; specs 1 and 4. |
| AC-003: Executor claim and execution outside the server | pass | `capability_server.rs` claim/result fences, `crates/orbit-cli/src/capability_serve.rs`; grep evidence; specs 1 and 3. |
| AC-004: Reconciliation and idempotent replay | pass | `reconcile` route + `capability-model.reconcileCopy`; specs 3 and 4 (`recovered_from: uncertain` and `claimed`). |
| AC-005: Full cycle gates | pass | Commands above; spec 5 (restart durability, second run idle, one note). |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Workspace tests | pass | `cargo test --workspace --offline` | 1,837 passed, 0 failed |
| Lints | pass | `cargo clippy … -D warnings` | clean |
| Formatting | pass | `cargo fmt --all -- --check` | clean |
| Client typecheck / unit tests | pass | `tsc`, `vitest` | 0 / 383 tests |
| Terminology guard | pass | `check-terminology.mjs` | 256 files |
| Production build | pass | `vite build` | built |
| Browser acceptance (I4) | pass | `playwright test capability-execution` | 5 passed |
| Browser acceptance (full) | pass | `playwright test` | 136 passed, 4 skipped, 0 failed |
| Acceptance record | pass | `.agent/evidence/m32-i4-acceptance.md` | written from the runs above |

## Checks not run

- M15 release profile and M27 publication verification.
- Live Herdr/model acceptance.

## Waivers

- None.

## Residual risk

- Operator-only administration and no uninstall route.
- An action does not move the work item.
