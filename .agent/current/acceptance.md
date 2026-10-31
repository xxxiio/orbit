---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: incomplete
task_id: TASK-0015
---

# Current Acceptance Report

## Task

TASK-0015 — M33 corrective iteration: Milestone unification, full task creation,
unified Tasks workspace, compact Board and Milestone Roadmap, plus the opt-in
local-trusted browser authentication mode.

M33 remains **active**. The corrected implementation, the authentication work, the
automated gates, targeted browser acceptance and representative captures are in place.
The human visual/usability review (EC-010, NS-001..NS-006) is still owed, so this
report does not claim completion or approval.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Typography and visual system | pass | Inter self-hosted, neutral tokens and restrained scale; retained from the first pass. |
| G-0002 | Auth first-visit/expiry/revocation | pass | `orbit.auth.seen` marker plus the retained unit and Playwright auth coverage; `auth.spec.ts` 6/6. |
| G-0003 | Milestone authority consolidation | pass | Rich Milestone authority, Task/Work Item membership, progress, activity, guards and one-time conversion. |
| G-0004 | Rich task creation | pass | One accepted command with every documented field, criteria and prerequisites; `task-create-full.spec.ts` and the local-trusted spec create a fully configured task. |
| G-0005 | Unified Tasks workspace | pass | One Tasks tab, `?view=list\|board\|backlog`, shared URL filters and legacy redirects; `tasks-workspace.spec.ts`. |
| G-0006 | Compact Board | pass | Compact default with `orbit.board.density`, priority/deadline/blocked cues, on-demand inspector, authoritative moves. |
| G-0007 | Milestone Roadmap | pass | One Milestone hierarchy with duration/checkpoint/unscheduled shapes, child windows, progress/blocked/dependency cues. |
| G-0008 | Automated web gates | pass | 75 unit files / 466 tests pass; `tsc --noEmit`, terminology (273 files) and the production build are clean. |
| G-0009 | Real-browser acceptance (EC-009) | pass | Full Chromium suite: 152 passed, 6 env-gated skips, 0 failed (25.7 min). 28 captures in `web/evidence/m33-corrective/`. |
| G-0010 | Human review (EC-010) | not-run | No human has reviewed the corrected build. This report does not claim approval. |
| G-0011 | Local-trusted policy and startup | pass | `auth.rs` mode/principal/bind/Host validation, `ORBIT_AUTH_MODE` fail-closed parsing, validation before storage open and a post-bind loopback re-check; `crates/orbit-server/tests/local_trusted_startup.rs` 2/2 (non-loopback binds exit 1 with the loopback message and create no database; loopback serves `/health` and the product API). Manual `0.0.0.0:8971` start exited 1 with no database file. |
| G-0012 | Privilege separation and CSRF | pass | Local principal refused by admin (403 `administrator credential required`), operator (403), executor (403), machine transports (403 `enrolled device credential required`), automation-run/proposal claim+settle (403) and `/mcp` mutations; capability authorization gates still apply; `local-trusted` is a reserved device id. Mutations need `x-orbit-csrf: 1`, refuse `Sec-Fetch-Site: cross-site`, a foreign `Origin` and a non-loopback `Host`; forwarded headers never grant access. |
| G-0013 | Browser acceptance, both modes | pass | `local-trusted.spec.ts` 2 passed + 1 capture skip (3 passed with `ORBIT_CAPTURE=1`): workspace enters with no login/cookie, project + milestone + fully configured task, List/Board/Backlog, Roadmap, reload, second browser, server restart, policy switch to authenticated mode, and the full boundary checks. `auth.spec.ts` 6/6 for the default mode. 12 captures in `web/evidence/m33-auth/`. |
| G-0014 | Authentication documentation | pass | New `docs/guides/local-web-quickstart.md`; `docs/web-app.md`, `docs/guides/web-quickstart.md`, `docs/server-api.md`, `docs/self-hosted-architecture.md`, `docs/guides/self-hosting.md`, `docs/README.md` updated; documentation suites 39/39. |
| EC-001 | Retained typography criterion | pass | Inter and the restrained scale remain. |
| EC-002 | Retained auth criterion | pass | First visit is quiet; expiry/revocation is explained; sign-out is quiet. |
| EC-003 | One grouping entity | pass | Epic is gone from `web/src`, the unit suites and the authority; only a decode-only sync tag remains. |
| EC-004 | Rich Milestone aggregate | pass | Authority, UI, conversion and restart proof; 142 Tasks preserved on the real dogfood copy. |
| EC-005 | Full task create | pass | One accepted command; dialog, unit and browser coverage. |
| EC-006 | One Tasks entry | pass | Unified workspace, URL views and legacy redirects. |
| EC-007 | Compact Board | pass | Compact density, card cues, inspector on demand and authoritative moves. |
| EC-008 | Milestone Roadmap | pass | Milestone hierarchy and duration/checkpoint/unscheduled shapes. |
| EC-009 | Real-browser acceptance | pass | Full Chromium suite 152 passed, 6 env-gated skips, 0 failed; captures at both sizes and themes. |
| EC-010 | Human review | not-run | Human approval remains open. |
| EC-011 | Local-trusted authentication UX | pass | Explicit opt-in, loopback-only startup refusal, no Device ID/token/cookie for the browser, product surface usable, privileged surfaces separately authenticated, no stale trusted UI after a policy switch; `local_trusted_startup.rs`, `local-trusted.spec.ts`, Rust guard tests and `web/evidence/m33-auth/`. Human visual approval is covered by EC-010. |

