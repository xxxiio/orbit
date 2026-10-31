---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: passed
task_id: TASK-0020
---

# Current Acceptance Report

## Task

TASK-0020 — M34 I5 coverage, failure injection, real recovery rehearsal,
documentation and final acceptance. Together with the completed I1–I4 slices this
closes MILESTONE-0034.

M34 is accepted as **internal engineering acceptance on a frozen tree** (commit
`f82e3fa13d4f1fe4ca216aeed105e6efdd5ad250`, dirty with the M34 changes; binaries
`target/debug/orbit` SHA-256 `ad31c036…5a83c80`, `orbit-server` `9bbf52de…0db92f8`,
`orbit-mcp` `3c805e95…37d15104`). Each gate below passes on recorded evidence. No
public-release qualification is claimed, and the connected-mode/external-agent
limitations are explicit. M33 historical rows are retained from the completed M33
acceptance.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | M33 typography / auth / model (historical) | pass | M33 completed with internal-dogfooding acceptance; retained, not re-run. |
| G-0002 | I1 backup crate and format | pass | `cargo test -p orbit-backup --offline`: 126 passed / 12 targets (I1 core 49 lib + `create_verify` 14 + `verify_integrity` 9 + `inspect_list` 9 = 81); `cargo test --workspace --locked --offline` 2032 passed / 0 failed; fmt and clippy `-D warnings` clean. |
| G-0003 | I1 CLI create/list/inspect/verify | pass | Disposable-workspace CLI transcript and failure matrix: create (default/`--output`/`--label`/JSON, verified + sidecar), list (default/external, corrupt skipped), inspect, deep verify 8/8 and `--structural-only`; tampered archive exit 1; permission-denied destination exit 4 with mode unchanged. |
| G-0004 | I1 failure-safe creation + defect fix | pass | Staging/verify-before-publish/atomic publish; interrupted create (SIGKILL) left no partial archive or staging and the workspace intact; `create_private_dir` mode-rewrite defect fixed with regression test `existing_directory_permissions_are_never_rewritten` and the `0500` E2E check. |
| G-0005 | I1 format documentation (EC-008) | pass | `docs/reference/backup-format.md` documents the ustar container, layout, manifest, version compatibility, relative-path/traversal rules, secret exclusion, verification semantics and sidecar. |
| G-0006 | I1 live-server smoke | pass | Create over a live `orbit-server` (WAL, active connection) succeeded with the fence released and the server healthy; an API-created project appeared in the restored copy; attachment SHA matched. |
| G-0007 | I2 restore preflight / isolated restore | pass | `restore --dry-run` strict non-mutation (recursive file list/hashes unchanged, no destination created) with the required plan fields; isolated restore opens via the CLI with expected tasks/notes and never modifies the active workspace; `restore_preflight` 13 / `restore_isolated` 6 tests. |
| G-0008 | I2 replacement / rollback / refusals | pass | Replacement requires `--confirm`, preserves the previous workspace + recovery journal that reopens; active-target refusal exit 4 with target intact; interrupted restore leaves no usable partial target; replica snapshots refused from local and connected deployments; `restore_replace` 7 / `restore_rollback` 4 tests. |
| G-0009 | I3 server API and Web GUI | pass | `cargo test -p orbit-server --offline` 95 passed; Playwright `backup-restore.spec.ts` 4/4 (create→verification→inspect→download, typed isolated restore drill, admin-only offline replacement plan, corrupted archive with no restore offered); full Chromium 156 passed / 6 environment-gated skips / 0 failed (29.8 min); path-free responses, generated isolated targets, admin-only replace-plan; manual API smoke. |
| G-0010 | I3 web gates | pass | `tsc --noEmit` clean; Vitest 77 files / 490 tests; terminology 286 files clean; production build clean. |
| G-0011 | I4 automatic lifecycle | pass | Wired scheduler created a verified `automatic-daily` archive ~30 s after startup, no duplicate same UTC day, missing-attachment workspace failed closed; retention protects the last verified point; `policy` 7 / `retention` 8 tests plus CLI acceptance. |
| G-0012 | I4 pre-upgrade recovery point | pass | `orbit update` creates and deep-verifies a complete workspace recovery point (label `before-update-<from>-to-<to>`), fails closed with "refusing to update" and replaces no binaries when it cannot, applies retention after verification, `--check` read-only; helper-level evidence only (full network flow not run offline). |
| G-0013 | I5 real recovery rehearsal | pass | 14/14 steps PASS; archive SHA-256 `953cb9b22b4ca43be1b45115d111bceb8d64b95247416da8fbeece124c62c874`; all 64 tables equal before/after, DB snapshot SHA-256 identical, vault and attachment tree hashes identical; earlier data present, later changes absent, agent records readable, no automatic external execution, disposable write + restart persistence. |
| G-0014 | I5 CLI acceptance and failure injection | pass | 22 PASS / 1 NOT_APPLICABLE / 0 defects (`web/evidence/m34-recovery/failure-injection.json`); the NA is the pending connected-client outbox case (no CLI replica restore target; server/preflight fail-closed blocker covered by I2). |
| G-0015 | I5 MCP/Herdr record coverage | pass | Rehearsal restored 2 reserved-only agent jobs (queued, never launched), a capability install+grant+real execution receipt (`succeeded`, work_item_note), workflow Work Items, notes/references and an MCP-authored task change accepted via `authority.flush` (`Accepted(1)`); exclusions documented. |
| G-0016 | M34 documentation | pass | `docs/reference/backup-format.md`, backup-and-recovery, upgrades-and-recovery, local web quickstart, self-hosting, CLI reference, Web GUI guide and architecture; `unittest` 40 OK and `pytest` 40 passed; same-disk warning and no zero-loss/zero-downtime claims present. |
| G-0017 | Full workspace gate | pass | `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` clean; `cargo test --workspace --locked --offline` 2032 passed / 0 failed. |
| G-0018 | Environment limitation (recorded) | pass | First full Chromium attempt failed fixture seeding under the sandboxed `/tmp` (child SQLite writers); rerun with `TMPDIR` inside the workspace passed 156/6/0. The same default-`/tmp` failure reproduced on the pre-M34 HEAD baseline, so it is an environment limitation, not an M34 defect. |
| EC-001 | Backup creates a complete versioned `.orbitbak` | pass | Verified archive + SHA-256 manifest (relative paths/totals/inventory) from CLI and tests; rehearsal archive contains 13 inventory files. |
| EC-002 | Online-backup API + bounded writer fence, WAL handled | pass | Fence/snapshot tests in the 49-test lib target; live WAL-server backup released the fence with the server healthy; no live main-file copy. |
| EC-003 | Failure-safe creation | pass | Staging/verify/publish/cleanup; interrupted create and permission-denied destination evidence; source workspace never mutated. |
| EC-004 | `list`/`inspect`/`verify` semantics | pass | Default/external directories; deep verify 8/8; corrupt skipped; tampered archive exit 1; structural-only mode. |
| EC-005 | Restore preflight reports plan, mutates nothing | pass | Strict non-mutation preflight evidence (file list/hashes, no destination) with target/source/schema/replaced/preserved/downtime/connected/external fields. |
| EC-006 | Isolated restore opens, never overwrites active | pass | Isolated restore drill + CLI opens restored workspace; active workspace untouched. |
| EC-007 | Replacement authorized, maintenance boundary, rollback | pass | Confirmation gating, preserved previous workspace + journal, active-target refusal, interrupted-restore refusal; live database never replaced in place. |
| EC-008 | Format documented (paths/traversal/version/secrets) | pass | `docs/reference/backup-format.md`; version v999 rejected with upgrade message; traversal/secret tests. |
| EC-009 | Web GUI Backup & Restore section | pass | Playwright 4/4 + server 95 tests + full suite; page at `/settings/backup` with create/inspect/verify/download/history/schedule/preflight/isolated restore and admin-only offline replacement plan; path-free, browser never supplies server paths. |
| EC-010 | Automatic backups and retention | pass | Scheduler verified (~30 s first tick, 15-min re-read, no same-day duplicate, fail-closed on missing attachments); retention protects last verified point; CLI schedule/retention tests. |
| EC-011 | `orbit update` complete pre-upgrade recovery point | pass | Complete recovery point + fail-closed tests; `--check` read-only; full network update flow not run offline (recorded limitation). |
| EC-012 | Agent/MCP/workflow/capability records survive | pass | Rehearsal restored agent jobs, capability install/grant/real receipt, Work Items, MCP-authored task change, activity; exclusions documented; no automatic re-execution. |
| EC-013 | Real recovery rehearsal | pass | 14/14 with recorded counts/hashes and 64/64 table equality. |
| EC-014 | Failure injection | pass | 22 PASS / 1 NA / 0 defects with the original workspace recoverable in every case. |
| EC-015 | Documentation | pass | Required guides updated; documentation suites 40/40; same-disk/no-zero-loss claims present. |

