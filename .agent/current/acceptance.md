---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: passed
task_id: TASK-0010
---

# Current Acceptance Report

## Task

TASK-0010 — M32 I1: project rename/status, unified search, saved views and task
timestamps.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Project rename and status through authoritative commands | pass | `ProjectCommandRequest` `action:"update"` (`crates/orbit-runtime/src/store.rs`), domain `Project::rename`/`restore`/`ProjectStatus::parse`/`ALL`, `GET /v1/authoritative/project-statuses`. Order is envelope → receipt replay → revision fence → domain, so a stale fence answers 409 before any existence or no-op check. 8 runtime command tests, 5 server HTTP tests, and `web/tests/e2e/project-settings.spec.ts` 6/6 (rename on every surface with a stable id, status persistence, server-offered edges plus a refused illegal move, stale revision that never claims success, Cancel, blank name). |
| G-0002 | Unified search across projects, tasks and knowledge | pass | `crates/orbit-runtime/src/search.rs` (escaped `LIKE`, `LOWER()` on both sides, `limit + 1` per group instead of COUNT, project filter) + `GET /v1/authoritative/search`; notes reuse the file-backed vault search, no new index. 7 runtime tests and 5 server tests; `search.spec.ts` 5/5 (three kinds, each destination, empty query keeps commands, honest empty state, keyboard + focus, unauthenticated 401, stale note target). |
| G-0003 | Saved views persist and stay out of workflow definitions | pass | `crates/orbit-runtime/src/saved_views.rs` with its own tables and receipts; a test asserts the authority revision does not move and the Playwright spec re-reads the workflow binding before/after. 9 runtime tests, 2 server tests; `saved-views.spec.ts` 6/6 including a second browser context and a stale-version refusal. |
| G-0004 | Authoritative task timestamps with unknown state | pass | `AuthorityStore::task_timestamps` derives `MIN(kind='created')` and `MAX(occurred_at_ms)` from `activities`; `TaskDetailResponse` carries both as nullable. Runtime tests cover known, advanced and unknown; `task-metadata.spec.ts` 2/2; the `Unknown` rendering branch is unit-tested because API-created tasks always have history. |
| G-0005 | Browser acceptance and quality gates | pass | Full gates below, run by the orchestrator on the reconciled tree. |

## Commands run

```text
cargo fmt --all -- --check                                   # clean
cargo test --workspace --offline                             # 1,730 passed, 0 failed, 31 binaries, exit 0
cargo clippy -p orbit-domain -p orbit-application -p orbit-runtime -p orbit-server -p orbit-client \
  --all-targets --offline -- -D warnings                     # clean
cd web && node node_modules/typescript/bin/tsc --noEmit      # exit 0
cd web && node node_modules/vitest/vitest.mjs run            # 46 files, 249 tests passed
cd web && node scripts/check-terminology.mjs                 # 230 files, no legacy feature name
cd web && node node_modules/vite/bin/vite.js build           # built (pre-existing chunk-size warning only)
cd web && node node_modules/@playwright/test/cli.js test <four I1 specs>   # 19 passed
cd web && node node_modules/@playwright/test/cli.js test     # 106 passed, 4 skipped, 0 failed (5.5 min)
```

Hand-run wire check against a disposable server on a fresh database: create →
`action:"update"` (rename + status) → replay `duplicate:true` at the original
revision → stale fence 409 before domain checks → six statuses with legal edges →
grouped search with server subtitles → task-detail timestamps → saved-view
save/list; empty `q` is 400.

## Checks not run

- M15 full release profile and M27 publication verification (unchanged external
  release qualification, deliberately not regenerated).
- Live Herdr/model acceptance (unchanged external requirement).

## Waivers

- None.

## Residual risk

- Project completion criteria are not modelled in the authority SQL project row,
  so a status move validates transition edges and name rules, not
  success-criterion gating — the same limitation the authority create path has.
- The board carries no URL-backed filter state, so a saved board view restores the
  display plus whatever query the URL already has.
- Search case folding is ASCII (SQLite `LOWER`), stated rather than approximated.
- Saved-view ids are primary keys across soft deletes, so a tombstoned id cannot be
  reused; the client mints fresh ids.
