# Local trusted web quickstart

This page starts `orbit-server` and the web app on one machine without enrolling a device. It is
for a single person working on a personal or development install where both the server and the
browser run on the same host and nothing else can reach the port. If the browser connects from
another machine, or the server is reachable from a network, use the normal
[web app quickstart](web-quickstart.md) with device authentication instead.

The mode is opt-in and explicit: `ORBIT_AUTH_MODE=local-trusted`. Unset or `authenticated` means
the default device authentication described in the [web app guide](../web-app.md).

## Security warning

> **Read this before you start.** In local-trusted mode, every process and every browser on this
> machine that can reach the loopback listener is trusted. The mode does not defend against a
> malicious local process, another user session on a shared machine, or a reverse proxy that
> terminates on the same host. It is inappropriate on shared or untrusted development hosts, and
> it must never be used behind a public-facing reverse proxy. It is not a way to make a remote
> server easier to sign in to.

The server rejects a request whose `Host` is not a loopback name, which stops DNS rebinding from a
hostile page. That check is a rebinding defence, not a proxy filter: a proxy that rewrites `Host`
to the upstream address (the nginx default) bypasses it, which is one more reason not to put a
public proxy in front of this mode.

`ORBIT_SERVER_ADMIN_KEY` is still required in this mode. It still guards the administrator
endpoints listed under [What still needs a credential](#what-still-needs-a-credential), and it is
never sent to the browser.

## Prerequisites

- Linux x86_64, which is the runtime-verified scope in this repository.
- A checkout of this repository.
- Node 18.18 or newer, with dependencies already installed in `web/node_modules` (the install
  step is in [`web/README.md`](../../web/README.md)).
- A Rust toolchain.

Nothing here needs a release channel, a deployed server or a network-facing port.

## Build

Build the browser client first. The type check exits silently on success; Vite writes
`web/dist/index.html` and `web/dist/assets/`:

```sh
cd web
node node_modules/typescript/bin/tsc --noEmit
node node_modules/vite/bin/vite.js build
cd ..
```

Then build the server from the repository root:

```sh
cargo build -p orbit-server --locked
```

## Start in local trusted mode

Run this from the repository root. It is the complete command:

```bash
ORBIT_AUTH_MODE=local-trusted \
ORBIT_SERVER_BIND=127.0.0.1:8787 \
ORBIT_SERVER_ADMIN_KEY=local-development-only-not-a-secret-key \
ORBIT_SERVER_DB=./orbit-local.db \
cargo run -p orbit-server
```

`ORBIT_SERVER_ADMIN_KEY` above is a clearly non-secret placeholder, valid only because this
listener is reachable only from this machine. It is 39 characters, so it satisfies the
32-character minimum. Replace it with a real random value — for example
`openssl rand -hex 32` — if this server is ever made reachable beyond loopback, and remember that
local-trusted mode itself is refused on a non-loopback bind.

`ORBIT_SERVER_BIND` must be a literal loopback address: `127.0.0.0/8` (such as
`127.0.0.1:8787`) or `[::1]` (such as `[::1]:8787`). Startup is refused, before the database is
opened, for `0.0.0.0`, `[::]`, LAN addresses, IPv4-mapped IPv6 such as `[::ffff:127.0.0.1]`,
hostnames such as `localhost`, and any other non-loopback value.

## Expected startup output

A successful start prints exactly these three lines and then serves the app:

```text
Orbit server listening on http://127.0.0.1:8787
Authentication: Local trusted mode
Access: Local machine only
```

The address in the first line is the resolved listen address; for the command above it is the
same `127.0.0.1:8787` you passed. If the second line says
`Device authentication` or the third says `Access: Enrolled devices only`, the mode was not
applied; check the spelling of `ORBIT_AUTH_MODE`.

Check the process independently in a second terminal:

```sh
curl -sS http://127.0.0.1:8787/health
{"status":"ok"}
```

## First visit

Open `http://127.0.0.1:8787/` in a browser on the same machine. The workspace opens directly:

- There is no Device ID, no device token, no **Sign in to Orbit** form and no session cookie.
- Reloading the page or restarting the server never prompts. The mode is read from the server
  process, not from browser state.
- Existing projects and tasks in `ORBIT_SERVER_DB`, and notes in its vault, stay visible. Turning
  the mode on does not copy, reset or migrate data.

The mode indicator in the sidebar replaces the device identity with a single **Local trusted
mode** note and hides the sign-out control, because there is no session to end. The startup
banner above remains the three-line indicator of the running policy.

Browser mutations still send the `x-orbit-csrf: 1` header, and the server still rejects
cross-site requests. Forwarded headers (`X-Forwarded-For`, `X-Real-IP`) never grant access, and
the server does not treat the request's source address as authorization. The trust decision comes
from the startup policy, not from where a request appears to come from.

## What still needs a credential

Local-trusted removes the browser sign-in, not the server's privileged surfaces. The admin key is
still required for:

- Device enrollment, revocation and token rotation under `/v1/devices`.
- Admin proposals and automations.

Two more surfaces keep their own enrolled-device credentials and are not reachable from the
local-trusted browser principal:

- Capability install, authorization and revocation require an enrolled operator device sending
  `Authorization: Bearer <device-token>` plus `x-orbit-device-id`.
- Agent executor enrollment and capability claim/result require an enrolled executor device.
- Automation runs and agent proposals are claimed and settled by worker credentials, not by the
  browser: `/v1/automation-runs/claim`, `/v1/automation-runs/{id}/settle`,
  `/v1/agent-proposals/claim` and `/v1/agent-proposals/{id}/settle` all refuse the local principal.

`/mcp` is a separate case: with no credential it returns `403`. With the admin key it can inspect
tools and state, but write tools still require an enrolled device, so a local MCP client is
read-only in this mode. Use an enrolled device credential for MCP mutations.

If you switch an existing database from device authentication to local-trusted, agent jobs that a
device started earlier stay owned by that device and are not listed for the local principal. This
is the existing per-device ownership rule, not data loss: the jobs return when you sign back in as
that device, and jobs started in local-trusted mode are owned by the local principal.

The browser cannot reach any of those endpoints in this mode. Device enrollment returns `403`
(`administrator credential required`) without the admin key; capability install/authorize/revoke
and executor enrollment return `403` as well, each naming the credential it needs. Device clients
still authenticate with their device ID and token. Agent executors still enroll and
claim work as devices. Use `curl` with the admin key from the shell you started the server in:

```sh
curl -sS -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:8787/v1/devices \
  -H "Authorization: Bearer $ORBIT_SERVER_ADMIN_KEY" \
  -H 'Content-Type: application/json' \
  -d "{\"device_id\":\"browser-laptop\",\"token\":\"$(openssl rand -hex 32)\"}"
201
```

## Back up before you experiment

A personal loopback workspace is still the only copy of its projects, notes and
attachments. Before you experiment with modes, upgrades or agent work, take one
verified workspace backup (database, Markdown vault, attachment objects and
normalized non-secret configuration in a single `.orbitbak` archive):

```sh
# Label it, and keep it outside the workspace directory you are testing.
orbit backup create --db ./orbit-local.db --output ./orbit-backups --label "before experiments"
orbit backup list --output ./orbit-backups
orbit backup verify ./orbit-backups/NEWEST.orbitbak
orbit backup inspect ./orbit-backups/NEWEST.orbitbak
```

`orbit backup create` takes a consistent online snapshot, verifies the archive
before publishing it and never modifies the workspace. `verify` re-checks the
archive structure, manifest, per-file hashes and SQLite integrity; verification
is **not** a restore test.

To rehearse recovery, restore into an isolated directory and open it there — the
active workspace is never touched:

```sh
orbit backup restore ./orbit-backups/NEWEST.orbitbak --dry-run --to ./orbit-restored
orbit backup restore ./orbit-backups/NEWEST.orbitbak --to ./orbit-restored
orbit --db ./orbit-restored/orbit.db status
```

Confirm the projects, tasks, notes and one attachment you expect, then discard
the rehearsal copy.

The same workflow is available in the browser under **Settings → Backup &
Restore** (`/settings/backup`, also in the sidebar footer and the command
palette as **Go to Backup & Restore**):

1. Check **Last verified backup** and **Server backup folder** (Writable or Not
   writable). The page warns when no verified backup exists yet.
2. In **Create a backup**, type an optional label and press **Create backup**; it
   shows **Creating and verifying…**, then **Verification passed**.
3. Use **Backup history** → **Inspect** (the **Backup details** drawer),
   **Verify** and **Download**. A skipped-archive panel names files that could
   not be read. Downloads are memory-buffered and capped (default 512 MiB); a
   larger archive gets `413`, so copy very large archives from the server's
   backup folder.
4. In **Restore**, choose **Isolated copy** (Recommended), type `RESTORE` and
   press **Create isolated copy**. The copy is verified and created under the
   server's restore root; the active workspace is not modified. **Replace
   authority** is maintenance-only: it takes the administrator key and only
   prepares an offline replacement plan — the live workspace is never replaced
   from the browser.

For the full screen contract see [the web app guide](../web-app.md#backup-and-restore).

> **A backup on the same disk is not protection against disk failure.** Point
> `--output` at a different physical device or copy verified archives to one.
> Orbit makes no zero-data-loss or zero-downtime claim.

Do not copy a live WAL-mode SQLite file with `cp` as a backup: use the built-in
backup (or SQLite's online backup API). The `-wal`/`-shm` sidecars are only part
of a valid copy when every writer is stopped and the files are copied as a set.

## Stop and where data lives

Stop the server with `Ctrl+C`. With the command above, state lives beside your working directory:

- `./orbit-local.db` and its SQLite `-wal`/`-shm` sidecars.
- `./orbit-local.knowledge/`, the Markdown vault.
- `./attachments/objects/`, the attachment object store.

Deleting those paths starts an empty workspace. That is destructive and is not an upgrade path;
take a verified workspace backup first if the data matters. See
[backup and recovery](backup-and-recovery.md).

## Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| Startup exits with a message that the bind is not loopback | `ORBIT_SERVER_BIND` was `0.0.0.0`, `[::]`, a LAN address, an IPv4-mapped IPv6 address, `localhost`, or another non-loopback value. | Use `ORBIT_SERVER_BIND=127.0.0.1:8787` or `[::1]:8787`. Local-trusted mode refuses every other bind before opening the database. |
| `Address already in use` when starting | Another process holds the port. | Stop that process, or pick another loopback port and use it in the URL and `ORBIT_SERVER_BIND`. |
| The page is blank, or returns `503 Orbit web app is not built` | `web/dist` is missing or unreadable, usually because the client was not built or the server was started outside the repository root. | Run the build steps again, start the server from the repository root, or set `ORBIT_WEB_DIST` to an absolute build path. |
| `403 csrf check failed` | A scripted client sent a mutation without the CSRF header. | Send `x-orbit-csrf: 1` and do not send `Sec-Fetch-Site: cross-site`. Browsers do this automatically. |
| `403` from an admin route, with `administrator credential required` | Device enrollment, revocation, rotation, proposals or automations still require the admin key. | Send `Authorization: Bearer $ORBIT_SERVER_ADMIN_KEY`; the browser does not hold this credential in local-trusted mode. |
| `403` naming an operator, executor or enrolled-device credential | Capability install/authorize/revoke, executor enrollment, or a machine transport was called from the browser. | Use an enrolled device credential from the shell; the local browser principal cannot perform those operations. |

## Switch back to authenticated mode

Stop the server, then either unset `ORBIT_AUTH_MODE` or set it to `authenticated`:

```bash
unset ORBIT_AUTH_MODE
# or: export ORBIT_AUTH_MODE=authenticated
```

Restart the server with the same database. The startup banner now reports
`Authentication: Device authentication` and `Access: Enrolled devices only`, and the browser shows
the normal sign-in form. Local-trusted clears any `orbit_session` cookie it is shown and revokes
that session server-side, so switching back always requires a normal sign-in. Sign in again with an
enrolled device ID and token; if no device is enrolled, enroll one with the admin key as described
in the [web app guide](../web-app.md#enroll-a-browser-device).

Replace the loopback placeholder admin key at the same time. Once the server accepts device
authentication again, that key can enroll and revoke devices, so it must be a real random value:
`ORBIT_SERVER_ADMIN_KEY="$(openssl rand -hex 32)"`.

For the product itself — every screen, shortcut and refusal — see
[the web app guide](../web-app.md). For the authenticated walkthrough, see the
[web app quickstart](web-quickstart.md).
