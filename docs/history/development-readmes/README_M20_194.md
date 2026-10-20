# Orbit M20 — Development 194: connected core workflows

## Implemented

The connected terminal now offers canonical project creation, habit creation,
habit occurrence recording, metric target creation and metric observation recording.
These join the prior task, schedule, routine, milestone and strategy palette
operations. Commands are fully validated before entering the durable SQLite
outbox. The `batch` command checks duplicate new project/habit identities and
rejects malformed requests before enqueueing any commands. Edits always use
the server-accepted snapshot and never mutate the replica's projection.

The desktop MCP connected mode now exposes the same core project/task/evidence
command families, in addition to the existing rich-domain commands. It also
exposes accepted habits, habit occurrences, metrics, and metric observations.
Each mutation uses typed Rust domain command deserialization, the canonical
`AuthorityConnection` and a durable remote outbox. Connected tools report
pending disposition, never fake authoritative acceptance.

## Authorization boundaries

* Project creation checks the project allowlist.
* Task creation checks the destination project's scope, including unassigned
  task permissions.
* Existing task changes require an accepted task visible in the last refreshed
  snapshot and check its current project scope; project reassignment also
  checks the destination project, while milestone assignment checks the
  milestone's accepted owning project.
* Habit and metric evidence require explicit unscoped-execution permission.
  This also applies to accepted evidence reads, to prevent inadvertent exposure
  to project-scoped agents.
* Read-only agents cannot enqueue new commands.
* No remote command uses the local `AuthoritativeHost` or changes the accepted
  snapshot before server acknowledgement.

## Limitations and scope

M20 is an implementation milestone. Broad workspace compilation, deployed
integration testing and release acceptance remain deferred to M15, per project
priorities. Focused Rust tests have been added but not executed here. The
connected MCP also intentionally does not expose legacy local knowledge
mutation APIs, which would violate the authoritative server boundary.

Internal-testing readiness remains unverified until a runnable Rust build and
successful core smoke workflows, with no known blocking data-loss issues.
