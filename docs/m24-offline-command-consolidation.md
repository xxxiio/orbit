# M24 Offline Command Consolidation — Development 268

## Ownership achieved

- `orbit-client::offline_command_journal` now owns durable transport-intention persistence, file locking, FIFO acknowledgments, bound server/device identity, stable-id replay, and atomic file replacement.
- `orbit-cli::agent_outbox` is a compatibility re-export only. No separate public command or changed outbox format was introduced.
- `orbit-client::authority::AuthorityReplica` remains the canonical SQLite authority outbox for domain intentions; the moved transport journal remains a distinct queue for now.
- Herdr side effects are never executed by this persistence component.

## Remaining M24-I3 work

Converge agent commands into the canonical SQLite authority outbox, preserving the existing agent command envelope and authoritative idempotency receipts. This requires an explicit dispatch policy: agent effects must never be treated as local mutations, and ambiguous remote responses must retain the original command ID and block unsafe automatic relaunch. Do not merge queues by blindly copying commands or lose FIFO ordering between agent and domain intentions. The full cutover must also address existing file-backed pending intents without silently losing them. No cutover is claimed here.

## Scope and acceptance

This is a source-architecture iteration. Focused test cases accompany the new owner module. Broad Rust compilation and integration acceptance remain in M15.

## Development 269 — canonical SQLite queue cutover (M24-I3)

The connected agent CLI now submits `queue-start`, `queue-resume`, `queue-continue`,
`queue-handoff`, and `queue-review` to `AuthorityReplica::queue_agent_transport`.
The same `authority_outbox` assigns global ordinals across agent and domain
intentions. `orbit agent flush` calls the **same** replica flush method used by
`orbit replica flush`, so an earlier unresolved task or Markdown command cannot
be overtaken by an agent launch. An agent command is a transport intention, not
an optimistic mutation to the replica. On acknowledgment, the original
command ID and server response are retained in the canonical receipt ledger.

The former JSON-backed agent journal and CLI facade were removed as redundant
implementations. This is a deliberate **greenfield cutover**: existing personal
JSON agent outboxes are not automatically migrated or deleted by the application;
operators with previously queued commands must process or manually reconcile
those files before switching. A replica created by an older development build
with a restrictive SQL kind CHECK is explicitly rejected, rather than silently
maintaining two independent queues. Create a fresh connected replica to use
this cutover; server canonical data remains unaffected. The old
`ORBIT_AGENT_OUTBOX_PATH` configuration is retired.

Agent flush uses the same device-authenticated HTTPS transport, never invokes
Herdr, and preserves the pending row on uncertain network outcomes. Conflicts
block the global queue until reviewed. This does not assert that arbitrary
server agent endpoints are safe to replay without their existing stable
idempotency identifiers; those identifiers remain in the persisted wire body.

Source progress: M24 58% → 66% (+8 pp). This implements the core I3 outbox
convergence, leaving runtime capability interface cleanup and the I4 worker
ownership/documentation cleanup. Broad compilation, integration, and runtime
checks belong to M15.
