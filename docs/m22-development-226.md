# Development 226 — Accepted Task, Project and Milestone Navigation

## Scope and priority

M22 prioritizes making the deterministic Beads-inspired planning features
usable as a daily workflow, not building another task graph or persistence
store. This follows the existing Today/Next and portfolio planning features:
users can drill into why a task is blocked, which work depends on it, and
what else is actionable in the same project or milestone.

This delivery uses the accepted `WorkBrief` as one source for all navigation.
No new mutable planning cache or authority command is introduced. No
higher-priority daily planning capability is displaced by this scope.

## APIs

- `orbit task planning-focus TASK --now-ms N --ends-at-ms N`
- `orbit task planning-group PROJECT --now-ms N --ends-at-ms N [--milestone M]`
- `GET /v1/authoritative/planning-focus?task_id=...&now_ms=...&ends_at_ms=...`
- `GET /v1/authoritative/planning-group?project_id=...&now_ms=...&ends_at_ms=...`
- MCP `planning.task_focus` and `planning.group_focus` in both authority modes
- Local Today: `v` opens accepted task context; `g` opens project context
- Connected TUI: `plan.focus TASK` and `plan.group --project PROJECT [--milestone M]`
- Rust: `planning_focus::focus_task`, `planning_focus::focus_group`, host,
  embedded client, and offline-capable connected client entrypoints

Task focus includes accepted task details, visible direct prerequisites and
dependents, bounded upstream and downstream dependency trails, project and
milestone peers, adjacent Today tasks, and explicit hidden-prerequisite counts.
Project/milestone focus includes action counts, proposed work minutes, overdue
work, visible prerequisite bottlenecks, and ranked action candidates.

## Safety and correctness

- All reads are from the accepted planning snapshot, not pending commands.
- Host and client reject a supplied brief with a stale accepted revision.
- Missing or unauthorized task identities are not recovered from another
  repository. Unknown prerequisite IDs are redacted from task navigation.
- Dependency paths have bounded breadth (100 maximum) and depth (32), with
  bounded exploration to prevent runaway work on malformed cyclic data.
- Task and project focus are read-only: no implicit schedule or task mutation.
- MCP follows the conservative workspace-level access policy already applied
  to `planning.work_brief` (unrestricted planning read access required).
- HTTP endpoints currently use workspace-level device authentication. They
  must not be exposed to project-restricted principals without additional
  endpoint authorization. This is an explicit, non-silent integration limit.

## Testing and readiness

Rust source includes tests for cross-project scope, chain order, diamond
paths, deterministic sorting, hidden prerequisites, cycles, bounded traversal,
group statistics, and read-only behavior. Python source-contract tests cover
CLI, HTTP, MCP, TUI, and the shared Rust entrypoints.

These are implementation checks only. Rust compiler and executable integration
acceptance remain assigned to M15. Internal-testing readiness is **not yet
verified**, because no runnable Rust build or core smoke workflows were
executed in this environment.
