# M32 I3 — acceptance evidence

Recorded from real runs on the reconciled tree after lanes I (agent authority),
J (launch client), K (browser acceptance) and L (executor honesty) landed.
Commands were run by the orchestrator.

## Commands and results

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo test --workspace --offline` | 1,781 passed, 0 failed, 31 binaries, exit 0 |
| `cargo test -p orbit-runtime -p orbit-server -p orbit-storage --offline` | runtime 610, server 59, storage 111, contract 2 — all passed |
| `cargo clippy -p orbit-runtime -p orbit-server -p orbit-storage -p orbit-cli --all-targets --offline -- -D warnings` | clean |
| `cd web && node node_modules/typescript/bin/tsc --noEmit` | exit 0 |
| `cd web && node node_modules/vitest/vitest.mjs run` | 56 files, 342 tests passed |
| `cd web && node scripts/check-terminology.mjs` | 249 files, no user-visible legacy feature name |
| `cd web && node node_modules/vite/bin/vite.js build` | built; only the pre-existing chunk-size warning |
| `cd web && playwright test agents-launch agents` | 11 passed |
| `cd web && playwright test` | 131 passed, 4 skipped (opt-in evidence captures), 0 failed, 6.7 min |

## What the browser specs prove

- **Full cycle (6 specs).** A task in the drawer launches a job with a real
  executor and an objective; the page shows `preparing` with
  `claim_state: "none"`; a deterministic executor — the fixture's own enrolled
  device credential, driven over the real machine routes — claims, confirms,
  observes `working` and submits a handoff; the review records **Approved** and
  **Task not completed**; the objective is in the frozen brief; the task status is
  unchanged in the server record and on screen; and everything survives a *real*
  server restart (`server.restart()` keeps the origin, the database and the
  credentials).
- **Unknown executor.** A direct start with `machine_ref: "nobody"` answers
  400 `unknown agent executor` and the overview gains no row.
- **No usable executor.** With the only enrolled device revoked through the real
  admin API, both launch triggers are disabled with the server-side explanation and
  nothing is posted; zero jobs are stored.
- **Never-claimed executor.** In the fresh-install shape (no prior claims) the
  confirmation names the device, states that the roster cannot show whether a
  runner is active, that the job waits at *Preparing*, and that *Preparing* does
  not mean work has started — and the launch is still allowed.
- **Pinned identity.** A refusal keeps the dialog open and the retry reuses one
  `job_id`: exactly one job row exists.
- **Awaiting an executor.** A queued job nobody has claimed is grouped separately
  and is not reported as attention.
- **Foreign job.** A job created for a genuinely second enrolled device is 403
  `device identity mismatch`, absent from the overview, and "not visible, never not
  found" on screen.

## Defects found and fixed

1. **An unknown `machine_ref` silently created a `preparing` job nothing would
   claim (server).** Start now validates the executor against enrolled, active
   devices *before* the job transaction. This also made the launch UX honest: the
   form can only offer real devices.
2. **Every start/resume refusal was one opaque `400 invalid agent job request`
   (server).** An allowlist of fixed storage messages now reaches the wire, so the
   dialog can quote `task already has active agent`, `stale agent command
   revision`, `predecessor launch is unconfirmed…`, and 40 more, while anything
   carrying SQL or a path still falls back to the generic text.
3. **A `preparing` job with no claim reported `next_action: "none"` (server +
   client).** It now reports `await_executor`, and the overview groups those jobs
   as *Awaiting an executor* instead of implying there is nothing to do.
4. **The picker offered a device that has never claimed a job as if it were
   ready (client).** Flagged in the panel, the picker and the confirmation, with
   the consequence spelled out; launching stays enabled because a roster cannot
   prove that no runner exists.
5. **A false summary suffix (client).** "`(no claim yet)`" was appended even to
   devices that had claimed; removed, and the unit test that pinned the false
   sentence was corrected rather than left green.
6. **A pre-existing broken e2e typecheck.** `saved-views.spec.ts` had an unused
   import that made `tsc -p tests/e2e/tsconfig.json` fail; removed.

## Known limitations, stated rather than hidden

- No machine registry or lease: the roster lists enrolled devices. A device that
  claimed yesterday is not proof a runner is alive now, and `claimed_jobs` counts
  non-terminal jobs rather than "running now"; the confirmation says the roster
  cannot show liveness.
- The "no executor enrolled" branch is unreachable for a signed-in browser in this
  deployment, because the session device is itself an active roster row. It is
  kept as a defensive branch whose copy claims nothing more.
- The objective is optional; requiring it would block launches the authority
  accepts. The form nudges and states that the brief then carries the task alone.
- A job's completion remains job-terminal only; there is no HTTP path that marks a
  task done from a job, and approval writes one review row with
  `task_completed: false`.
