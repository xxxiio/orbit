---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: passed
task_id: TASK-0011
---

# Current Acceptance Report

## Task

TASK-0011 — M32 I2: knowledge folders, links and project context.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Folder create/rename/move and note create/rename/move | pass | `crates/orbit-runtime/src/folder_commands.rs` + the `folder-moves`/`folder-creates`/`markdown-commands` routes; `web/src/features/knowledge/{folder-view,folder/*,note-view,note/*}`. `knowledge-folders.spec.ts` 8/8 with every claim read back from `knowledge-tree`/`markdown-inventory`/`markdown-document`. Includes a folder rename carrying its notes, honest old-path deep links, and refusals for a missing parent, a nested destination and an existing name. |
| G-0002 | Note links, back-references and broken targets | pass | `knowledge-links.spec.ts` 4/4: link via the id picker, follow, back-reference, unlink (chips, backlink and bytes), a hand-written unknown id shown as unresolved, and an external writer refused with exactly one POST and none of the page's bytes on disk. |
| G-0003 | Project/task references and project knowledge | pass | `crates/orbit-runtime/src/references.rs` (19 tests) with `markdown-reference-commands`, `markdown-references` and `project-knowledge`; `project-knowledge.spec.ts` 7/7 covers the project panel, the task Notes section, add/remove from both, a broken target, and task context on a note. |
| G-0004 | Shared reference grammar with unchanged CLI behaviour | pass | Grammar moved to `orbit-runtime::references`; `orbit-cli` delegates (`markdown_metadata`, `markdown_discovery`), keeps its `habit:` policy locally, and 130 CLI tests (including two new parity tests) pass unchanged. |
| G-0005 | Browser acceptance and quality gates | pass | Full gates below. A refused folder command no longer blocks later writes — fixed and covered at the runtime and browser level. |

## Commands run

```text
cargo fmt --all -- --check                                   # clean
cargo test --workspace --offline                             # 1,759 passed, 0 failed, 31 binaries, exit 0
cargo test -p orbit-runtime -p orbit-server -p orbit-cli --offline   # 596 / 57 / 130 (+2 contract)
cargo clippy -p orbit-runtime -p orbit-server -p orbit-cli --all-targets --offline -- -D warnings   # clean
cd web && node node_modules/typescript/bin/tsc --noEmit      # exit 0
cd web && node node_modules/vitest/vitest.mjs run            # 52 files, 299 tests passed
cd web && node scripts/check-terminology.mjs                 # 244 files, no legacy feature name
cd web && node node_modules/vite/bin/vite.js build           # built
cd web && playwright test <I2 specs>                         # 19 passed
cd web && node node_modules/@playwright/test/cli.js test     # 125 passed, 4 skipped, 0 failed
```

## Checks not run

- M15 full release profile and M27 publication verification (release
  qualification remains M15/M27; deliberately not regenerated).
- Live Herdr/model acceptance (unchanged external requirement).

## Waivers

- None.

## Residual risk

- No note/folder deletion, note kinds/statuses, work-item references or relative
  in-app Markdown links; the existing contracts do not cover them.
- An *ambiguous* folder state (both source and destination present after external
  filesystem interference) returns 409 with the generic mapping sentence.
- Removing a typed reference keeps its surrounding newlines, matching the CLI bytes.
- Reference and project-knowledge reads scan the vault once per request.
