# M33 local-trusted authentication evidence

Real Chromium against a real `orbit-server`, a real throwaway SQLite database and
the built SPA (`web/dist`) served by that same server. Nothing is mocked: the
captures and the acceptance run below come from HTTP requests and rendered React.

## What this set shows

1. `ORBIT_AUTH_MODE=local-trusted` opens the workspace with no device login and no
   cookie, and the product surface is fully usable (project, milestone, task,
   List/Board/Backlog, Roadmap).
2. The same durable data survives a real server restart in local-trusted mode.
3. Every privileged credential boundary, the CSRF/browser-proof guard, the
   `Origin`/`Sec-Fetch-Site` rules and the loopback `Host` rule still refuse.
4. Restarting the same database with `ORBIT_AUTH_MODE` removed switches the
   server back to the default authenticated policy: the browser revalidates to
   `/login` and the data is still there after signing in with the enrolled
   harness device.

## Provenance

| fact | value |
| --- | --- |
| commit | `d7d05ee` (`git rev-parse --short HEAD`) |
| working tree | the local-trusted implementation (server `auth.rs`/`main.rs`, web session/guard/sidebar changes, this spec) was **uncommitted** at capture time; the commit above is the last committed baseline |
| date | 2026-10-31 |
| node | v18.19.1 |
| Playwright | 1.56.1 (bundled Chromium, headless) |
| viewports | `1440x900`, `390x844` at `deviceScaleFactor: 1` |
| themes | `light`, `dark` (`orbit.theme` in `localStorage` before first paint + `colorScheme`) |
| server binary | `target/debug/orbit-server` |
| database | throwaway: `${os.tmpdir()}/orbit-web-e2e/<uuid>/data/server.db`, deleted by teardown |

### Build

```bash
cargo build -p orbit-server --offline           # target/debug/orbit-server
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vite/bin/vite.js build  # web/dist, served by orbit-server
```

### Server command / environment

The spec starts the harness (`web/tests/e2e/harness/server.ts`), which spawns
the binary exactly once per test with `ORBIT_*` variables from the caller's shell
stripped first:

```text
target/debug/orbit-server
  ORBIT_AUTH_MODE=local-trusted
  ORBIT_SERVER_BIND=127.0.0.1:<free port>
  ORBIT_SERVER_DB=<tmp>/orbit-web-e2e/<uuid>/data/server.db
  ORBIT_SERVER_ADMIN_KEY=<random 64 hex chars>
  ORBIT_WEB_DIST=<repo>/web/dist
```

Startup printed:

```text
Orbit server listening on http://127.0.0.1:<port>
Authentication: Local trusted mode
Access: Local machine only
```

The policy switch reuses the same database and re-spawns with `ORBIT_AUTH_MODE`
removed (`restart({ env: { ORBIT_AUTH_MODE: '' } })`), which printed
`Authentication: Device authentication`.

### Capture command

```bash
cd web
ORBIT_CAPTURE=1 LD_LIBRARY_PATH=/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu \
  node node_modules/@playwright/test/cli.js test \
  tests/e2e/local-trusted.spec.ts --project=chromium
```

Capture is opt-in: without `ORBIT_CAPTURE=1` the capture test skips, so a normal
acceptance run never rewrites committed evidence. The capture test fails if any
page error or `console.error` occurs on a captured page.

## Files

All screenshots contain the project `Local Trusted Workspace`, the milestone
`Local Trusted Milestone` (authored start + target) and the fully configured task
`Ship the local-trusted acceptance lane`, created through the UI before capture.

| file | frame |
| --- | --- |
| `workspace-local-trusted-light-1440x900.png` | Home workspace, light, desktop |
| `workspace-local-trusted-dark-1440x900.png` | Home workspace, dark, desktop |
| `workspace-local-trusted-light-390x844.png` | Home workspace, light, phone |
| `workspace-local-trusted-dark-390x844.png` | Home workspace, dark, phone |
| `sidebar-local-trusted-light-1440x900.png` | sidebar footer (`Local trusted mode`), light, desktop |
| `sidebar-local-trusted-dark-1440x900.png` | sidebar footer, dark, desktop |
| `sidebar-local-trusted-light-390x844.png` | navigation drawer with the policy footer, light, phone |
| `sidebar-local-trusted-dark-390x844.png` | navigation drawer with the policy footer, dark, phone |
| `roadmap-local-trusted-light-1440x900.png` | project Roadmap with the milestone bar, light, desktop |
| `roadmap-local-trusted-dark-1440x900.png` | milestone bar, dark, desktop |
| `roadmap-local-trusted-light-390x844.png` | milestone bar, light, phone |
| `roadmap-local-trusted-dark-390x844.png` | milestone bar, dark, phone |

At 1440x900 the sidebar footer is already in view, so the desktop "sidebar"
frame is the same viewport-wide composition as the workspace frame; it exists to
prove the footer carries the plain `Local trusted mode` line (no device id, no
sign-out control). At 390x844 the sidebar is hidden until the drawer opens, and
the drawer is what is captured.

## Observed results

Run on the machine and working tree described above; these are the observed
counts, not expectations.

