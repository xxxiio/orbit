---
template: project-status
template_version: 2.0.0
template_digest: "bae979b223043d02aa7be0cacebed8dd05241d5cc9f0d330796a7c919fbbe019"
project_owned: true
record_contract: project-status
record_schema_version: 1
status: current
---

# Project Status

## Current state

Orbit is a local-first project manager with an optional server-authoritative
connected mode. The TUI opens on Home/Today, navigates by project and restores the
active project, view, focused component, selected entity and scroll offsets after a
restart. The default workspace is two regions — sidebar plus workspace — with an
explicit, dismissible item inspector. Tasks carry Markdown descriptions, priority,
labels, planning metadata, ordered acceptance criteria and content-addressed
attachments. Trusted Blueprint capabilities have a durable local path with
installation, per-project authorization, fenced requests, single-claim execution,
receipts and reconciliation; connected mode refuses capability execution
explicitly (M25 EC-004).

The web client is the reference product experience. M33 replaced the decorative
serif system with Inter, fixed the false first-visit expiry warning, consolidated
the project workspace around Milestones (one grouping entity, one Tasks tab with
URL List/Board/Backlog views, compact Board, Milestone Roadmap) and added the opt-in
`ORBIT_AUTH_MODE=local-trusted` browser mode for personal loopback installs. M33 was
accepted by the project maintainer on 2026-10-31 for **internal dogfooding**
(internal-use acceptance, not public-release certification).

MILESTONE-0034 (Reliable Workspace Backup & Restore) is **complete**: the shared
`orbit-backup` service, the versioned `.orbitbak` format, fenced online SQLite
snapshots, failure-safe creation, CLI and Web GUI backup/restore, automatic
schedules/retention, the complete `orbit update` pre-upgrade recovery point, and a
real recovery rehearsal plus failure-injection matrix all pass on recorded evidence.
No milestone is active. MILESTONE-0035 (Orbit Self-Dogfooding: MCP, Workflows and
Herdr) is proposed and deliberately gated on maintainer review/approval; no M35
implementation has started.

## Current milestone

None active. MILESTONE-0034 — M34 Reliable Workspace Backup & Restore is complete:
EC-001..EC-015 are met, with internal engineering acceptance on the frozen tree and
the documented connected-mode/external-agent limitations (no public-release claim).
MILESTONE-0035 — M35 Orbit Self-Dogfooding: MCP, Workflows and Herdr is proposed and
awaits the maintainer's review of M34 and explicit approval (NEXT-0023).
MILESTONE-0033 — M33 Professional Web UX, Milestone-Based Planning & Unified Project
Work is complete: EC-001..EC-011 met and the human review (EC-010) recorded as
internal-dogfooding acceptance only. MILESTONE-0032 is complete (I1–I6),
MILESTONE-0031 is complete (I1–I6), MILESTONE-0030 is complete (I1–I4), and
MILESTONE-0025 EC-001..EC-007 remain satisfied. M27 remains blocked externally.

## Recently completed

- M34 completion — Reliable Workspace Backup & Restore: all fifteen exit criteria
  pass on the frozen tree (commit `f82e3fa`, dirty with the M34 changes; binaries
  `orbit`/`orbit-server`/`orbit-mcp` hashes recorded). The shared `orbit-backup`
  service covers format/manifest, fenced online snapshot, create/verify/inspect/
  list, restore preflight/isolated/replacement/rollback and retention; CLI
  `orbit backup create|list|inspect|verify|restore|schedule|retention`; the
  server API and the `/settings/backup` Web GUI page; the wired 15-minute automatic
  scheduler; and the complete fail-closed `orbit update` pre-upgrade recovery
  point. A real WAL-server backup of a deterministic 103-task dataset (3 projects,
  3 milestones, 206 acceptance criteria, 6 Work Items, notes/links, 5 attachments,
  reserved-only agent jobs, a capability install+grant+real execution receipt, an
  MCP-authored task change, 124 activity rows) restored in isolation with all 64
  tables equal, identical DB/vault/attachment hashes, no automatic external
  execution, and restart persistence (14/14 rehearsal steps). CLI failure injection
  was 22 PASS / 1 NA / 0 defects with the original workspace recoverable in every
  case. Final gates: Rust workspace 2032 passed / 0 failed; `orbit-backup` 126
  passed / 12 targets; server 95; CLI 164; docs 40/40; web tsc clean, Vitest 490,
  terminology 286, production build clean; Playwright `backup-restore` 4/4 and full
  Chromium 156 passed / 6 environment-gated skips / 0 failed. Acceptance is internal
  engineering acceptance with documented limitations: replica snapshots refused,
  connected-client pending data excluded from server backups, browser downloads
  memory-buffered/capped, server replacement offline-only, preflight free-space
  unknown, and helper-level `orbit update` network evidence.
