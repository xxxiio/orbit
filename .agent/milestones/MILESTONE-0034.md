---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0034
status: completed
---

# Milestone: M34 — Reliable Workspace Backup & Restore

## Intended outcome

Orbit can create a complete, consistent, portable, verifiable workspace backup of
its authoritative state and restore it safely, through both the CLI and the Web
GUI, backed by one shared Rust service. The result is good enough for Orbit to
become the source of truth for its own development, including MCP and Herdr agent
work.

## Why this milestone matters

Orbit is about to become the authoritative workspace for its own development, and
MCP and Herdr will initiate real agent work against it. Long-lived records —
projects, milestones, tasks, acceptance criteria, workflow definitions, capability
grants, agent jobs, handoffs, receipts and activity events — must survive upgrades
and operational failures; losing them would destroy development history and trust.

The existing manual backup procedure (`sqlite3 .backup` plus copying the vault,
attachments and configuration by hand) is sufficient for a one-off rescue but not
for convenient, repeatable recovery by an operator: it depends on knowing which
files matter, on taking a consistent snapshot, and on stopping the server at the
right moment. A real product needs one verifiable archive and a safe restore path.

The web experience is now usable enough — accepted for internal dogfooding in M33 —
that a standalone redesign can wait. Reliable data stewardship is now the
higher-value work and the prerequisite for using Orbit as its own workspace.

## Scope

- Versioned `.orbitbak` archive format with a complete manifest: file paths, sizes,
  SHA-256, total size and a component inventory.
- Consistent SQLite online snapshot plus the Markdown vault, the attachment object
  store and normalized non-secret configuration.
- One shared Rust `BackupService`/`RestoreService` with create, list, inspect,
  verify, preflight and restore operations, used by both CLI and server.
- CLI `orbit backup create|list|inspect|verify|restore`.
- Safe restore with an isolated-workspace path and a maintenance replacement path;
  failure-safe creation and restore.
- Web GUI Backup & Restore settings section backed by the same service.
- Optional automatic backups and retention (disabled/daily/weekly/pre-upgrade) that
  protect the last verified recovery point.
- Connected-mode limitations: client outbox and staged uploads are not in a server
  backup; reconciliation is required; incompatible replica state fails closed.
- MCP/Herdr data coverage: agent jobs, reviews/receipts/handoffs, workflow and
  capability metadata, MCP-made task/milestone changes and events.
- Real recovery rehearsal, failure injection and documentation.

## Non-goals

- TUI redesign or TUI feature work.
- Broad release qualification, public-readiness claims or M15/M27 release evidence.
- Unrelated infrastructure hardening.
- Cloud, Redis or broker dependencies.
- Deduplication or incremental backup.
- Encrypting or embedding credentials in the archive.
- Replacing a live active-server database while pretending that is safe.
- Automatically re-executing external agent or capability actions after recovery.

## Deliverables

- A shared Rust backup/restore crate and service with focused tests.
- The `.orbitbak` versioned format, manifest model and format documentation.
- CLI `orbit backup create|list|inspect|verify|restore` wired to the service.
- Restore preflight, isolated restore, maintenance replacement with rollback
  preservation and connected-replica fail-closed behaviour.
- Server HTTP endpoints and a Web GUI Backup & Restore settings section that
  reuse the shared service and the existing authorization architecture.
- Automatic backup scheduling and retention configuration, plus a complete
  pre-upgrade recovery point from `orbit update`.
- Failure-injection and real recovery-rehearsal suites with recorded evidence.
- Documentation: backup-and-recovery guide, upgrades-and-recovery, local web
  quickstart, self-hosting, CLI reference, Web GUI guide and architecture.

## Dependencies

- MILESTONE-0033 is complete (M33 accepted for internal dogfooding).
- The existing authoritative SQLite store (WAL mode), Markdown vault, attachment
  object store and normalized configuration.
- The existing server authorization architecture, CLI, web client and `orbit update`
  path.
- SQLite's supported online backup API in the bundled SQLite build.
- No optional service, cloud dependency or new external crate expected.

## Risks

