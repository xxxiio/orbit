# M32 I1 — acceptance evidence

Recorded from real runs on the working tree after lanes A (Rust authority), B (web
client) and C (browser acceptance) were reconciled. Commands were run by the
orchestrator, not quoted from a lane report.

## Commands and results

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo test -p orbit-domain -p orbit-application -p orbit-runtime -p orbit-server --offline` | domain 59, application 17, runtime 570, runtime contract 2, server 56 — all passed |
| `cargo clippy -p orbit-domain -p orbit-application -p orbit-runtime -p orbit-server -p orbit-client --all-targets --offline -- -D warnings` | clean |
| `cargo test -p orbit-client project_workflow` | 2 passed (after fixing a pre-existing stale literal, see below) |
| `cd web && node node_modules/typescript/bin/tsc --noEmit` | exit 0 |
| `cd web && node node_modules/vitest/vitest.mjs run` | 46 files, 249 tests passed |
| `cd web && node scripts/check-terminology.mjs` | 230 files scanned, no user-visible legacy feature name |
| `cd web && node node_modules/vite/bin/vite.js build` | built; only the pre-existing 900 kB chunk-size warning |
| `cd web && node node_modules/@playwright/test/cli.js test <four I1 specs>` | 19 passed |
| `cd web && node node_modules/@playwright/test/cli.js test` | 106 passed, 4 skipped (opt-in evidence captures), 0 failed, 5.5 min |

## What the browser specs prove

- `project-settings.spec.ts` (6): rename reaches header, page title, sidebar and
  index without a reload and keeps the project id; a status move persists across a
  reload; the control offers exactly the server's edges and an illegal move is
  refused without writing; a stale revision is reported, refreshed and never shown
  as success; Cancel and a blank name write nothing.
- `search.spec.ts` (5): one server query groups a project, a task and a note that
  share a marker; each destination opens (including `?task=`); an empty query keeps
  the commands; honest empty state; ↑/↓/Enter navigation and focus restoration;
  unauthenticated search is refused; a result whose note moved away lands on a
  not-found state.
- `saved-views.spec.ts` (6): the filtered list is stored as its query string plus
  display; filters/sort/display restore after a reload and in a second browser
  context (fresh cookies — the server-persistence proof); rename and delete are
  server writes; saving changes neither the workflow binding nor the ledger
  revision; a stale version is refused with the server's words and the list
  refreshes.
- `task-metadata.spec.ts` (2): the drawer's Created/Updated are the server's
  `created_at_ms`/`updated_at_ms`, the full timestamp is available, and a change
  advances Updated while leaving Created alone. The `null` → Unknown branch is
  unit-covered, because a task created through the API always has history.

## Wire verification by hand

Against a disposable `orbit-server` on a fresh database:
create → `action:"update"` (rename + status in one command) → replay of the same
command id returns `duplicate:true` at the original revision → a stale fence is
409 `stale server revision` **before** any domain check → all six statuses with
their legal edges → search groups with server-composed subtitles → task detail
timestamps → saved-view save/list. Empty `q` is 400 `search query is required`.

## Defects found and fixed

1. **Saved-view stale version did not refresh (client).** A stale `expected_version`
   answers 400, not 409, so the shared mutation frame did not refetch and the
   dialog kept showing a row the server had already changed. The three saved-view
   mutations now opt into `invalidateOnRejection`.
2. **Pre-existing red test (unrelated to M32).** `orbit-client`
   `rich_client_tests::project_workflow_binding_roundtrips_through_the_client`
   asserted the bundled `development` workflow version as the literal `2` while
   the bundle declares `3`. Reproduced on a pristine `HEAD` worktree before
   touching anything. The expectation now reads the bundled descriptor, so a
   legitimate manifest bump cannot look like a client regression.
3. **Stale binary trap (process, not product).** The Playwright harness spawns
   `target/debug/orbit-server`; `cargo test` does not rebuild it, so a browser run
   after a server change silently exercised the old routes. Recorded in the
   progress log; the working loop is `cargo build -p orbit-server` → `vite build`
   → `playwright test`.

## Known limitations, stated rather than hidden

- Project completion criteria are not modelled in the authority SQL project row, so
  a status move validates the transition edges and name rules, not success-criterion
  gating. This matches the existing authority create path.
- The board has no URL-backed filter state in this build, so a saved board
  configuration restores the display plus whatever query the URL already carries.
- `LOWER()` folding in search is ASCII, matching SQLite; non-ASCII case folding is
  not attempted.
- A tombstoned saved-view id keeps its primary key, so re-creating that exact id is
  refused; the client mints fresh ids.
