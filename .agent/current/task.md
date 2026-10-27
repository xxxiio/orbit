---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0014
status: completed
milestone_id: MILESTONE-0032
---

# Task: M32 I5–I6 — dogfooding, final acceptance, installation and TUI handoff

## Objective

Close M32: use the product for real development management across repeated sessions,
fix the highest-impact defects the use exposes, prove a reproducible clean install,
and hand the next milestone a TUI specification derived from proven behaviour.

## Current behaviour

I1–I4 shipped the operation surface with real-browser coverage. What was missing was
evidence of sustained use, a clean-install proof, and a TUI specification matching
the larger product.

## Desired behaviour

A persistent-workspace dogfooding session exercises the daily workflows end to end
and records defects honestly; blockers are fixed in the owning layer with regression
tests; the full browser suite covers every added operation; a lockfile-only install
builds byte-identically; and the TUI specification classifies each proven pattern
with the shared API it needs.

## Scope

- Nine-session dogfooding run with captures, step logs, a defects table and fixes.
- Fixes: plan review refusing the server's own proposal; silent acceptance-criterion
  draft loss; settled capability request still offered; unreadable planning-failure
  copy.
- Final real-browser acceptance; clean-install evidence with the host limitation
  recorded.
- TUI redesign specification rewrite and stale documentation corrections.

## Out of scope

- Any TUI implementation; M15/M27 release qualification; the three low-severity
  dogfooding findings recorded but left open (D-04, D-05, D-06/06b).

## Dependencies

- M32 I1–I4 committed; the dogfooding workspace; the M31 TUI specification.

## Risks

- A scripted session can be mistaken for human use; the record labels it and lists
  what still needs an operator.
- A dogfooding finding can look like a test artefact; the blocker was reproduced in
  five separate sessions before it was fixed.
- Clean-install claims can overreach on a host with a broken npm; the limitation is
  recorded instead of worked around in the tooling.

## Acceptance criteria

- [x] AC-001: A persistent-workspace dogfooding session runs repeatedly without
  resetting, with a labelled record, captures and a defects table.
- [x] AC-002: The blocking dogfooding defect is fixed in the owning layer, with a
  regression test, and the same session now completes the step it blocked.
- [x] AC-003: Full browser acceptance covers every operation added by M32.
- [x] AC-004: A lockfile-only installation builds byte-identically, and the
  supported loop is run end to end with the host limitation stated.
- [x] AC-005: The TUI specification reflects proven M31/M32 usage and names the
  shared APIs it needs; no TUI change is implemented.

## Validation

```sh
cargo test --workspace --offline
cargo clippy -p orbit-planner -p orbit-runtime -p orbit-server -p orbit-storage -p orbit-cli -p orbit-blueprint --all-targets --offline -- -D warnings
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node node_modules/@playwright/test/cli.js test
ORBIT_DOGFOOD=1 cd web && node node_modules/@playwright/test/cli.js test tests/e2e/dogfooding
python3 -m pytest -q tests/documentation
getdone-validate-project
```
