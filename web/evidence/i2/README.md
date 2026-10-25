# I2 evidence

Browser captures of the M31 I2 build, produced from the deterministic fixture in
`web/tests/fixture/seed.ts` (seeded through the public HTTP API on an isolated
`orbit-server`). No mocked responses and no preview server.

Captured on 2026-10-25 from the working tree based on commit
`23a8fdff8c3832030ef7c410f1bc0884ffb12520` (includes the uncommitted Gate 2
registry/cache fixes), using `target/debug/orbit-server` built 2026-10-25
22:18 local (newer than every file under `crates/orbit-server/src`) and the
`web/dist` build from that tree. Re-run the commands below after any change to
the build; a capture is only comparable to the commit and binary named here.

Reproduce:

```sh
cd web
node node_modules/vite/bin/vite.js build
ORBIT_I2_EVIDENCE=1 node node_modules/@playwright/test/cli.js test \
  tests/e2e/evidence-i2.spec.ts --project=chromium
```

| File | Screen |
| --- | --- |
| `board-light-1440x900.png`, `board-light-390x844.png`, `board-dark-1440x900.png` | Board |
| `planning-light-1440x900.png`, `planning-light-390x844.png`, `planning-dark-1440x900.png` | Day plan + schedule window |
| `planning-review-light-1440x900.png`, `planning-review-dark-1440x900.png` | Reviewed plan and approval gate |
| `planning-receipt-light-1440x900.png` | Applied-plan receipt |
| `milestones-light-1440x900.png`, `milestones-light-390x844.png` | Milestones |
| `settings-light-1440x900.png`, `settings-light-390x844.png` | Project settings |
| `task-list-light-1440x900.png`, `task-list-light-390x844.png` | Task list with URL state |

Viewport: 1440x900 desktop, 390x844 phone. Theme comes from `orbit.theme`; dark
captures exist for the Board and Planning.
