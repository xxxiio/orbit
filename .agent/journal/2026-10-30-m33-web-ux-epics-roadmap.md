---
template: journal-entry
template_version: 2.0.0
template_digest: "8f6d1e5f8c5b4a3d2e1f0a9b8c7d6e5f4a3b2c1d0e9f8a7b6c5d4e3f2a1b0c9d"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
date: "2026-10-30"
status: recorded
task_id: TASK-0015
---

# M33 — professional web UX, epics and roadmap

## Context

M31/M32 had the operations but not the interaction model: decorative type, no
List/Backlog/Roadmap, and no way to group work into epics. M33 was next because the
TUI redesign must adapt a validated browser model, not invent one.

## Work completed

- Inter self-hosted; removed Fraunces/Instrument Sans; neutral light/dark tokens; restrained 24/18/16/14/12px scale; removed decorative grids and uppercase tracking.
- First-visit/auth fix via a boolean `orbit.auth.seen` marker; first visit is quiet, expiry/revocation explained, sign-out clears the marker.
- Canonical project sections and compact header; `/tasks`/`/timeline` aliases.
- List epic column and URL-backed epic/status/priority grouping; Backlog view; project epic filter on Board; shared task-drawer Epic field.
- Authoritative Epic family in Rust with task and Work Item membership, progress, activity, safe delete and same-project enforcement; Milestone `set_target`.
- Roadmap with three scales, authored/derived epic bars, task windows, milestone diamonds, dependency indicators, unscheduled work and today line.
- Real-browser captures at 1440×900, 1280×800 and 390×844 in light and dark.
- TUI specification M33 addendum and web guide updates.

## Decisions made

- Epics are their own authority family, not a Milestone or a Task. Membership is a separate command so task/Work Item identity is preserved and a milestone stays a checkpoint.
- Only conventional view ids are promoted into canonical sections; Research Evidence/Workflow and Learning Lessons keep their own names.
- The roadmap never invents dates. Single-date items are one-day markers; undated items are listed, not plotted.
- Browser storage holds only a non-secret "seen" boolean; no token, device id or session material.

## Files or components changed

- Rust: `crates/orbit-runtime/src/authority_epics.rs`, `authority_milestones.rs`, `store.rs`, `lib.rs`; `orbit-sync` EntityKind; `orbit-server/src/main.rs` routes.
- Web: styles/fonts/tailwind, auth/session, project workspace, task list/table, epics/roadmap/backlog features, Work Item sheet, dialog/drawer primitives, API contracts/endpoints/mutations/adapters/queries.
- Tests: `epics.spec.ts`, `roadmap.spec.ts`, `project-workspace-nav.spec.ts`, auth additions, `epic-model.spec.ts`, `roadmap-model.spec.ts`, session history.
- Docs/evidence: `docs/web-app.md`, `docs/guides/tui-redesign-spec.md`, `web/evidence/m33/`.

## Validation performed

- `cargo test --workspace --offline`: 1,848 passed, 0 failed, 31 binaries.
- `cargo fmt --all -- --check`; clippy runtime/server/sync with `-D warnings`.
- `tsc --noEmit`; Vitest 63 files / 410 tests; Vite build; terminology 256 files.
- Playwright chromium 141 passed / 5 skipped / 0 failed; mobile auth 6/6.
- Captures reviewed for type, contrast, density, mobile wrapping and roadmap legibility.

## Problems encountered

- A portaled Select could dismiss the task drawer mid-edit; fixed by ignoring popper-layer outside/focus interactions in the drawer.
- Canonicalization initially promoted an overview's embedded task panel into List, producing duplicate nav entries; fixed by promoting only conventional ids.
- Axe caught 4.43:1 muted table text; `ink-faint` was darkened.
- 390px header overflowed until the action group wrapped at full width.
- Dialog content could exceed the viewport; dialogs now cap and scroll.
- Acceptance-criterion Enter was swallowed while a save was pending; the input stays enabled, submit is ignored while pending, and the draft survives.

## Remaining work

- A human visual/usability review (EC-008/AC-010).
- Optional per-edge status explanations and drag-to-reschedule after that review.

## New TODOs

- Record the human review outcome and close EC-008 if it passes.
- Decide whether to add a TUI epic/roadmap implementation in the next milestone.

## Risks or blockers

- No human has yet reviewed the built interface; M33 cannot be marked complete on agent screenshots alone.
- Epic and milestone concepts are easy to conflate; the docs and UI keep them separate, and the TUI spec repeats the distinction.
