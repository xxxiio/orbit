# Connected mode onboarding

Connected Orbit requires a separately installed, authenticated self-hosted server. **The server's SQLite is authoritative**; any local replica is only an accepted-state cache plus a durable outbox. Unlike local-only mode, an offline mutation is not accepted merely because it was queued.

## Deployment and enrollment

1. Install optional server components using [installation components](../guides/installation-components.md), then follow [self-hosting](../guides/self-hosting.md) to initialize a private server and TLS ingress for remote access.
2. Use the deployment's supported enrollment path to establish a device identity and authentication token. Never commit credentials to Git or paste them into bug reports.
3. Keep a *different database path* for each replica/server binding. Never reuse a local-authoritative workspace as a replica.
4. Run `orbit sync --help` and `orbit sync replica --help` to inspect the installed client contract.

## Replica inspection and connection

The replica commands use `--db` for a **replica** SQLite file and `--server` for the server URL. Authenticated actions also require `--token` and `--device` (or `ORBIT_SYNC_HTTP_TOKEN` and `ORBIT_AUTHORITY_DEVICE`). Do not print credentials in shell history.

```sh
orbit sync replica --db "$HOME/.local/state/orbit/connected-replica.db" --server https://orbit.example.invalid snapshot
orbit sync replica --db "$HOME/.local/state/orbit/connected-replica.db" --server https://orbit.example.invalid reconnect
orbit sync replica --db "$HOME/.local/state/orbit/connected-replica.db" --server https://orbit.example.invalid refresh
```

The example hostname is intentionally nonfunctional. Replace it with your own TLS-protected endpoint. Anonymous snapshot access may show cached data but server refresh/reconnect requires enrolled credentials.

## Failure and offline expectations

- Pending requests remain pending until acknowledged; failed or conflicting requests require user attention.
- Do not reissue a command with a new ID after a timeout; reconcile the durable outbox.
- Running `reconnect` refreshes accepted snapshots but does not make rejected actions successful.
- Agent launch is a privileged external effect and never runs from a disconnected client simply because it is queued.

For the deeper model, see [Offline operation](../guides/offline-operation.md) and [Troubleshooting](../guides/troubleshooting.md).
