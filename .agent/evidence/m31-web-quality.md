# M31 I6 lane A — measured quality sweep

Scope: the browser client at `web/`. No visual redesign, no Rust changes, no
change to product behaviour beyond the fixes below. Lane A touched no `docs/**`
and no `.agent/**` record except this file and the two perf JSONs/Markdown.

Measured app: `dfcfb8a` plus the edits listed here. Lane B's `docs/**` commit
(`27d7edc`) does not touch `web/src`, so it does not change the measured build.

## Commands

```sh
cd web
node node_modules/typescript/bin/tsc --noEmit
node node_modules/typescript/bin/tsc -p tests/e2e/tsconfig.json --noEmit
node node_modules/vitest/vitest.mjs run
node scripts/check-terminology.mjs
node node_modules/vite/bin/vite.js build
node node_modules/@playwright/test/cli.js test
node scripts/measure.mjs --repeat 10 --label after
```

New scripts/specs:

| File | Purpose |
|---|---|
| `web/scripts/measure.mjs` | Starts the real server + 122-task fixture, measures six scenarios × 10 repeats, prints and writes median/p95 JSON. |
| `web/tests/e2e/quality-a11y.spec.ts` | axe-core audit of every primary route in light and dark, mobile drawer semantics, heading outline, reduced motion. |
| `web/tests/e2e/quality-keyboard.spec.ts` | Keyboard-only shell, skip link, drawer trap/Escape/focus return, command palette, board move, planning review/approve, aria-live save. |
| `web/tests/e2e/quality-responsive.spec.ts` | 1440/1280/1024/900/390 × every primary route, plus task form and task drawer with attachments. |

### Dependency added (and why)

`@axe-core/playwright@4.14.0` (+ `axe-core@4.14.0`, MPL-2.0) as a devDependency.
It reuses the Chromium already installed for Playwright — no browser download,
no second browser. The environment's `npm` is the Windows binary and cannot
install into the WSL tree, so the registry tarballs were verified against their
published sha512 and unpacked into `web/node_modules`; `package.json` and
`package-lock.json` carry the same version/integrity. `require('@axe-core/playwright')`
was then exercised by the passing audit. No other dependency was added.

## Performance

Full method and numbers: `m31-web-perf.md`; raw data in
`m31-web-perf-before.json` / `m31-web-perf-after.json`.

Headline (median / p95, ms, n = 10):

| Scenario | Before | After |
|---|---:|---:|
| Cold load → first meaningful content | 181.5 / 187 | 185.0 / 196 |
| First contentful paint | 64 / 72 | 64 / 72 |
| Project switch | 330.5 / 359 | 331.0 / 351 |
| Task list (20 rows from 122-task fixture) | 120.0 / 152 | 108.5 / 152 |
| Board move (total) | 336.0 / 355 | 325.5 / 351 |
| Board move (server round trip) | 320.5 / 329 | 311.0 / 336 |
| Task detail open | 108.5 / 144 | 112.0 / 132 |
| Mutation (total) | 705.5 / 755 | 702.5 / 740 |
| Mutation (server round trip) | 659.5 / 718 | 654.5 / 695 |

- **Fix decision: none.** The measured slow paths are authoritative command
  round trips (95.5 % of a board move, 93.2 % of a mutation). Client overhead is
  ~15 ms and ~48 ms respectively, so a frontend optimisation cannot materially
  move these numbers; changing the Rust server is outside this lane.
- **No regression from the accessibility/focus fixes:** every before/after delta
  is within run-to-run noise (largest 11.5 ms, and the affected p95 is unchanged).
- Task list caveat: no route renders all 122 tasks in one list. The measured list
  is the largest declared `Tasks` view (Learning, 20 rows) after the client has
  loaded and filtered the 122-task snapshot; the row count is in the JSON.

## Accessibility