- M34 I1 backup core: shared `orbit-backup` service and `.orbitbak` v1 format with
  fenced online SQLite snapshot, vault/attachment/config coverage, SHA-256 manifest,
  failure-safe create and CLI create/list/inspect/verify; acceptance found and fixed
  the `create_private_dir` destination-permission defect (regression test
  `existing_directory_permissions_are_never_rewritten`).
- M33 completion with internal dogfooding acceptance: on 2026-10-31 the project
  maintainer reviewed the corrected app and representative captures and judged it
  "generally good enough to begin internal use". EC-010 is recorded as internal-use
  acceptance, explicitly not public-release certification; EC-011 local-trusted
  authentication passes on recorded evidence. Minor UX follow-ups are
  TODO-0015..TODO-0017.
- M33 local-trusted authentication: explicit opt-in `ORBIT_AUTH_MODE=local-trusted`,
  literal loopback bind refusal before storage open, no Device ID/token/cookie for
  the browser, separate authentication for administrator/operator/executor/machine/
  MCP surfaces, CSRF/Origin/Host guards, and a clean switch back to authenticated
  mode.
- M33 corrective web UX: Epic removed and converted once into Milestones; rich
  Milestone fields, Task and Work Item membership, derived progress and activity;
  full task creation in one accepted command; one Tasks tab with URL
  List/Board/Backlog views and shared filters; compact Board with persisted density;
  Milestone Roadmap; 28 corrective captures under `web/evidence/m33-corrective/`.
- M32 I5–I6 dogfooding and acceptance: nine automated interactive dogfooding
  sessions on one persistent workspace with 23 captures and a defects table; the
  plan-review blocker fixed by one shared rule used by proposal and approval; a
  lockfile-only frontend install built byte-identically (with a documented host
  workaround); the TUI redesign specification rewritten from proven M31/M32 usage.
- M32 I4 connected Workflow Capability execution; M32 I3 browser agent execution;
  M32 I2 knowledge workflows; M32 I1 everyday project management; M31 web
  foundation and daily core; connected MCP blueprint writes queue-first through the
  outbox; two-region shell and explicit inspector; durable capability
  installation/grants/receipts/reconciliation.

## In progress

- None. M35 has not started: it begins only after the maintainer reviews the M34
  evidence and the documentation walkthrough and explicitly approves it
  (NEXT-0023).

## Blocked

- M27 real GitHub Release installation/update verification: the local account is
  read-only, no release is published, and publication needs maintainer approval.
  The release workflow file is intentionally withheld pending an OAuth
  workflow-scope fix, so two M15 release-contract checks cannot run.

## Key decisions

- Navigation state is intent, not authority: it stores ids/indices only and is
  discarded or clamped when the authority no longer matches.
- The inspector is an overlay with a full-screen fallback; there is no docked
  detail mode, and `Tab` only cycles visible panes.
- Capability installation, authorization, request, claim, execution and receipt
  are separate durable facts; a blueprint manifestation never grants execution.
- A crash after claim or an `uncertain` outcome is settled only by an explicit
  operator decision, atomically, and never by automatic retry.
- Local-trusted authentication is configuration, never inferred from a peer
  address or a forwarded header; it never grants administrative or machine
  privileges and is refused on every non-loopback bind.
- The M33 human review is internal-dogfooding acceptance only; M34 acceptance is
  internal engineering acceptance only. Neither is a public-release certification.
