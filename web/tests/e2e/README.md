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

## Defects this suite currently catches (as of the M31 I1 build)

These fail on purpose; fix the product, not the assertion.

1. `home.spec.ts` → *an empty workspace says so*: a fresh database makes Home
   throw (`Slot failed to slot onto its children`) and the root error boundary
   shows "UNEXPECTED ERROR / This screen stopped working". Repro: enroll a device
   on an empty DB, sign in.
2. `home.spec.ts` → *"Show all" reveals the rest*: the toggle writes
   `expanded.blocked` while the renderer reads `expanded['Blocked tasks']`, so the
   label flips to "Show less" and the list stays capped at five rows.
3. `auth.spec.ts` → *an expired session … says why*: the redirect to `/login` is
   silent; the login page never explains that the session ended.

## Rules the specs follow

* No `page.waitForTimeout`: readiness is asserted with web-first assertions,
  `expect.poll`, and bounded deadlines.
* No weakened assertions to get green. A missing screen is reported as pending
  work, not as a pass.
