---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-27"
task_id: TASK-0010
---

# Journal Entry: 2026-10-27 — M31 I6: refinement, measured quality, docs and the TUI redesign spec

## Context

I1–I5 delivered and verified the web client, ending with a real Orbit Development cycle
whose evidence is pinned to a clean commit. I6 had to make the client fit as the internal
reference client: close or honestly record the remaining dogfooding friction, measure
rather than assert quality, document the shipped product, and derive the TUI redesign
specification from that usage.

## Work completed

- Responsive matrix: 1440/1280/1024/900/390 across ten primary routes with no document
  overflow; desktop navigation at ≥900, drawer below; forms and the attachments drawer
  reachable at every width.
- Measured performance with a new deterministic script (real server, 122-task fixture, 10
  repeats, median/p95): cold load ~185 ms, project switch ~331 ms, task list ~109 ms, board
  move ~326 ms (server round trip ~311 ms), detail ~112 ms, mutation ~703 ms (server round
  trip ~655 ms). The measurement showed 93–96% of board/mutation time is the authoritative
  round trip, so no frontend optimisation was justified; the accessibility fixes caused no
  measurable regression.
- Accessibility: axe over every primary route in both themes plus the mobile drawer found
  three contrast failures (fixed in tokens: light faint 3.81→5.10, light muted hover
  4.42→4.97, dark faint 3.68→4.89), chrome outside a landmark (sidebar is now an aside),
  a brand label that omitted visible text, and accessible-name/visible-text mismatches on
  the board move menu and drawer controls; heading order passes everywhere.
- Keyboard: real lost-focus defects fixed — the mobile nav drawer, deep-linked task drawer,
  planning review/apply and the command palette all dropped focus to `<body>`; the drawer
  now restores the pre-open element and the shell restores the palette opener, and the
  reviewed plan and receipt are focusable labelled regions. Shell tab order, skip link,
  focus trap, board moves and aria-live saves are covered by a keyboard spec.
- Documentation: `docs/web-app.md` rewritten for the shipped product (work items,
  transitions/guards, upgrades, actions, planning, knowledge, attachments, agents,
  bundled install, settings, honest gaps), a new `docs/guides/web-quickstart.md`, and
  index/README updates using Workflow terminology. Every documented command was executed.
- TUI redesign specification: `docs/guides/tui-redesign-spec.md` (788 lines) derived from
  the I5 run, classifying twelve patterns 7 must-match / 3 adapt / 1 not-appropriate /
  1 shared-domain, with concrete terminal translations, explicit non-goals, the five things
  the web app still owes the terminal, and A1–A14 acceptance checks.

## Decisions made

- Measure first: no client optimisation without a number, and the numbers then argued
  against optimising the client.
- Fix only real accessibility/focus defects; name what cannot be automated.
- The TUI specification is evidence-led and cites I5; it deliberately excludes surfaces the
  cycle did not exercise (agent review rests on code and specs, not a cycled review).
- Milestone criteria were marked complete only where a browser or measured proof exists.

## Files or components changed

- `web/src/styles/tokens.css`, `components/layout/sidebar.tsx`, `components/ui/drawer.tsx`,
  `app/app-shell.tsx`, `features/planning/project-planning.tsx`,
  `components/forms/inline-title.tsx`, `task-drawer/labels-editor.tsx`,
  `workflows/views/board-view.tsx`.
- `web/scripts/measure.mjs`; `web/tests/e2e/quality-{a11y,keyboard,responsive}.spec.ts`.
- `.agent/evidence/m31-web-perf.md`, `m31-web-quality.md`, before/after JSON.
- `docs/web-app.md`, `docs/guides/web-quickstart.md`, `docs/guides/tui-redesign-spec.md`,
  `docs/README.md`, root `README.md`.
- Dev dependency `@axe-core/playwright@4.14.0` (MPL-2.0, no browser download).

## Validation performed

- `tsc --noEmit` clean (app and e2e); Vitest 42 files / 227 tests; terminology 217 files;
  `vite build` OK; Playwright 87 passed / 4 opt-in captures skipped; quality specs a11y 7/7,
  keyboard 7/7, responsive 5/5.
- `tests/documentation` + `tests/contracts` 82 passed; links and anchors resolve.
- `getdone-validate-project --project-root . --skills-root /home/xliu/dev/getdone` passes
  (27 managed files, 14 benign template-digest warnings).

## Problems encountered

- The image's `npm` is the Windows build and cannot install into the WSL tree; the axe
  dependency was installed from verified registry tarballs and the lockfile updated
  consistently. `npm ci` therefore remains unverified in this environment and is called out
  in the records rather than claimed.
- Lane B's first full Playwright run was invalidated by its own harness cleanup; the clean
  rerun is the recorded result, and the final full-suite run on the frozen tree was repeated
  by the orchestrator after lane A.
- A concurrent accessibility edit briefly made the docs run stale; tsc/build and the full
  suite were re-run after it.

## Remaining work

- Post-milestone: the TUI redesign implementing `tui-redesign-spec.md`; the recorded web
  follow-ups (project rename/status, task timestamps, knowledge folders, note links, saved
  views, agent start, capability execution, the v3 creation default, the `store.rs`
  fallback distinction); and MILESTONE-0027, which remains blocked on external release
  authorization.

## New TODOs

- None outside the recorded post-milestone follow-ups.

## Risks or blockers

- `npm ci` unverified in this image (Windows npm shim); the lockfile is consistent and the
  install path is documented.
- M27 release verification stays externally blocked.