| command | result |
| --- | --- |
| `node node_modules/typescript/bin/tsc --noEmit` | pass (no output) |
| `cargo build -p orbit-server --offline` | pass (`Finished dev profile`) |
| `node node_modules/vite/bin/vite.js build` | pass (`✓ built in 5.14s`) |
| `... playwright test tests/e2e/local-trusted.spec.ts --project=chromium` | **2 passed, 1 skipped, 0 failed** (skip = capture test without `ORBIT_CAPTURE=1`) |
| `ORBIT_CAPTURE=1 ... playwright test tests/e2e/local-trusted.spec.ts --project=chromium` | **3 passed, 0 failed** |
| `... playwright test tests/e2e/auth.spec.ts --project=chromium` | **6 passed, 0 failed** (scenario B) |

### Scenario A — local-trusted workspace

Observed pass. With `ORBIT_AUTH_MODE=local-trusted` and loopback bind:

- `GET /` rendered Home (`Today · Orbit`, a level-1 heading, sidebar footer
  `Local trusted mode`) with **no** `Sign in to Orbit` heading; `/login`
  redirected straight into the workspace.
- No `orbit_session` cookie was ever present (`context.cookies()` empty for the
  session name), and a second, independent browser context also entered directly.
- Creating a project, a dated milestone and a fully configured task (title,
  project, description, milestone, status `Planned`, priority `Urgent`, deadline,
  estimate `90`, acceptance criterion) all succeeded through the real
  authoritative commands; the drawer showed every value and the milestone.
- The task appeared in List, Board (`view=board`, board card with the title) and
  Backlog (`view=backlog`, heading `Backlog`); the Roadmap drew the milestone's
  authored start+target bar and the child task row.
- Reload kept the same workspace with no prompt.
- `restart()` (same DB, same local-trusted env) kept the project, task and
  milestone; `GET /v1/web/session` returned
  `{"mode":"local-trusted","device_id":"local-trusted","display_name":"Local browser","expires_at_ms":null}`
  with `Cache-Control: no-store` and no `Set-Cookie`.
- Policy switch (`restart({ env: { ORBIT_AUTH_MODE: '' } })`) made the cached
  local-trusted answer stale: reload landed on `/login`, the workspace/sidebar
  were gone, `GET /v1/web/session` answered `401`, and a product read carrying
  `X-Forwarded-For: 203.0.113.9` / `X-Real-IP: 127.0.0.1` also answered `401`.
  Signing in with the enrolled harness device then showed the same project and
  task, proving the switch reused the database.

### Scenario B — default authenticated mode is unchanged

Observed pass (`auth.spec.ts`, 6/6). A first-time visitor still gets the sign-in
form, a wrong token is refused inline without a cookie, valid credentials set an
HttpOnly `orbit_session` cookie and survive a reload, sign-out clears the server
session, an expired session is explained on `/login`, and revoking the device
kills a live browser session. Product reads without a credential remain `401`.

### Scenario C — security boundary

Observed pass, all against the real local-trusted listener (no mocks):

| request | observed |
| --- | --- |
| `POST /v1/devices` (no credential) | `403 administrator credential required` |
| `POST /v1/authoritative/capabilities/install` | `403 operator credential required` |
| `POST /v1/authoritative/capability-executors/announce` | `403 executor credential required` |
| `POST /v1/authoritative/agent-machines/claim` | `403 enrolled device credential required` |
| `POST /v1/authoritative/project-commands` without `x-orbit-csrf` | `403 csrf check failed` |
| same, with `x-orbit-csrf: 1` and `Sec-Fetch-Site: cross-site` | `403` |
| same, with `x-orbit-csrf: 1` and `Origin: https://evil.example` | `403` |
| same, with `x-orbit-csrf: 1` and no foreign origin (control) | `200` |
| `GET /v1/authoritative/projects` with `Host: public.example.com` | `403 host not allowed in local trusted mode` |
| same with `Host: 127.0.0.1.nip.io` | `403 host not allowed in local trusted mode` |
| `GET /v1/authoritative/projects` with `X-Forwarded-For: 203.0.113.9` | `200` in local mode; `401` in authenticated mode (policy-switch half) |

Playwright request contexts forward an explicit `Host` header verbatim, so the
`Host` cases exercise the real listener; no raw socket workaround was needed.

### What remains protected in local-trusted mode

- Administrator actions (`/v1/devices` enroll/revoke/rotate) need the server
  admin key; local trust never grants administrator rights.
- Operator actions (capability install/authorize/revoke) need an operator
  credential.
- Executor actions (`capability-executors/announce`) need an executor credential.
- Machine transports (`/v1/authoritative/agent-machines/*`,
  `/v1/authoritative/agent-jobs/continue`) need an enrolled device credential;
  the synthetic local principal is refused.
- `/mcp` still refuses without a credential (it has no local-trust branch).
- Every mutation still needs the browser proof (`x-orbit-csrf: 1`, no
  `Sec-Fetch-Site: cross-site`, and an absent or same-as-`Host` `Origin`), even
  though there is no cookie to protect.
- Every route rejects a non-loopback `Host` (public DNS name or a name that
  resolves to loopback such as `127.0.0.1.nip.io`), and the server refuses to
  start in local-trusted mode unless the bind address is loopback.

### Explicit limitation

Local-trusted mode is a **machine-level** policy: any process on the machine
that can reach the loopback listener is trusted exactly like the browser, with
no prompt, cookie or per-process credential. It is suitable only for a
single-user workstation. It does not defend against other local users,
malware, or a local reverse proxy that rewrites `Host` to a loopback name, and it
must never be combined with a non-loopback bind (the server refuses that
configuration) or with exposing the listener beyond the machine.
