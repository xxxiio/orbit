---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: incomplete
task_id: TASK-0015
---

# Current Acceptance Report

## Task

TASK-0015 — M33 I1–I4: professional UX, project workspace, epics and roadmap.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Typography and visual system | pass | Inter self-hosted, Fraunces/Instrument removed; neutral tokens; 14px body/24px titles; no decorative serif; captures in `web/evidence/m33/`. |
| G-0002 | Auth first-visit/expiry/revocation | pass | Boolean `orbit.auth.seen` marker; unit 6 tests; Playwright first visit, sign-out, expiry and revocation pass on chromium and Pixel 7. |
| G-0003 | Canonical project workspace | pass | One each of Overview/List/Board/Backlog/Roadmap/Milestones plus workflow extras; `/tasks` and `/timeline` aliases; tests in `project-workspace-nav.spec.ts` and the updated registry/saved-view suites. |
| G-0004 | Epic authority family | pass | `authority_epics.rs` commands/snapshot/progress/activity; same-project and delete guards; Rust tests; Playwright create/assign/complete/restart and Work Item membership. |
| G-0005 | Milestone target dates | pass | `set_target` on the authoritative milestone command; roadmap/milestone UI; Rust and Playwright tests. |
| G-0006 | Roadmap | pass | Week/month/quarter, epic/task bars, milestone diamonds, dependency indicators, unscheduled work, current-date line; `roadmap-model.spec.ts` and `roadmap.spec.ts`. |
| G-0007 | Full browser acceptance | pass | Chromium 141 passed / 5 skipped / 0 failed; mobile auth 6/6. |
| G-0008 | Unit/build/terminology/Rust | pass | 63 unit files / 410 tests; build succeeds; 256 files terminology clean; Rust 1,848 passed / 0 failed; fmt and clippy on touched crates clean. |
| G-0009 | Human visual review | not-run | A human must review the running build and `web/evidence/m33/`; this report does not claim approval. |

## Commands run

```text
cargo test --workspace --offline                        # 31 binaries, 1,848 passed, 0 failed
cargo fmt --all -- --check                              # clean
cargo clippy -p orbit-runtime -p orbit-server -p orbit-sync --all-targets --offline -- -D warnings   # clean
cd web && node node_modules/typescript/bin/tsc --noEmit # exit 0
cd web && node node_modules/vitest/vitest.mjs run       # 63 files, 410 tests
cd web && node node_modules/vite/bin/vite.js build      # built
cd web && node scripts/check-terminology.mjs            # 256 files
cd web && playwright test --project=chromium            # 141 passed, 5 skipped, 0 failed
cd web && playwright test tests/e2e/auth.spec.ts --project=mobile-chromium  # 6 passed
```

## Checks not run

- Human visual/usability review (EC-008). The build, run and sign-in path is ready
  and `web/evidence/m33/README.md` lists the screens.
- Live Herdr/model execution and M15/M27 release qualification (unchanged).
- Drag-to-reschedule and per-edge status explanations (deliberate non-goals/known
  rough edges).

## Waivers

- None.

## Residual risk

- A scripted browser pass and screenshots cannot judge comfort, trust or visual
  taste; EC-008 remains open.
- Epic membership lives in the authority tables and an epic snapshot; the browser
  never recomputes progress, but the roadmap's child-derived epic envelope is a
  presented inference and is labelled as derived.
- The TUI has not been redesigned; only the specification addendum changed.
