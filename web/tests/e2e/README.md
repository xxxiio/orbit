# Browser acceptance suite (M31 I1 + I2)

These specs drive the **built** web app in real Chromium against a **real**
`target/debug/orbit-server` with a throwaway SQLite database. Nothing is mocked;
`web/tests/unit` (vitest) covers the API layer separately.

## How it runs

`npm run test:e2e` builds the app, then Playwright:

```bash
# npm is the Windows interop build on this machine; use node directly:
node node_modules/typescript/bin/tsc --noEmit
node node_modules/vite/bin/vite.js build
node node_modules/@playwright/test/cli.js test
```

Each test starts its own server (`harness/server.ts`) with:

| env | value |
|---|---|
| `ORBIT_SERVER_DB` | `${os.tmpdir()}/orbit-web-e2e/<uuid>/data/server.db` |
| `ORBIT_SERVER_BIND` | `127.0.0.1:<free port>` |
| `ORBIT_SERVER_ADMIN_KEY` | random 64 hex chars |
| `ORBIT_WEB_DIST` | `<repo>/web/dist` |

The app is served by **that server**, not by `vite preview`: `orbit-server` has no
CORS layer, so the SPA and the API must share an origin. `ORBIT_*` variables from
the caller's environment are stripped before spawning, so a developer's real
configuration cannot leak into the test database.

`ORBIT_KEEP_E2E_STATE=1` keeps the temp directory (log + database) after a run.
`ORBIT_E2E_EVIDENCE=1` writes traces/screenshots to `web/evidence/e2e/<run-id>/`.

Committed I2 captures are produced from the seeded fixture, not a preview server:

```bash
ORBIT_I2_EVIDENCE=1 node node_modules/@playwright/test/cli.js test \
  tests/e2e/evidence-i2.spec.ts --project=chromium
```

The capture spec skips itself in a normal run (it would otherwise rewrite committed
evidence on every pass).

## Specs

| File | Covers |
| --- | --- |
| `auth.spec.ts` | sign-in, expiry, revocation, sign-out |
| `home.spec.ts` | server-derived Home sections and empty state |
| `projects.spec.ts` | project creation, duplicate names, validation, task creation |
| `project-settings.spec.ts` | rename + status through authoritative commands, server-offered status edges, illegal move refused, stale revision, cancel, blank name |
| `search.spec.ts` | one server search across projects/tasks/knowledge: grouping, keyboard navigation, opening each result, unauthenticated refusal, stale target |
| `saved-views.spec.ts` | save/restore/rename/delete a personal view, browser restart, no workflow or ledger change, stale version refused |
| `task-metadata.spec.ts` | recorded Created/Updated values in the drawer, Updated advances on a change |
| `tasks.spec.ts` | capture, drawer edits, rejected transition, bulk status |
| `conflict.spec.ts` | stale revision refusal and recovery |
| `workspaces.spec.ts` | board moves (accepted/rejected), milestone lifecycle + invalid edge, dependency add/remove + readiness, workflow change, task-list URL state |
| `planning.spec.ts` | day plan requires authored capacity, review → approval → apply survives reload, session renewal on visibility |
| `knowledge-folders.spec.ts` | folder create → note inside it, note rename/move, folder rename carries its notes, invalid destination refusals, breadcrumb destinations — each checked on screen and through `knowledge-tree`/`markdown-inventory`/`markdown-document` |
| `knowledge-links.spec.ts` | link two notes by id (outliner picker), follow the link, back-references, unlink removes token + backlink, an unknown `[[id]]` is named as unresolved, and an external writer between the read and the send is refused with no retry |
| `project-knowledge.spec.ts` | project and task references from a note land in the project's `knowledge_refs` panel and the task drawer's Notes, add/remove follows the server read, a reference to a missing task is broken and not project knowledge, and the panel never claims unavailability on a server that serves the route |
| `evidence-i2.spec.ts` | opt-in evidence capture into `web/evidence/i2/` |
| `capability-execution.spec.ts` | connected capability execution: install → publish → grant → announce, the panel's availability codes (`executor_missing`, `not_authorized`) and the refusals with no request control, a request through the form → real `orbit capability serve` → settled receipt + exactly one durable note with the item unchanged, single-claim/idempotency across two devices, an `uncertain` outcome settled by an explicit operator decision, and a real server restart keeping the claim, receipt and note |
| `terminology.spec.ts` | no legacy feature name in rendered primary routes |
| `fixture.spec.ts` | the fixture is deterministic and replayable |

