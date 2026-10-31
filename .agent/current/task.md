---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0020
status: completed
milestone_id: MILESTONE-0034
---

# Task: I5 — MCP/Herdr coverage, failure injection, real recovery rehearsal, documentation and acceptance

## Objective

The backup/restore system is proven against the records MCP/Herdr agent work
produces, exercised through a real recovery rehearsal and a failure-injection
matrix, documented for operators, and honestly accepted against EC-012–EC-015 —
closing MILESTONE-0034.

## Current behaviour

I5 is implemented and verified and MILESTONE-0034 is complete on a frozen tree
(commit `f82e3fa` dirty with the M34 changes; gates all pass). A real WAL-server
backup was taken from a deterministic 103-task dataset that includes workflow Work
Items, notes with links/references, attachments, reserved-only agent jobs, a
capability install+grant+real execution receipt and an MCP-authored task change.
An isolated restore opened as a working instance with earlier data present, later
changes absent, agent/capability records readable and no automatic external
execution; all 64 database tables were equal before/after with identical
database/vault/attachment hashes. The CLI failure-injection matrix passed 22
checks with 1 not applicable and 0 defects while leaving the original workspace
recoverable. Documentation was updated and both documentation suites pass.

## Desired behaviour

All M34 exit criteria are met with honest, recorded evidence. The accepted result
is internal engineering acceptance with documented connected-mode/external-agent
limitations — no public-release qualification. The remaining work is the
maintainer review/approval gate and the proposed M35 self-dogfooding milestone.

## Scope

- MCP/Herdr record coverage and external-boundary exclusions.
- No automatic re-execution of external actions after recovery.
- Failure-injection matrix and original-workspace recoverability.
- Real recovery rehearsal with counts and hashes.
- Documentation updates across the required guides.
- Honest EC-012–EC-015 acceptance and M34 close-out.

## Out of scope

- Public-release qualification or live MCP/Herdr support claims beyond what was
  exercised.
- Reopening accepted M33/M34 scope; TUI redesign; new cloud dependencies.
- Starting M35 or any implementation before maintainer approval.

## Dependencies

- MILESTONE-0034's I1–I4 slices complete and verified.
- The shared `orbit-backup` service, the server backup API/Web GUI, the automatic
  lifecycle and the existing agent/capability/MCP surfaces.
- A disposable deterministic rehearsal dataset and a live WAL server for the
  backup.
- Maintainer review to close the milestone and approve the next milestone.

## Risks

- An agent or capability record could look present while its external action is
  not restorable; the rehearsal records both the record and the boundary.
- The full `orbit update` network flow was not run offline (helper-level evidence
  only) — recorded as a limitation, not hidden.
- MCP/Herdr live integrations were not exercised in M34; treating interface
  existence as proof would be dishonest, which is why M35 requires live processes
  and maintainer approval.
- A sandboxed `/tmp` environment can break child-process fixture seeding; the full
  Chromium run needed a workspace `TMPDIR` and the same failure reproduced on the
  pre-M34 baseline, so it is recorded as an environment limitation.

## Acceptance criteria

- [x] AC-001: Agent-job records, reviews/receipts/handoffs, workflow definitions,
  installed capability metadata, MCP-made task/milestone changes and
  activity/event records survive restoration.
- [x] AC-002: External-boundary exclusions are documented; recovery never
  automatically re-executes external actions; uncertain execution is marked for
  reconciliation.
- [x] AC-003: The real recovery rehearsal passes with recorded before/after counts
  and content hashes for the specified dataset and sequence.
- [x] AC-004: The failure-injection matrix is exercised and the original workspace
  remains recoverable in every case.
- [x] AC-005: Documentation is updated across the required guides and states the
  same-disk limitation without claiming zero data loss or zero downtime.
- [x] AC-006: EC-012–EC-015 evidence is complete, the full affected gates pass, and
  M34 acceptance is recorded honestly.

## Validation

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
cargo test -p orbit-backup --offline
python3 -m unittest discover -s tests/documentation
python3 -m pytest -q tests/documentation
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node scripts/check-terminology.mjs
cd web && node node_modules/vite/bin/vite.js build
cd web && TMPDIR=<workspace>/tmp node node_modules/@playwright/test/cli.js test --project=chromium
getdone-validate-records --project-root /home/xliu/dev/orbit --skills-root /home/xliu/dev/getdone
getdone-validate-project --project-root /home/xliu/dev/orbit --skills-root /home/xliu/dev/getdone
```
