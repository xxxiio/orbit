# M31 I6 — measured web performance

- Measured app: `web/dist` built from `dfcfb8a` plus the lane-A edits recorded in
  `m31-web-quality.md` (the only app changes in this lane are accessibility,
  focus and label fixes; no rendering path was rewritten).
- Server: `target/debug/orbit-server` (149,936,584 bytes), real SQLite database in
  a throwaway directory, SPA served by that same server (`ORBIT_WEB_DIST`).
- Fixture: the deterministic 122-task / 7-project workspace, seeded through the
  public API by `web/tests/fixture/seed.ts` (bundled with esbuild for this Node
  script).
- Host: Linux x86_64, Node v18.19.1, Chromium 1194 (Playwright 1.56.1),
  headless, viewport 1440×900.
- Repeats: **10 per scenario** (contract minimum is 5).
- Raw evidence: `m31-web-perf-before.json`, `m31-web-perf-after.json`
  (same directory). Every number below is generated from those files.
- Reproduce:

```sh
cd web
node scripts/measure.mjs --repeat 10 --label after
```

## Method (so the numbers are not hand-waving)

- **Cold load** — a fresh browser context per repeat (cold HTTP cache, cold JS
  heap, fresh cookie jar); the server stays warm. Time runs from `page.goto('/')`
  until the Home `h1` exists **and** no `main [aria-busy="true"]` surface remains,
  i.e. the skeleton is gone and data has rendered. FCP, DOMContentLoaded and
  `load` are read from the Paint/Navigation Timing entries of that same
  navigation.
- **Warm interactions** — one page and one signed-in session. Before each
  measured action the route is re-opened and settled, so the measured cost is the
  action, not the previous sample. Times are Node wall clock around the
  Playwright action, resolved by an in-page condition; this includes a few
  milliseconds of CDP transport. An in-page `performance.now()` clock was
  rejected after measurement showed that Playwright's fake clock (needed for
  deterministic dates) replaces the Performance Timeline (`timeOrigin = -1`,
  empty navigation/paint entries), which would have turned timing into a guess.
- **Server round trip** — the `apiMs` columns are the matching
  `POST /v1/authoritative/task-commands` request→response wall time, measured by
  a `response` listener. `moveMs`/`mutationMs` are the full UI path.
- **Task list** — no route renders all 122 tasks in one list. The Learning
  project's declared `Tasks` view renders 20 rows after the client has loaded and
  filtered the 122-task snapshot; it is the largest declared task list in the
  fixture. The row count is recorded in the JSON (`fixture.taskListRows`).
- **Statistics** — median is the middle value of the sorted samples. p95 is the
  nearest-rank percentile, `sorted[ceil(0.95 × n) − 1]`; with n = 10 it is the
  largest sample. Small-sample percentiles are descriptive for this host and
  fixture, not a cross-machine benchmark.
- The measuring script refuses a dirty tree by default (`ORBIT_MEASURE_ALLOW_DIRTY=1`
  was used for these records because the evidence files themselves are
  uncommitted). The `build.dist.sha256` in each JSON identifies the exact app
  build that produced the numbers.

## Results — before / after (milliseconds)

| Scenario | Metric | Before median | Before p95 | After median | After p95 |
|---|---|---:|---:|---:|---:|
| Cold load Home | first meaningful content | 181.5 | 187.0 | 185.0 | 196.0 |
| Cold load Home | FCP | 64.0 | 72.0 | 64.0 | 72.0 |
| Cold load Home | DOMContentLoaded | 38.2 | 41.1 | 38.3 | 41.9 |
| Cold load Home | load event | 38.3 | 41.2 | 38.4 | 42.0 |
| Project switch (Orbit → Learning) | menu → new project content | 330.5 | 359.0 | 331.0 | 351.0 |
| Task list (Learning `Tasks`, 20 rows) | navigation → rows + skeleton gone | 120.0 | 152.0 | 108.5 | 152.0 |
| Board move (`m` → Planned → Enter) | interaction total | 336.0 | 355.0 | 325.5 | 351.0 |
| Board move | server command round trip | 320.5 | 329.0 | 311.0 | 336.0 |
| Task detail open | card click → drawer + description | 108.5 | 144.0 | 112.0 | 132.0 |
| Mutation (priority change) | server response → footer announcement | 705.5 | 755.0 | 702.5 | 740.0 |
| Mutation | server command round trip | 659.5 | 718.0 | 654.5 | 695.0 |

Build identity: before `dist.sha256 = 5d1bb694a6fa…`, after `dist.sha256 = 956eb23be5fb…`.

## What the numbers say

- **No client-side bottleneck crossed a useful threshold.** The slow interactions
  are server-bound, not render-bound:
  - board move: 95.5 % of the total is the authoritative command round trip
    (311.0 of 325.5 ms median after); the client adds ~15 ms;
  - mutation: 93.2 % is the command round trip (654.5 of 702.5 ms); the client
    adds ~48 ms including the live-region update.
  - An optimisation in the client cannot remove more than ~5 % of the board
    interaction or ~7 % of the mutation without changing the server. The command
    cost is the debug `orbit-server` + SQLite commit on this host; changing Rust
    is outside lane A and was not done.
- Cold load is 185 ms to meaningful content with FCP at 64 ms on loopback; the
  122-task snapshot does not make the first Home render slow.
- The task list renders 20 rows in 108.5 ms after loading/filtering the 122-task
  snapshot.
- Project switch is 331 ms and includes the project binding/workflow reads plus
  the snapshot; it is the slowest purely client-driven navigation measured and
  is still within a third of a second.
- **No performance fix was justified by these measurements.** The before/after
  columns are the same build path plus accessibility/focus fixes; every delta is
  within run-to-run noise (largest 11.5 ms, task-list median, while its p95 is
  unchanged at 152 ms).

## Caveats

- Debug server binary and loopback transport; absolute numbers will differ on a
  release build or a real network.
- The 122-task fixture is fully loaded on every route that uses the workspace
  snapshot; the task list itself windows to 20 rows.
- No HTTP compression/caching was added to the harness (the server serves the
  build directly), so cold-load numbers include uncompressed local transfers.
- Mutation samples change priority between four values; a no-op selection would
  not issue a command and was excluded by construction.
