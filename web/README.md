# Orbit web

The reference web client for Orbit: a self-hosted workspace for projects, tasks, agents,
and the workflows that connect them. This directory owns the product shell, design system,
and reusable UI primitives. Feature screens are added lane by lane.

## Stack

- React 18 + TypeScript (strict) + Vite 5
- React Router 6 + TanStack Query 5
- Radix UI primitives, styled with Tailwind CSS 3.4 and CSS-variable design tokens
- Vitest + Testing Library for unit tests; Playwright is configured for the next lane

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
| `npm run test:unit`       | Vitest unit tests (jsdom)                                       |
| `npm run test:e2e`        | Playwright (config only; add specs under `e2e/`)                |
| `npm run check:terminology` | Guard user-facing copy                                         |

## Structure

```text
src/
  api/            single fetch wrapper + web session calls
  app/            providers, routing, guard, theme, pages
  components/
    ui/           reusable primitives (Button, Field, DataTable, Dialog, …)
    layout/       app shell, sidebar, theme toggle
    navigation/   navigation configuration and project item shape
    feedback/     session gate, global error boundary
  features/
    auth/         sign-in screen and error copy
  styles/         design tokens and global styles
```

`src/features/workflows/` is reserved for the workflows lane. Do not introduce a parallel
feature directory under the legacy name.

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

- Project data in the sidebar (`projects` is `[]`) and the **New project** action (disabled
  with an explanation).
- Feature screens for Inbox, Projects, Agents, tasks, workflows, planning, knowledge,
  attachments, and agent administration.
- Playwright specs; only configuration is present.