## Commands run

```text
cargo test --workspace --offline                               # 32 binaries, 1883 passed, 0 failed
cargo clippy --workspace --all-targets --offline -- -D warnings # clean
cargo fmt --all -- --check                                     # clean
cargo test -p orbit-server --test local_trusted_startup --offline # 2 passed
cd web && node node_modules/typescript/bin/tsc --noEmit        # exit 0
cd web && node node_modules/vitest/vitest.mjs run              # 75 files, 466 tests passed
cd web && node scripts/check-terminology.mjs                   # clean, 273 files
cd web && node node_modules/vite/bin/vite.js build             # clean
cd web && ... playwright test --project=chromium               # 152 passed, 6 skipped, 0 failed
cd web && ... playwright test tests/e2e/local-trusted.spec.ts  # 2 passed, 1 capture skip
cd web && ORBIT_CAPTURE=1 ... local-trusted.spec.ts            # 3 passed (12 PNGs)
cd web && ... playwright test tests/e2e/auth.spec.ts           # 6 passed
python3 -m unittest discover -s tests/documentation            # 39 passed
python3 -m pytest -q tests/documentation                       # 39 passed
```

Manual process checks against `target/debug/orbit-server`:

```text
ORBIT_AUTH_MODE=local-trusted ORBIT_SERVER_BIND=0.0.0.0:8971 ...  # exit 1, loopback message, no DB created
ORBIT_AUTH_MODE=local-trusted ORBIT_SERVER_BIND=127.0.0.1:8972 ... # banner, /health 200,
  GET /v1/web/session -> 200 {"mode":"local-trusted",...}, no-store, no Set-Cookie,
  GET /v1/authoritative/projects -> 200, POST /v1/devices -> 403,
  POST project-commands without x-orbit-csrf -> 403, Host: public.example.com -> 403
```

## Checks not run

- Human visual/usability review of the corrected build and captures (EC-010, NS-001..NS-006).
- Live agent execution and M15/M27 release qualification (unchanged).
- TLS/reverse-proxy deployment of authenticated mode (unchanged release gate).

## Waivers

- None.

## Residual risk

- Local-trusted trust is machine-level: any process or local proxy that can reach the
  loopback listener is inside the boundary. Documented; not defended. A proxy that
  rewrites `Host` to the upstream also passes the `Host` check, which is why the mode
  must not sit behind one.
- Agent jobs keep the pre-existing per-device ownership rule; jobs started by a device
  are not listed for the local principal after switching an existing database. They
  return when signing back in as that device.
- `/mcp` is read-only in local-trusted mode with the admin key; writes need an enrolled
  device.
- Screenshots cannot judge comfort, trust or visual taste; the human gate stays open.