- M34 backup and restore use one shared Rust service for both CLI and server; the
  Web GUI reuses it and never re-implements archive logic or accepts arbitrary
  browser-supplied server paths.
- Recovery never automatically re-executes external agent or capability actions;
  uncertain external execution is marked for reconciliation.
- Client replica snapshots are refused as a recovery point in this build; connected
  deployments back up the server authority and keep replica outbox/staging with
  the replica.
- A backup on the same disk is not protection against disk failure, and no zero
  data loss or zero downtime claim is made.
- M35 must not start until the maintainer reviews M34 and explicitly approves; live
  MCP/Herdr integration will be tested against actual processes, never inferred
  from interface existence.

## Project health

- Build: `cargo build --workspace --locked --offline` on the frozen M34 tree;
  binaries `orbit`/`orbit-server`/`orbit-mcp` hashed and build mtimes recorded
  (2026-10-31T10:06:40/41Z).
- Tests: `cargo test --workspace --locked --offline` 2032 passed / 0 failed;
  `orbit-backup` 126 passed / 12 targets; `orbit-server` 95; `orbit-cli` 164; docs
  40/40; web Vitest 77 files / 490 tests; Chromium 156 passed / 6 environment-gated
  skips / 0 failed; Playwright `backup-restore` 4/4.
- Format/lint: `cargo fmt --all -- --check` clean; `cargo clippy --workspace
  --all-targets --locked --offline -- -D warnings` clean; web tsc/terminology/
  production build clean.
- Known environment limitation: the default sandboxed `/tmp` breaks child-process
  SQLite fixture seeding for the browser suite; running with `TMPDIR` inside the
  workspace passes. The same default-`/tmp` failure reproduced on the pre-M34
  baseline, so it is an environment limitation, not an M34 defect.
- M15 release profile: not re-run; unchanged release qualification remains M15/M27.
- Internal-testing readiness: yes for local-first use including the M33 web
  experience and M34 backup/restore. Connected-mode and external-agent limitations
  are documented; live MCP/Herdr readiness is unproven and is M35 scope.

## Risks

- M34 acceptance is internal engineering acceptance; public-release qualification,
  accessibility/cross-browser certification and TLS ingress remain outside it.
- Connected-client pending commands and staged uploads are excluded from server
  backups; reconciliation is required and replica snapshots are refused.
- Server replacement is an offline maintenance operation; the browser only writes
  a plan and never replaces the live database.
- Browser archive downloads are memory-buffered and capped at 512 MiB; very large
  archives must be copied from the server's backup folder (TODO-0020).
- Preflight free-space measurement is `None`, so a full disk surfaces during
  extraction with the original workspace preserved (TODO-0022).
- Restored-database re-enrollment of an existing device returns HTTP 500 instead of
  409/idempotent; recovery is unaffected (TODO-0018).
- External Herdr sessions, processes, git worktrees/repositories and credentials
  are outside the backup/restore boundary and are never re-executed; live
  MCP/Herdr integration is unproven until M35.
- The authority is single-tenant: any enrolled device may access any existing
  attachment owner. Per-device project ACLs are TODO-0013.
- No server-side attachment GC endpoint (TODO-0014); local purge only.
- Public release remains unverified; M27 is blocked externally.

## Remaining milestones

1. MILESTONE-0035 — M35 Orbit Self-Dogfooding: MCP, Workflows and Herdr, proposed;
   requires maintainer review/approval (NEXT-0023) before any implementation.
2. MILESTONE-0027 — blocked on authorized external release verification.
3. Post-milestone work: the TUI redesign using
   `docs/guides/tui-redesign-spec.md`, connected capability execution, MCP
   capability tools, per-device project ACLs, server attachment GC, live Herdr
   acceptance, and the M33/M34 follow-ups TODO-0015..TODO-0023.

## Next deterministic step

NEXT-0023 is ready for MILESTONE-0035: provide the maintainer a bounded human
walkthrough and review of M34's completion evidence (the
`web/evidence/m34-recovery/` rehearsal and failure-injection records, the operator
documentation, and the running backup/restore experience) and obtain an explicit
approval or rejection to start M35. No M35 implementation begins before that
decision.