- Snapshot consistency across SQLite, vault and attachment references needs a
  writer fence; a leaked write window could produce a plausible but inconsistent
  archive, so creation must verify before publishing and fail closed.
- Restore is destructive by nature; accidental replacement of an active workspace
  or an in-place live-database overwrite must be blocked by an explicit
  authorization and maintenance boundary, with the previous workspace preserved.
- Connected clients may hold outbox commands, staged uploads or accepted-but-not-
  observed work that a server backup cannot contain; this must be reported and
  reconciled, never silently treated as restored.
- Configuration could accidentally include secrets; secret exclusion must be an
  allow-list with tests, not a best-effort filter.
- Disk-full, interruption or permission failures during staging or publish could
  leave a partial archive or a damaged target; staging, verification, atomic
  publish and cleanup are required.
- Automatic retention could delete the only recovery point; cleanup runs only after
  a verified creation and never removes the last verified point.
- Large vaults or attachments could exhaust memory if archived naively; archive
  writing and verification must stream.

## Ordered implementation sequence

1. I1 — Backup core: shared Rust backup service, `.orbitbak` format and manifest,
   fenced consistent snapshot, failure-safe creation, CLI create/list/inspect/verify
   (TASK-0016).
2. I2 — Restore engine: preflight, isolated restore, maintenance replacement,
   rollback preservation, connected-replica fail-closed, CLI restore (TASK-0017).
3. I3 — Web backup management: server API and Backup & Restore settings UI reusing
   the shared service (TASK-0018).
4. I4 — Automatic backups and lifecycle: schedules and configuration, retention
   protecting the last verified point, pre-upgrade recovery point (TASK-0019).
5. I5 — MCP/Herdr coverage, failure injection, real recovery rehearsal,
   documentation and acceptance (TASK-0020).

## Exit criteria

- [x] EC-001: `orbit backup create` produces a versioned `.orbitbak` containing a
  consistent SQLite snapshot, the Markdown vault, referenced attachment objects,
  normalized non-secret config and a manifest with file paths/sizes/SHA-256, total
  size and component inventory.
- [x] EC-002: Snapshot creation uses SQLite's supported online backup API and a
  bounded writer fence so database, vault and attachment references correspond to
  one snapshot instant; WAL-mode databases are handled without copying a live main
  file.
- [x] EC-003: Backup creation is failure-safe: staging, verification before
  publish, atomic publish, nonzero/clear errors, temporary cleanup, and the
  original workspace is never mutated.
- [x] EC-004: `orbit backup list|inspect|verify` operate on the default and external
  directories; verify checks archive structure, manifest completeness, per-file
  hashes, SQLite integrity, schema compatibility, referenced attachment presence
  and vault manifest consistency, and exits nonzero on failure.
- [x] EC-005: `orbit backup restore --dry-run` reports target workspace, backup
  source, schema compatibility, objects replaced, preservation plan, required
  downtime, connected-client warnings and non-restorable external dependencies, and
  mutates nothing.
- [x] EC-006: Isolated restore into a new location verifies and opens successfully
  as a working Orbit instance and never overwrites the active workspace.
- [x] EC-007: Replacing an existing workspace requires explicit
  authorization/confirmation and a maintenance boundary (no live-database
  overwrite), stages and verifies before switching, preserves the previous
  workspace for rollback, and fails closed on incompatible connected-replica state.
- [x] EC-008: The backup format is documented with manifest fields,
  relative-path-only rules, traversal rejection, explicit format-version validation
  that rejects newer formats with an actionable message, and default secret
  exclusion.
- [x] EC-009: The Web GUI exposes a Backup & Restore settings section (overview,
  last backup, destination, schedule, history, verification status/errors; create,
  download/export, inspect, verify, schedule configuration, restore preflight,
  isolated restore, controlled recovery for replacement) using the same Rust
  service and the existing authorization architecture, without accepting arbitrary
  browser-supplied server paths, with destructive restore clearly separated and
  confirmed.
- [x] EC-010: Optional automatic backups (disabled/daily/weekly/pre-upgrade) and
  retention (max retained, minimum retention period, optional max storage) work;
  cleanup runs only after a verified creation and never removes the last verified
  recovery point or the user's external copies.
