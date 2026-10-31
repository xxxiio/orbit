# Orbit self-hosted architecture — current contract

> Normative connected-mode overview. For dependency direction and local mode,
> see [architecture.md](architecture.md). For dated design history, see
> [the archived October 10 decision](historical-self-hosted-decision-2026-10-10.md).

## Two supported modes

1. **Local-only authoritative:** `AuthoritativeHost` owns the local SQLite
   database. No server, user account, Herdr, or cloud is required.
2. **Optional self-hosted authoritative:** The server `AuthoritativeHost` owns
   canonical SQLite and business rules. Native clients persist accepted
   snapshots and pending commands in local SQLite, and do not pretend pending
   offline operations have been accepted. Browser clients use the server API.

Connected mode is **not** peer-to-peer event relay or Git synchronization.
No legacy Git/relay migration adapters are required for this greenfield
architecture. Offline pending commands are submitted in their canonical order
and receive authoritative acceptance receipts or conflicts.

## Agent execution and security

The server persists jobs, authorization, assignments, handoffs, and review
outcomes, but does not invoke agents or accept general-purpose shell requests.
An explicitly configured machine worker retrieves its assigned commands, fences
potentially irreversible launches, runs optional Herdr locally/on its enrolled
machine, and reports observations. Machine crash-recovery journals are separate
from clients' canonical offline outboxes. Herdr session completion does not
close the Orbit task. Approval is explicit.

Device authentication, TLS except loopback, immutable command identities, and
fail-closed uncertainty are required interfaces, not optional conveniences.
The ordinary Orbit CLI is `orbit`; deployment scripts are not public CLI
alternatives. Flutter integration remains in the separate Atomic App project.

Browser access has one opt-in exception. `ORBIT_AUTH_MODE=local-trusted` exists
only for a loopback-only personal installation where the server and browser run
on the same machine. It is refused on every non-loopback bind, must not be
combined with a reverse proxy, and does not change the single-user
device-authentication trust described here for remotely reachable deployments.
Those deployments keep device authentication, TLS and the existing enrollment
and revocation rules.

## Backup and recovery boundary

A server workspace backup is one portable `.orbitbak` archive of the server's
authoritative SQLite database, its Markdown vault, the referenced attachment
objects and normalized non-secret configuration. The server and the CLI call the
same `orbit-backup` service; the browser is a transport over it.

- The server backup directory is `ORBIT_BACKUP_DIR` (default `<server database
  parent>/backups`); isolated restores are created under
  `ORBIT_BACKUP_RESTORE_ROOT` (default `<server database parent>/restores`). The
  archive download response is bounded by `ORBIT_BACKUP_DOWNLOAD_MAX_BYTES`
  (default 512 MiB) and answers `413` above it.
- Ordinary authenticated product principals may list, create, inspect, verify,
  download and perform isolated restores. Producing a **replacement plan** is
  the only administrator-credential route, and it only writes a pending offline
  plan: the running server never replaces its live database from a browser
  request. An operator performs the maintenance replacement from the server
  shell with writers stopped.
- A server-mode archive records `source.excluded_client_pending_data: true` and
  a `client_pending_data` exclusion: it does **not** contain what clients hold —
  a replica's accepted snapshot, its unresolved outbox commands and its
  `attachment-staging/` directory. Client replica snapshots are also rejected as
  a recovery point (`--source-mode replica` fails closed); a replica is not an
  authority. Preserve each replica and reconcile through the supported
  convergence/conflict workflow after a server restore; never replay pending
  commands automatically and never delete outbox data to silence a warning.
- The long-running server spawns the backup scheduler. It re-reads
  `<ORBIT_BACKUP_DIR>/policy.json` every 15 minutes (first check about 30 seconds
  after startup) and creates a verified automatic backup when a `daily`/`weekly`
  policy is due, then applies retention while protecting the new archive. A
  policy change takes effect without a restart; `disabled`/`before-upgrade` never
  run on a timer.
- Also outside the boundary: external Herdr sessions and their working
  directories, running processes, Git worktrees/branches and other repositories,
  external credentials, and actions that already executed outside Orbit. A
  restore never re-executes those actions; uncertain external execution is
  marked for reconciliation.
- Isolated restore writes a new workspace copy under the restore root and never
  touches the active server workspace. Replacement is a maintenance operation
  that stages, verifies and preserves the previous database/vault plus a
  recovery journal for rollback.
- Credentials and enrollment material are separate secrets and are never in the
  archive. A backup on the same disk as the server database is not protection
  against disk failure; copy verified archives to another device or location.
  No zero-data-loss or zero-downtime claim is made.

## Ownership and acceptance

- `orbit-application` owns use cases and ports.
- `orbit-storage` adapts SQLite to application repositories.
- `orbit-client` and `orbit-sync` own replicas, transport and pending commands.
- `orbit-server` is an authenticated transport, not a second domain engine.
- M24 owns source-only framework consolidation (complete).
- M15 owns broad compilation, connected end-to-end and live Herdr testing, and
  any fixes discovered during release acceptance.

## Verification status

Linux local/loopback/local-Herdr M15 acceptance has passed (37/37 checks, 1235
Rust tests). TLS/self-hosted ingress, remote-machine agent execution,
cross-platform targets, signing and real version-to-version upgrades have not
been verified. The connected server is also single-user trust: any enrolled
device may call mutation routes.

See [worker roles](m24-worker-architecture.md) and
[server APIs](server-api.md) for operational details.
