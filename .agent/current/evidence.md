---
template: evidence-manifest
template_version: 1.0.0
template_digest: "2787d570257f4ffbf8add20f62aa21227a4aea1a6723583e8928b5e71e09f79b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: final
task_id: TASK-0004
---

# Evidence Manifest

## Task

TASK-0004 — M28 project-centric TUI and blueprint UI composition.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001: Shared view schema/validation | pass | `orbit-blueprint` 20 tests; presentation validation and bundled round-trip |
| AC-002: Project-centric shell | pass | `orbit-tui/src/{main,app,backend,render,theme,terminal}.rs`; one shell, responsive + PTY |
| AC-003: Real application data | pass | local/connected project/workspace view data, blueprint work items, agent projection |
| AC-004: Registered components | pass | 13 component types; 125 TUI tests incl. readable board/table/detail states |
| AC-005: Blueprint views/legacy defaults | pass | bundled v2, generated fallback, PTY blueprint bind/work-item edit/transition/restart |
| AC-006: Contextual actions/forms/feedback | pass | structured forms, action catalogue, duplicate suppression, pending/accepted/stale/offline states |
| AC-007: Tests | pass | `.agent/evidence/m28-recovery-tests.txt`, `m28-workspace-tests.txt`; 1,350 workspace tests, 37 PTY |
| AC-008: Documentation | pass | TUI guide, UI reference, developer guide, connected/quit/agent docs; 29 doc tests |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Focused M28 tests | pass | `cargo test -p orbit-blueprint -p orbit-ui -p orbit-client -p orbit-tui --offline` | 20 + 14 + 305 + 125 = 464 passed |
| Workspace regression | pass | `cargo test --workspace --offline` | 1,350 passed, 0 failed |
| Clippy | pass | affected crates, `--all-targets -- -D warnings` | clean |
| Format | pass | `cargo fmt --all -- --check` | clean |
| Documentation | pass | `python3 -m unittest discover -s tests/documentation` | 29 passed |
| Local PTY | pass | `python3 -m pytest -q tests/acceptance/tui` | 37 total; 36 local, 1 connected |
| Connected PTY | pass | disposable server, pending→flush→accepted | project + task with earliest/estimate verified server-side |
| Full M15 profile | pass | `./scripts/m15-full-automated.sh --profile full` | 42 passed, 0 failed, 0 not_run; `.agent/evidence/m28-m15-final.zip` |

## Checks not run

- Live external agent review/resume (no configured runtime in acceptance).
- Connected multi-command atomicity (documented residual).
- Real public release installation/update.

## Waivers

- None.

## Residual risk

- Connected blueprint/work-item/habit/metric/note editing remains on the legacy command surface.
- Agent inspection is available; review/resume is intentionally disabled without an authorized runtime.
