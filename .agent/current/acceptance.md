---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: passed
task_id: TASK-0013
---

# Current Acceptance Report

## Task

TASK-0013 — M32 I4: connected Workflow Capability execution.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Catalog and executor enrollment without provider code in the server | pass | `crates/orbit-runtime/src/capability_contracts.rs` (contract-only catalog built from the existing contract constructors), durable `orbit_capability_executors` enrollment. Grep proof: no `TrustedComposition`, executor registry or `provider.execute` in `orbit-server`; a runtime test asserts the effect never happens in-process. |
| G-0002 | Install/grant/publish/offer/request fences and codes | pass | `capability_server.rs` + 13 routes; availability codes preserved (`not_authorized` vs `executor_missing` kept distinct); C25 publication against the durable installed set; `capability-execution.spec.ts` refusals (publish-before-install, no-control offers, cookie 403s, empty input with zero POSTs, stale revision 409, forged body 422). |
| G-0003 | Executor claim, execution outside the server, atomic receipt + effect | pass | Single claim with owner + unique token (same device+token replays, anything else 409); terminal-only execution in `orbit capability serve --once`; settlement writes state + receipt + declared effect in one savepoint, keyed by command id; one note per command id proven across replay and restart. |
| G-0004 | Browser request → receipt → reconcile | pass | Capability panel: status, availability codes, schema-generated inputs, confirmation, `pending/claimed/succeeded/failed/uncertain`, verbatim receipt; reconcile for `uncertain` and `claimed` with distinct copy and no retry. |
| G-0005 | Full gates including restart durability and no duplicate effect | pass | Commands and browser proofs below. |

## Commands run

```text
cargo fmt --all -- --check                                   # clean
cargo test --workspace --offline                             # 1,837 passed, 0 failed, 31 binaries
cargo clippy -p orbit-runtime -p orbit-server -p orbit-storage -p orbit-cli -p orbit-blueprint   --all-targets --offline -- -D warnings                     # clean
cd web && node node_modules/typescript/bin/tsc --noEmit      # exit 0
cd web && node node_modules/vitest/vitest.mjs run            # 59 files, 383 tests
cd web && node scripts/check-terminology.mjs                 # 256 files, clean
cd web && node node_modules/vite/bin/vite.js build           # built
cd web && playwright test capability-execution               # 5 passed
cd web && node node_modules/@playwright/test/cli.js test     # 136 passed, 4 skipped, 0 failed
```

## Checks not run

- M15 full release profile and M27 publication verification (release
  qualification remains M15/M27; deliberately not regenerated).
- Live Herdr/model acceptance (unchanged external requirement).

## Waivers

- None.

## Residual risk

- Capability administration (install/authorize/revoke) is operator-only and there is
  no uninstall route, so a `capability_not_installed` offer cannot be produced over
  HTTP; the publish-before-install refusal is asserted instead.
- An action does not move the work item; the receipt and effect are the durable
  result.
- `permission` uses the durable storage vocabulary (`read|mutate|external_effect`).
- Pre-I4 request rows read back with empty `from`/`to`.