- [x] EC-011: `orbit update` creates a complete pre-upgrade recovery point (not
  database-only) when a workspace is present, and fails closed if it cannot.
- [x] EC-012: Agent-job records, reviews/receipts/handoffs, workflow definitions,
  installed capability metadata, MCP-made task/milestone changes and
  activity/event records survive restoration; external-boundary exclusions are
  documented; recovery never automatically re-executes external agent/capability
  actions, and uncertain external execution is marked for reconciliation.
- [x] EC-013: A real recovery rehearsal with a deterministic realistic dataset
  (multiple projects, several Milestones, 100+ Tasks, dependencies, acceptance
  criteria, workflow Work Items, Markdown notes, knowledge links, attachments with
  real content, agent-job records, capability authorization/execution history,
  activity events) passes: create → verify → disposable changes → isolated restore
  → isolated instance opens → earlier data/relationships/activity present → later
  changes absent → agent records readable → no automatic external execution →
  disposable write → restart persistence, with recorded before/after counts and
  content hashes.
- [x] EC-014: Failure injection is tested (interrupted backup, corrupted archive,
  missing attachment, unreadable vault file, disk-full/permission failure,
  unsupported manifest version, wrong destination, restore while target active,
  interrupted restoration, missing external agent session, pending
  connected-client outbox) and the original workspace remains recoverable in every
  case.
- [x] EC-015: Documentation is updated (backup-and-recovery guide,
  upgrades-and-recovery, local web quickstart, self-hosting, CLI reference, Web GUI
  guide, architecture, records) covering backup scope, excluded secrets, format,
  verification semantics, restore steps, recovery limitations, connected-mode and
  external-agent limits, and storage practice; it must state that a backup on the
  same disk is not protection against disk failure and must not claim zero data
  loss or zero downtime.

## Evidence

- EC-001: TASK-0016 — `orbit backup create` produces a verified `.orbitbak` with
  the SQLite snapshot, vault, referenced attachment objects, normalized
  non-secret configuration and a SHA-256 manifest (relative paths, totals,
  component inventory). CLI E2E on a disposable workspace (default dir,
  `--output`, `--label`, `--json`) plus `create_verify` (14) and
  `inspect_list` (9) tests; Lead smoke over a live server WAL database.
- EC-002: TASK-0016 — SQLite online backup API under a bounded writer fence,
  vault/attachment references materialized inside the same window; snapshot and
  fence tests in the 49-test `orbit-backup` lib target; live-server smoke with WAL
  and an active connection released the fence and left the server healthy;
  WAL-mode handling covered without copying the live main file.
- EC-003: TASK-0016 — staging, verify-before-publish, atomic publish, cleanup and
  nonzero errors; `create_verify` failure tests; tampered archive verify exits
  `1`; permission-denied destination (dir `0500`) exits `4`, keeps the mode and
  publishes nothing; the source workspace is never mutated. Acceptance found and
  fixed the `create_private_dir` mode-rewrite defect (regression test
  `existing_directory_permissions_are_never_rewritten`).
- EC-004: TASK-0016 — `list` and `inspect` on default and external directories;
  copied archive reports `unknown`, corrupt archive is skipped, non-`.orbitbak`
  files are ignored; `verify` deep 8/8 checks (structure, manifest, paths,
  `file_hashes`, `sqlite_integrity` 59 tables, `schema_compatibility`,
  `attachment_presence`, `vault_consistency`) and `--structural-only`;
  `verify_integrity` 9 tests plus CLI E2E.
- EC-005: TASK-0017 — `restore --dry-run` strict non-mutation (recursive file
  list/hashes unchanged, no destination created) and the required plan fields;
  `restore_preflight` 13 tests plus CLI acceptance; deferred apply-path checks
  recorded.
- EC-006: TASK-0017 — isolated restore creates and verifies a separate workspace
  that opens via the CLI with the expected tasks/notes and never modifies the
  active workspace (`restore_isolated` 6 tests + rehearsal).
