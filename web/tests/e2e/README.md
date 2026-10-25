# Browser acceptance suite (M31 I1)

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

## Layout

```
tests/e2e/harness/server.ts    spawn/health-wait/enroll/teardown, log tail on failure
tests/e2e/harness/session.ts   form sign-in, API sign-in, signed-out assertions
tests/e2e/harness/seed.ts      fixture wiring for one harness server
tests/e2e/harness/fixtures.ts  `server` and `workspace` Playwright fixtures
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
| `tasks.spec.ts` | capture → rename → description → priority → labels → status survives a reload (drawer deep-links via `?task=`); an illegal transition shows the server's refusal and keeps the stored status; a bulk status change applies to every selected task and both persist after reload. |
| `conflict.spec.ts` | a stale-revision command is refused with the server's explanation, the UI keeps the stored value, and a refresh makes the next attempt succeed. |
| `fixture.spec.ts` | the fixture is replayable: a second seed over the same database duplicates no rows and does not advance the authority revision. |
| `terminology.spec.ts` | user-facing copy never shows the legacy internal feature name. |

## Rules the specs follow

* No `page.waitForTimeout`: readiness is asserted with web-first assertions,
  `expect.poll`, and bounded deadlines.
* No weakened assertions to get green. A missing screen is reported as pending
  work, not as a pass.
* Selectors are user-facing (`getByRole`, `getByLabel`) except where the shared
  table and the stacked mobile list both exist in the DOM; those are scoped with
  `page.locator('table')` rather than a hidden-element workaround.

