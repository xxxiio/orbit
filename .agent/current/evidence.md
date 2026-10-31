---
template: evidence-manifest
template_version: 2.0.0
template_digest: "2f9c7d6c9e4a0f2b3c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: final
task_id: TASK-0020
---

# Evidence Manifest

## Task

TASK-0020 — M34 I5 coverage, failure injection, real recovery rehearsal,
documentation and final acceptance. Together with the completed I1–I4 slices this
closes MILESTONE-0034 as internal engineering acceptance on the frozen tree
(commit `f82e3fa13d4f1fe4ca216aeed105e6efdd5ad250`, dirty with the M34 changes).
No public-release claim is made.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| EC-001 | pass | Verified `.orbitbak` + SHA-256 manifest (relative paths/totals/component inventory); CLI create E2E and `create_verify`/`inspect_list` tests; rehearsal archive 13 inventory files. |
| EC-002 | pass | SQLite online-backup API under a bounded writer fence, WAL handled; fence/snapshot tests; live-server WAL backup released the fence with the server healthy. |
| EC-003 | pass | Staging, verify-before-publish, atomic publish, cleanup and nonzero errors; interrupted create (SIGKILL) left no partial archive or staging; permission-denied destination exit 4 with mode unchanged; source workspace never mutated; `create_private_dir` defect fixed with regression test. |
| EC-004 | pass | `list`/`inspect` on default and external directories (copied `unknown`, corrupt skipped, non-`.orbitbak` ignored); deep `verify` 8/8 and `--structural-only`; tampered archive exit 1. |
| EC-005 | pass | `restore --dry-run` strict non-mutation (recursive file list/hashes unchanged, no destination) with target/source/schema/replaced/preserved/downtime/connected/external fields; `restore_preflight` 13 tests. |
| EC-006 | pass | Isolated restore verified and opened via the CLI with expected tasks/notes, active workspace untouched; `restore_isolated` 6 tests + rehearsal. |
| EC-007 | pass | Replacement requires `--confirm`, preserves previous workspace + recovery journal that reopens, refuses active target (exit 4), interrupted restore leaves no usable partial target; replica recovery targets refused; `restore_replace` 7 / `restore_rollback` 4 tests. |
| EC-008 | pass | `docs/reference/backup-format.md`: ustar container, layout, manifest fields, version compatibility, relative-path rules, traversal rejection, secret exclusion, verification semantics, sidecar; v999 rejected with upgrade message. |
| EC-009 | pass | Web GUI `/settings/backup`: Playwright `backup-restore.spec.ts` 4/4, server 95 tests, full Chromium 156 passed / 6 skipped; path-free responses, generated isolated targets, admin-only offline replacement plan, typed isolated restore. |
| EC-010 | pass | Scheduler verified (~30 s after startup, 15-min policy re-read, no same-UTC-day duplicate, fail-closed on missing attachments); retention protects the last verified point; `policy` 7 / `retention` 8 tests + CLI acceptance. |
| EC-011 | pass | `orbit update` creates and deep-verifies a complete recovery point (`before-update-<from>-to-<to>`), fails closed and replaces no binaries when it cannot, retention after verification, `--check` read-only; helper-level evidence (full network flow not run offline). |
| EC-012 | pass | Rehearsal restored 2 reserved-only agent jobs (queued, never launched), capability install+grant+real execution receipt (`succeeded`, work_item_note), workflow Work Items, notes/references, MCP-authored task change (`authority.flush` `Accepted(1)`), activity; no automatic external re-execution. |
| EC-013 | pass | Real rehearsal 14/14 PASS on a deterministic 103-task dataset; archive SHA-256 `953cb9b22b4ca43be1b45115d111bceb8d64b95247416da8fbeece124c62c874`; all 64 tables equal before/after, DB snapshot SHA-256 identical, vault and attachment tree hashes identical; earlier data present, later changes absent, write + restart persistence. |
| EC-014 | pass | CLI acceptance + failure injection 22 PASS / 1 NOT_APPLICABLE / 0 defects (`web/evidence/m34-recovery/failure-injection.json`); original workspace recoverable in every case. NA = pending connected-client outbox case (no CLI replica target; server/preflight fail-closed blocker covered by I2). |
| EC-015 | pass | Required docs updated (`docs/reference/backup-format.md`, backup/upgrades guides, local web quickstart, self-hosting, CLI reference, Web GUI guide, architecture); documentation suites 40/40; same-disk warning and no zero-loss/zero-downtime claims present. |
| AC-001 | pass | Agent/MCP/workflow/capability records survive restoration (rehearsal + I2 tests). |
| AC-002 | pass | External-boundary exclusions documented; no automatic external re-execution; uncertain execution marked for reconciliation. |
| AC-003 | pass | Real rehearsal 14/14 with before/after counts and content hashes. |
| AC-004 | pass | Failure-injection matrix 22 PASS / 1 NA / 0 defects; original workspace recoverable. |
| AC-005 | pass | Documentation updated and tested; same-disk limitation stated; no zero-loss/zero-downtime claim. |
| AC-006 | pass | EC-012–EC-015 evidence complete; full affected gates pass; M34 acceptance recorded honestly. |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Rust format | pass | `cargo fmt --all -- --check` | clean. |
| Rust lint | pass | `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | clean. |
| Rust workspace tests | pass | `cargo test --workspace --locked --offline` | 2032 passed, 0 failed. |
| Backup crate tests | pass | `cargo test -p orbit-backup --offline` | 126 passed across 12 targets (49 lib, create_verify 14, verify_integrity 9, inspect_list 9, policy 7, retention 8, restore_preflight 13, restore_replace 7, restore_isolated 6, restore_rollback 4). |
| Server tests | pass | `cargo test -p orbit-server --offline` | 95 passed. |
| CLI tests | pass | `cargo test -p orbit-cli --offline` | 164 passed. |
| Documentation tests | pass | `python3 -m unittest discover -s tests/documentation`; `python3 -m pytest -q tests/documentation` | 40 tests OK; 40 passed. |
| Web type-check | pass | `node node_modules/typescript/bin/tsc --noEmit` | clean. |
| Web unit tests | pass | `node node_modules/vitest/vitest.mjs run` | 77 files / 490 tests passed. |
| Web terminology | pass | `node scripts/check-terminology.mjs` | 286 files clean. |
| Web production build | pass | `node node_modules/vite/bin/vite.js build` | clean. |
| Playwright backup/restore spec | pass | `playwright test tests/e2e/backup-restore.spec.ts` | 4/4 passed. |
| Full Chromium regression | pass | `playwright test --project=chromium` with workspace `TMPDIR` | 156 passed, 6 environment-gated skips, 0 failed (29.8 min). |
| Environment limitation | pass | First default-`/tmp` Chromium attempt | fixture seeding failed under sandboxed `/tmp`; same failure on pre-M34 baseline; rerun with workspace `TMPDIR` passed. Recorded as environment limitation, not a product defect. |
| Real recovery rehearsal | pass | `web/evidence/m34-recovery/` (14/14 PASS) | 64/64 tables equal, DB/vault/attachment hashes identical, agent records readable, no external execution. |
| CLI failure injection | pass | `web/evidence/m34-recovery/failure-injection.json` | 22 PASS / 1 NOT_APPLICABLE / 0 defects. |
| Project records | pass | `getdone-validate-records`; `getdone-validate-project` | Run after this record update; result recorded in the final journal entry. |

## Checks not run

- Full `orbit update` network flow (release download/checksum/binary replacement)
  not run offline; EC-011 evidence is helper-level as recorded.
- Live MCP and Herdr integrations against actual processes (M35 scope; not claimed).
- Public-release qualification, accessibility/cross-browser certification, M15
  remaining gates and TLS ingress.

## Waivers

- None.

## Residual risk

- Internal engineering acceptance only; not a public-release certification.
- Connected-client pending data is excluded from server backups and replica
  snapshots are refused; reconciliation is required after recovery.
- Server replacement is an offline maintenance operation; browser downloads are
  memory-buffered and capped at 512 MiB.
- Preflight free-space measurement is `None`; a full disk surfaces during
  extraction with the original workspace preserved.
- External Herdr sessions, processes, git worktrees/repositories and credentials
  are outside the backup/restore boundary and are never re-executed.
- Documented follow-ups: restored-DB re-enrollment HTTP 500 (TODO-0018), replica
  snapshot support (TODO-0019), streaming download (TODO-0020), live-server
  replacement activation (TODO-0021), preflight free-space (TODO-0022),
  `knowledge_folders` internal-dir counting (TODO-0023).
