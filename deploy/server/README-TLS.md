# Optional HTTPS deployment (M16)

This Compose override publishes only the Caddy HTTPS front end (and the ACME HTTP challenge). The Orbit authority server remains loopback-bound on the host and on a private Compose network. This is **optional**: all local CLI/TUI clients work without any server.

1. Configure DNS for your own `ORBIT_DOMAIN`; ensure inbound ports 80/443 can reach the host. Never use a domain you do not control. Prefer a private overlay network and the default loopback Compose file if public exposure is unnecessary.
2. Export `ORBIT_DOMAIN` and a persistent `ORBIT_SERVER_ADMIN_KEY` (at least 32 characters), and run from the `deploy/server` directory: `docker compose -f compose.yaml -f compose.tls.yaml up -d --build`.
3. Enroll devices using an admin bearer token over HTTPS, and point connected clients at the server with `ORBIT_AUTHORITY_SERVER=https://$ORBIT_DOMAIN` (or the equivalent `ORBIT_AUTHORITY_URL`). `ORBIT_SYNC_SERVER` belongs to the optional polling loop (`sync-loop.sh`), not to connected client authorization. Protect admin keys, device tokens, the server database, and backups.
4. During final review, verify TLS certificate issuance, external reachability, authentication rejection, revocation, sync, and backup recovery against a disposable deployment.

**Security boundary:** TLS encrypts transit but the server stores semantic history in plaintext. Authenticated devices can currently access the single-user authority API according to their granted permissions. This is not a multi-user authorization system. Caddy cannot replace Orbit API token verification. No unauthenticated admin routes should be exposed. Public exposure carries risk; private-network operation is preferred.

Agent proposal execution uses opt-in `agent-worker.py` with an enrolled-device token. It reads the canonical server snapshot and submits supported approved task-create/task-update/habit-record/metric-observe intents exclusively through `/v1/authoritative/*-commands`. There is no local database access or Orbit CLI subprocess. Never expose worker credentials to browser clients. An expired lease may already have committed; reconcile its authoritative state and command receipt manually before deciding whether further action is appropriate. The proposal settlement and domain commit are two distinct transactions.

### Optional M23 Herdr machine executor (Development 249)

The Orbit server records machine-authorized execution intent; it **never** runs
Herdr or arbitrary task instructions itself. On the enrolled execution machine,
run `deploy/server/agent-machine-worker.py` as a trusted local service:

```sh
ORBIT_SYNC_HTTP_TOKEN='enrolled-device-token' python3 deploy/server/agent-machine-worker.py \
  --server https://orbit.example.com --device enrolled-machine-id \
  --journal-dir /var/lib/orbit/agent-machine --once
```

The worker polls the authenticated machine inbox, retrieves the authority-captured
brief, creates a durable **local claim fence before** contacting the claim API,
and invokes Herdr locally only if the server confirms a newly acquired launch
claim. It then reports the session identity. It never retries an uncertain
claim or Herdr launch. Operators must reconcile unresolved journal entries
against the machine's actual Herdr sessions and server state; **do not delete
journal entries merely to rerun a job**. Use distinct job IDs for new work.

This is an opt-in implementation component (not another public `orbit` CLI).
The normal user-facing commands remain under `orbit agent`. Configure the
journal directory on a persistent local volume with private permissions and
register the device ID before enabling the service. Run the worker where the
repository filesystem and Herdr executable are available. It does not execute
remote shell commands, forward credentials in prompts, or require cloud APIs.

**Limitations:** the worker implements launch, status monitoring, recovery and evidence reporting, but its complete integration with the real Rust server and Herdr still requires M15 release acceptance. Never infer task acceptance from an idle Herdr session.

### M23 worker recovery (Development 254)

The internal `agent-machine-worker.py` supports `--mode health --once` for
read-only diagnostics of launch journals. In particular, `claim_unknown`,
`dispatch_unknown`, `reconcile_required`, and `ambiguous_reported` require an
operator to reconcile with the authority and Herdr: never delete those journals
and replay the job speculatively. Launch-result reports in `report_pending` are
replayed using the existing claim token and session identity, without a second
Herdr launch. `ambiguous_report_pending` replays an explicit ambiguous result.

Only one worker may use a journal directory at a time. A non-blocking advisory
lock fails a competing instance immediately. Use a persistent, access-restricted
journal directory per enrolled machine identity. This worker uses POSIX `fcntl`
and is intended for Linux/macOS executors. This testing does not establish the
Rust server protocol or real Herdr CLI compatibility.
