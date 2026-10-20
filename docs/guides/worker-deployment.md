# Optional machine workers

Orbit's local CLI requires no Python. The optional operational workers under
`deploy/server/` use Python and are **not** alternative public CLIs.

| Component | Responsibility |
| --- | --- |
| `agent-machine-worker.py` | Claims authorized agent jobs, calls Herdr, monitors sessions, submits evidence and handoffs |
| `agent-worker.py` | Processes authorized domain proposal work |
| `automation-worker.py` | Executes supported automation/reporting operations |
| `worker_transport.py` | Shared authenticated HTTP transport |
| `agent_worker_journal.py` | Machine-side launch/recovery journal for external effects |

The server records commands and outcomes; it does not run an agent terminal
directly. Deploy the machine worker only on a machine you trust to execute code
and explicitly enroll it against the server. Install and configure Herdr
separately on machines that execute coding agents.

Read the worker `--help` output and `docs/m24-worker-architecture.md` before
provisioning. Put credentials in private configuration, not shell history or
committed source. Use a persistent, writable journal directory. **Never clear an
ambiguous launch claim** without reconciling the corresponding Herdr session.

## Install the optional worker files

The default `orbit` installation never installs Python or the worker:

```sh
./scripts/install-worker.sh --prefix "$HOME/.local"
./scripts/install-worker.sh --prefix "$HOME/.local" --check
```

The worker and its support modules are installed under
`~/.local/libexec/orbit/agent-machine-worker/`, not into the public `bin`
directory. Nothing starts automatically, no device is enrolled, and no tokens
are generated or embedded.

The worker needs Python 3 and a separately installed Herdr runtime on the
execution machine. `orbit update` updates the Orbit CLI; it does not update
worker files.

## Run a worker

Enroll a machine device through the authority's administrative workflow, then
provide its token securely through `ORBIT_SYNC_HTTP_TOKEN` (for example via a
service manager's secret facility):

```sh
python3 "$HOME/.local/libexec/orbit/agent-machine-worker/agent-machine-worker.py" \
  --server https://your-orbit-server.example \
  --device ENROLLED_MACHINE_ID \
  --journal-dir /path/to/persistent-private-journals --mode all
```

Use `--once` for a single polling cycle or `--mode health --once` for journal
inspection. Run continuously only under a service manager with a persistent,
private journal directory. Do not install as root merely to run agent sessions.
Do not point a worker at a production repository during acceptance testing.

## Uninstall

```sh
./scripts/install-worker.sh --prefix "$HOME/.local" --uninstall
```

This removes worker files without touching journal data or device credentials.

## Boundaries

- Worker credentials are machine secrets; never share them with client devices.
- Journals fence external side effects and are not the client outbox.
- A worker claim is not proof that a session started.
- Server acceptance is not task completion; a human still reviews evidence and
  runs `orbit task complete`.

Installer contract checks run with `./scripts/test-worker-install.sh`. Live
executor and authenticated server acceptance remain release gates. See
[self-hosting](self-hosting.md) and [offline operation](offline-operation.md).
