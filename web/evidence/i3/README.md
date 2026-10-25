# I3 evidence

Browser captures of the M31 I3 build, produced from the deterministic fixture in
`web/tests/fixture/seed.ts` (seeded through the public HTTP API on an isolated
`orbit-server`). No mocked responses and no preview server.

Captured on 2026-10-26 from commit
`05f1e74ad5f1d4faea454f58dd7e62d9870c8f91` (M31 I3: work items, transitions,
upgrades, actions). Later commits widen test timeouts, scope one evidence
assertion, and refresh a refusal string on a panel no capture shows, so most
screenshots still describe this commit's UI — except for two visible deltas added
by `e03caa8` after the capture: the work-item board's keyboard hint (would show
in the two 1440-wide board frames; hidden at phone width) and the count badge on
workspace tabs (would show in every frame's tab strip). Those frames predate
`e03caa8` and were not re-captured. Captured using `target/debug/orbit-server` built 2026-10-25 22:18
local. I3 changed no Rust source, so that binary matches the committed server.
The `web/dist` build used for the capture came from the same commit. Re-run the
commands below after any change to the build; a capture is only comparable to
the commit and binary named here.

Reproduce:

```sh
cd web
node node_modules/vite/bin/vite.js build
ORBIT_I3_EVIDENCE=1 node node_modules/@playwright/test/cli.js test \
  tests/e2e/evidence-i3.spec.ts --project=chromium
```

The spec is **capture-only**: it screenshots the seeded fixture and never creates,
moves or publishes anything. Acceptance flows live in `work-items.spec.ts` and
`upgrades.spec.ts`. Frames that required a write (`work-item-refusal`, the three
`upgrade-*` captures) were produced by an earlier revision of the spec and stay
here as historical evidence for the commit named above.

| File | Screen |
| --- | --- |
| `work-items-board-light-1440x900.png`, `work-items-board-light-390x844.png` | Work-item board: columns are the kind's declared states, cards show declared fields |
| `work-items-board-dark-1440x900.png` | The same board in dark mode |
| `work-item-sheet-light-1440x900.png`, `work-item-sheet-light-390x844.png` | Work-item sheet: typed properties, revision, declared moves |
| `work-item-actions-light-1440x900.png` | Actions section: local-authority explanation and capability status |
| `work-item-refusal-light-1440x900.png` | Historical: a declared move refused by the server; the item stays in place |
| `work-item-create-light-1440x900.png` | Create dialog with the kind fixed by the view and a typed required field |
| `upgrade-preview-light-1440x900.png` | Historical: same-workflow version preview: field/view diffs and warnings |
| `upgrade-receipt-light-1440x900.png` | Historical: applied receipt with previous → new and the previewed fingerprint |
| `upgrade-blocked-light-1440x900.png` | Historical: blocked preview with every blocker and its subjects |

Viewport: 1440x900 desktop, 390x844 phone. Theme comes from `orbit.theme`; the
board has a dark capture. The historical upgrade captures used a Development
v3/v4 published by that revision of the evidence spec through
`POST /v1/authoritative/blueprints`; the blocked target removed `priority` while
live items still stored it.
