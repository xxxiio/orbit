---
template: change-impact
template_version: 1.0.0
template_digest: "8618a859250c7346e16cb5e20990e6c6d496e6b238ab8f44824028eba527e134"
project_owned: true
record_contract: change-impact
record_schema_version: 1
status: current
task_id: TASK-0015
---

# Change Impact Declaration

## Task

TASK-0015 — M33 authentication UX: opt-in local trusted browser mode, with the
default device authentication retained.

## Impact classification

| Impact | Value | Reason | Activated gate |
|---|---|---|---|
| public_api | yes | `GET /v1/web/session` now returns `mode` and `display_name` and a nullable `expires_at_ms`; `POST /v1/web/session` returns `409` in local-trusted mode and `DELETE` is a no-op there. No other route shape changed. | Compatibility gate: server HTTP tests, web unit session tests, `auth.spec.ts`, local-trusted browser suite. |
| persisted_data | no | No schema change. Sessions, devices and the authoritative database keep their existing tables; local-trusted mode reuses the same database and writes no new credential row. | none |
| configuration | yes | New `ORBIT_AUTH_MODE` (`authenticated` default, `local-trusted` opt-in), validated at startup; local-trusted requires a literal loopback `ORBIT_SERVER_BIND` and refuses to start otherwise. `ORBIT_SERVER_ADMIN_KEY` stays mandatory. | Configuration gate: `crates/orbit-server/tests/local_trusted_startup.rs`, `auth.rs` bind matrix, quickstart documentation tests. |
| dependencies | no | No new crate or npm dependency; the process test uses `std` only and the browser suite uses the existing Playwright harness. | none |
| security_boundary | yes | New authorization policy and principal, mode-aware CSRF, a loopback `Host` allowlist, and an explicit split between product routes and administrator/operator/executor/machine/MCP routes. Local trust never grants those privileges and never derives from the peer address or forwarded headers. | Security gate: Rust guard tests, browser scenario C, Oracle review, `docs/web-app.md` and `docs/guides/local-web-quickstart.md`. |
| concurrency | no | No new shared mutable state beyond the existing `AppState`; the mode is copied at startup and read-only afterwards. | none |
| performance_sensitive | no | One additional header comparison per request; no measured or plausible cost at loopback scale. | none |
| user_interface | yes | Local-trusted enters the workspace directly, `/login` redirects in, the sidebar replaces the device identity and sign-out with a plain mode note. Authenticated UI is unchanged. | UI gate: unit tests, `local-trusted.spec.ts`, light/dark desktop/phone captures in `web/evidence/m33-auth/`. |
| deployment | yes | A new documented startup mode with a hard loopback requirement; existing deployments are unaffected because the default is unchanged. | Deployment gate: startup refusal test, local quickstart, self-hosting and API docs. |

## Assumptions and unknowns

- Local-trusted trust is machine-level: every process and browser that can reach
  the loopback listener is trusted. That limitation is documented and not
  defended against; it is not a shared-host or remote-deployment mode.
- Authenticated mode keeps its existing semantics byte for byte: device sign-in,
  HttpOnly SameSite=Strict cookie, 12-hour sliding / 7-day absolute session,
  revocation, CSRF, sign-out.
- Request source addresses and `X-Forwarded-*`/`X-Real-IP` headers are never
  authorization inputs; the `Host` allowlist is the only request-header trust
  check, and only in local-trusted mode.
- Human visual/usability review (EC-010) remains owed; automated captures are not
  approval.

## Required outputs

- `crates/orbit-server/src/auth.rs` (mode, principal, bind and host validation),
  guard wiring in `main.rs`, and the session/CSRF contract changes.
- Server tests at unit, router and real-process level, including the refusal to
  start on a non-loopback bind.
- Web session handling, login redirect and sidebar mode note with unit and browser
  coverage.
- `web/evidence/m33-auth/` captures plus scenario A/B/C results.
- Documentation: `docs/guides/local-web-quickstart.md`, `docs/web-app.md`,
  `docs/guides/web-quickstart.md`, `docs/server-api.md`,
  `docs/self-hosted-architecture.md`, `docs/guides/self-hosting.md`.
- Milestone criterion EC-011, the acceptance report, the evidence manifest, the
  next step and a journal entry.

## Test tier

- Tier 4: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --locked -- -D warnings`, `cargo test --workspace`, the TypeScript check, the
  Vitest suite, the terminology check, the production build, the full Chromium
  suite including the new local-trusted spec, the documentation suites and
  `getdone-validate-project`.
