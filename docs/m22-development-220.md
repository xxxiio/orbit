# Development 220 — Cross-project dependency planning

This iteration adds **accepted-only**, deterministic cross-project planning insights.
It extends the M22 task dependency graph without introducing writable project-level
edges or an additional authoritative state machine.

## Delivered read models

- `project_dependency_planning::project_dependency_report` provides per-project
  actionable work, blocked tasks, critical prerequisites, transitive downstream
  impact, estimated waiting work, and cross-project bottlenecks.
- `project_handoffs::analyze_handoffs` derives project-to-project handoffs from
  accepted task prerequisites, identifies unfinished handoffs and round-trip
  coordination situations without confusing them with task dependency cycles.
- Both reports are available from the shared `AuthoritativeHost` and
  `AuthorityConnection`. Connected replicas use the last **accepted** snapshot
  and do not interpret queued commands as successful mutations.
- CLI: `orbit task project-dependencies --now-ms <ms> [--project <id>]`
  and `orbit task project-handoffs --now-ms <ms> [--project <id>]`.
- HTTP: `/v1/authoritative/project-dependencies` and
  `/v1/authoritative/project-handoffs` with matching query fields.

## Visibility contract

A project-filtered report is a deliberately limited projection. It shows
counts for external dependencies but does not expose foreign task titles,
identifiers, or foreign project identifiers in handoff details. This is **not**
a substitute for the authentication and authorization of the selected project:
HTTP transport still needs explicit project permission enforcement before
returning a scoped report to a partially privileged principal. Existing device
admission does not, by itself, establish project-level authorization.

Workspace-wide requests continue to require workspace-wide read permission.
Do not treat these endpoints as suitable for untrusted project-scoped agents
until the transport is wired to the corresponding authorization policy.

## Validation and remaining acceptance

Unit scenarios were added for diamond graphs, transitive impact, graph
corruption, cycle rejection, completed blockers, partial estimates, scoped
redaction, ordering, round-trip handoffs, and accepted-only behavior.
Source-level assertions are possible without Cargo, but executing the new
Rust tests and verifying build/API integration remains part of M15 acceptance.

This iteration does **not** add project-level mutations, change the
server-authoritative protocol, reintroduce Git sync, or change the offline
command queue.