## Layout

```
tests/e2e/harness/server.ts    spawn/health-wait/enroll/teardown, restart (same DB and
                               credentials, same origin), log tail on failure
tests/e2e/harness/session.ts   form sign-in, API sign-in (any enrolled device), signed-out
                               assertions
tests/e2e/harness/seed.ts      fixture wiring for one harness server
tests/e2e/harness/authority.ts authoritative reads/writes for the I1 specs
tests/e2e/harness/fixtures.ts  `server` and `workspace` Playwright fixtures
tests/e2e/harness/agents.ts    the deterministic fake executor: start, claim,
                               launch-result, observe, handoff, interrupt, review, plus
                               the job/overview/executor/handoff/review reads
tests/e2e/harness/capability.ts connected capability cycle: operator (bearer) install and
                               grant, executor (bearer) announce/claim/result, the
                               browser-legal reads and writes, and `runExecutor` — the real
                               `orbit capability serve --once --json` against the harness
                               server, awaited with a deadline
tests/e2e/harness/knowledge.ts server reads (tree, inventory, document, relations,
                               references, project knowledge) and the knowledge
                               dialogs/pickers the I2 specs drive
tests/fixture/seed.ts          the reusable seed itself (also usable for dogfooding)
```

`tests/fixture/seed.ts` is transport-agnostic: `seedWorkspaceForServer({ base,
deviceId, token })` seeds any running server through the public API with
deterministic ids (6 projects, 122 tasks across every status/priority, milestones,
acceptance criteria, prerequisites, markdown notes, attachments, agent jobs with
one handoff awaiting review).

The fixture is a fixed command sequence with `expected_revision` derived from the
command index, so running it twice against the same database replays the receipts
instead of duplicating rows — but only when both runs use the same `nowMs`,
because deadlines are part of the command payload. `fixture.spec.ts` enforces
both properties. Anything else writing to the database first fails loudly
("advanced the authority revision to N, expected M") rather than silently
diverging.

The `workspace` fixture seeds against `fixtureNowMs()` — today at 09:00 local —
and pins the browser clock to that same instant. "Overdue" and "due today" are
then judged at the moment the deadlines were authored, so a suite that runs at
22:00 asserts the same sections a suite that runs at 10:00 does.

## Environment prerequisites on this machine

* `npm` on `PATH` is the Windows build (WSL interop) and fails; call the CLI
  entries through `node` as shown above.
* Playwright's Chromium needs `libnspr4`, `libnss3`, `libnssutil3`, `libsmime3`
  and `libasound2`, which are **not** installed system-wide and cannot be
  installed (`sudo` needs a password). A prior lane unpacked the `.deb` payloads
  to `/tmp/pwlibs`; run Playwright with that directory on the loader path:

  ```bash
  LD_LIBRARY_PATH=/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu \
    node node_modules/@playwright/test/cli.js test
  ```

  `playwright.config.ts` honours `ORBIT_E2E_BROWSER_LIBS` (or the same
  `~/.cache/orbit-e2e/pwlibs` fallback) and prepends it for the workers, so a
  plain `playwright test` works too when either path exists.

## What this suite asserts (as of the M31 I1 build)

There is no "known defect" list any more: every spec below is expected to pass
against the shipped build, and a failure is a product regression.