`axe-core` with tags `wcag2a, wcag2aa, wcag21a, wcag21aa, best-practice` over:
Home, Inbox, Projects, project overview, board, milestones, planning, knowledge,
knowledge note, agents, and the task drawer — in **both themes**, plus the mobile
navigation drawer. The audit waits for fonts and every finite animation: axe
reads computed colours at the instant it runs, and measuring during the shell's
fade-in produced false contrast failures (e.g. `#eae9e7` nav text that is really
`#494d55` once settled). That wait is part of the audit, not a workaround for a
product defect.

Initial result: 4 failing tests (light/dark primary routes, light/dark mobile
drawer). Rule groups found and what was real:

| Rule | Finding | Fix |
|---|---|---|
| `color-contrast` | Small `text-ink-faint` was 3.81:1 on white (3.41:1 on the sunken surface), below 4.5:1; light `text-ink-muted` fell to 4.42:1 on the hover surface; dark `ink-faint` hit 3.68:1 on raised surfaces. | `src/styles/tokens.css`: light `--ink-faint` 52 % → 44 % (5.10:1 on white, 4.56:1 on sunken) and `--ink-muted` 43 % → 40 % (4.97:1 on hover); dark `--ink-faint` 48 % → 56 % (4.89:1 on raised). No layout or font-size change. |
| `region` | Sidebar brand/search/session chrome sat outside any landmark on every route. | `src/components/layout/sidebar.tsx`: the sidebar root is now `<aside aria-label="Workspace">`. |
| `label-content-name-mismatch` | The brand link had `aria-label="Orbit — go to Home"` while its visible text is “Orbit / Self-hosted”; the board's “Move to…” button's label omitted its visible text; the drawer title/labels controls had the same pattern. | Brand link: removed the overriding `aria-label` (name now comes from visible content). Board: `aria-label="Move to… <task title>"`. `inline-title.tsx`: `aria-label="Edit <label>: <value>"`. `labels-editor.tsx`: sr-only “Edit task labels” plus content-derived name. |
| heading outline | Focused DOM check found none: one `h1` per route, no skipped levels. | Test added to keep it true. |
| reduced motion | `prefers-reduced-motion` already caps animation/transition duration at 0.01 ms and disables smooth scroll. | Test added asserting no animation runs longer than 0.02 ms (the reduced-motion rule uses 0.01 ms). |

Final result: **7/7 a11y tests pass** in both themes. The existing board spec was
updated to the corrected accessible name (`Move to… <title>`).

What cannot be automated and is therefore covered elsewhere or left as a named
limitation:

- **Focus visibility and focus order** are not axe rules; proven by
  `quality-keyboard.spec.ts` (visible outline/ring on every shell tab stop).
- **The meaning/quality of a live announcement** (whether “Priority saved.” is
  the right sentence) is a human judgement; the test proves an `aria-live`
  region receives it.
- **Reading order vs visual order**, image/gradient contrast, and announcement
  timing are not fully decidable by axe; they were not mechanically changed.
- Drag-and-drop is pointer-only by nature, but the board exposes the same moves
  by keyboard (`m` + arrows), which is tested.

## Keyboard-only pass

`quality-keyboard.spec.ts` — 7/7 pass. Findings and fixes:

| Finding | Evidence | Fix |
|---|---|---|
| Mobile navigation drawer dropped focus to `<body>` on Escape. | After close: `document.activeElement === body`; the trigger was not focused. | `src/components/ui/drawer.tsx` captures the pre-open element in `onOpenAutoFocus` and restores it in `onCloseAutoFocus` (fallback: `#main`). Test: Shift+Tab stays trapped, Escape returns to “Open navigation”. |
| Task drawer opened by deep link / card dropped focus to `<body>` on Escape. | Same: focus was `body` after close. | Same Drawer fix. Opened from a card, focus returns to the card; deep link falls back to `#main`. |
| Planning **Review** dropped focus to `<body>`: the Review button is replaced by its loading state while the server answers. | Focus probe after the reviewed panel appeared returned `null`. | `project-planning.tsx`: the reviewed plan is a labelled `role="region"` with `tabIndex={-1}` and receives focus when it appears. |
| Planning **Apply** dropped focus to `<body>` when the approval controls hid. | Same probe after the receipt appeared. | The receipt is a labelled region with `tabIndex={-1}` and receives focus when it appears. |
| Command palette closed with Escape and left focus on `<body>`; a global shortcut is not a Radix Trigger. | Focus probe after close: `body`. | `src/app/app-shell.tsx` records the element focused when the palette opens and restores it on close (falls back to `#main`). Test returns to the sidebar search button. |
| Board move menu Escape | Focus returns to the row's “Move to…” trigger, not the title. | No fix needed: focus stays inside the card row; after an accepted move focus follows the card. Test asserts both. |

