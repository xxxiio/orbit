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
