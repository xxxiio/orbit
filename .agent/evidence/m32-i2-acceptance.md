# M32 I2 — acceptance evidence

Recorded from real runs on the reconciled tree after lanes D (reference authority),
E (knowledge UI), F (browser acceptance), G (folder command queue fix) and H
(task context) landed. Commands were run by the orchestrator.

## Commands and results

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo test --workspace --offline` | see the gate table below |
| `cargo test -p orbit-runtime -p orbit-server -p orbit-cli --offline` | runtime 596, server 57, CLI 130, contract 2 — all passed |
| `cargo clippy -p orbit-runtime -p orbit-server -p orbit-cli --all-targets --offline -- -D warnings` | clean |
| `cd web && node node_modules/typescript/bin/tsc --noEmit` | exit 0 |
| `cd web && node node_modules/vitest/vitest.mjs run` | 52 files, 299 tests passed |
| `cd web && node scripts/check-terminology.mjs` | 244 files, no user-visible legacy feature name |
| `cd web && node node_modules/vite/bin/vite.js build` | built; only the pre-existing chunk-size warning |
| `cd web && playwright test <I2 specs>` | 19 passed (folders 8, links 4, project knowledge 7) |
| `cd web && playwright test` | 125 passed, 4 skipped (opt-in evidence captures), 0 failed, 6.4 min |

## What the browser specs prove

Every step is asserted twice: through the interface a person uses, and through the
server read the interface claims to be a view of (`knowledge-tree`,
`markdown-inventory`, `markdown-document`, `markdown-relations`,
`markdown-references`, `project-knowledge`).

- **Folders and documents (8):** folder create at the root; a note created through
  the folder picker inside it (preview path asserted) and reloaded; note rename;
  note move to another folder; a folder rename carrying its notes with the old deep
  link answered honestly; a create whose parent is missing refused with nothing
  created **and a valid folder created immediately afterwards persisted** (the
  regression for the queue-poisoning defect); a folder moved into its own subtree
  refused by name; a move onto an existing name refused with both folders intact;
  breadcrumb parent/child navigation as reload-stable destinations.
- **Links (4):** two notes created through the UI, linked by the id picker, the link
  followed, the back-reference seen on the target, both reloaded; unlink removes the
  outgoing chip, the backlink and the token from the bytes while keeping the prose; a
  hand-written unknown id is shown as unresolved rather than fabricated; an external
  writer between the page's fresh read and its update makes the server refuse, with
  exactly one POST and none of the page's bytes on disk.
- **Project knowledge (7):** a project reference appears in the project's
  `knowledge_refs` panel with its chip and survives reload; a task reference appears
  in the project panel *and* in the task drawer's Notes; add/remove from the panel and
  from the drawer each move the `project-knowledge` read; a reference to an id this
  workspace never had is `broken` in the note and absent from the live project list
  while the vault read still sees the token; a task on a note is a context, not a
  drawer, and can be added, followed to its surface and removed; stopping the context
  keeps the note and the project context across reload.

## Defects found and fixed

1. **Refused folder commands poisoned every later knowledge write (server).**
   Reproduced three times by hand: after `folder-creates {"path":"ghost/child"}` or a
   move onto an existing destination, *every* later folder, note and reference write
   answered 409 `immutable sequence already used`, and a restart did not clear it.
   Cause: the pending intent was never abandoned, and recovery replayed it before every
   command (and at open). Fix: classify the publish outcome, abandon a
   proven-before-write refusal (delete `state='pending'` only), keep ambiguous states
   pending, and return precise 400s (`folder parent does not exist`,
   `folder destination already exists`, `folder does not exist`). Crash semantics are
   unchanged: an intent whose work landed is still accepted on replay. Six regression
   tests plus the browser-level "valid write after a refusal" assertion.
2. **`?task=` on a knowledge route covered the note with the task drawer (client).**
   The note uses `?task=` as context, but `workspace-layout` mounted the drawer on every
   route, hiding the note's own "Reference this task" control. Fix: the drawer is not
   mounted on knowledge routes; a labelled **Task context** bar names the task with
   **Open the task** (its real surface) and **Stop linking** (removes only `task`,
   keeps `?project=`/`?folder=`, hands focus to the note panel). Every other surface
   keeps `?task=` = drawer. Two browser tests plus six unit tests.
3. **A redundant `aria-label`** on the vault's "New folder" button failed axe
   `label-content-name-mismatch`; removed so the visible text is the accessible name.
4. **Two specs had been made stale by the shipped UI** (note create/move dialogs, and
   the knowledge panel now reading project knowledge). Both were updated to assert the
   shipped behaviour; no assertion was dropped.

## Known limitations, stated rather than hidden

- No note or folder deletion, no note kinds/statuses, no work-item references, and no
  relative in-app Markdown links: the existing contracts do not cover them.
- An *ambiguous* folder state (both source and destination present after external
  filesystem interference) still answers 409 with the generic
  `immutable sequence already used`; the status is right, the sentence is generic, and
  the state is unreachable without an external actor.
- Removing a typed reference leaves the reference's surrounding newlines, matching the
  CLI's byte behaviour rather than reflowing the body.
- `markdown-relations.outgoing` also lists ids that appear in `unresolved`; the client
  splits them for display.
- A stale `?folder=` deep link stays selectable in the folder picker; nothing is typed,
  the server refuses, and the refusal is quoted.
- Reference and project-knowledge reads scan the vault once per request (the same
  shape as relations); no index was added.
