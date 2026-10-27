---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0016
status: completed
milestone_id: MILESTONE-0032
advances_exit_criteria: [EC-001, EC-002, EC-003, EC-004, EC-009, EC-011]
---

# Next Deterministic Step

## Objective

Land M32 iteration I2: knowledge folders and document locations, note links with
back-references, and project/task-contextual knowledge — each proven by
real-browser acceptance against an isolated server.

## Why this is next

I1 closed the everyday project-management gaps. Knowledge is the next daily
surface that still needs the CLI: the vault and the server already support folder
and note moves with receipts, but the browser offers no folder control, no way to
create a link, and no project-scoped knowledge. Until those exist, notes cannot be
used as part of project work, and I5 dogfooding cannot evaluate a knowledge
workflow.

## Preconditions

- M32 I1 committed (`1c27354`); knowledge vault, folder commands, note commands
  and `[[id]]` relations ship and are tested.
- Frozen I2 contracts in `.slim/deepwork/m32-web-completeness/i2-plan.md`.

## Inputs

- `web/src/features/knowledge/**`, `web/src/api/{endpoints,mutations,queries}.ts`.
- `crates/orbit-runtime/src/{references.rs,markdown_commands.rs,lib.rs}`,
  `crates/orbit-server/src/main.rs`, `crates/orbit-cli/src/{markdown_metadata,
  markdown_discovery}.rs`.
- `.slim/deepwork/m32-web-completeness/i2-plan.md`.

## Ordered actions

1. Runtime lane: shared reference grammar, reference command through the Markdown
   command path, reference read and project-knowledge read, with tests.
2. Client lane: folder lifecycle with pickers and breadcrumb, note rename,
   link/unlink authoring, typed references with broken-target state,
   project-aware `knowledge_refs`, task-drawer Notes section.
3. Browser acceptance lane: folder, link and project-knowledge specs; fix
   integration defects in the owning layer.
4. Run the gates, record evidence, update milestone criteria and journal, then
   start I3.

## Expected outputs

- Working folder create/rename/move and document move/rename from the browser.
- A link picker with back-references, and project/task references that navigate.
- A project knowledge panel and a task Notes section backed by one server scan.
- Rust, unit and browser evidence.

## Acceptance criteria

- [x] NS-001: Folder create/rename/move and note create/rename/move persist and
  refuse invalid destinations with the server's message.
- [x] NS-002: A note link can be created and removed and its back-reference is
  visible; a broken target is indicated.
- [x] NS-003: Project/task references can be added and removed and appear in the
  project knowledge read; a reference to a deleted entity is shown as broken.
- [x] NS-004: The reference grammar is shared with the CLI and CLI behaviour is
  unchanged.
- [x] NS-005: Full Rust tests, clippy, typecheck, vitest, terminology, build and
  Playwright pass on the frozen tree.
- [x] NS-006: `getdone-validate-project` exits 0.

## Validation

```sh
cargo test -p orbit-runtime -p orbit-server -p orbit-cli --offline
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node scripts/check-terminology.mjs
cd web && node node_modules/vite/bin/vite.js build
cd web && node node_modules/@playwright/test/cli.js test
getdone-validate-project
```

## Stop conditions

- A second content store, a body column, or a revived SQL knowledge table would be
  needed.
- A reference write would bypass the Markdown command receipt or revision.
- The client would have to reimplement the reference grammar.
- Project scoping would have to be faked by filtering the global inventory.

## Out of scope

- Note/folder deletion, note kinds/statuses, work-item references, I3–I6 work, and
  any TUI change.
