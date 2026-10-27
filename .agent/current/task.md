---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0011
status: completed
milestone_id: MILESTONE-0032
---

# Task: M32 I2 — knowledge workflows

## Objective

Make knowledge usable as a working part of Orbit rather than a read-only
Markdown browser: manage folders and document locations, connect notes to each
other and to project entities, and reach relevant notes from the project and task
that need them.

## Current behaviour

The vault and the authority already support folder create/move (a rename is a
move) and note create/edit/move with receipts, crash replay and symlink
rejection, and the note view shows `[[id]]` outgoing links, backlinks and
unresolved targets. But no browser surface calls the folder endpoints, a note
cannot be renamed without using "Move", there is no way to create a link, typed
references to projects/tasks exist only in the CLI, and the declared
`knowledge_refs` panel renders the global inventory on a project tab.

## Desired behaviour

A person can create a folder, create a note inside it, rename or move either,
see where a note lives, and recover from an invalid destination with the
server's explanation. Notes can be linked and unlinked from a searchable id
picker, project and task references can be added and removed from a note, and a
reference to a deleted entity is shown as broken rather than dropped. A project
shows the notes that reference it or its tasks, and a task drawer shows the notes
that reference it, with a link action that uses the same reference grammar.

## Scope

- Shared typed-reference grammar in `orbit-runtime`, with `orbit-cli` delegating
  to it and its behaviour unchanged.
- `POST markdown-reference-commands` (add/remove, entity existence, receipt and
  revision through the existing Markdown command path).
- `GET markdown-references?target=` and `GET project-knowledge?project_id=`, one
  vault scan each.
- Web: folder lifecycle UI with pickers and breadcrumb, note rename, link/unlink
  authoring, typed-reference chips with broken-target state, project-aware
  `knowledge_refs`, and a Notes section in the task drawer.
- Unit tests, Playwright acceptance and documentation.

## Out of scope

- Note or folder deletion, note kinds/statuses, note attachments in the view,
  and work-item references: the existing contracts do not support them and this
  iteration does not invent them.
- Any second content store, a body column, or reviving the retired SQL knowledge
  tables.
- Agent execution (I3), connected capabilities (I4), dogfooding (I5), final
  acceptance and the TUI specification (I6). No TUI change.

## Dependencies

- M31/M32 I1 knowledge routes, vault, `[[id]]` relations and the shared mutation
  frame.
- `orbit_reference_work_item_notes` is unrelated free text and is not reused.

## Risks

- A folder rename changes every path under it; the client must invalidate the
  whole knowledge query family and follow or report the open note.
- A malformed `.md` fails every knowledge read closed with 409; reads must not
  degrade to "no notes".
- Reimplementing the reference grammar in the server would drift from the CLI;
  the parser moves to the runtime and the CLI delegates.
- A reference command must reuse the Markdown command path, not add a second
  write path with its own receipt rules.

## Acceptance criteria

- [x] AC-001: Folders can be created, renamed and moved from the browser, notes
  can be created in, renamed and moved between them, location and hierarchy are
  visible, and invalid operations are refused with the server's message.
- [x] AC-002: Two notes can be linked and unlinked, a link navigates, the
  back-reference is visible, and a broken target is indicated.
- [x] AC-003: A note can reference a project or task, the reference is shown and
  navigable, a reference to a deleted entity is shown as broken, and removes work.
- [x] AC-004: A project shows the notes referencing it or its tasks, and a task
  drawer shows and can add the notes referencing that task.
- [x] AC-005: Rust, client typecheck, vitest, terminology, build and Playwright
  acceptance pass against a real server.

## Validation

```sh
cargo fmt --all -- --check
cargo test -p orbit-runtime -p orbit-server -p orbit-cli --offline
cargo clippy -p orbit-runtime -p orbit-server -p orbit-cli --all-targets --offline -- -D warnings
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node scripts/check-terminology.mjs
cd web && node node_modules/vite/bin/vite.js build
cd web && node node_modules/@playwright/test/cli.js test
getdone-validate-project
```
