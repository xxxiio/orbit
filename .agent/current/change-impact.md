---
template: change-impact
template_version: 1.0.0
template_digest: "8618a859250c7346e16cb5e20990e6c6d496e6b238ab8f44824028eba527e134"
project_owned: true
record_contract: change-impact
record_schema_version: 1
status: current
task_id: TASK-0011
---

# Change Impact Declaration

## Task

TASK-0011 — M32 I2: knowledge folders, links and project context.

## Impact classification

| Impact | Value | Reason | Activated gate |
|---|---|---|---|
| public_api | yes | New `markdown-reference-commands`, `markdown-references` and `project-knowledge` routes; the typed-reference grammar moves into `orbit-runtime` with `orbit-cli` delegating. | Compatibility gate: CLI parity tests plus the existing CLI suite. |
| persisted_data | yes | One additive table, `markdown_reference_commands`, records the receipt for a reference write (the derived Markdown payload differs after the first acceptance, so the existing Markdown ledger cannot compare it). No content store, no column and no identity change: notes stay Markdown files. | Migration gate: the table is created on open beside the other journals; replay and restart tests. |
| configuration | no | No new keys. | none |
| dependencies | no | No new crate or npm dependency. | none |
| security_boundary | yes | A reference write publishes Markdown bytes; path safety, symlink rejection and entity existence must hold, and the browser must not author arbitrary paths. | Security gate: vault refusal tests, entity-existence tests, path validation. |
| concurrency | yes | Reference writes reuse the Markdown command receipt and revision; a folder rename changes every path beneath it. | Concurrency gate: replay/conflict tests, invalidation of the whole knowledge family. |
| performance_sensitive | yes | Reference and project-knowledge reads scan the vault once per request; relations are already O(vault) and must not be called per card. | Performance gate: one-scan tests, no per-note relation calls. |
| user_interface | yes | Folder controls, pickers, breadcrumb, link authoring, typed-reference chips, project panel and task Notes section. | UI gate: unit tests, Playwright, keyboard and 390 px passes. |
| deployment | no | No installer or packaging change. | none |

## Assumptions and unknowns

- The vault already owns folder create/move and note create/update/move with
  receipts, crash replay and symlink rejection; I2 exposes them rather than
  rebuilding them.
- Note identity is `orbit_id` frontmatter, so a path change is not an identity
  change; the client must invalidate every knowledge query on a move.
- Typed references support `project` and `task` over HTTP. Work items, habits and
  note kinds/statuses stay out: the existing contracts do not cover them and this
  iteration does not invent them.
- One malformed `.md` fails every knowledge read closed with 409; that is surfaced,
  not smoothed over.

## Required outputs

- Shared reference grammar plus reference command and the two scoped reads.
- Browser folder lifecycle, link authoring, typed references and project/task
  knowledge surfaces.
- Unit tests, Playwright acceptance, documentation and records.

## Test tier

- Tier 4 for the touched surface: runtime/server/CLI tests, clippy
  `-D warnings`, client typecheck and vitest, terminology guard, production build,
  the full Playwright suite against isolated real servers, and
  `getdone-validate-project`. Release qualification remains M15/M27.
