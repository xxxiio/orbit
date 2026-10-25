# Orbit web

The reference web client for Orbit: a self-hosted workspace for projects, tasks, agents,
and the workflows that connect them. This directory owns the product shell, design system,
and reusable UI primitives. Feature screens are added lane by lane.

## Stack

- React 18 + TypeScript (strict) + Vite 5
- React Router 6 + TanStack Query 5
- Radix UI primitives, styled with Tailwind CSS 3.4 and CSS-variable design tokens
- Vitest + Testing Library for unit tests; Playwright for the acceptance suite against a real
  `orbit-server`

## Getting started

Requires Node 18.18 or newer.

```bash
npm ci
npm run dev
```

The dev server runs at `http://127.0.0.1:5173` and proxies `/v1` to
`http://127.0.0.1:8787`, where `orbit-server` listens by default. Override the proxy target
with `ORBIT_API_PROXY` if the server runs elsewhere.

### Environment

| Variable        | Default | Meaning                                                         |
| --------------- | ------- | --------------------------------------------------------------- |
| `VITE_API_BASE` | `/v1`   | Same-origin API prefix compiled into the client.                |
| `ORBIT_API_PROXY` | `http://127.0.0.1:8787` | Dev-only proxy target for `/v1` (not exposed to the browser). |

Create `web/.env.local` for local overrides:

```dotenv
# Keep this same-origin in production. Absolute cross-origin URLs are not supported.
VITE_API_BASE=/v1

# Only used by the Vite dev server.
ORBIT_API_PROXY=http://127.0.0.1:8787
```

There is no token in `localStorage`, no token in a URL, and no admin surface. Sign-in
exchanges device credentials for an HttpOnly cookie; the client only sets
`credentials: 'include'` and sends `x-orbit-csrf: 1` on non-GET requests.

## Scripts

| Command                   | Purpose                                                        |
| ------------------------- | -------------------------------------------------------------- |
| `npm run dev`             | Vite dev server with `/v1` proxy                                |
| `npm run build`           | Type-check and emit `dist/`                                     |
| `npm run preview`         | Serve the built app at `http://127.0.0.1:4173`                  |
| `npm run typecheck`       | `tsc --noEmit`                                                  |
| `npm run test:unit`       | Vitest unit tests under `tests/unit` (jsdom)                    |
| `npm run test:e2e`        | Build, then run the Playwright suite under `tests/e2e`           |
| `npm run check:terminology` | Guard user-facing copy                                         |

## Structure

```text
src/
  api/            fetch wrapper, wire contracts, adapters, queries/mutations
    adapters.ts   compatibility barrel over the domain mappers below
    adapters/     wire → domain mappers split by domain (projects, tasks,
                  workspace, planning-reads, workflows, attachments, agents,
                  planning) plus the shared `asStatus` helper
    mutations.ts  compatibility barrel over the command families below
    mutations/    command hooks split by domain (core, tasks, projects,
                  milestones, workflows, attachments, knowledge, agents,
                  planning)
    contracts/    wire contracts split by domain (planning)
    endpoints/    one module per endpoint family
  app/            providers, routing, guard, page shims, shell orchestration
  components/
    surface/      the shared surface kit: ComponentPanel, UnavailablePanel,
                  InlineReadError, ProvenanceNote, DeclaredActionsNote,
                  ResourceReadPrelude, filterSortLimit, SurfaceFooter,
                  board chrome, table chrome — re-exported by `components/ui`
    ui/           reusable primitives (Button, Field, Dialog, Select, Badge,
                  Section, table headers, …) plus the surface kit
    forms/        labelled select, property select, inline title, edit guard
    markdown/     the single Markdown renderer and the one draft editor
                  (write/preview, dirty guard, commit/save hooks)
    theme/        theme provider and preference toggle
    layout/       presentational sidebar (data comes from the app shell)
    navigation/   navigation configuration and project item shape
    feedback/     session gate, global error boundary
  domain/         framework-free shared rules (project/milestone copy, task
                  progress, workflow copy, calendar/clock formatting)
  features/
    auth/         sign-in screen and error copy
    tasks/        shared task kernel: table, row meta, controls, list model,
                  URL state, project task list, drawer (task-drawer/)
    inbox/        Inbox list and quick capture
    home/         Today, including the agent attention rail
    projects/     project index, workspace, settings, create dialog
    knowledge/    Knowledge workspace, note page and declared reference panels:
                  knowledge-model.ts, note-view.tsx, knowledge-page.tsx,
                  knowledge-panel.tsx, note-create-dialog.tsx
                  note/  relations panel and move dialog
    attachments/  the one attachment list, mounted by the task drawer and the
                  work-item sheet
    agents/       agent overview, job page and declared job panels:
                  agent-model.ts, agents-overview-page.tsx, agent-job-page.tsx,
                  agent-jobs-panel.tsx
                  agent-job/  read-only blocks and the review form
    workflows/    the workflow surface:
                  view-metadata.ts (compatibility barrel for declarations/)
                  declarations/  filters, metrics, columns, sorts, views, reserved ids
                  views/         one renderer per component type (board, table,
                                 list, detail, metric, timeline, text)
                    work-item/   board, table, list, detail and their shared pieces
                  view-registry.tsx, view-types.ts
                  work-item-model.ts, work-item-fields.tsx
                  work-item-sheet.tsx, work-item-create-dialog.tsx
                  upgrade-preview.tsx, milestone-create-dialog.tsx
                  use-work-item-drawer.ts, index.ts (public barrel)
    planning/     project day plan, schedule window and the approval gate
  hooks/          shared hooks (media query, command feedback, idempotent
                  attempt, workflow catalog, URL-param resource drawers, …)
  lib/            browser-local stores and small helpers (recent projects, humanize)
  styles/         design tokens and global styles
tests/
  unit/
    api/          request shape, revision fences, adapters and errors
    components/   design-system primitives and the Markdown editor
    domain/       pure models shared across features (status labels and
                  progress rules, knowledge note model)
    features/     feature behaviour (panels, list model, URL state, registry)
  e2e/            Playwright acceptance suite (real orbit-server)
    harness/      server/session/fixture wiring shared by every spec
  fixture/        deterministic seed data
```

