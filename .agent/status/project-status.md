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
connected mode. The TUI opens on Home/Today, navigates by project and restores
the active project, view, focused component, selected entity and scroll offsets
after a restart. The default workspace is two regions — sidebar plus workspace —
with an explicit, dismissible item inspector (`Enter`/`D` open, `Esc` closes,
full-screen below 80x20) instead of a permanent Details column. New projects are
pinned to the Basic workflow offline and open Overview/Tasks/Board/Timeline.
Tasks carry Markdown descriptions, priority, labels, planning metadata, ordered
acceptance criteria and content-addressed attachments. The interface adapts from
60x18 to 160x45, windows Kanban columns, ranks contextual actions and palette
results, and degrades to a colour-independent theme with complete status glyphs.
Trusted Blueprint capabilities now have a public local path: durable
installation, separate per-project authorization, offer discovery, explicit
fenced requests, single-claim execution, durable receipts and explicit
reconciliation, exposed through `orbit blueprint capability|action`, the
contextual action menu, palette and inspector. One shared shell serves local and
connected operation: `!` inspects pending commands, conflicts are reviewed and
replaced fail-closed, and the connected console covers the record grammar plus
accepted routine/strategy/milestone/schedule/planning reads. Capability execution
is local-authority only; connected mode refuses it explicitly (M25 EC-004).

The web client is the reference product experience. Since M32 I1 it can rename a
project and move it through the domain's legal status transitions, search
projects, tasks and knowledge in one server query, save and restore personal
task/board views, and show authoritative task Created/Updated times with an
explicit unknown state. M32 I2–I6 remain: knowledge folders and links, browser
agent execution, connected capability execution, extended dogfooding and final
acceptance. Per-device project ACLs, server attachment GC and live Herdr
acceptance remain documented follow-ups.

## Current milestone

MILESTONE-0032 — M32 Web Product Completeness, Agent Workflows & Extended
Dogfooding, active (I1 complete). MILESTONE-0031 is complete (I1–I6). M31
shipped the React client served by `orbit-server` with session authentication,
Home, Inbox, project management, workflow-driven workspaces and a task inspector
against the authoritative domain. MILESTONE-0030 is complete (I1–I4), and
MILESTONE-0025 EC-001..EC-007 remain satisfied. M27 remains blocked externally.

## Recently completed

- M32 I1 everyday project management: project rename and status now go through
  one authoritative `update` command (domain-validated, fenced, idempotent, with
  a no-op that writes nothing), and the status control offers exactly the edges
  the server reports. One `/authoritative/search` finds projects, tasks and notes
  with bounded grouped results, and the palette searches it as you type. Saved
  views store the current URL query plus list/board display as personal
  preferences outside the semantic ledger, so they survive a browser restart and
  never change a workflow. Task Created/Updated are derived from recorded
  activity and report `Unknown` when no history exists. 1,730 workspace tests,
  249 client unit tests and 106 browser checks pass.
- M31 I1 web foundation: `orbit-server` now serves the React application at `/`
  with a traversal-safe SPA fallback (JSON 404s for API paths) and exchanges an
  operator-issued device credential once for an HttpOnly `SameSite=Strict`
  cookie session (hashed at rest, sliding 12h / absolute 7d, login throttle,
  revocation-aware, CSRF header required for cookie mutations). Bearer clients
  keep their existing path. 38 server and 543 runtime tests pass.
- M31 I1 daily core: Home (server-derived readiness, overdue, due today,
  blocked, recently opened), Inbox with quick capture and triage, Projects with
  creation and automatic Basic workflow binding, a project workspace whose view
  tabs come from the pinned workflow, and a task drawer with status, priority,
  labels, Markdown description, acceptance criteria, dependencies and activity,
  deep-linkable via `?task=`.
- M31 I1 authority discipline: `expected_revision` is read from the query cache
  at mutate time, mutations are never optimistic, and rejected transitions,
  stale revisions and session expiry surface the server answer rather than a
  silent success. Workflow upgrades are reachable over HTTP with a
  recomputed fingerprint when the caller omits one.
- M31 I1 acceptance: seven Playwright specs (20 checks) run against a real
  `orbit-server` with an isolated database per worker and a deterministic
  6-project/122-task fixture; documentation and a terminology guard
  (`check-terminology.mjs`, 127 files) ship with the app.
- Navigation rail fix: `Enter` activates the focused rail row on every screen
  (the Projects screen previously hijacked it), Home/Inbox/Agents/Projects keep
  the rail focused, and opening a project hands the keyboard to the workspace;
  4 regression tests plus the full PTY suite.
- Connected MCP blueprint writes are queue-first through the canonical replica
  outbox with `accepted`/`pending`/`conflict` dispositions, immutable receipts,
  offline replay and lost-ack recovery; no direct connected write bypass remains.
- Previewable blueprint upgrades: typed diff of kinds/states/fields/transitions/
  actions/capabilities/views, warning/blocker classification, SHA-256 fingerprint
  over the accepted state, explicit `--confirm` apply that recomputes and fences,
  preserves items and is idempotent. Publish/install never rebind.
- Habit recurrence/streak evaluation moved into the opt-in
  `orbit-habit-capability` package (`orbit.habit.recurrence`), registered but not
  authorized by default; streak/evidence reads return an explicit unavailable
  status until installed (and granted for project evidence), while counts and
  last-occurrence projections keep working.
- Two-region default shell; docked Details pane, `show_detail` and
  `FocusPane::Detail` removed; workspace receives all width beyond the sidebar at
  80x24/120x35/160x45.
