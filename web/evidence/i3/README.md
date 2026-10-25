# I3 evidence

Browser captures of the M31 I3 build, produced from the deterministic fixture in
`web/tests/fixture/seed.ts` (seeded through the public HTTP API on an isolated
`orbit-server`). No mocked responses and no preview server.

Captured on 2026-10-26 from commit
`88dea90d70397c2f05e266a0f1b18648d9b6f336` (M31 I3: work items, transitions,
upgrades, actions; the follow-up commit fences the confirmed upgrade
fingerprint), using `target/debug/orbit-server` built 2026-10-25 22:18
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

| File | Screen |
| --- | --- |
| `work-items-board-light-1440x900.png`, `work-items-board-light-390x844.png` | Work-item board: columns are the kind's declared states, cards show declared fields |
| `work-items-board-dark-1440x900.png` | The same board in dark mode |
| `work-item-sheet-light-1440x900.png`, `work-item-sheet-light-390x844.png` | Work-item sheet: typed properties, revision, declared moves |
| `work-item-actions-light-1440x900.png` | Actions section: local-authority explanation and capability status |
| `work-item-refusal-light-1440x900.png` | A declared move refused by the server; the item stays in place |
| `work-item-create-light-1440x900.png` | Create dialog with the kind fixed by the view and a typed required field |
| `upgrade-preview-light-1440x900.png` | Same-workflow version preview: field/view diffs and warnings |
| `upgrade-receipt-light-1440x900.png` | Applied receipt with previous → new and the previewed fingerprint |
| `upgrade-blocked-light-1440x900.png` | Blocked preview: every blocker with its subjects; apply disabled |

Viewport: 1440x900 desktop, 390x844 phone. Theme comes from `orbit.theme`; the
board has a dark capture. The upgrade captures use a Development v3/v4 published
by the evidence spec through `POST /v1/authoritative/blueprints`; the blocked
target removes `priority` while live items still store it.
