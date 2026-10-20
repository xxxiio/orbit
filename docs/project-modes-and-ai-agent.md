# Project modes and AI project agent

Orbit does not introduce a separate Goal aggregate. A `Project` is the user's desired-outcome container; project mode changes how Orbit interprets progress, evidence, reviews, and AI assistance.

## Preset modes

- `standard` — delivery/outcome work; milestones, tasks, blockers, forecast.
- `learning` — curriculum, practice, mastery evidence, assessment checkpoints, study consistency.
- `habit_driven` — repeated behavior, consistency and sustainable cadence.
- `metric_target` — a measurable target and trajectory.
- `maintenance` — staying within an acceptable range rather than reaching 100% completion.
- `custom` — user-selected Orbit-supported progress signals; not an arbitrary schema/formula engine.

A project also carries an optional objective, selected supported progress signals, and explicit success criteria.

## Completion criteria

Success criteria are keyed, required/optional, and have status `pending`, `satisfied`, or `waived`. Required criteria gate the canonical `ProjectStatus::Completed` transition. AI recommendations cannot bypass this rule.

Criterion ordering is non-semantic and canonicalized by key so field-level sync merges converge deterministically.

## Persistence and sync

SQLite schema v8 stores project mode/objective/progress signals and project success criteria. Semantic sync represents strategy fields separately and each criterion as its own `criterion:<key>` field. This keeps criteria independently mergeable and allows explicit deletions without hiding the strategy in an opaque JSON blob.

## AI project-agent contract

The AI layer exposes a mode-aware playbook for roadmap focus, evidence focus, and review questions. It supports two structured proposal classes:

1. roadmap proposals: assumptions, project success criteria, milestones with their own criteria/dependencies/targets, suggested habits/metrics, risks, and review cadence;
2. completion reviews: per-criterion satisfied/not-satisfied assessment, concrete evidence references, rationale, and a model recommendation.

Remote/local provider calls use the existing disclosure/context-policy boundary. Cloud AI remains user-selected. The provider receives only context allowed by Orbit's configured project/context policy.

For completion reviews Orbit recomputes deterministic readiness after parsing. A required criterion is not considered supported by an AI assessment unless the assessment marks it satisfied and includes evidence. The model may recommend completion while Orbit still reports `deterministic_completion_ready = false`.

## Client and agent surfaces

Embedded client API v2 exposes mode, objective, progress signals, criteria, criterion status mutation, mode-aware playbooks, structured roadmap proposals, and completion-assessment DTOs.

MCP project listing exposes the same semantics. Developer-scoped agents may configure project strategy or change a criterion status only through audited application commands and only inside their Orbit-owned project scope.

The CLI exposes project strategy configuration and criterion status updates. The TUI project view displays project mode and criterion readiness.

## Remaining work

The first source slice deliberately does not pretend all progress signals already have full quantitative implementations. Follow-up work should add explicit project↔habit associations and metric-series/target primitives so habit-driven, metric-target, and maintenance modes can have first-class deterministic progress calculations rather than task-derived fallbacks.

## Deterministic signal calculation (development 037)

`orbit-progress::habit_consistency_evidence` calculates fulfilled/evaluated period share, with no periods reported as unknown rather than zero. `metric_target_evidence` calculates a clamped, directional baseline-to-target percentage using overflow-safe intermediate arithmetic. These are reusable evidence primitives, **not yet project-linked**: project binding persistence and semantic sync are now implemented in development 040; selection of evidence windows, persisted metric targets, and project-health wiring remain M14 feature work.

## Evidence binding protocol (development 040)

Project evidence binds and unbinds are field-level changes on the existing
Project semantic entity. `evidence:habit:<habit-id>` and
`evidence:metric:<metric-id>` are independently mergeable fields. A value of
`1` means bound, and semantic field deletion means unbound. Commands persist
the association and reserve the outgoing change in one SQLite transaction.
Only changed memberships produce new events. Incoming associations with a
missing habit are deferred until the dependency is available; local pending
mutations retain precedence during inbound materialization. These metadata
bindings alone do not constitute metric observations or a target definition.

## Integrated M14 evidence workflow (development 044)

Orbit now supports one local-first project-evidence workflow through application,
SQLite, Git-compatible semantic change history, CLI, TUI, embedded clients, and
project-scoped MCP. Metric targets are semantic `metric` entities with a `target`
field containing validated baseline/target/direction, and each observation is an
independent `observation:<timestamp>` field. This preserves field-level merge and
conflict-resolution semantics without a second transport or server requirement.
Local writes persist and reserve sync changes atomically, and inbound replay
upserts resolved observations idempotently. Observations without a known target
are deferred, not installed as orphan records.

The explicit-window mode-aware health query reports three separate values:
canonical task rollup, combined bound evidence, and average selected/available
progress-signal families. Missing measurements/signals are identified; they are
never turned into 0%. Operational signals (e.g. forecast, blockers) are not
fabricated as normalized progress scores. Required completion criteria remain the
sole canonical completion gate. A default seven-day evidence window is used for
CLI/TUI/MCP health views; the client/query API accepts explicit windows and
as-of cutoffs for reproducibility.

### Surfaces

- CLI: project evidence bind/unbind/list/report, metric target/observe, project health.
- TUI Projects tab: canonical health and evidence plus `b` bind,
  `U` unbind, `t` metric target and `o` metric observation prompts.
- MCP: scoped project health/evidence reads and developer-only writes, including
  bound metric target/observation operations. MCP never trusts a caller-supplied
  project identity to bypass Orbit's allowlist.
- Embedded client API v3: primitive request/response DTOs for bindings, metric
  writes and explicit-window project evidence reporting.

### Compatibility and acceptance

New readers continue accepting earlier Git semantic histories; `metric` is a new
entity kind, so **older client versions must be upgraded before reading newer
histories containing metric records**. As usual, sync does not mean the server is
mandatory. Metric records already written by development 042/043 were local-only:
those historical records are **not automatically backfilled into semantic history**
by this version, and must be republished through subsequent writes or a migration.
This remains a compatibility follow-up before broad deployment.

Source regression tests cover atomic outbox reservation, repeated writes, inbound
dependency deferral, idempotent replay and same-timestamp resolution, signal
aggregation, embedded-client reporting, and MCP project scoping. The tests have
**not been executed** because Cargo/Rust is unavailable. This milestone cannot
be accepted as complete until compilation, formatting, clippy, tests, CLI/TUI/MCP
smoke workflows, and compatibility/backfill are verified.