Also verified: first nine tab stops are Skip link → brand → search → Home →
Inbox → Projects → Knowledge → Agents with a visible focus indicator; the skip
link moves focus to `#main`; the board `m` menu is reachable with ArrowDown and
Enter without a pointer; the drawer's save announcement appears in
`footer [aria-live="polite"]` after a keyboard-only description edit.

## Responsive matrix

`quality-responsive.spec.ts` — 5/5 pass. Every row is 10 routes × the width,
plus a New-task dialog and a task drawer (with its Attachments section).

| Width | Document horizontal overflow | Navigation | New-task form | Task drawer + attachments |
|---:|---|---|---|---|
| 1440 | none | desktop sidebar | opens, no overflow | opens, inside viewport |
| 1280 | none | desktop sidebar | opens, no overflow | opens, inside viewport |
| 1024 | none | desktop sidebar | opens, no overflow | opens, inside viewport |
| 900 (the `lg` breakpoint) | none | desktop sidebar | opens, no overflow | opens, inside viewport |
| 390 | none | hamburger → labelled drawer, no overflow | opens, no overflow | opens, inside viewport |

The assertion is the document's measured `scrollWidth` against
`documentElement.clientWidth` (+1 px rounding), not a screenshot impression.
Offenders are listed on failure, ignoring content inside a horizontally
scrollable ancestor (tables intentionally use one). At 900 px the sidebar is
still visible because this project's Tailwind `lg` is **900 px**, not 1024.

**No product overflow was found, so no layout fix was made.** One test-only flake
was corrected: measuring the drawer's bounding box while its slide-in animation
was still running reported 391.37 px against a 390 px viewport. The test now
waits for finite animations first; that was test timing, not layout.

## Validation

| Command | Result |
|---|---|
| `tsc --noEmit` | passed |
| `tsc -p tests/e2e/tsconfig.json --noEmit` | passed |
| `vitest run` | 42 files / 227 tests passed |
| `check-terminology.mjs` | 217 files, no legacy feature name |
| `vite build` | passed (873.7 kB JS, 253.7 kB gzip; the 1.2 kB increase is the focus-restore logic added here) |
| `playwright test` (full suite) | 87 passed, 4 skipped, 0 failed (4.4 min) |
| `quality-a11y.spec.ts` | 7 passed |
| `quality-keyboard.spec.ts` | 7 passed |
| `quality-responsive.spec.ts` | 5 passed |
| `measure.mjs --repeat 10` before/after | see `m31-web-perf.md` |

The 4 skipped Playwright tests are the opt-in evidence specs
(`ORBIT_I2/I3/I4_EVIDENCE=1`), not tests disabled by this lane.

## Chosen not to fix (measured reasons)

- **Client performance**: no client stage is a bottleneck; the slow interactions
  are 93–96 % authoritative server round trips (see above). Optimising the client
  would change the measured total by less than measurement noise.
- **Server command latency (~650–700 ms mutation)**: debug-build Rust + SQLite
  commit time; Rust is out of scope for lane A and changing it would be a
  product/server change, not a quality sweep.
- **Responsive layout**: no document overflow at any required width; there was
  nothing to fix. Tables keep their deliberate internal horizontal scroll.
- **Screen-reader wording/reading order**: not machine-decidable; not changed
  speculatively.
- **Focus restoration inside third-party poppers**: Radix poppers already return
  focus to their triggers; only the dialog paths above lost focus.