- Explicit inspector with subject/focus state, overlay and full-screen
  presentation, selection/scroll preservation, deleted-entity fallback, edit
  round trip and legacy-session tolerance.
- Durable capability installation, project grants, action receipts and
  idempotent reference-note effects; request planning accepts schema-declared
  optional inputs and enforces length/range bounds.
- `TrustedComposition` host composition with trusted executor registration, a
  deterministic reference capability, offers with six availability codes,
  explicit requests, single-claim execution, receipts and atomic settlement of
  `claimed`/`uncertain` without re-execution.
- CLI `blueprint capability {list,install,uninstall,authorize,revoke}` and
  `blueprint action {offers,request,execute,show,list,reconcile}`; connected
  refusal; local `blueprint.transition` uses durable grants.
- TUI capability actions in the contextual menu, palette and inspector, with
  schema-driven forms, pending-request confirmation, result modal and
  reconciliation; no renderer-side mutation.
- Isolated end-to-end acceptance including restart and replay; PTY inspector and
  capability UI scenarios; documentation, captures and records.

## In progress

- None. M31 I6 completed; post-milestone work is listed under remaining milestones.

- (historical) MILESTONE-0031 I2: board/timeline rendering from workflow-declared views,
  milestones, dependencies and readiness, day/schedule planning with explicit
  approval and apply, project settings, and list filters. The I2 API surface
  (milestone commands, graph/dependencies/work-brief/day-plan/schedule-plan/
  workload/portfolio reads, plan review plus fingerprint-fenced apply) is
  implemented and unit-covered; its views are the next authorized step.

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
  are separate durable facts. A blueprint manifestation never grants execution;
  the request path re-checks install, grant, requirement, revision and inputs.
- A crash after claim or an `uncertain` outcome is settled only by an explicit
  operator decision, atomically, and never by automatic retry.
- Capability execution is local-authority only in this iteration; connected mode
  refuses with the tracked EC-004 gap instead of queueing an unexecutable command.
- `App`/renderer is authority-agnostic; local and connected differences live in
  `backend.rs`/`local_backend.rs`, never in view rendering.
- Connected mutations keep the accepted-vs-pending distinction and use the
  durable outbox; unknown connected outcomes fail closed as pending.
- Agent job completion never completes an Orbit task.

## Project health

- Build: `cargo build --workspace --offline` pass; `orbit-cli`/`orbit-tui`
  rebuilt for acceptance.
- Tests: `cargo test --workspace --offline` 1,658 passed, 0 failed (31 binaries);
  `orbit-tui` 250, `orbit-client` 348, `orbit-storage` 105, `orbit-runtime` 542,
  `orbit-mcp` 76, `orbit-cli` 128, `orbit-habit-capability` 5.
- Acceptance suites: TUI 65 passed/1 skipped; MCP 4 passed; local CLI workflows
  60 passed; capability workflow 1 passed; contracts 43; documentation 39;
  `python3 scripts/check-m25-blueprint-contracts.py` OK.
- Format/lint: fmt clean; clippy `-D warnings` on 12 crates.
- M15 release profile: not re-run in this iteration; unchanged release
  qualification remains M15/M27 (last recorded 45/47 with the withheld workflow).
- Internal-testing readiness: yes for local-first use including the new inspector
  and capability workflow; connected mode is ready for single-tenant testing with
  the documented limits (per-device ACLs, server attachment GC, live Herdr
  acceptance, no connected capability execution).
- Interactive evidence: PTY suites in `tests/acceptance/tui/`, regenerated
  captures recorded in `.agent/evidence/m30-i1-captures.md`, PTY artefacts under
  `target/tui-evidence/`.

## Risks

- The authority is single-tenant: any enrolled device may access any existing
  attachment owner. Per-device project ACLs are TODO-0013.
- No server-side attachment GC endpoint (TODO-0014); local purge only.
- Connected capability execution is unavailable by design; users on a server
  deployment see an explicit refusal. Connected *blueprint* writes are now
  outbox-backed (M25 EC-004), but capability actions remain local-only.
- Habit streak/evidence reads are unavailable until `orbit.habit.recurrence` is
  installed (and granted for project evidence); this is the intended opt-in
  behaviour, not a silent fallback.
- Blueprint upgrade blockers intentionally refuse incompatible field-type or
  removed-kind/state changes with live items instead of remapping data.
- `uncertain` outcomes are proven by the runtime matrix with a synthetic executor;
  the reference capability cannot produce them, so the TUI path is covered at the
  state/modal layer rather than end to end.
- The reference capability is a deterministic demonstration adapter, not a
  general user-facing feature.
- Live Herdr/model acceptance remains opt-in; Scenario C uses a scripted CLI.
- Public release remains unverified; M27 is blocked externally.
- Terminal tests cover representative sizes/emulators, not every terminal.

## Remaining milestones

1. MILESTONE-0027 — blocked on authorized external release verification.
2. Post-milestone work: the TUI redesign using `docs/guides/tui-redesign-spec.md`
   (7 must-match / 3 adapt / 1 not-appropriate / 1 shared-domain), the recorded
   web follow-ups (project rename/status, task timestamps, knowledge folders, note
   links, saved views, agent start, capability execution), and connected capability
   execution, MCP capability tools, per-device project ACLs, server attachment GC,
   live Herdr acceptance.

## Next deterministic step

NEXT-0014 is completed with M31. The next authorized work is the TUI redesign
against `docs/guides/tui-redesign-spec.md`; MILESTONE-0027 remains blocked on
external release authorization.