## Commands run

```text
cargo fmt --all -- --check                                       # clean
cargo clippy --workspace --all-targets --locked --offline -- -D warnings # clean
cargo test --workspace --locked --offline                        # 2032 passed, 0 failed
cargo test -p orbit-backup --offline                             # 126 passed, 12 targets
python3 -m unittest discover -s tests/documentation              # 40 tests OK
python3 -m pytest -q tests/documentation                         # 40 passed
cd web && node node_modules/typescript/bin/tsc --noEmit          # clean
cd web && node node_modules/vitest/vitest.mjs run                # 77 files, 490 tests
cd web && node scripts/check-terminology.mjs                     # 286 files clean
cd web && node node_modules/vite/bin/vite.js build               # clean
cd web && TMPDIR=<workspace>/tmp ... playwright --project=chromium # 156 passed, 6 skipped, 0 failed
cd web && ... playwright tests/e2e/backup-restore.spec.ts        # 4/4 passed
getdone-validate-records / getdone-validate-project              # run for this records update
```

## Checks not run

- The full `orbit update` network flow (download/checksum/replace against a real
  release) was not run offline; EC-011 evidence is helper-level as recorded.
- Live MCP and Herdr integrations were not exercised in M34; they are M35 scope and
  must be tested against actual processes before any claim.
- Public-release qualification, accessibility/cross-browser certification, M15's
  remaining qualification and TLS ingress remain outside M34.

## Waivers

- None.

## Residual risk

- M34 acceptance is internal engineering acceptance on a frozen working tree; it
  does not certify a public release or general live MCP/Herdr support.
- Connected-client pending commands/staged uploads are excluded from server
  backups; reconciliation is required and replica snapshots are refused.
- Server replacement is an offline maintenance operation; the browser only
  produces a plan.
- Browser archive downloads are memory-buffered and capped at 512 MiB.
- Preflight free-space measurement is `None`; a full disk surfaces during
  extraction with the original workspace preserved.
- Restoring an authority backup over newer accepted history remains a
  disaster-recovery decision, not an automatic merge.
- External Herdr sessions, processes, git worktrees/repositories and credentials
  are outside the backup/restore boundary and are never re-executed.