| spec | owned properties |
|---|---|
| `auth.spec.ts` | a wrong token is refused inline with no cookie; valid credentials leave `/login` and survive a reload with an HttpOnly cookie; sign-out clears the server session and lands on `/login`; an expired session is explained on `/login`; revoking the device with the admin key kills a live browser session (`reload → /login`, `/v1/web/session` 401, cookie gone). `mobile-chromium` runs this file at a phone viewport. |
| `home.spec.ts` | an empty workspace renders an empty state rather than an error; stat counts come from the snapshot; section "Show all" actually reveals the rest; status controls write through the server. |
| `projects.spec.ts` | creating a project is an authoritative write that appears in the shell, the index and after reload; two projects may share a name; empty names are refused without a request; a task created *inside* a new project, edited and status-changed, keeps its values after reload. |
| `project-settings.spec.ts` | a rename reaches the header, page title, sidebar and index without a reload and keeps the project id; a status move persists across a reload; the status control offers exactly the server's edges and an illegal move is refused without writing; a stale revision is reported, refreshed and never shown as success; Cancel and a blank name write nothing. |
| `search.spec.ts` | one server query groups a project, a task and a note that share a seeded marker, activates each destination (including `?task=`), keeps the command list for an empty query, shows an honest empty state, navigates with ↑/↓/Enter, restores focus on Escape, refuses unauthenticated search, and lands a stale result on a not-found state. |
| `saved-views.spec.ts` | status + priority + sort are stored as the URL query with the list display; the view restores filters, sort and display after a reload and in a second browser context; rename and delete are server writes; saving changes no workflow binding and does not advance the ledger; a stale version is refused with the server's words and the list refreshes. |
| `task-metadata.spec.ts` | the drawer's Created/Updated `datetime` values are the server's `created_at_ms`/`updated_at_ms`, readable labels carry the full timestamp, a change advances Updated and leaves Created alone, and a real task never shows `Unknown` (the null branch is unit-covered). |
| `tasks.spec.ts` | capture → rename → description → priority → labels → status survives a reload (drawer deep-links via `?task=`); an illegal transition shows the server's refusal and keeps the stored status; a bulk status change applies to every selected task and both persist after reload. |
| `conflict.spec.ts` | a stale-revision command is refused with the server's explanation, the UI keeps the stored value, and a refresh makes the next attempt succeed. |
| `fixture.spec.ts` | the fixture is replayable: a second seed over the same database duplicates no rows and does not advance the authority revision. |
| `agents.spec.ts` | the Agents workspace reads only jobs this device owns: overview → job → review, and the task is unchanged; an unconfirmed review is discarded by an explicit refresh; an interrupted job resumes into a new queued job; a foreign job id — including one another enrolled device really started — is "not visible", never "missing". |
| `agents-launch.spec.ts` | the launch path end to end with a deterministic fake executor (the fixture's own device credential posting `agent-machines/*`): drawer → dialog → queued `preparing` job → claim → launch result → observe `running` → handoff → review, with the objective frozen into the brief and the Orbit task unchanged; the confirmation names a chosen device that has never claimed a job and still queues the job; the "every enrolled executor is revoked" state disables both launch triggers with the server's own explanation and posts nothing; an unknown `machine_ref` is refused `400 unknown agent executor` before a job exists; a refused launch retries the *same* `job_id` and leaves exactly one job row; an unclaimed job is "Awaiting an executor", not attention; and the job, its claim state, handoff and review all survive a real server restart. |
| `terminology.spec.ts` | user-facing copy never shows the legacy internal feature name. |
| `capability-execution.spec.ts` | the panel reads durable install/grant/executor facts; availability names exactly one code (`executor_missing` before an announcement, `not_authorized` after one but before a grant) and an unavailable action has no request control; the form refuses an empty required note without posting; a stale `expected_revision` is `409 capability request conflict` and a forged body is `422`; a request through the form reaches `pending`, the real executor reports `succeeded`, the row and receipt settle, exactly one note with the typed text is stored, and the item's state and revision never move; one command id takes one claim (a second device's bearer is `409 capability action already claimed`, its result refused) and a repeated result returns the original receipt; an `uncertain` report offers no retry and is settled only by "Confirm it succeeded", with `reconciled_by` recorded as the operator device and no effect invented; a real server restart keeps the claim, receipt and note, and the next executor run is `idle`. |

## Rules the specs follow

* No `page.waitForTimeout`: readiness is asserted with web-first assertions,
  `expect.poll`, and bounded deadlines.
* No weakened assertions to get green. A missing screen is reported as pending
  work, not as a pass.
* Selectors are user-facing (`getByRole`, `getByLabel`) except where the shared
  table and the stacked mobile list both exist in the DOM; those are scoped with
  `page.locator('table')` rather than a hidden-element workaround.

