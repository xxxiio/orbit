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
  api/            fetch wrapper, wire contracts, adapters, queries/mutations, endpoints/
  app/            providers, routing, guard, page shims, shell orchestration
  components/
    ui/           reusable primitives (Button, Field, Dialog, Select, Badge, …)
    forms/        labelled select, property select, field/form error primitives
    markdown/     the single Markdown renderer
    theme/        theme provider and preference toggle
    layout/       presentational sidebar (data comes from the app shell)
    navigation/   navigation configuration and project item shape
    feedback/     session gate, global error boundary
  features/
    auth/         sign-in screen and error copy
    tasks/        shared task kernel: table, controls, list model, drawer
    inbox/        Inbox list and quick capture
    home/         Today
    projects/     project index, workspace, overview, task list, status labels
  hooks/          shared hooks (media query, command feedback, document title, …)
  lib/            browser-local stores and small helpers (recent projects, humanize)
  styles/         design tokens and global styles
tests/
  unit/           Vitest + Testing Library (jsdom)
  e2e/            Playwright acceptance suite (real orbit-server)
```

### Shared modules and boundaries

- `src/features/tasks/index.ts` is the **public surface of the task kernel**. Inbox, Projects,
  Home and the app shell import task UI from `@/features/tasks` only; sections under
  `task-drawer/` are composition detail.
- Nothing under `src/components/**` imports `src/app/**` or `src/features/**`. Layout
  orchestration that needs workspace data (dialogs, drawer, navigation commands) lives in
  `src/app/workspace-*.tsx` and passes plain data down.
- Cross-cutting modules live outside features: hooks in `src/hooks/`, browser-local stores and
  helpers in `src/lib/`, Markdown in `src/components/markdown/`, form primitives in
  `src/components/forms/`.
- `src/api/**` imports only `api/*`, React and TanStack Query.

Views declared by a project's workflow are resolved by `features/projects/use-project-workflow`;
there is no separate `features/workflows/` module yet.

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

- Work-item views: a workflow may declare views beyond Overview and Tasks; those render an
  honest "this view is not available yet" state until a view registry exists.
- Planning, knowledge, attachments, and agent administration screens, and agent
  administration beyond the read-only Agents placeholder.
- Work items themselves: the task drawer is task-typed, though its field editors are shared.
