---
template: evidence-manifest
template_version: 1.0.0
template_digest: "2787d570257f4ffbf8add20f62aa21227a4aea1a6723583e8928b5e71e09f79b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: final
task_id: TASK-0010
---

# Evidence Manifest

## Task

TASK-0010 — M32 I1: project rename/status, unified search, saved views and task
timestamps.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001: Project rename and status editing | pass | `crates/orbit-domain/src/project.rs` (`rename`, `restore`, `ProjectStatus::{ALL,parse}`), `crates/orbit-runtime/src/domain_commands.rs` (`ProjectCommandRequest`, `ProjectCommandAction`), `crates/orbit-runtime/src/store.rs` (`authority_project_command` update path, `append_command_activity`, `latest_field_version`), `crates/orbit-runtime/src/project_command_tests.rs` (8), `crates/orbit-server/src/main.rs` (status edges + 5 HTTP tests); client `project-identity-form.tsx`, `project-edit.ts`; `web/tests/e2e/project-settings.spec.ts` 6/6. |
| AC-002: Unified search | pass | `crates/orbit-runtime/src/search.rs`, `search_tests.rs` (7), `GET /v1/authoritative/search`; client `adapters/search.ts`, `endpoints/search.ts`, `command-palette.tsx`, `app-shell.tsx`; `web/tests/e2e/search.spec.ts` 5/5, `web/tests/unit/api/search-adapter.spec.ts`. |
| AC-003: Saved views | pass | `crates/orbit-runtime/src/saved_views.rs`, `saved_views_tests.rs` (9, including "a saved view never moves the authority revision"), server list/command routes; client `features/saved-views/**`, `mutations/saved-views.ts`, `endpoints/saved-views.ts`; `web/tests/e2e/saved-views.spec.ts` 6/6, `web/tests/unit/features/saved-views.spec.ts`. |
| AC-004: Task timestamps | pass | `AuthorityStore::task_timestamps` + `TaskDetailResponse` timestamps and server test `task_detail_reports_history_derived_timestamps`; client `task-timestamps.tsx`, `formatTaskTimestamp`; `web/tests/e2e/task-metadata.spec.ts` 2/2, `web/tests/unit/domain/task-timestamps.spec.ts`. |
| AC-005: Quality gates | pass | See the gate table below. |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Workspace tests | pass | `cargo test --workspace --offline` | 1,730 passed, 0 failed, 31 binaries, exit 0 |
| Lints | pass | `cargo clippy -p orbit-domain -p orbit-application -p orbit-runtime -p orbit-server -p orbit-client --all-targets --offline -- -D warnings` | clean |
| Formatting | pass | `cargo fmt --all -- --check` | clean |
| Client typecheck | pass | `node node_modules/typescript/bin/tsc --noEmit` | exit 0 |
| Client unit tests | pass | `node node_modules/vitest/vitest.mjs run` | 46 files, 249 tests passed |
| Terminology guard | pass | `node scripts/check-terminology.mjs` | 230 files, no user-visible legacy feature name |
| Production build | pass | `node node_modules/vite/bin/vite.js build` | built; only the pre-existing chunk-size warning |
| Browser acceptance (I1) | pass | `playwright test` on the four I1 specs | 19 passed |
| Browser acceptance (full) | pass | `playwright test` | 106 passed, 4 skipped (opt-in captures), 0 failed |
| Acceptance record | pass | `.agent/evidence/m32-i1-acceptance.md` | written from the runs above |

## Checks not run

- M15 full release profile and M27 publication verification (release
  qualification remains M15/M27; deliberately not regenerated).
- Live Herdr/model acceptance (unchanged external requirement).

## Waivers

- None.

## Residual risk

- Project completion-criteria gating is not modelled in the authority SQL project
  row; a status move validates edges and name rules only.
- A saved board view restores display plus the current URL query, because the board
  has no URL-backed filters yet.
- Search folding is ASCII, matching SQLite `LOWER`.
- A soft-deleted saved-view id keeps its primary key and cannot be reused.
