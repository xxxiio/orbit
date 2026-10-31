---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-31"
task_id: TASK-0015
---

# Journal Entry: 2026-10-31 — Local-trusted browser authentication inside M33

## Context

M33's browser client required a Device ID and device token for every fresh browser
profile. That is right for enrolled machines and automation, but it makes a personal
single-machine installation feel like a remote deployment. The milestone was asked to
keep the default authenticated mode, add an explicitly opt-in local mode for a
loopback-only personal install, and keep administrator, operator and executor
privileges out of the browser's reach entirely.

The work happened on top of the corrected M33 tree (`d7d05ee`), with the Milestone
model, unified Tasks workspace, compact Board and Milestone Roadmap retained. M33
stays active: the human review (EC-010) and the new authentication criterion (EC-011)
are not self-approvable.

## Work completed

- **Policy.** New `crates/orbit-server/src/auth.rs` with `AuthenticationMode`
  (`authenticated` default, `local-trusted` opt-in), a `Principal` type
  (`Device` / `LocalTrusted`), literal-`SocketAddr` bind validation, a `Host`
  allowlist and an `Origin`-equals-`Host` check. `ORBIT_AUTH_MODE` is parsed at
  startup; an unknown value fails closed and a local-trusted bind is validated
  before the database is opened, then re-checked after `TcpListener::bind`.
- **Guards.** `principal()` resolves bearer first (a bad bearer never falls back to
  local trust), then the local principal, then the cookie session. `device()` keeps
  its `String` shape so the ~120 product call sites are unchanged. `enrolled_device()`
  refuses the local principal for `/agent-machines/*`, `/agent-jobs/continue`,
  automation-run and agent-proposal claim/settle, and `/mcp`. `admin()`,
  `operator_device()` and `executor_device()` keep their credential-only semantics and
  answer `403` (not `401`) to a local browser, so the SPA does not read "not
  authorized for this operation" as a dead session.
- **Session contract.** `GET /v1/web/session` now returns `mode`, `device_id`,
  `display_name` and a nullable `expires_at_ms`; in local-trusted mode it returns the
  local principal with `no-store`, never sets a cookie, revokes and clears any stale
  session cookie it is shown. `POST` returns `409`; `DELETE` is a clearing no-op.
- **CSRF and host.** Local-trusted mutations need `x-orbit-csrf: 1`, must not be
  `Sec-Fetch-Site: cross-site`, and a present `Origin` must match `Host`. Every route
  in local-trusted mode rejects a non-loopback `Host` (`localhost`, `*.localhost`,
  `127.0.0.0/8`, `::1` only), which stops DNS rebinding; the documentation states
  plainly that a proxy which rewrites `Host` to the upstream is not stopped.
- **Reserved identity.** `RESERVED_LOCAL_PRINCIPAL_ID = "local-trusted"` is exported
  by `orbit-runtime` and rejected by device enrollment and token rotation, so the
  synthetic actor cannot collide with a real device.
- **Web.** `WebSession` carries the mode; `useSession` exposes `mode` /
  `isLocalTrusted` and treats a cached local-trusted answer as immediately stale;
  `/login` redirects into the workspace; the sidebar replaces the device identity and
  sign-out with a single "Local trusted mode" line that reserves the same 44px band,
  so switching modes does not shift the shell. A trusted-local session never writes
  the durable `orbit.auth.seen` marker, so switching back to authenticated mode shows
  a clean first-visit form rather than a false expiry notice.
- **Tests and evidence.** Rust unit/inline/process tests cover the mode matrix, bind
  refusals, the host matrix, CSRF/Origin, forwarded-header immunity, privilege
  separation and the stale-cookie clearing. `web/tests/e2e/local-trusted.spec.ts`
  covers scenario A (open workspace, project/milestone/task, List/Board/Backlog,
  Roadmap, refresh, second browser, server restart, policy switch) and scenario C
  (administrator/operator/executor/machine refusals, CSRF/Origin/Host, forwarded
  headers). `web/evidence/m33-auth/` holds 12 captures and the observed results.
- **Documentation.** New `docs/guides/local-web-quickstart.md`; updated
  `docs/web-app.md`, `docs/guides/web-quickstart.md`, `docs/server-api.md`,
  `docs/self-hosted-architecture.md`, `docs/guides/self-hosting.md` and the docs
  index.

