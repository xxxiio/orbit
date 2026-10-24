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
after a restart. New projects are pinned to the Basic workflow offline and open
Overview/Tasks/Board/Timeline. Tasks carry Markdown descriptions, priority,
labels, planning metadata, ordered acceptance criteria and content-addressed
attachments. The interface adapts from 60x18 to 160x45, windows Kanban columns,
ranks contextual actions and palette results, and degrades to a
colour-independent theme with complete status glyphs. One shared shell serves
local and connected operation: `!` inspects pending commands, conflicts are
reviewed and replaced fail-closed, and the connected console covers the record
grammar plus accepted routine/strategy/milestone/schedule/planning reads. The
legacy connected TUI and `ORBIT_TUI_LEGACY_CONNECTED` are gone. M29 is complete
for the verified scope; per-device project ACLs, server attachment GC and live
Herdr acceptance remain documented follow-ups.

## Current milestone

MILESTONE-0029 — M29 TUI Product Experience, Interaction Quality and
Maintainability, completed. EC-001..EC-010 are satisfied. The remaining gaps are
per-device project authorization, server attachment GC, live Herdr acceptance
and public release verification.

## Recently completed

- Durable navigation state: project/view/focus/component, stable selected id,
  index and scroll offsets, with view-wide id resolution, clamping, corrupt and
  newer-version tolerance.
- Ranked compact action bar, grouped menu with disabled reasons, context-aware
  hotkeys, ranked palette with destination MRU and disabled refusal.
- `E` quick edit (title/priority), reordered task forms, field-level validation,
  multiline caret window and discard safety.
- Component-aware responsive layout, windowed Kanban with ownership-aware
  selection, priority table columns, clamped detail scroll and a restrained
  no-colour-safe theme.
- Shared-shell pending inspector, conflict review and connected console; legacy
  connected presentation (1,994 lines) deleted.
- `app/`, `input/`, `render/` extraction; `main.rs` is 196 lines; detail-model
  cache and title-only quick-edit refresh.
- 20-project/200-task/1,017-activity fixture, PTY perf probe and recorded
  before/after measurements; PTY scenarios A–G; 21 terminal captures.

## In progress

- None. NEXT-0010 (deliver M29) is completed.

## Blocked

- M27 real GitHub Release installation/update verification: the local account is
  read-only, no release is published, and publication needs maintainer approval.
  The release workflow file is intentionally withheld pending an OAuth
  workflow-scope fix, so two M15 release-contract checks cannot run.

## Key decisions

- Navigation state is intent, not authority: it stores ids/indices only and is
  discarded or clamped when the authority no longer matches.
- `App`/renderer is authority-agnostic; local and connected differences live in
  `backend.rs`/`local_backend.rs`, never in view rendering.
- Connected mutations keep the accepted-vs-pending distinction and use the
  durable outbox; unknown connected outcomes fail closed as pending.
- One shared shell serves both deployments; the connected console is a modal in
  that shell, not a second event loop.
- Blueprint declarations are presentation metadata, not authority; legacy
  blueprints receive generated views.
- Agent job completion never completes an Orbit task.

## Project health

- Build: `cargo check --workspace --all-targets --offline` pass.
- Tests: `cargo test --workspace --offline` 1,563 passed, 0 failed (29 binaries);
  `orbit-tui` 219, `orbit-client` 348, `orbit-ui` 28.
- Acceptance suites: TUI 54 passed/1 skipped (full fixture opt-in 2 passed);
  contracts 43; documentation 39.
- Format/lint: fmt clean; clippy `-D warnings` on 12 crates.
- Acceptance: `./scripts/m15-full-automated.sh --profile full` 45 passed, 2
  failed (release-workflow contract and its 12 packaging tests only; workflow
  file withheld); `orbit-m15-evidence-20261024T135239Z.zip`.
- Internal-testing readiness: yes for local-first use; connected mode is ready for
  single-tenant testing with the documented limits (per-device ACLs, server
  attachment GC, live Herdr acceptance).
- Interactive evidence: 21 captures in `.agent/evidence/m29-captures/`;
  before/after measurements in `.agent/evidence/m29-perf-*.{md,json}`; PTY
  evidence under `target/tui-evidence/`.

## Risks

- The authority is single-tenant: any enrolled device may access any existing
  attachment owner. Per-device project ACLs are TODO-0013.
- No server-side attachment GC endpoint (TODO-0014); local purge only.
- Connected MCP does not expose attachment tools.
- Connected multi-command edits are not atomic; the inspector reports per-command
  state.
- Advanced connected record families are edited through the connected console
  rather than structured forms; discoverability is lower than a purpose-built
  form.
- Live Herdr/model acceptance remains opt-in; Scenario C uses a scripted CLI.
- Public release remains unverified; M27 is blocked externally.
- Interaction measurements are PTY wall-clock medians, not microbenchmarks.
- Terminal tests cover representative sizes/emulators, not every terminal.

## Remaining milestones

1. MILESTONE-0027 — blocked on authorized external release verification.
2. MILESTONE-0025 — deferred (EC-003 through EC-006).

## Next deterministic step

None. NEXT-0010 (deliver M29) is completed. The next milestone is a maintainer
decision (for example the M25 deferred capability work or the M27 external
release gate); remaining product gaps are tracked as TODOs and documented
follow-ups.
