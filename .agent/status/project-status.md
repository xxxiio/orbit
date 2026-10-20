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

Orbit builds and passes the focused M28 and full Rust workspace suites. The
local TUI is now a project-centric, blueprint-driven workspace with a shared
`orbit-ui` presentation/registry/data contract, real application data,
authorized actions, responsive layouts, docs and 80 focused terminal tests.
M27 distribution source remains complete but its real-release gate is blocked on
external authorization.

## Current milestone

MILESTONE-0028 — M28 Project-Centric TUI and Blueprint UI Composition, status
active. Eight source-level exit criteria are met at approximately 92% of the
full M28 definition of done; full connected interactive-shell parity and local
agent review remain. The full local M15 profile passes 42/42.

## Recently completed

- M28 I1–I3: blueprint `views` schema/validation, shared `orbit-ui` registry and
  resolution, client view data and blueprint work-item projections.
- M28 I3–I5: project-centric Home/Inbox/Projects/Agents shell, sidebar,
  workspace, details, responsive wide/medium/narrow layout, theme and focus.
- M28 I4–I5: contextual action menu, command palette, search, prompts,
  confirmations, duplicate-submission suppression and real mutations.
- M28 I5: 13 built-in components and bundled v2 development/research/learning/
  habits views with generated defaults for legacy blueprints.
- M28 I6: TUI/blueprint/UI docs and focused schema, terminal-buffer,
  interaction and architecture tests.

## In progress

- M28 connected parity: read-only connected project views are implemented over
  the accepted snapshot; the connected mutation surface remains command-centric.
  The local renderer is complete.

## Blocked

- M27 real GitHub Release installation/update verification: the local account
  has read-only permission, no release exists and publication needs explicit
  maintainer authorization. This does not block M28 source work.

## Key decisions

- Preserve M27 source and the repaired M15 baseline; do not revert uncommitted
  distribution work.
- Blueprint declarations are presentation metadata only; actions are derived
  from selection and revalidated by the authority.
- Project views are project-scoped; Home/Inbox/Agents are host-generated global
  aggregations.
- Habits and knowledge remain global domain records referenced by project views.
- Legacy blueprints receive generated defaults rather than rewrites.
- Bundled versions advance to 2 so older pinned versions stay unchanged.
- Local TUI is the M28 renderer; the connected command surface is unchanged.
- No dynamic plugins, SQL, scripts or shell commands in manifests.

## Project health

- Build: `cargo check --workspace --all-targets --offline` pass.
- Tests: `cargo test --workspace --offline` 1,306 passed, 0 failed.
- Focused M28: 420 tests across blueprint/UI/client/TUI; 0 failed.
- Format/lint: fmt clean; clippy `-D warnings` clean on affected crates.
- Documentation: 29 tests pass.
- Acceptance: `./scripts/m15-full-automated.sh --profile full` 42 passed, 0 failed, 0 not-run; `.agent/evidence/m28-m15-final.zip`.
- Data integrity: no schema change; storage/regression suites pass.
- Interactive smoke: 140x40 and 60x18 pty runs passed; generated fallback and development@2 views verified (`m28-terminal-smoke.txt`).
- Deployment: unchanged; M27 real-release gate remains external.

## Risks

- Connected TUI still has a separate presentation path.
- Local agent review is inspect-only.
- Loading is synchronous in the local client.
- Terminal-buffer tests cover representative layouts, not every emulator.
- M27 public release remains unverified; do not claim it.
- M25 EC-003 through EC-006 remain deferred.

## Remaining milestones

1. MILESTONE-0028 — active; full connected interactive parity and local agent review remain.
2. MILESTONE-0027 — blocked on authorized external release verification.
3. MILESTONE-0025 — deferred (EC-003 through EC-006).

## Next deterministic step

NEXT-0007 — add connected project navigation and shared action dispatch while
preserving durable pending state.
