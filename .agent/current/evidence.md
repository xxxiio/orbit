---
template: evidence-manifest
template_version: 2.0.0
template_digest: "2f9c7d6c9e4a0f2b3c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: incomplete
task_id: TASK-0015
---

# Evidence Manifest

## Task

TASK-0015 — M33 I1–I4: professional UX, project workspace, epics and roadmap.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001 | pass | `web/src/main.tsx`, `web/src/styles/globals.css`, `web/tailwind.config.ts`; no Fraunces/Instrument imports remain. |
| AC-002 | pass | `web/src/api/session-history.ts`, `protected-route.tsx`, `tests/unit/api/session-history.spec.ts`, `tests/e2e/auth.spec.ts` (chromium and Pixel 7). |
| AC-003 | pass | `project-workspace-page.tsx` canonical sections; `tests/e2e/project-workspace-nav.spec.ts`; updated registry and saved-view suites. |
| AC-004 | pass | `crates/orbit-runtime/src/authority_epics.rs`; server routes; `tests/e2e/epics.spec.ts` create/assign/complete/restart and Work Item case. |
| AC-005 | pass | `authority_epics::tests::cross_project_assignment_is_refused_without_partial_state`; `epics.spec.ts` cross-project drawer case. |
| AC-006 | pass | `authority_milestones.rs` `set_target`; `roadmap.spec.ts` milestone target case. |
| AC-007 | pass | `web/src/features/roadmap/**`; `tests/unit/features/roadmap-model.spec.ts`; `tests/e2e/roadmap.spec.ts`. |
| AC-008 | pass | Chromium 141 passed / 5 skipped / 0 failed; mobile auth 6 passed. |
| AC-009 | pass | Vitest 63 files / 410 tests; `tsc --noEmit`; terminology 256 files; Vite build. |
| AC-010 | not-run | Human review of the built app and `web/evidence/m33/` has not happened; `NEXT-0020` records the request. |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Rust workspace | pass | `cargo test --workspace --offline`: 31 binaries, 1,848 passed / 0 failed. | pass |
| Rust format | pass | `cargo fmt --all -- --check`. | pass |
| Clippy (touched crates) | pass | runtime/server/sync with `-D warnings`. | pass |
| TypeScript | pass | `node node_modules/typescript/bin/tsc --noEmit`. | pass |
| Vitest | pass | 63 files / 410 tests. | pass |
| Vite build | pass | `web/dist` emitted. | pass |
| Terminology | pass | `check-terminology.mjs`, 256 files. | pass |
| Chromium E2E | pass | 141 passed / 5 skipped / 0 failed. | pass |
| Mobile auth | pass | 6 passed on Pixel 7. | pass |
| Visual captures | pass | `web/evidence/m33/`, 24 PNGs at three sizes and two themes. | pass |

## Checks not run

- Human visual/usability review (EC-008/AC-010).
- Live agent execution and M15/M27 release qualification.
- Drag-to-reschedule and per-edge status explanations (deliberate gaps).

## Waivers

- None.

## Residual risk

- Screenshots are evidence, not human approval.
- A child-derived epic bar is labelled as an inference; undated items stay unscheduled.
- Browser-only mechanisms are documented as not portable to the TUI.
