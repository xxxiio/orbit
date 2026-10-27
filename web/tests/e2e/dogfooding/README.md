# Automated interactive dogfooding (M32 I5)

A Playwright-driven session that uses the built product the way a person doing
development management would, against a **persistent** workspace, and records
what happened in
`.slim/deepwork/m32-web-completeness/i5-dogfood/`.

This is **not** an acceptance suite. Acceptance specs assert one property each on
a throwaway database; this session performs a day of work in order (capture,
find, prioritise, plan, progress, unblock, write knowledge, review an agent job,
run a capability, complete, switch projects) and treats a step that cannot be
completed as a finding, not as a test failure. The one assertion it makes is that
the session recorded something.

## Run it

```bash
# one-time build (the harness serves web/dist from a real orbit-server)
cargo build --offline
cd web && node node_modules/vite/bin/vite.js build

# the session — opt-in, because it writes to a long-lived database
cd web && ORBIT_DOGFOOD=1 \
  node node_modules/@playwright/test/cli.js test tests/e2e/dogfooding --project=chromium

# a second run resumes the same workspace instead of resetting it
cd web && ORBIT_DOGFOOD=1 \
  node node_modules/@playwright/test/cli.js test tests/e2e/dogfooding --project=chromium
```

Without `ORBIT_DOGFOOD=1` the session skips itself, so the full suite stays
hermetic (`node node_modules/@playwright/test/cli.js test`).

Useful switches:

| variable | meaning |
| --- | --- |
| `ORBIT_DOGFOOD_DIR` | workspace directory (default `/tmp/opencode/orbit-dogfood`) |
| `ORBIT_DOGFOOD_PORT` | fixed port (default `8781`) |
| `ORBIT_DOGFOOD_ONLY` | comma-separated phase ids, e.g. `09-knowledge,10-agents` |

## The workspace

`workspace.ts` starts one real `orbit-server` on a fixed port with
`<dir>/data/server.db`, stores the enrolled device credential and admin key in
`<dir>/workspace.json`, and **never deletes the database**. A later run starts a
server against the same file, signs in as the same device and continues. A server
already listening on the port is reused only if it proves it owns this workspace
(it answers as the stored device); otherwise the run refuses to touch it.

`seed.ts` populates the workspace **once**:

- **through the public API** — the reusable fixture (`web/tests/fixture/seed.ts`):
  seven projects, 122 tasks with priorities/deadlines/criteria/prerequisites,
  milestones, markdown notes, attachments and agent jobs, because typing 122
  tasks through the UI would only prove the capture form repeats;
- **through the operator API** — install + grant the reference capability and
  the announce of the enrolled executor, because those routes are
  `operator_device`/`executor_device` by design and the SPA has no control for
  them (`tests/e2e/harness/capability.ts` says so).

Everything else in the session is done through the browser.

## What it writes

`captures/` inside the deliverable:

- `<step-id>.png` — one screenshot per recorded step;
- `steps-run-<n>.json` / `.md` — the step log, friction, slow requests, refused
  requests and browser errors;
- `trace-run-<n>.zip` — a Playwright trace (screenshots + DOM snapshots) for the
  whole session.

`session.md` in the same directory is the human-written account, including the
defects table and what still needs a person.
