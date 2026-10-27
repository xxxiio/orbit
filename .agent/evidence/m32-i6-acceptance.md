# M32 I6 — acceptance, clean installation and TUI handoff

## Clean installation (honest result)

A directory containing only `package.json` and `package-lock.json` was created and
`npm ci` was attempted with a clean cache.

- **Plain `npm ci` fails on this host**: `PATH` resolves the Windows npm shim
  (`/mnt/c/Program Files/nodejs/npm`, node v24.13.0) and its postinstall dies with
  `CMD.EXE … UNC paths are not supported` → `Cannot find module
  'C:\Windows\install.js'`. There is no native Linux npm here (no
  `/usr/lib/node_modules/npm`, no `npm-cli.js`, no corepack).
- Two documented deviations work: `npm ci --ignore-scripts` (290 packages, but
  win32-x64 optional dependencies), and `npm ci --ignore-scripts --os=linux
  --cpu=x64` (291 packages, `@esbuild/linux-x64`, `@rollup/rollup-linux-x64-*`).
  `--ignore-scripts` skips esbuild's own chmod, so one
  `chmod +x node_modules/@esbuild/linux-x64/bin/esbuild` is required before
  `vite build`.
- The resulting tree typechecks and builds, and its `dist` is **byte-identical on
  all 16 files** to the build from the checkout. That is the reproducibility claim
  this environment can support.

## Documented loop on the current tree

`cargo build -p orbit-server --offline` → exit 0; `tsc --noEmit` → exit 0;
`vite build` → exit 0; server on a scratch database under `/tmp/opencode` →
`/health` `{"status":"ok"}`, `/` 200, an asset 200, `/v1/nope` 404, enroll 201 →
sign-in 201 → session 200; `search.spec.ts` → 5 passed. One mistake is recorded:
the first probe hit `AddrInUse` from another lane's server, so those results were
discarded and the probe repeated on a verified-free port.

Not verified here, and not claimed: plain `npm ci` on a Linux-native npm, an
offline install from an empty cache, a clean-machine Playwright run (needs the
browser libraries bundle). Those need a human on a supported host.

Details: `.slim/deepwork/m32-web-completeness/i6-install.md`.

## Final web acceptance

The full real-browser suite runs against an isolated `orbit-server` per worker and
covers every operation this milestone added: authentication and expiry; project
identity and status with stale revisions; unified search across three kinds;
saved views across a browser restart; task Created/Updated; knowledge folders,
note links, typed references and project/task knowledge; agent launch, claim
states, handoff, review, restart and foreign-job invisibility; capability
install/grant/publish/offer/request/claim/execute/receipt/effect/reconcile/restart.
Conflicts, authorization refusals and empty/error states are asserted in each
area; responsive, keyboard and accessibility suites cover every primary route.

Numbers for this tree are recorded in the iteration table below.

## TUI redesign specification

`docs/guides/tui-redesign-spec.md` was rewritten from the M31 surface to proven
M31/M32 usage: 18 patterns classified as 6 **match the web**, 3 **adapt to the
terminal**, 1 **inappropriate for the terminal**, and 8 **need a shared Rust API**,
with two classification changes explained (search moved to "needs shared API"
because the TUI's `/` only filters loaded rows; "agent review" became "agent jobs"
with a launch/resume facet).

Each pattern carries its evidence; the missing APIs are named (`search`,
saved-view read/commands, `update_project_identity`, `TaskDto` timestamps,
`markdown_references`, `project_knowledge`, agent start/review/resume/roster,
connected capability offers/status/request/state/history/reconcile — never
`execute`). Acceptance is A1–A23 with a "shared service today" column, and the one
M31 gap M32 did not close (transition-edge read) is flagged. The page states at the
top that it is a specification only and that M32 implements no TUI change.

Three stale documentation claims were corrected during the update: a duplicated
heading in `docs/web-app.md`, a version line that still said M32 I3, and a
troubleshooting row claiming the API exposes no project-status transition.

## Verification of the documentation

`python3 -m pytest -q tests/documentation` → 39 passed;
`node scripts/check-terminology.mjs` → 256 files, no user-visible legacy feature
name.
