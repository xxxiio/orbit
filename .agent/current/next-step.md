---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0020
status: ready
milestone_id: MILESTONE-0033
advances_exit_criteria: [EC-008, EC-009]
---

# Next Deterministic Step

## Objective

Close M33 with the evidence it still lacks: the full Rust workspace gate and a
human visual/usability review of the built app. Then run the persistent-workspace
dogfooding script against the final build and record any defects it finds.

## Why this is next

The implementation and automated acceptance are complete enough to review. M33
must not be called complete from DOM assertions and my own screenshots; the
definition of done requires a person to judge the actual interface, and the Rust
workspace gate has not yet been recorded after the Epic module landed.

## Preconditions

- M33 I1–I4 changes in the working tree; `web/dist` built from them.
- A local server with a device credential and the persistent dogfooding database.
- The browser-libraries bundle at `/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu`.

## Inputs

- `web/evidence/m33/` captures and README.
- `web/tests/e2e/{epics,roadmap,project-workspace-nav}.spec.ts`.
- `crates/orbit-runtime/src/authority_epics.rs` and its tests.
- `docs/web-app.md`, `docs/guides/tui-redesign-spec.md`.

## Ordered actions

1. Run `cargo test --workspace --offline` and record the exact totals; fix any
   failure in the owning layer.
2. Run `cargo fmt --all -- --check` and clippy on the touched crates.
3. Give a human the exact build/run/sign-in commands and ask them to review the
   light and dark desktop, laptop and phone captures against the running app.
4. Run the persistent dogfooding flow (create two epics, assign tasks and a Work
   Item, set milestone targets, edit dates, complete a task, restart) and record
   the steps and findings under `.slim/deepwork/m33-web-ux/`.
5. Fix only defects the review or session exposes; re-run the affected gates.
6. Update the M33 milestone status and acceptance record honestly.

## Expected outputs

- Rust workspace test output with a pass/fail count.
- A human review note naming what they inspected and any issues.
- A dogfooding record with screenshots and persistence proof.
- M33 marked completed only when EC-008 and EC-009 are satisfied.

## Acceptance criteria

- [ ] NS-001: `cargo test --workspace --offline` passes, or every failure is
  recorded with an owner and a waiver.
- [ ] NS-002: A human has reviewed the built app at desktop and phone sizes in
  both themes and their findings are recorded.
- [ ] NS-003: The persistent dogfooding flow passes its create/assign/progress/
  date/restart steps on one database.
- [ ] NS-004: M33 milestone progress reflects actual exit criteria, not line count.

## Validation

```sh
cargo test --workspace --offline
cargo fmt --all -- --check
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node scripts/check-terminology.mjs
```

## Stop conditions

- A human review cannot be performed in this environment: record the manual steps
  and leave EC-008 open rather than claiming approval.
- A workspace failure points at an unrelated pre-existing area: record it, do not
  waive it silently.

## Out of scope

- TUI implementation, public release, or redesigning M31/M32 screens that M33 did
  not touch.
