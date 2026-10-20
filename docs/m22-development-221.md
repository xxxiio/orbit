# M22 — Development 221: Project-aware portfolio next actions

## Rationale and priority

The previously delivered dependency graph and cross-project handoff report show
which project depends on which other project, but neither gives users an actionable
portfolio-ordered queue. This iteration adds that missing read-only workflow without
changing the existing daily `task next` ordering, which has different semantics.

## Contract

`PortfolioActionRequest` includes the explicit evaluation timestamp, optional
project filter, optional unassigned work, and bounded result limit. Ranking is
stable: overdue status, number of tasks **immediately** unblocked, total unfinished
transitive impact, cross-project breadth, due date, estimated duration, task ID.
A diamond-shaped dependency only contributes each descendant once.

Results distinguish immediate unblock from long-run impact. A downstream task
may count toward future impact without becoming executable today; it is only an
immediate unlock if all other prerequisites are complete, its earliest-start time
has arrived, and its workflow status is `ready`.

The per-action rationale includes exact counts, incomplete effort estimates,
when the task is overdue, and a disclaimer that effects are prospective. This
is a recommendation, not a task mutation or promise of future execution.

## Consistency and authority

The evaluator consumes a single accepted revision, task rows and dependency
edges. The host rejects revisions that disagree between the two reads. The
connected client reads the last accepted snapshot and never interprets pending
outbox writes as accepted. This result requires no LLM, server during offline
use, or new authoritative database tables.

## Privacy boundary

A project-scoped report includes task identifiers **only** from the requested
project. Foreign dependency details are summarized as counts and distinct
project breadth. CLI and HTTP consumers of an unscoped report have workspace
credentials; MCP policy explicitly requires an authorized project when agent
project access is restricted. These are adapter obligations, not a substitute
for a future object-level authorization review of all HTTP endpoints.

## Usage

```bash
orbit task portfolio-actions --now-ms 1800000000000 --limit 20
orbit task portfolio-actions --now-ms 1800000000000 --project project-123
```

Read-only HTTP: `GET /v1/authoritative/portfolio-actions?now_ms=...&limit=20`.

MCP tool `planning.portfolio_actions` is available in embedded and connected
modes. Agents with project allowlists must specify an allowed `project_id`.

## Verification

Focused Rust unit tests exercise transitive and direct impact, deduplication,
scoped redaction, input bounds, stable ordering and status checks. Python source
contract checks validate interface presence and authorization guards but cannot
substitute for Rust executable acceptance, which remains unverified pending M15.

## Internal-testing assessment

Not ready on this evidence alone. Rust compilation, full runtime behavior,
server/client acceptance and absence of blocking data-loss issues remain to be
verified before an internal-testing readiness declaration.

## Portfolio time budgeting

The same accepted-state recommendation order can drive `task portfolio-capacity`,
which greedily fits ranked work into an explicit minute budget. It never
silently fills missing effort estimates, and reports why actions were excluded.
This operation is accessible through the shared Rust client, read-only HTTP
`/v1/authoritative/portfolio-capacity`, local CLI, and both MCP modes via
`planning.portfolio_capacity`. A project-scoped agent must supply a project ID
allowed by its policy. No calendar block or task is changed by this operation.

```bash
orbit task portfolio-capacity --now-ms 1800000000000 --available-minutes 120 --max-tasks 5
orbit task portfolio-capacity --now-ms 1800000000000 --available-minutes 60 --project project-123
```

The connected TUI exposes `plan.portfolio --limit 20 --project project-123` as
an accepted-snapshot report, with displayed explanations for cross-project
impact. This complements rather than replaces the existing Today view.
