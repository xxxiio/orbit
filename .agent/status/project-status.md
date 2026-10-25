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
Per-device project ACLs, server attachment GC and live Herdr acceptance remain
documented follow-ups.

## Current milestone

MILESTONE-0030 — TUI Workspace Simplification and Executable Blueprint
Capabilities, complete (I1–I4). MILESTONE-0025 is complete: EC-001..EC-007 are
satisfied, including connected MCP outbox/receipt parity, previewable
fingerprint-fenced upgrades and habit recurrence ownership by the opt-in
`orbit-habit-capability` package. M27 remains blocked externally.

## Recently completed

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

- None. NEXT-0011 (deliver M30 I1) is completed. NEXT-0012 (M30 I2, M25 EC-004)
  is the next authorized step and has not started.

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
2. Product follow-ups after M25/M30: connected capability execution, MCP
   capability tools, per-device project ACLs, server attachment GC, live Herdr
   acceptance.

## Next deterministic step

NEXT-0012 — M30 I2: route connected Blueprint MCP writes through the canonical
durable outbox and authoritative receipts (M25 EC-004), including offline
queuing, replay, conflict handling and lost-ack recovery, and remove the
direct-write bypasses.