### Shared modules and boundaries

- `src/domain/` holds genuinely shared, framework-free rules. Its modules import no
  React, hooks or UI; features import it through `@/domain`. Calendar and clock copy
  (`formatMoment`, `formatDay`, `startOfDay`, `formatTimeOfDay`) lives in
  `domain/datetime.ts` so no feature imports another feature's barrel just to print a date.
- `src/components/surface/` is the **surface kit** every declared source shares: one panel
  shell, one unavailable/refusal state, one resource read guard and the board/table chrome.
  It lives under `components/` because it is cross-cutting UI, so a workflow view, a
  knowledge panel and an agent panel use the same chrome without importing each other.
  `@/components/ui` re-exports it.
- `src/features/tasks/index.ts` is the **public surface of the task kernel**. Inbox, Projects,
  Home and the app shell import task UI from `@/features/tasks` only; sections under
  `task-drawer/` are composition detail.
- `src/features/workflows/index.ts` is the **public surface of the workflow feature**.
  Projects compose workflows through it; workflows never import a projects module. A view
  is data and a renderer is chosen by `component.type`, never by a workflow or view id.
- Nothing under `src/components/**` imports `src/app/**` or `src/features/**`. Layout
  orchestration that needs workspace data (dialogs, drawer, navigation commands) lives in
  `src/app/workspace-*.tsx` and passes plain data down.
- Cross-cutting modules live outside features: hooks in `src/hooks/`, browser-local stores and
  helpers in `src/lib/`, Markdown in `src/components/markdown/`, form primitives in
  `src/components/forms/`. Declaration resolvers (`lib/declarations.ts`) are pure functions over
  a field-descriptor list, so a new source adds descriptors rather than a second resolver.
- `src/api/**` imports only `api/*`, React and TanStack Query. `api/accepted-write.ts` is the one
  place that writes an accepted response back into the cache, and none of its writes ever lowers
  a cached revision. `api/adapters.ts` and `api/mutations.ts` are barrels over
  `api/adapters/*` and `api/mutations/*` (split by domain); import the domain module directly
  in new code.
- Idempotency is one rule with one implementation: `hooks/use-idempotent-command.ts` pins an
  attempt per payload (mint on change, reuse on an identical retry, clear when confirmed).
  Every create dialog, note write/resume and agent review/resume goes through it.

Views declared by a project's workflow are resolved by `features/projects/use-project-workflow`
and rendered by the registry in `features/workflows`.

## Design system

Tokens live in `src/styles/tokens.css`; Tailwind maps them in `tailwind.config.ts`.

- Surfaces are warm neutral paper tones; a single ultramarine accent carries selection,
  focus, and primary actions.
- Type: **Fraunces** for display/headings, **Instrument Sans** for UI/body, **JetBrains
  Mono** for identifiers. Fonts are self-hosted through Fontsource.
- Spacing follows a 4px base scale; type steps run 11 → 60px with tighter tracking as size
  increases.
- Semantic roles: success, warning, danger, info. Each has a soft surface and an ink colour
  for use on that surface.
- Breakpoints: the sidebar is fixed above **900px**; below it, navigation becomes a Radix
  Dialog drawer with a focus trap. The content column uses full width.
- Motion is limited to short fades/rises, one staggered sidebar reveal, and the slow orbit
  mark. `prefers-reduced-motion` disables it.
- Themes are light/dark/system. Only the preference is stored (`orbit.theme`), and
  `public/theme-init.js` applies it before first paint without an inline script.

## Terminology guard

```bash
npm run check:terminology
```

Scans `src/**/*.{ts,tsx,html,css}` plus `index.html`. User-facing copy must use
**Workflow**. The script ignores comments and allowlists internal identifiers and API paths
(`scripts/check-terminology.mjs`). It exits non-zero on findings.

## Evidence

Screenshots produced from the built app live in `evidence/`:

- `login-light-1440x900.png`, `login-dark-1440x900.png`
- `login-light-390x844.png`, `login-dark-390x844.png`
- `shell-light-1440x900.png`, `shell-dark-1440x900.png`
- `shell-light-390x844.png`, `shell-dark-390x844.png` (drawer open)

Regenerate with `node scripts/capture-screenshots.mjs` while a preview server is running.
The shell capture stubs only `GET /v1/web/session`; it shows the real empty states, not
fabricated data.

## What is intentionally not here yet

- Capability execution: the work-item sheet lists declared actions and says the local Orbit
  authority owns execution, because this server exposes no execution route.
- Starting an agent job. Knowledge, Attachments and Agents ship as read/write screens, but a job
  is started by an executor or the command line; the browser only reviews a handoff and resumes a
  finished job into the queue.
- Project rename: a project cannot yet be named after the work it holds (the settings screen says
  so twice).
- Task created/updated timestamps in the drawer. Accessories show absolute dates; tasks show
  relative freshness only.
- Search across projects: the vault has its own search and the palette navigates or creates a
  task; there is no cross-project task/note search yet.
- Saved views: list/board filters live in the URL, and only `orbit.theme` is persisted.
