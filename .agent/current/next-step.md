---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0019
status: completed
milestone_id: MILESTONE-0032
advances_exit_criteria: [EC-001, EC-002, EC-003, EC-004, EC-009, EC-011]
---

# Next Deterministic Step

## Objective

Close M32 with evidence rather than assertions: a persistent-workspace dogfooding
record, the blocker it found fixed, full browser acceptance, a reproducible clean
install, and a TUI specification derived from the shipped product.

## Why this is next

I1–I4 completed the operation surface. M32's completion depends on having used it,
on having fixed what the use exposed, and on handing the next milestone a
specification built from evidence instead of taste.

## Preconditions

- M32 I1–I4 committed (`1c27354`, `b571d97`, `0cbb79b`, `2212ca3`).
- The dogfooding workspace from I5 and the M31 TUI specification exist.

## Inputs

- `.slim/deepwork/m32-web-completeness/{i5-dogfood,i6-install.md}`.
- `docs/guides/tui-redesign-spec.md`, `docs/web-app.md`.
- The full browser suite and workspace tests.

## Ordered actions

1. Run the dogfooding session on a persistent workspace, record it honestly and fix
   the highest-impact findings in their owning layer.
2. Re-run the session to prove the blocker is gone.
3. Run the full gates and record the numbers.
4. Reproduce a lockfile-only install and the supported loop; record what this host
   cannot verify.
5. Rewrite the TUI specification from proven M31/M32 usage and correct stale docs.

## Expected outputs

- A labelled dogfooding record with captures and a defects table.
- Fixes with regression tests, and the same session passing the blocked step.
- Full acceptance numbers, clean-install evidence, an updated TUI specification.

## Acceptance criteria

- [x] NS-001: The dogfooding session resumes on the same database and records its
  defects.
- [x] NS-002: The blocker (plan review refusing the server's proposal) is fixed and
  the session's plan step passes.
- [x] NS-003: Full browser acceptance passes with every M32 operation covered.
- [x] NS-004: A lockfile-only install builds byte-identically and the loop runs.
- [x] NS-005: The TUI specification reflects proven usage and implements nothing.
- [x] NS-006: `getdone-validate-project` exits 0.

## Validation

```sh
cargo test --workspace --offline
cd web && node node_modules/@playwright/test/cli.js test
ORBIT_DOGFOOD=1 cd web && node node_modules/@playwright/test/cli.js test tests/e2e/dogfooding
python3 -m pytest -q tests/documentation
getdone-validate-project
```

## Stop conditions

- A dogfooding finding would need a redesign rather than a fix in the owning layer.
- A clean-install claim cannot be reproduced here.

## Out of scope

- M15/M27 release qualification and any TUI implementation.
- Any further step: M32 is complete and the next milestone (the TUI redesign) is not
  yet defined in `.agent/roadmap.md`, so no next-step is valid until it is approved.
