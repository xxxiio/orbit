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

TASK-0015 — M33 corrective iteration: Milestone unification, full task creation,
unified Tasks workspace, compact Board and Milestone Roadmap.

M33 stays active. The corrective implementation, the opt-in local-trusted
authentication mode and the automated/browser evidence are in place; the human review
(EC-010) and the authentication experience review (EC-011) remain the open gates.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| EC-001 | pass | Inter is self-hosted and the restrained type scale is retained (`web/src/styles/globals.css`, `web/tailwind.config.ts`). |
| EC-002 | pass | First-visit/expiry behaviour stays in `session-history.ts`, `protected-route.tsx` with unit and Playwright auth coverage. |
| EC-003 | pass | `grep -rin epic web/src web/tests/unit` is empty; `authority_epics.rs` and the Epic server routes are deleted; the conversion test drops the legacy tables; `EntityKind::Epic` remains only as a decode-only tag. |
| EC-004 | pass | Rich authority in `crates/orbit-runtime/src/authority_milestones.rs` and the store-open conversion in `store.rs`; runtime 1862-test workspace gate; real dogfood database converted 2 Epics to 2 Milestones with 142 Tasks preserved, 41 milestone links, revision 889 after restart. |
| EC-005 | pass | One accepted create command with all fields, criteria and prerequisites in `store.rs`; `web/src/features/tasks/create-task-dialog.tsx`; Rust `m33_rich_task_create`; `web/tests/e2e/task-create-full.spec.ts`. |
| EC-006 | pass | `features/tasks/tasks-workspace.tsx`, `task-list-state.ts`, `project-workspace-page.tsx`; `web/tests/e2e/{project-workspace-nav,tasks-workspace}.spec.ts`. |
| EC-007 | pass | `features/tasks/task-board.tsx`, `components/surface/board.tsx`; compact default, `orbit.board.density`, on-demand inspector, authoritative moves; `tasks-workspace.spec.ts`. |
| EC-008 | pass | `features/roadmap/roadmap-model.ts`, `roadmap-view.tsx`; one Milestone hierarchy with duration/checkpoint/unscheduled shapes; `roadmap.spec.ts`. |
| EC-009 | pass | Full Chromium suite: 150 passed, 5 env-gated skips, 0 failed. Targeted 29/29 and 9/9 after the final UI change. 28 captures at 1440×900 and 390×844, light and dark. |
| EC-010 | not-run | No human has reviewed the corrected build or the corrective captures. |
| EC-011 | pass | `crates/orbit-server/src/auth.rs` (mode, principal, bind and host validation) and the mode-aware guards in `main.rs`; `ORBIT_AUTH_MODE` is opt-in, fails closed, and local-trusted refuses every non-loopback bind before opening storage. `crates/orbit-server/tests/local_trusted_startup.rs` proves refusal and a working loopback listener. `web/tests/e2e/local-trusted.spec.ts` proves the open workspace, a fully configured task with a milestone, List/Board/Backlog, Roadmap, reload, second browser, server restart, the switch back to authenticated mode, and the administrator/operator/executor/machine/CSRF/Origin/Host boundary. `web/evidence/m33-auth/` holds 12 captures and the observed A/B/C results. |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Rust workspace | pass | `cargo test --workspace --offline` | 32 test binaries, 1883 passed, 0 failed. |
| Rust lint/format | pass | `cargo clippy ... -D warnings`; `cargo fmt --all -- --check` | clean. |
| TypeScript | pass | `node node_modules/typescript/bin/tsc --noEmit` | exit 0. |
| Vitest | pass | `node node_modules/vitest/vitest.mjs run` | 75 files, 466 tests passed. |
| Terminology | pass | `node scripts/check-terminology.mjs` | clean, 273 files. |
| Production build | pass | `node node_modules/vite/bin/vite.js build` | clean `web/dist`. |
| Chromium E2E | pass | `node node_modules/@playwright/test/cli.js test --project=chromium` | 152 passed, 6 env-gated skips, 0 failed (25.7 min). |
| Auth browser specs | pass | `local-trusted.spec.ts`, `auth.spec.ts` | 2 passed + 1 capture skip (3 with `ORBIT_CAPTURE=1`); 6 passed. |
| Startup refusal | pass | `cargo test -p orbit-server --test local_trusted_startup` | 2 passed; manual `0.0.0.0` start exited 1 and created no database. |
| Documentation suites | pass | `python3 -m unittest discover -s tests/documentation`; `pytest -q tests/documentation` | 39 passed each. |
| Visual captures | pass | `web/evidence/m33-corrective/`, `web/evidence/m33-auth/` | 28 + 12 PNGs, light/dark, 1440×900 and 390×844, no page or console errors. |

## Checks not run

- Human visual/usability review (EC-010).
- Live agent execution and M15/M27 release qualification.

## Waivers

- None.

## Residual risk

- Human approval is still pending; screenshots and DOM assertions are evidence, not
  approval.
- The Development workflow keeps a declared mixed Board because it also owns a Work
  Item surface; this is the documented workflow exception, not a second task model.
- Screenshots cannot judge comfort, trust or visual taste.
