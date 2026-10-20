# M22 Development 227: Accepted Planning Action Discovery and Selection

## Why this increment

Development 226 completed accepted-state task/project navigation but left the
user without a consistent set of safe next operations. Development 227 makes
those navigation views actionable **without creating an implicit mutation
path**. This is higher priority than adding more planning rankings, because
Today users and MCP agents need to move from explanations to a specific
next action.

## What changed

- `planning_actions` contains one deterministic, read-only action menu for a
  task and one for a project/milestone. Both operate solely over the caller's
  already-scoped accepted `WorkBrief`.
- Each action includes an accepted revision, availability, an explanation,
  visible identity references, and an explicit mutation/confirmation marker.
  Hidden prerequisite identities are never returned.
- `ActionSelection` records the chosen index, kind, task identity and related
  prerequisite from the menu. `resolve_task_action` and
  `resolve_group_action` **recompute** the available actions at the current
  accepted revision and reject stale, forged, or unavailable selections.
- `ResolvedAction` routes to a safe navigation/preview destination. It always
  has `executable: false` and never acts as a command receipt or approval.
- `AuthoritativeHost` checks the current revision before producing or
  resolving actions. Connected clients check their last accepted snapshot.
  Pending offline commands never become accepted planning state.
- CLI, authenticated HTTP, embedded/connected MCP and both terminal Today
  experiences expose consistent action discovery and safe selection.
- Behavior tests cover permission and scope boundaries, hidden prerequisites,
  completed tasks, offline restrictions, revision changes, forgery, deterministic
  ordering, and non-mutating resolution.

## CLI usage

Local authority has no network requirement. Action discovery is read-only.

```sh
orbit task planning-actions task-123 --now-ms 1800000000000 --ends-at-ms 1800028800000
orbit task planning-group-actions project-123 --now-ms 1800000000000 --ends-at-ms 1800028800000
```

To resolve a selection, create a JSON `ActionSelection` using the action index
and the exact fields from the displayed menu. In particular, `reviewed_revision`
must match the accepted brief, `kind` and optional `task_id` and
`prerequisite_id` must match the selected action. The selection is not
an approval token:

```sh
orbit task resolve-planning-action task-123 --selection selection.json \
  --now-ms 1800000000000 --ends-at-ms 1800028800000
```

The returned destination is a navigation or preview suggestion only. Use the
existing review/approval/authority-command workflow for all changes.

## Server and MCP

GET `/v1/authoritative/planning-task-actions` and
`/v1/authoritative/planning-group-actions` authenticate the device before
reconstructing accepted-state menus.

POST `/v1/authoritative/planning-resolve-task-action` and
`/v1/authoritative/planning-resolve-group-action` accept a JSON selection,
authenticate the device and perform **read-only** resolution.

MCP tools: `planning.task_actions`, `planning.group_actions`,
`planning.resolve_task_action`, `planning.resolve_group_action`. They remain
fully workspace-scoped: project-scoped grants are denied rather than
allowing a cross-project dependency identity to leak.

The local Today TUI uses `a` to open the accepted task action menu; connected
Today also uses `a`. Existing drilldown keys and read-only impact previews
are preserved.

## Authority and safety contract

1. Never execute a command as a consequence of inspecting or resolving an
   action. `ResolvedAction.executable` is always false.
2. Never trust serialized menu items as authority. Reconstruct from a current
   accepted revision and compare the selected identity and operation.
3. Reject invisible task/project targets. No fallback search of the global
   task table for hidden prerequisites.
4. Allow explicit mutations only via established revision-fenced authority
   commands and existing approval flows, never by granting a read tool write
   privileges.
5. In connected mode, a disconnected device may inspect accepted cached
   menus, but mutation intents are marked unavailable.

## Verification and milestone reporting

- Source diff: 1,004 changed Rust lines (1,001 additions, 3 removals) across
  14 Rust files compared against Development 226; documentation and Python
  checks are excluded from this number.
- `scripts/legacy-contracts/check_m22_227.py`: 21 source-contract checks passed.
- `scripts/legacy-contracts/check_m22_219.py` and `scripts/legacy-contracts/check_m22_225.py`: pass.
- Rust test cases were added but were **not executed** because Cargo/Rust was
  unavailable in the current environment. This is source-implementation
  progress only and not executable acceptance.
- M22 increases from 85% to **90% provisional source implementation**. M15
  retains executable compilation, smoke workflows, no-known-blocking-data-loss
  review, and release acceptance; internal-test readiness is **not verified**.

## What's next

Finish M22's remaining real feature work: navigable selection dispatch and
consistent action execution handoff without granting read-only tools mutation
rights. Do not spend the next iteration on a purely cosmetic registry update.
