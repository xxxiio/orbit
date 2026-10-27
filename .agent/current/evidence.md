---
template: evidence-manifest
template_version: 1.0.0
template_digest: "2787d570257f4ffbf8add20f62aa21227a4aea1a6723583e8928b5e71e09f79b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: final
task_id: TASK-0011
---

# Evidence Manifest

## Task

TASK-0011 — M32 I2: knowledge folders, links and project context.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001: Folder and document lifecycle | pass | `crates/orbit-runtime/src/folder_commands.rs` (refusal classification + queue release), `web/src/features/knowledge/folder-view.tsx`, `folder/{folder-picker,folder-create-dialog,folder-move-dialog}.tsx`, `note/note-move-dialog.tsx`; `web/tests/e2e/knowledge-folders.spec.ts` 8/8; `web/tests/unit/domain/folder-model.spec.ts`. |
| AC-002: Note links and back-references | pass | `web/src/features/knowledge/reference-model.ts`, `note/{note-link-dialog,note-references}.tsx`, `note/use-note-body-edit.ts`; `knowledge-links.spec.ts` 4/4; `web/tests/unit/domain/reference-model.spec.ts`. |
| AC-003: Project/task references | pass | `crates/orbit-runtime/src/references.rs` (19 tests), three routes, `web/tests/unit/domain/reference-model.spec.ts`; `project-knowledge.spec.ts` covers add/remove/broken/navigation. |
| AC-004: Project and task knowledge context | pass | `web/src/features/knowledge/knowledge-panel.tsx` (project-aware `knowledge_refs`), `project-knowledge-model.ts`, `features/tasks/task-drawer/notes-section.tsx`, `note-context-bar.tsx`; `project-knowledge.spec.ts` 7/7; `web/tests/unit/features/task-notes-section.spec.tsx`. |
| AC-005: Quality gates | pass | See the gate table below. |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Workspace tests | pass | `cargo test --workspace --offline` | 1,759 passed, 0 failed, 31 binaries, exit 0 |
| Lints | pass | `cargo clippy -p orbit-runtime -p orbit-server -p orbit-cli --all-targets --offline -- -D warnings` | clean |
| Formatting | pass | `cargo fmt --all -- --check` | clean |
| Client typecheck | pass | `tsc --noEmit` | exit 0 |
| Client unit tests | pass | `vitest run` | 52 files, 299 tests passed |
| Terminology guard | pass | `check-terminology.mjs` | 244 files, clean |
| Production build | pass | `vite build` | built; only the pre-existing chunk-size warning |
| Browser acceptance (I2) | pass | `playwright test <I2 specs>` | 19 passed |
| Browser acceptance (full) | pass | `playwright test` | 125 passed, 4 skipped (opt-in captures), 0 failed |
| Acceptance record | pass | `.agent/evidence/m32-i2-acceptance.md` | written from the runs above |

## Checks not run

- M15 full release profile and M27 publication verification (release
  qualification remains M15/M27).
- Live Herdr/model acceptance (unchanged external requirement).

## Waivers

- None.

## Residual risk

- No deletion, kinds/statuses, work-item references or relative in-app links.
- An ambiguous folder state (external filesystem interference) returns the generic
  409 mapping sentence.
- Typed-reference removal keeps surrounding newlines, matching the CLI bytes.
