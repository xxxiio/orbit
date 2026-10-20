# Optional machine workers

Orbit's local CLI requires no Python. The server's optional operational workers currently use Python and are shipped under `deploy/server/`. They are **not** alternative public CLIs.

| Component | Responsibility |
| --- | --- |
| `agent-machine-worker.py` | Claims authorized agent jobs, calls Herdr, monitors sessions, submits evidence and handoffs |
| `agent-worker.py` | Processes authorized domain proposal work |
| `automation-worker.py` | Executes supported automation/reporting operations |
| `worker_transport.py` | Shared authenticated HTTP transport |
| `agent_worker_journal.py` | Machine-side launch/recovery journal for external effects |

The server accepts commands and records outcomes; it does not run the agent terminal directly. Deploy the machine worker only on machines you trust to execute code and explicitly enroll against the server's device-authentication scheme. Install and configure Herdr separately on machines that will execute coding agents.

Read the worker `--help` output and `docs/m24-worker-architecture.md` before provisioning a worker. Put machine credentials in private configuration, not shell history or committed source. Use a persistent, writable journal location, and **never clear ambiguous launch claims** without reconciling the corresponding Herdr session.

For developers maintaining the workers, run the Python contract suite under `deploy/server/`. Release-wide execution and integration checks belong to the M15 automated runner.

## Install the optional worker files

The source package includes a **separate opt-in installer**. The default `orbit`
installation never installs Python or the worker:

```bash
./scripts/install-worker.sh --prefix "$HOME/.local"
./scripts/install-worker.sh --prefix "$HOME/.local" --check
```

The worker and its three Python support modules are installed under
`~/.local/libexec/orbit/agent-machine-worker/`, **not** into the public `bin`
directory. Nothing starts automatically, no device is enrolled, and no tokens
are generated or embedded. The worker needs Python 3 and a separately installed
Herdr runtime on the execution machine.

To configure operation, enroll a machine device with the authority using the
supported Orbit administrative workflow. Provide its token securely through
`ORBIT_SYNC_HTTP_TOKEN` (for example through your service manager's secret
facility), then invoke:

```bash
python3 "$HOME/.local/libexec/orbit/agent-machine-worker/agent-machine-worker.py" \
  --server https://your-orbit-server.example \
  --device ENROLLED_MACHINE_ID \
  --journal-dir /path/to/persistent-private-journals --mode all
```

Use `--once` for a single polling cycle or `--mode health --once` for journal
inspection. Run continuously only under a service manager with a persistent,
private journal directory. **Do not** install as root merely to run agent
sessions. Do not point it at a production repository during acceptance testing.

Uninstall the optional worker files without touching journal data or device
credentials:

```bash
./scripts/install-worker.sh --prefix "$HOME/.local" --uninstall
```

Installer contract checks: `./scripts/test-worker-install.sh`. Full live
executor and authenticated server acceptance remain part of M15.
