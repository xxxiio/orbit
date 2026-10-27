# M32 I4 — acceptance evidence

Recorded from real runs on the reconciled tree after lanes M (server surface),
N (executor CLI), Q (declared effects), P (browser acceptance) and R (contract
bound + claimed reconciliation) landed. Commands were run by the orchestrator
unless the row says otherwise; the live transactions quoted below were run by the
lanes on disposable servers and are reproduced in their reports.

## Commands and results

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo test --workspace --offline` | 1,844 passed, 0 failed, 31 binaries |
| `cargo test -p orbit-runtime -p orbit-server -p orbit-storage -p orbit-cli -p orbit-blueprint --offline` | runtime 635, server 62, storage 115, CLI 154, blueprint 23, contract 2 — all passed |
| `cargo clippy -p orbit-runtime -p orbit-server -p orbit-storage -p orbit-cli -p orbit-blueprint --all-targets --offline -- -D warnings` | clean |
| `cd web && node node_modules/typescript/bin/tsc --noEmit` | exit 0 |
| `cd web && node node_modules/vitest/vitest.mjs run` | 59 files, 383 tests passed |
| `cd web && node scripts/check-terminology.mjs` | 256 files, no user-visible legacy feature name |
| `cd web && node node_modules/vite/bin/vite.js build` | built |
| `cd web && playwright test capability-execution` | 5 passed |
| `cd web && playwright test` | 136 passed, 4 skipped (opt-in captures), 0 failed |

## What the browser acceptance proves

`capability-execution.spec.ts` drives the real `target/debug/orbit capability serve
--once --json` process from the harness, against a real `orbit-server` per worker:

1. **Full cycle, browser-first.** Install → publish a capability-bearing workflow →
   create a project on it → create a work item → authorize → announce. The panel
   shows installed / granted / executor-announced; the offer is available; the typed
   form (`note`, `tag`) asks for explicit confirmation; the history shows
   `pending` "waiting for an executor". The executor returns `reported`/`succeeded`
   with the effect it produced; the history reaches `succeeded`, the receipt names
   the executor, `GET capability-effects` returns exactly one note with the typed
   text and tag, and the work item's state and revision are unchanged in the server
   read and in the sheet.
2. **Refusals.** Publish before install is `400 blueprint operation rejected`;
   `executor_missing` before an announcement and `not_authorized` after one without
   a grant are distinct and neither offers a request control; cookie sessions are
   `403 operator credential required` / `403 executor credential required`; an empty
   `note` is refused in the form with **zero** POSTs; a stale `expected_revision` is
   `409 capability request conflict`; a forged body carrying `to`/`provider` is 422.
3. **Single claim and idempotency.** A second device's claim is
   `409 capability action already claimed`; replaying the identical result returns
   the byte-identical receipt; exactly one note exists; a second executor run is
   `idle` with still one note.
4. **Reconcile.** An executor reports `uncertain`; the panel offers no retry, only
   the decision; **Confirm it succeeded** settles it, `recovered_from: "uncertain"`,
   `reconciled_by` is the operator device, and no effect is invented. A request
   claimed with no result (`claimed`, no receipt) is settled from the panel with its
   own copy, `recovered_from: "claimed"`. `pending` offers no reconcile.
5. **Restart durability.** After a full cycle, `server.restart()` on the same
   database leaves the request, receipt and effect byte-identical, and a second
   executor run is `idle`.

## Trust boundary

- `orbit-server` contains no `TrustedComposition`, no executor registry and no call
  to `execute_capability_action`, `execute_claimed` or `provider.execute` (grep).
- A runtime test asserts the provider effect never happens in-process
  (`reference_work_item_note_count == 0` after a full request + claim).
- A cookie session cannot announce as an executor or install/authorize as an
  operator; the browser has no claim/result function at all.
- The effect is a **declared, schema-checked** payload written inside the
  settlement savepoint: a receipt can never claim an effect that is not stored, and
  one command id can never produce two notes.

## Defects found and fixed

1. **A capability that "ran" changed nothing (server + executor).** The executor
   ran the provider against a scratch authority and only a receipt statement
   travelled back. Fixed with a declared effect: the capability contract owns the
   kind and schema, the executor submits what it really produced, the server
   validates and persists it in the same savepoint as state and receipt, and
   `capability-effects` reads it back.
2. **An unbounded `tag` stranded requests (server contract).** The action input
   schema bounded `note` but not `tag`, while the effect contract bounded it at 64;
   a longer tag was accepted as a request, the executor ran, the report was refused
   and the request stayed `claimed` forever. The bound is now declared on the action
   and derived from one constant, with a test pinning offer-bound == effect-bound.
3. **A `claimed` request could not be settled from the browser (client).** The
   server accepts reconciling a lost acknowledgement, but the panel only offered
   reconcile for `uncertain`. Both are now offered with distinct, honest copy, no
   retry control anywhere.

## Known limitations, stated rather than hidden

- `capability install|uninstall|authorize|revoke` are operator routes (bearer); the
  product UI requests and reconciles but does not administer, and there is no
  uninstall route at all, so a `capability_not_installed` offer cannot be produced
  over HTTP (the publish-before-install refusal is asserted instead).
- The item does not move when an action runs; the receipt and effect are the
  durable result, and only the item read can report a state change.
- `permission` uses the durable storage vocabulary (`read|mutate|external_effect`).
- Pre-I4 request rows read back with empty `from`/`to`; the server reports only what
  is durable.
