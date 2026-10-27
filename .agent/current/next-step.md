---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0015
status: completed
milestone_id: MILESTONE-0032
advances_exit_criteria: [EC-001, EC-002, EC-003, EC-004, EC-009, EC-011]
---

# Next Deterministic Step

## Objective

Land M32 iteration I1: project rename/status through authoritative commands,
one server-backed search across projects/tasks/knowledge, saved views that
survive a browser restart, and authoritative task timestamps — each proven by
real-browser acceptance against an isolated server.

## Why this is next

The four gaps are the operations a person performs daily in the shipped web
product, and each currently forces a CLI workaround. They are also the
prerequisites for the later M32 iterations: I2 needs the same command/read
discipline for knowledge, I3/I4 need the browser to operate rather than only
inspect, and I5 dogfooding cannot evaluate workflows that cannot be completed.

## Preconditions

- MILESTONE-0031 complete at `d1e916a`; M31 web client, Playwright harness and
  API contract documentation are the baseline.
- Frozen I1 contracts in `.slim/deepwork/m32-web-completeness/i1-plan.md`.

## Inputs

- `web/src/{api,features,app,components}/**`, `web/tests/{unit,e2e}/**`.
- `crates/orbit-{domain,application,runtime,server}/**`.
- `.slim/deepwork/m32-web-completeness/{progress.md,i1-plan.md}`.

## Ordered actions

1. Rust authority lane: project update command + status edges + search +
   task timestamps + saved views, with focused tests.
2. Web client lane: settings form, palette search, saved-view controls, task
   metadata, unit tests.
3. Browser acceptance lane: Playwright specs for rename/status/conflict,
   search, saved views and timestamps; fix integration defects.
4. Run the full gates, record honest evidence, update the milestone criteria
   and journal, then start I2.

## Expected outputs

- Four working operations in the browser, each with a server contract, tests
  and real-browser evidence.
- Saved views persisted server-side without touching the semantic ledger or a
  workflow definition.
- Task timestamps that report `Unknown` instead of guessing.

## Acceptance criteria

- [x] NS-001: `POST /authoritative/project-commands` with `action=update`
  renames and transitions a project, fences stale revisions, replays receipts
  and leaves identity stable.
- [x] NS-002: `GET /authoritative/search` returns grouped bounded results and
  the client opens each destination.
- [x] NS-003: Saved views survive a browser restart and never modify a workflow
  definition; the authority revision does not move on a saved-view write.
- [x] NS-004: Task detail exposes created/updated from recorded activity, with
  `null` for records without history.
- [x] NS-005: Full Rust tests, clippy, typecheck, vitest, terminology, build and
  Playwright pass on the frozen tree.
- [x] NS-006: `getdone-validate-project` exits 0.

## Validation

```sh
cargo test -p orbit-domain -p orbit-application -p orbit-runtime -p orbit-server --offline
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node scripts/check-terminology.mjs
cd web && node node_modules/vite/bin/vite.js build
cd web && node node_modules/@playwright/test/cli.js test
getdone-validate-project
```

## Stop conditions

- A proposed command would bypass the revision fence, the receipt, or the
  semantic ledger for actual domain state.
- A saved view would require mutating a pinned workflow definition.
- A timestamp cannot be derived from recorded history without fabricating it.
- The client would need a second implementation of a server rule.

## Out of scope

- I2–I6 work, any TUI change, M27 publication, M15 evidence regeneration.
