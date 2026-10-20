# M24 optional worker boundaries (Development 271)

Orbit supports three optional server-side integrations with different permissions.
They are not interchangeable and are not alternative Orbit authority engines.

| Entry point | Responsibility | Side effects |
| --- | --- | --- |
| `deploy/server/agent-machine-worker.py` | Enrolled machine Herdr execution, journaling, inspection and handoff reporting | Executes Herdr only on the enrolled machine; durable launch fencing |
| `deploy/server/agent-worker.py` | Apply already-approved proposal intents through canonical server domain commands | Sends allowlisted, revision-checked domain mutations; no shell execution |
| `deploy/server/automation-worker.py` | Obtain an authoritative snapshot and settle scheduled deterministic reports | Read-only domain access; reports results to scheduler |
| `deploy/server/worker_transport.py` | Common HTTP transport and credential/base URL policy | No domain operations, no worker polling or execution |

## Ownership and dependency rule

The scripts own their own endpoint-specific request/response semantics; the shared
`worker_transport.json_request` owns device-header construction, JSON encoding,
response bounds, loopback-only cleartext policy, and rejecting redirects.
Import is sibling-relative through the script directory and requires no third-party
package or installation. Normal users interact with `orbit`, not these deployment
scripts. The canonical Rust server remains authoritative; workers must not write
its SQLite DB directly.

The Herdr worker deliberately keeps its local write-ahead launch journal: it fences
external process side effects, **not** a competing client offline-command outbox.
The client command outbox was consolidated in M24-I3 and is unrelated to this
machine-local crash journal.

## Maintenance rules

* Keep HTTP auth and URL policy in `worker_transport.py`, not three copies.
* Keep agent execution, approved-intent translation, and scheduled reporting in their
  distinct workers. Do not combine their credentials or privileges.
* Never forward bearer credentials across HTTP redirects or to non-loopback cleartext hosts.
* Don't retry potentially completed Herdr side effects on an ambiguous network outcome.
* Preserve the single public CLI `orbit` and the runtime adapter boundary.

Broader Rust build, full HTTP acceptance, live Herdr smoke testing, and fixes found
in acceptance remain M15 scope. This document records source architecture only.

## Module ownership (Development 272)

Inside the machine-execution worker, `agent_worker_evidence.py` owns read-only
Git and worktree evidence, `agent_worker_journal.py` owns the machine-local
crash journal and single-worker lock, and `agent-machine-worker.py` coordinates
authority requests, Herdr launch/status calls, and durable handoff submission.
No extracted helper has authority credentials or makes HTTP calls.
