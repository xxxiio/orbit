# Orbit Development 219 — Dependency Planning MCP Parity

## Scope and priority

M22 builds on deterministic local-first planning. Development 219 makes the
existing dependency engine operational through local and connected MCP,
completes read parity for graph/next actions, and adds a read-only dependency
preview. No server, Git synchronization, or hosted AI is required.

## Surface

- `task.dependencies.add`, `task.dependencies.remove` — authoritative command
  in local mode or durable pending remote command in connected mode. Both
  endpoints must be authorized before attempting a mutation.
- `task.graph`, `task.next` — accepted-state dependency and next-action reads.
  Without permission to inspect the whole task graph, requests fail closed
  rather than returning incomplete dependency chains.
- `task.dependencies.preview` — read-only add/remove simulation with explicit
  `now_ms`, accepted revision, changed readiness, and cycle rejection. A
  successful preview **never** stages or commits the proposed mutation.
- `orbit task preview-dependency TASK PREREQUISITE --now-ms TIME [--remove]`
  — equivalent local CLI preview.
- `planning.capabilities` includes the argument schema for each operation.

## Validation and security

The shared registry validates MCP arguments before dispatch. It rejects
missing required fields, invalid JSON value types, negative times/revisions,
excessive limits and unknown mutation arguments. Errors contain public stable
codes plus the offending capability/field; runtime authorization remains the
responsibility of the MCP adapter. Mutation schemas prohibit extra fields so
agent clients cannot silently request unsupported side effects.

Both task IDs are checked against accepted authoritative state for dependency
mutations. The connected agent cannot use pending creations to satisfy those
checks; pending edits remain separately visible in its outbox. The generic
`task.command` authorization path now explicitly handles prerequisite add and
remove operations, fixing a scope-check gap left in Development 213.

Offline preview reads accepted snapshots only, and does not reflect local
pending intentions as accepted. The result is a *proposal* tied to an observed
revision, not a commitment or a reservation: authoritative revision fencing
must still be enforced when the user/agent submits a command.

## Validation boundary

Source guards check contract registration and call dispatch wiring.
Dedicated Rust test cases exercise actual SQLite transactions, cycle rejection,
idempotence, ACL preflight, and local/connected read semantics. Rust executable
tests and compilation were not run in this environment (Cargo unavailable);
these remain explicit M15 acceptance requirements. No internal-testing claim.
