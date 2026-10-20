# Orbit Development 232 — M23 I1 continuation

The new authority-local persistence functions live in `crates/orbit-storage/src/agent_jobs.rs`.

- Schema version 15 creates `agent_job_briefs` (immutable reservation-time title and status JSON), `agent_job_commands` (durable accepted commands with idempotent command IDs), plus prior job tables.
- Existing v14 databases upgrade to v15; the v13 upgrade also lands on v15.
- New `SqliteStore::agent_job_brief` and `SqliteStore::accept_agent_job_command` operations.
- Source unit tests assert duplicate accepted-command replay, stale revision rejection, immutable context snapshot and separate task status.

Limitations: no Cargo/rustc available for compilation. This is not the public connected-mode command router; no Herdr runtime integration or user/machine permissions are implemented. Full I1 acceptance remains pending.

## Progress after Development 232

| Scope | Provisional implementation progress | Acceptance status |
| --- | ---: | --- |
| M22 | 100% source scope | M15 release acceptance pending |
| M23-I1 | 90% | Not accepted |
| M23 | 18% (+5pp) | Not runtime accepted |
| M15 | Unchanged | Pending |

Next: implement command authorization, task dependency evaluation and revision binding through the accepted authoritative application command boundary. Do not dispatch external processes from unaccepted/offline actions.
