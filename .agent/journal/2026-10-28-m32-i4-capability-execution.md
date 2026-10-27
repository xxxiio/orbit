---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-28"
task_id: TASK-0013
---

# Journal Entry: 2026-10-28 — M32 I4 connected Workflow Capability execution

## Context

Capability execution was the last operation the local authority could perform and
the connected server explicitly refused. I4 closed that gap without letting the
server or the browser execute provider code: the server persists an authorized,
fenced intent, and a separately enrolled executor claims and runs it.

## Work completed

- Contract-only capability catalog split out of the trusted composition
  (descriptors, action specs, input schemas, effect declarations, permissions) so
  the server can validate and offer without any executor.
- Durable executor enrollment (`capability-executors/announce` + roster,
  bearer-only) and executor-only claim/result routes.
- Capability status, offers with the durable availability vocabulary, fenced
  request, show/list, reconcile, and operator-only install/authorize/revoke.
- Single atomic claim with owner + unique token (same device+token replays,
  anything else 409); claims survive restart and are never reset.
- Settlement writes request state, receipt and the **declared effect** in one
  savepoint, keyed by command id; `capability-effects` reads the durable result.
- Publication of a capability-bearing workflow against the durable installed set,
  never a body-supplied one.
- `orbit capability serve --once`: announces what this build can execute, claims
  one pending intent, runs the frozen plan through the local provider, reports one
  outcome. Connected read/request/reconcile parity; `execute` refuses and names the
  runner.
- Browser capability panel: status, availability codes (with `not_authorized`
  distinct from `executor_missing`), schema-generated inputs, confirmation, all
  five states, verbatim receipt, and reconcile for `uncertain` and `claimed`.
- Acceptance drives the real CLI executor from Playwright: full cycle, refusals,
  single claim, replay, reconcile, restart durability.

## Decisions made

- The server never loads provider code and never dispatches. The contract catalog
  is compile-time data; nothing in a request can name a provider, a permission, an
  action list or a transition.
- An executor is an active enrolled device that announced the capability; a cookie
  session is refused on claim/result and on install/authorize/revoke.
- The connected path settles state + receipt + effect atomically instead of reusing
  the local three-step write, which could lose a receipt.
- The effect is declared and schema-checked; provider free text in `detail` is
  evidence, not authority. Lane N's scratch-authority receipt
  (`applied_to_authority: false`) was rejected as insufficient and replaced.

## Files or components changed

- `crates/orbit-runtime/src/{capability_contracts.rs,capability_server.rs,
  capability_composition.rs,lib.rs}`,
  `crates/orbit-storage/src/{capability_installs.rs,blueprint_actions.rs}`,
  `crates/orbit-server/src/{main.rs,blueprints.rs}`,
  `crates/orbit-cli/src/{capability_serve.rs,connected_blueprint.rs,
  blueprint_commands.rs,commands.rs,lib.rs}`.
- `web/src/api/**` capability modules, `web/src/features/workflows/capability-*`,
  `work-item-sheet.tsx`.
- `web/tests/{unit,e2e}/**` including `harness/capability.ts` and
  `capability-execution.spec.ts`.
- `docs/web-app.md`.

## Validation performed

- `cargo test --workspace --offline` → 1,837 passed, 0 failed, 31 binaries.
- clippy `-D warnings` clean on runtime/server/storage/cli/blueprint; fmt clean.
- `tsc` 0; vitest 59 files / 383 tests; terminology 256 files; `vite build` built.
- Playwright: 5 focused capability checks, 136 full, 4 opt-in skips, 0 failed.
- Live transcript (lane Q, disposable server): install → publish → bind → item →
  authorize → announce → offer → request → `capability serve --once` → effects read
  → restart → second run idle → still one note.

## Problems encountered

- The first executor implementation ran the provider against a scratch authority and
  only reported `applied_to_authority: false`; the workspace never changed. Replaced
  with a declared, validated, atomically persisted effect.
- The action input schema bounded `note` but not `tag`, while the effect contract
  bounded `tag` at 64; a longer tag stranded a request at `claimed`. Both bounds now
  derive from one constant with a drift test.
- The browser offered reconcile only for `uncertain`, leaving a lost-acknowledgement
  `claimed` request unsettleable; both states now reconcile with distinct copy.
- Two stale browser assertions describing the pre-I4 panel were updated to assert
  the shipped behaviour rather than weakened.

## Remaining work

- M32 I5 (extended dogfooding) and I6 (final acceptance, clean installation, TUI
  redesign specification).

## New TODOs

- None.

## Risks or blockers

- Capability administration is operator-only and there is no uninstall route, so a
  `capability_not_installed` offer cannot be produced over HTTP.
- An action does not move the work item; the receipt and effect are the durable
  result by design.