- EC-007: TASK-0017 — replacement requires `--confirm`, preserves the previous
  workspace plus a recovery journal that reopens, refuses an active target
  (exit `4`) and an interrupted restore leaves no usable partial target;
  `restore_replace` 7 / `restore_rollback` 4 tests; replica recovery targets are
  refused and connected-replica state fails closed.
- EC-008: TASK-0016 — `docs/reference/backup-format.md` documents the ustar
  container, archive layout, manifest fields, version compatibility, relative-path
  rules, traversal rejection, default secret exclusion, verification semantics and
  the verification sidecar; path/version rejection covered by `verify_integrity`
  and `manifest` tests.
- EC-009: TASK-0018 — server 95 tests, Playwright `backup-restore.spec.ts` 4/4
  (create→verification→inspect→download; typed isolated restore; admin-only
  offline replacement plan; corrupted archive with no restore offered), full
  Chromium 156 passed / 6 skipped, path-free product responses, generated
  isolated targets, admin-only replace-plan, manual API smoke; web tsc/Vitest
  490/terminology 286/build clean.
- EC-010: TASK-0019 — the wired `backup_scheduler` (15-minute re-read of
  policy.json) created a verified `automatic-daily` archive ~30 s after startup, a
  second same-UTC-day tick created no duplicate, a missing-attachment workspace
  failed closed, and retention protected the last verified recovery point
  (`policy` 7 / `retention` 8 tests plus CLI acceptance).
- EC-011: TASK-0019 — `orbit update` creates a complete verified workspace
  recovery point (db+vault+attachments+config, label
  `before-update-<from>-to-<to>`), inspects and deep-verifies after publish, fails
  closed with "refusing to update" and replaces no binaries when it cannot
  (missing attachment object / non-Orbit DB), applies retention after
  verification, and `--check` remains read-only; helper-level evidence only — the
  full update network flow was not run offline.
- EC-012: TASK-0020 — rehearsal dataset restored agent jobs (2 reserved-only,
  queued, never launched), a capability install+grant+real execution receipt
  (`succeeded`, work_item_note effect), 6 workflow Work Items, 6 notes + typed
  references and an MCP-authored task change accepted via `authority.flush`
  (`Accepted(1)`); external-boundary exclusions are documented and no external
  action re-executed after recovery.
- EC-013: TASK-0020 — real rehearsal 14/14 PASS on a deterministic 103-task
  dataset; archive SHA-256 `953cb9b22b4ca43be1b45115d111bceb8d64b95247416da8fbeece124c62c874`;
  all 64 tables equal before/after, DB snapshot SHA-256 identical, vault and
  attachment tree hashes identical; earlier data present, later changes absent,
  agent records readable, disposable write + restart persistence proven.
- EC-014: TASK-0020 — CLI acceptance + failure injection 22 PASS / 1
  NOT_APPLICABLE / 0 defects (`web/evidence/m34-recovery/failure-injection.json`):
  interrupted backup (SIGKILL), corrupted/truncated archive, missing attachment,
  unreadable vault file, permission-denied and mode-preserving destinations,
  unsupported format v999, wrong destination/mixed modes, active-target refusal,
  interrupted restore, replica-snapshot refusal; the original workspace remained
  recoverable in every case. The NA is the pending connected-client outbox case
  (CLI has no replica restore target; server/preflight blocker covered by I2).
- EC-015: TASK-0020 — `docs/reference/backup-format.md`, backup-and-recovery,
  upgrades-and-recovery, local web quickstart, self-hosting, CLI reference, Web
  GUI guide and architecture updated; both documentation suites 40/40; same-disk
  warning plus explicit no-zero-loss/no-zero-downtime claims are present.

## Remaining work

- None. All fifteen exit criteria are met and M34 is complete. Acceptance is
  internal engineering acceptance with the documented limitations: connected-client
  pending data is excluded from server backups, replica snapshots are refused,
  server replacement is an offline maintenance operation, the browser download is
  memory-buffered and capped, preflight free-space measurement is `None`, and
  external Herdr/git/credential boundaries are never restored. No public-release
  qualification is claimed.
- Documented follow-ups are tracked as TODO-0018 through TODO-0023 and do not
  reopen M34 scope.

## Next milestone

MILESTONE-0035