## Decisions made

- Local-trusted is configuration, never inferred from a peer address or a forwarded
  header, and it is refused on any non-loopback bind. The listener is the boundary;
  the documentation says every process that can reach it is trusted.
- Privileged surfaces were split by *credential kind*, not by a mode check in each
  handler: the local principal is a product-only principal, and machine/administrator
  operations keep their existing guards. This keeps authorization policy out of
  domain code.
- `403` rather than `401` for privileged routes in local-trusted mode: the browser is
  authenticated as the local principal but not authorized for those operations, and a
  `401` would trigger the client's sign-out path.
- The synthetic actor is the stable id `local-trusted` in receipts and activity, and
  that id is reserved so it can never be forged by enrollment.
- A stale authenticated cookie is revoked when presented in local-trusted mode, so
  switching the server back to authenticated mode always requires a normal sign-in.
- The `Host` allowlist is a rebinding defence, not a proxy filter; the documentation
  and the code comment both say so instead of over-claiming.

## Files or components changed

- `crates/orbit-server/src/auth.rs` (new), `main.rs`, `remote_mcp.rs`,
  `tests/local_trusted_startup.rs` (new).
- `crates/orbit-runtime/src/store.rs`, `lib.rs` (reserved principal id).
- `web/src/api/{session,contracts}.ts`, `web/src/hooks/use-session.ts`,
  `web/src/app/{protected-route,workspace-layout}.tsx`,
  `web/src/features/auth/login-page.tsx`, `web/src/components/layout/sidebar.tsx`.
- `web/tests/unit/**` session, login and unauthorized suites;
  `web/tests/e2e/local-trusted.spec.ts` (new), `web/tests/e2e/harness/server.ts`,
  `web/scripts/capture-screenshots.mjs`.
- `web/evidence/m33-auth/**` (new).
- `docs/guides/local-web-quickstart.md` (new), `docs/web-app.md`,
  `docs/guides/web-quickstart.md`, `docs/server-api.md`,
  `docs/self-hosted-architecture.md`, `docs/guides/self-hosting.md`, `docs/README.md`.
- `.agent/milestones/MILESTONE-0033.md`, `.agent/current/*`, this entry.

## Validation performed

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: clean.
- `cargo test --workspace --offline`: see `.agent/current/acceptance.md` for the final
  counts (all binaries green, including the new process-level startup test).
- `cargo test -p orbit-server --test local_trusted_startup`: 2 passed.
- `web`: `tsc --noEmit`, `vitest run` (466 tests), `check-terminology.mjs`,
  `vite build`: clean.
- `web`: targeted Playwright `local-trusted.spec.ts` (2 passed, 1 capture skip;
  3 passed with `ORBIT_CAPTURE=1`) and `auth.spec.ts` (6 passed); full Chromium suite
  result recorded in the acceptance report.
- `python3 -m unittest discover -s tests/documentation` and
  `python3 -m pytest -q tests/documentation`: 39 passed.
- Independent security review (Oracle): no blocking finding; the four worker routes,
  the reserved id, the host parser edge cases and the stale-cookie case it found were
  fixed and re-tested.

## Problems encountered

- The first pass left automation-run and agent-proposal claim/settle on `device()`,
  which let the local browser principal take a worker lease. The review caught it;
  those four handlers now use `enrolled_device()`.
- The first `Host` parser accepted trailing junk and duplicate headers; both are now
  rejected.
- A full Playwright run exceeded the first command timeout and was re-run in the
  background; the suite is long, not hung.

## Remaining work

- Human visual/usability review of the corrected build, both authentication modes and
  the captures (EC-010, NS-001..NS-006).
- M33 completion after that review; no M34 or TUI implementation work was started.

## New TODOs

- None. The future friendly interactive login (Mode C: passkey or local
  username/password, first-run account setup, device management in settings) is
  documented as roadmap only and was deliberately not implemented.

## Risks or blockers

- Local-trusted trust is machine-level: any local process or local proxy that can
  reach the listener is inside the boundary. Documented, not defended.
- Agent jobs keep the pre-existing per-device ownership rule, so jobs started by a
  device are not listed for the local principal after switching an existing database.
  Documented; the jobs return when signing back in as that device.
- `/mcp` writes still need an enrolled device in local-trusted mode and are documented
  as read-only with the admin key.
