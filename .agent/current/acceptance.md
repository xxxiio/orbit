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

TASK-0015 — M33 corrective iteration: Milestone unification, full task creation,
unified Tasks workspace, compact Board and Milestone Roadmap.

M33 remains **active**. The corrected implementation, legacy conversion, restart,
targeted browser acceptance and representative captures are in place. The final full
Chromium rerun and the human review (EC-010) are still owed, so this report does not
claim completion or approval.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Typography and visual system | pass | Inter self-hosted, neutral tokens and restrained scale; retained from the first pass. |
| G-0002 | Auth first-visit/expiry/revocation | pass | `orbit.auth.seen` marker plus the retained unit and Playwright auth coverage. |
| G-0003 | Milestone authority consolidation | pass | Rich Milestone authority, Task/Work Item membership, progress, activity, guards and one-time conversion. Rust workspace 1862 passed / 0 failed; Oracle gate 1 resolved. |
| G-0004 | Rich task creation | pass | One accepted command with every documented field, criteria and prerequisites; Rust `m33_rich_task_create`; `task-create-full.spec.ts`. |
| G-0005 | Unified Tasks workspace | pass | One Tasks tab, `?view=list\|board\|backlog`, shared URL filters and legacy redirects; `project-workspace-nav` and `tasks-workspace` specs. |
| G-0006 | Compact Board | pass | Compact default with `orbit.board.density`, priority/deadline/blocked cues, on-demand inspector, authoritative moves; `tasks-workspace.spec.ts`. |
| G-0007 | Milestone Roadmap | pass | One Milestone hierarchy with duration/checkpoint/unscheduled shapes, child windows, progress/blocked/dependency cues; `roadmap.spec.ts`. |
| G-0008 | Automated web gates | pass | 458 unit tests pass; `tsc --noEmit`, terminology and production build are clean. |
| G-0009 | Real-browser acceptance (EC-009) | pass | Full Chromium suite: 150 passed, 5 env-gated skips, 0 failed; 28 captures in `web/evidence/m33-corrective/`. |
| G-0010 | Human review (EC-010) | not-run | No human has reviewed the corrected build. This report does not claim approval. |
| EC-001 | Retained typography criterion | pass | Inter and the restrained scale remain. |
| EC-002 | Retained auth criterion | pass | First visit is quiet; expiry/revocation is explained; sign-out is quiet. |
| EC-003 | One grouping entity | pass | Epic is gone from `web/src`, the unit suites and the authority; only a decode-only sync tag remains. |
| EC-004 | Rich Milestone aggregate | pass | Authority, UI, conversion and restart proof; 142 Tasks preserved on the real dogfood copy. |
| EC-005 | Full task create | pass | One accepted command; dialog, unit and browser coverage. |
| EC-006 | One Tasks entry | pass | Unified workspace, URL views and legacy redirects. |
| EC-007 | Compact Board | pass | Compact density, card cues, inspector on demand and authoritative moves. |
| EC-008 | Milestone Roadmap | pass | Milestone hierarchy and duration/checkpoint/unscheduled shapes. |
| EC-009 | Real-browser acceptance | pass | Full Chromium suite 150 passed, 5 env-gated skips, 0 failed; captures at both sizes and themes. |
| EC-010 | Human review | not-run | Human approval remains open. |

## Commands run

```text
cargo test --workspace --offline                               # 31 binaries, 1862 passed, 0 failed
cargo clippy -p orbit-runtime -p orbit-server -p orbit-mcp ... # clean
cargo fmt --all -- --check                                     # clean
cd web && node node_modules/typescript/bin/tsc --noEmit        # exit 0
cd web && node node_modules/vitest/vitest.mjs run              # 72 files, 458 tests passed
cd web && node scripts/check-terminology.mjs                   # clean
cd web && node node_modules/vite/bin/vite.js build             # clean
```

## Checks not run

- Human visual/usability review of the corrected build and captures (EC-010).
- Live agent execution and M15/M27 release qualification (unchanged).

## Waivers

- None.

## Residual risk

- The final full-suite rerun is still in progress; targeted suites cover the changed
  Milestones section, navigation, planning, roadmap and responsive/a11y routes.
- The Development workflow keeps a declared mixed Board because it also owns a Work
  Item surface; documented as the workflow exception.
- Screenshots cannot judge comfort, trust or visual taste; the human gate stays open.
