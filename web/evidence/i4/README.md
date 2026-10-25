# I4 evidence

Browser captures of the M31 I4 build, produced from the deterministic fixture in
`web/tests/fixture/seed.ts` (seeded through the public HTTP API on an isolated
`orbit-server`). No mocked responses and no preview server.

Captured on 2026-10-26 from commit
`e486114` (M31 I4: show the selected note first on a phone) on top of the I4
feature commit `87119af` (M31 I4: knowledge, attachments and agent jobs). The
`web/dist` used for the capture came from the same source, and the server binary
was the existing `target/debug/orbit-server` built 2026-10-25 22:18 local; I4
changed no Rust source, so that binary matches the committed server. Re-run the
commands below after any change to the build; a capture is only comparable to
the commit and binary named here.

Reproduce:

```sh
cd web
node node_modules/vite/bin/vite.js build
ORBIT_I4_EVIDENCE=1 node node_modules/@playwright/test/cli.js test \
  tests/e2e/evidence-i4.spec.ts --project=chromium
```

The spec is **capture-only**: it reads the seeded vault, opens the create dialog,
opens the image preview, and navigates to jobs and declared panels. It never
uploads a file, creates a note or records a review, so a failed capture cannot
be mistaken for a failed acceptance flow. Those flows live in
`knowledge.spec.ts`, `attachments.spec.ts` and `agents.spec.ts`.

| File | Screen |
| --- | --- |
| `knowledge-note-light-1440x900.png`, `knowledge-note-light-390x844.png` | A note rendered from disk with its stable id, path, outgoing link and backlink chips |
| `knowledge-create-light-1440x900.png` | New note dialog: path, title and plain Markdown; the id is minted before the request |
| `knowledge-search-light-1440x900.png` | Inventory search results from `/markdown-search` |
| `attachments-task-drawer-light-1440x900.png`, `attachments-task-drawer-light-390x844.png` | Task drawer attachments: a real SVG fixture, preview open, download and remove controls |
| `attachments-work-item-sheet-light-1440x900.png` | The same attachment component in the work-item sheet |
| `agents-overview-light-1440x900.png`, `agents-overview-dark-1440x900.png` | Agent jobs grouped by attention and state, with project/task context |
| `agent-job-detail-light-1440x900.png` | Job detail: state, revision, runtime refs, timestamps and handoff |
| `agent-job-review-light-1440x900.png`, `agent-job-review-light-390x844.png` | Review form with the "approving does not complete the task" notice and resume context |
| `home-attention-light-1440x900.png` | Home's agent-attention panel, fed by the attention read |
| `registry-knowledge-refs-light-1440x900.png` | Research workspace: the declared knowledge table and `knowledge_refs` panel over real notes |
| `registry-agent-jobs-light-1440x900.png` | Development workspace: the declared `agent_jobs` panel and compact list over real jobs |

Viewport: 1440x900 desktop, 390x844 phone. Theme comes from `orbit.theme`; the
agents overview has a dark capture. The task drawer uses the SVG fixture so the
preview shows the stored bytes, not a placeholder.
