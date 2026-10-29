# M33 visual evidence

Captured from the built SPA (`web/dist`) served by a real `target/debug/orbit-server`
against the persistent dogfooding database at `/tmp/opencode/orbit-dogfood/data/server.db`.
The workspace was populated through the public API and the browser; the two demo
epics below were created through the same `epic-commands` family the UI uses.

| Item | Value |
| --- | --- |
| Baseline | `06aad53` plus the M33 working tree |
| Build | `node node_modules/vite/bin/vite.js build` |
| Browser | Playwright Chromium 1194, `LD_LIBRARY_PATH=/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu` |
| Server | `target/debug/orbit-server` on `127.0.0.1:8781` |
| Theme | `orbit.theme` set before first paint; light and dark |
| Data | Development project: 24 tasks, epics “Web UX” (3 tasks) and “Agent tools” (3 tasks), milestones “Reliability” and “Internal test” with target dates |

## Screens

| File | Screen |
| --- | --- |
| `login-light-1440x900.png`, `login-dark-390x844.png` | Clean sign-in page, no expiry warning |
| `list-{light,dark}-{1440x900,1280x800}.png` | List with Epic column, filters, grouping control |
| `list-{light,dark}-390x844.png` | Phone stacked rows, epic shown per row |
| `board-{light,dark}-{1440x900,1280x800}.png` | Workflow-state board with epic filter |
| `backlog-{light,dark}-{1440x900,1280x800}.png` | Backlog grouped by epic, move-to-Ready path |
| `roadmap-{light,dark}-{1440x900,1280x800}.png` | Split roadmap, epic bars, task windows, milestone diamonds, today line |
| `roadmap-{light,dark}-390x844.png` | Phone roadmap with horizontal axis scrolling |
| `epic-detail-light-1440x900.png` | Scrollable epic editor, progress, children, activity |
| `task-drawer-light-1440x900.png` | Shared inspector with Epic and milestone fields |

## Review notes (from the screenshots, not a human approval)

- Inter is used throughout; no serif display type remains. Small metadata is 11–12px,
  body is 14px and page titles are 24px.
- Light and dark palettes keep the same hierarchy; muted text on the sunken table
  header was darkened to meet 4.5:1 after the axe pass.
- At 390px the project header wraps, the task list becomes cards and the nav scrolls.
  There is no page-level horizontal overflow (quality-responsive passes).
- The roadmap keeps titles sticky while the axis scrolls; dates that are not authored
  stay in **Unscheduled** rather than becoming fake bars.
- The epic dialog is capped to the viewport and scrolls, so its title and actions stay
  reachable.

A human still needs to review the running app and these captures; this directory is
evidence, not approval.
