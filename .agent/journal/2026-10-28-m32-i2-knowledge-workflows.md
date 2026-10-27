---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-28"
task_id: TASK-0011
---

# Journal Entry: 2026-10-28 — M32 I2 knowledge folders, links and project context

## Context

I1 closed the everyday project-management gaps. Knowledge was the next surface
that still needed the CLI. Reconnaissance corrected the milestone's own
assumption: the vault and server already owned folder create/move (a rename is a
move) and note create/update/move with receipts, crash replay and symlink
rejection, and already computed `[[id]]` relations. What was missing was any
browser affordance, a shared typed-reference grammar over HTTP, and any
project-scoped knowledge read.

## Work completed

- `orbit-runtime::references` now owns the typed-reference grammar
  (`[project:orbit](orbit://project:orbit)`); `orbit-cli` delegates and keeps its
  `habit:` policy locally, with its behaviour and tests unchanged.
- Reference command add/remove through the existing Markdown command path (so
  receipts, per-note revision and index reconciliation are reused), plus
  `markdown-references` and `project-knowledge` reads — one vault scan each.
- Browser: folder create/rename/move with a picker and breadcrumb, a folder as a
  real destination, note create/rename/move through the picker, link/unlink
  authoring, typed-reference chips with broken-target display, a project-aware
  `knowledge_refs` panel, and a Notes section in the task drawer.
- Task context on a note (`?task=`) no longer opens the task drawer over the note.
- Fixed the folder command journal so a refused create/move releases its intent
  instead of blocking every later knowledge write, with precise refusal text.
- Documentation for the whole knowledge surface, and a corrected
  "Not available yet" list.

## Decisions made

- No second grammar: the parser moved to the runtime and the CLI delegates, so
  "the note references the project" and "the project lists this note" cannot
  drift.
- The reference write reuses the Markdown command path rather than adding a
  parallel write with its own receipt rules; a small `markdown_reference_commands`
  journal exists only because the derived update payload differs after the first
  acceptance.
- Work items and habits are refused over HTTP: the existing contracts cover
  `project` and `task`, and this iteration does not invent a grammar extension.
- `?task=` keeps one meaning per route instead of gaining a second parameter: on
  knowledge routes it is note context, everywhere else it opens the drawer.

## Files or components changed

- `crates/orbit-runtime/src/{references.rs (new),folder_commands.rs,lib.rs,
  markdown_commands.rs,store.rs}`, `crates/orbit-server/src/main.rs`,
  `crates/orbit-cli/src/{markdown_metadata.rs,markdown_discovery.rs}`.
- `web/src/features/knowledge/**`, `features/tasks/task-drawer/notes-section.tsx`,
  `app/{workspace-layout.tsx,route-scope.ts}`, `api/**`.
- `web/tests/{unit,e2e}/**` including three new specs and `harness/knowledge.ts`.
- `docs/web-app.md`.

## Validation performed

- `cargo test --workspace --offline` → 1,759 passed, 0 failed, 31 binaries.
- `cargo clippy -p orbit-runtime -p orbit-server -p orbit-cli -D warnings` → clean;
  `cargo fmt --all -- --check` → clean.
- `tsc --noEmit` → 0; `vitest run` → 52 files / 299 tests; terminology 244 files
  clean; `vite build` → built.
- Playwright: 19 focused I2 checks, 125 full, 4 opt-in skips, 0 failed.

## Problems encountered

- A refused folder create/move left a durable pending intent that recovery
  replayed before every later knowledge command, so one bad destination made the
  workspace unwritable until the filesystem changed. Fixed by classifying the
  publish outcome, abandoning a proven-before-write refusal, and returning precise
  400s; crash semantics unchanged.
- `?task=` on a knowledge route opened the task drawer over the note, hiding the
  note's own reference control. Fixed by scoping the drawer to non-knowledge
  routes and adding a labelled task-context bar.
- Two existing browser specs had been made stale by the shipped UI and were
  updated to assert the new behaviour without dropping assertions.

## Remaining work

- M32 I3–I6: browser agent execution, connected capability execution, extended
  dogfooding, final acceptance and the TUI specification.

## New TODOs

- None.

## Risks or blockers

- Note/folder deletion, note kinds/statuses and work-item references remain
  unsupported by the wire contract and are stated as absent in the docs.
