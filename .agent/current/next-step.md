---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0014
status: completed
milestone_id: MILESTONE-0031
advances_exit_criteria: [EC-002, EC-006, EC-007, EC-009, EC-010]
---

# Next Deterministic Step

## Objective

Complete M31 iteration I6: close the remaining dogfooding findings in their correct layers,
measure and fix the real performance and accessibility gaps, bring the user documentation up
to the shipped product, and derive the TUI redesign specification from the recorded I5 cycle.

## Why this is next

I1–I5 delivered and verified the web application: session auth, daily core, workflow-driven
workspaces, planning, knowledge, attachments, agents, and a real Orbit Development cycle on an
isolated server whose evidence is pinned to a clean commit. What remains is refinement and the
documentation artefacts the milestone requires — including the TUI redesign specification, which
must be derived from that usage rather than assumed.

## Preconditions

- MILESTONE-0031 I1–I5 committed and verified (`3f4101f`, `0ff8af9`, `987bcf1`, `dfcfb8a`).
- I5 issue log with layer classification at `.slim/deepwork/m31-web-app/i5-issues.md`.
- I5 evidence at `web/evidence/i5/` with a reproducible script.

## Inputs

- `web/src/**` (all features), `web/tests/**`, `web/evidence/i5/**`.
- `.slim/deepwork/m31-web-app/{i5-issues.md,i6-plan.md,gate-5.md}`.
- `docs/web-app.md` and the documentation index.

## Ordered actions

1. Close or explicitly record the remaining I5 findings: status controls on rows/table cards,
   the description double-alert, note links, humanized activity summaries, project status,
   board completion shortcut, time-input width, folder creation.
2. Responsive and accessibility sweep across every primary route at 1440/1280/1024/900/390,
   with keyboard-only passes, focus return, aria-live, contrast and reduced motion.
3. Measure cold load, project switch, 122-task list render, board interaction, detail open and
   mutation round trip; fix only measured bottlenecks and record the numbers.
4. Update `docs/web-app.md` and the index for the shipped I3–I6 behaviour, including the honest
   remaining gaps; verify every documented command and shortcut.
5. Write `docs/guides/tui-redesign-spec.md` from the I5 record, classifying each proven pattern.
6. Freeze, run the full suite and the dogfood script, reconcile records and close the milestone
   only for criteria genuinely demonstrated.

## Expected outputs

- Every I5 finding either fixed with a regression test or open with a recorded reason.
- A measured performance/a11y/responsive record, not impressions.
- Documentation that matches the product, using Workflow terminology.
- A TUI redesign specification derived from real usage.
- Green full suite and dogfood evidence on a frozen commit.

## Acceptance criteria

- [ ] NS-001: No primary route overflows horizontally at 390px and every route is keyboard-operable.
- [ ] NS-002: The measured route timings are recorded and any fix cites the measurement it improved.
- [ ] NS-003: `docs/web-app.md` documents only shipped behaviour and every command in it was run.
- [ ] NS-004: `docs/guides/tui-redesign-spec.md` classifies each pattern with evidence from I5.
- [ ] NS-005: The full Playwright suite and the dogfood script pass on a frozen commit.
- [ ] NS-006: Milestone exit criteria are marked only where demonstrated; the rest stay open with reasons.
- [ ] NS-007: `getdone-validate-project --project-root . --skills-root /home/xliu/dev/getdone` exits 0.

## Validation

```sh
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node scripts/check-terminology.mjs
cd web && node node_modules/vite/bin/vite.js build
cd web && node node_modules/@playwright/test/cli.js test
cd web && node tests/e2e/i5/dogfood.mjs
cargo test -p orbit-server -p orbit-runtime -p orbit-blueprint --offline
getdone-validate-project --project-root . --skills-root /home/xliu/dev/getdone
```

## Stop conditions

- A performance or accessibility claim cannot be measured or reproduced.
- Documentation would describe behaviour the build does not have.
- A milestone criterion cannot be evidenced end to end.

## Out of scope

- Reopening M22–M30, connected capability execution, or a TUI implementation change.
- Flutter client work and M27 public release verification.
