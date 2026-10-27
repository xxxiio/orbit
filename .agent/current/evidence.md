---
template: evidence-manifest
template_version: 1.0.0
template_digest: "2787d570257f4ffbf8add20f62aa21227a4aea1a6723583e8928b5e71e09f79b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: final
task_id: TASK-0014
---

# Evidence Manifest

## Task

TASK-0014 — M32 I5–I6: dogfooding, final acceptance, installation and TUI handoff.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001: Persistent-workspace dogfooding | pass | `.slim/deepwork/m32-web-completeness/i5-dogfood/session.md` (nine sessions, 23 captures, step JSON, trace) and `.agent/evidence/m32-i5-acceptance.md`. |
| AC-002: Blocker fixed with regression proof | pass | `orbit_planner::deadline_bound_at` + `schedule_planning`/`portfolio_schedule`/`planning_approval`; tests `overdue_deadline_is_proposed_and_reviews_cleanly`, `a_task_whose_deadline_already_passed_can_still_be_reviewed`; the dogfooding step 06b now passes on the same workspace. |
| AC-003: Full browser acceptance | pass | 136 passed, 5 skipped, 0 failed. |
| AC-004: Reproducible clean install | pass | `.slim/deepwork/m32-web-completeness/i6-install.md` and `.agent/evidence/m32-i6-acceptance.md`; byte-identical `dist`. |
| AC-005: TUI specification | pass | `docs/guides/tui-redesign-spec.md` (18 patterns, named APIs, A1–A23); no TUI change. |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Workspace tests | pass | `cargo test --workspace --offline` | 1,844 passed, 0 failed |
| Lints | pass | `cargo clippy … -D warnings` | clean |
| Formatting | pass | `cargo fmt --all -- --check` | clean |
| Client typecheck / unit tests | pass | `tsc`, `vitest` | 0 / 393 tests |
| Terminology guard | pass | `check-terminology.mjs` | 256 files |
| Documentation suite | pass | `pytest tests/documentation` | 39 passed |
| Production build | pass | `vite build` | built |
| Browser acceptance | pass | `playwright test` | 136 passed, 5 skipped, 0 failed |
| Dogfooding session | pass | `ORBIT_DOGFOOD=1 playwright test tests/e2e/dogfooding` | 1 passed (23 steps; plan step ok) |
| Project records | pass | `getdone-validate-project` | passed |

## Checks not run

- Plain `npm ci` on a native-Linux-npm host, an offline install, and a clean-machine
  Playwright run (browser libraries bundle). Recorded as unverified.
- Live Herdr/model acceptance; M15/M27 release qualification.

## Waivers

- None.

## Residual risk

- Three low-severity dogfooding findings remain open and recorded (D-04, D-05,
  D-06/06b).
- The clean-install proof relies on the documented workaround for this host's npm.
