# Backup and recovery

One Orbit workspace is **three things together**:

1. the authoritative SQLite database;
2. its Markdown knowledge vault, `<database>.knowledge/`;
3. its content-addressed attachment object store, `attachments/objects/` beside
   the database,

plus the normalized non-secret configuration.

Orbit's built-in backup collects all of them into one portable, verifiable
`.orbitbak` archive and restores that archive into a new location or, behind an
explicit maintenance step, over an existing workspace. The archive format is
specified in [the workspace backup format reference](../reference/backup-format.md).

> **Status in this build.** M34 backup and restore are implemented: the
> `.orbitbak` format, CLI `create`/`list`/`inspect`/`verify`/`restore`, the
> schedule/retention policy, the server backup/restore API and scheduled
> automatic backups, the **Settings → Backup & Restore** page, and the complete
> `orbit update` pre-upgrade recovery point. Remaining limits are stated below:
> connected-client pending data is excluded from server backups, client replica
> snapshots are rejected as a recovery point, browser downloads are
> memory-buffered and capped, replacement is an offline maintenance operation,
> and external Herdr/git/credential boundaries are never restored.

## What a backup contains

- **SQLite snapshot** — every authoritative row: projects, tasks, milestones,
  acceptance criteria, dependencies, workflow definitions and project bindings,
  Work Items, installed capability metadata, capability authorization/execution
  receipts, agent jobs, handoffs, reviews, activity and event records. Records
  created through the MCP surface are included because they live in the same
  authoritative database.
- **Markdown vault** — the notes and folders that belong to the database,
  including empty directories.
- **Referenced attachment objects** — the content-addressed bytes that the
  snapshot's attachment metadata points at. The manifest records every object by
  relative path, size and SHA-256.
- **Normalized non-secret configuration** — a small allow-listed projection
  (config version, authority mode, optional default agent provider, optional
  connected server), omitted with `--no-config`.
- **A manifest** — format and schema versions, creation time, label, component
  inventory, totals, record counts and the explicit exclusion list.

## What is never included

The archive never contains credentials, private keys, environment files,
administrator keys, device tokens, session cookies, provider API keys, SSH
material or the contents of external repositories. The database snapshot does
contain the credential **hashes** and session metadata Orbit itself stores;
those are required to restore a faithful authority and are not usable secrets.

Also outside the backup boundary, and listed in the manifest's `exclusions`:

- unaccepted changes, pending outbox commands and staged attachment uploads on
  disconnected connected-mode clients;
- external Herdr agent sessions and their working directories;
- running processes and operating-system state;
- Git worktrees, branches and other repositories;
- credentials held by external agents or providers;
- actions that already executed outside Orbit.

A restore does not bring those back. In particular, an external agent session
that existed before a backup is not running after a restore, and Orbit never
re-executes an external agent or capability action automatically.

## Create a backup

```sh
# Default destination: the configured Orbit backups directory (~/.orbit/backups).
orbit backup create --label "before agent testing"

# Explicit destination, e.g. a different physical disk.
orbit backup create --output /mnt/orbit-backups --label "weekly"

# A server workspace, run on the server host.
orbit backup create --db /var/lib/orbit/server.db --source-mode server --label "pre-upgrade"
```

Client replica snapshots are **not** supported as a recovery point in this
build. `--source-mode replica` fails closed with an actionable message: back up
the server workspace on the server host (`--source-mode server`) and keep each
replica's own pending outbox/staging directory with that replica. A replica is
not an authority snapshot, and an older authority archive restored over newer
client state is the dangerous case this build refuses to pretend is safe.

| Flag | Effect |
| --- | --- |
| `--output DIR` | Destination directory; created when missing. Defaults to the Orbit backups directory (`~/.orbit/backups`). A home-less process with an explicit `--db` uses `<workspace>/backups`. |
| `--label TEXT` | Human label stored in the manifest and the archive file name (at most 200 characters). |
| `--source-mode local\|server` | Declares which authority produced the snapshot; default `local`. A connected client cannot snapshot the server database remotely. `replica` is reserved and rejected in this build (see above). |
| `--no-config` | Omit the normalized non-secret configuration projection. |

Creation takes a consistent snapshot in a bounded writer window, writes a
staging file, verifies the assembled archive, and only then publishes it
atomically under a name that never overwrites an existing archive. Progress is
printed on stderr in human mode and suppressed in JSON mode. A failed creation
does not replace a previous archive and never mutates the source workspace.

A `--source-mode server` archive records `source.excluded_client_pending_data:
true` in its manifest plus a `client_pending_data` exclusion entry, because
unaccepted client commands and staged uploads are not in the server store. A
local-mode archive records `false`.

Exit status: `0` on success; `2` for a usage/argument error; `4` (`backup_error`)
for a backup, verification or recovery failure. Use `--json` for a stable
tagged response.

## List, inspect and verify

```sh
orbit backup list
orbit backup list --output /mnt/orbit-backups --limit 20
orbit backup inspect ~/.orbit/backups/orbit-20261031T090000Z-before-agent-testing.orbitbak
orbit backup verify ~/.orbit/backups/orbit-20261031T090000Z-before-agent-testing.orbitbak
orbit backup verify ARCHIVE --structural-only
```

`list` scans the default or `--output` directory, newest first, and reports the
creation time, format version, size, verification state and label. The
verification state comes from the `<archive>.verify.json` sidecar that a
successful create-and-verify writes beside the archive: `verified` for a passing
sidecar, `FAILED` for a failing one, and `unknown` for an archive copied from
elsewhere without a sidecar. A sidecar never substitutes for verifying a copy
that travelled between machines.

`verify` checks, and exits nonzero on any failure:

1. archive structure and end-of-archive marker;
2. manifest presence, JSON validity, version compatibility and internal
   consistency;
3. every inventory path is safe and unique;
4. every file's recorded size and SHA-256;
5. SQLite integrity, schema compatibility and referenced attachment presence;
6. vault inventory consistency (recorded notes and directories exist, no
   undeclared entries).

`--structural-only` skips the deep SQLite integrity/schema extraction check. It
is a speed trade-off, not a weaker safety claim: use a full verify before
depending on an archive.

> **Verification is not restoration.** A verified archive can still fail to
> become a working application — for example, when an external dependency the
> workspace referenced no longer exists. Verification proves the archive is
> intact and internally consistent; the restore preflight and a real rehearsal
> prove the workspace opens.

## Restore

Restore has two deliberately different paths. Start with the isolated path.

### Isolated restore (recommended first step)

```sh
orbit backup restore ARCHIVE --dry-run --to /tmp/orbit-restored
orbit backup restore ARCHIVE --to /tmp/orbit-restored
orbit --db /tmp/orbit-restored/orbit.db status
```

`--dry-run` prints the plan — archive and source mode, schema compatibility,
target paths, downtime, replaced objects, preservation plan, connected-client
warnings and non-restorable external dependencies — and changes nothing. A
refused preflight exits nonzero. The real restore extracts into the new
directory, verifies before it is considered usable, and never touches the
active workspace. Open the restored workspace with `orbit --db …` and check
representative projects, Milestones, tasks, notes, attachments and agent records
before switching anything.

### Replacing an existing workspace (maintenance only)

Replacement is destructive and is not a live operation. Stop every Orbit process
that writes to the workspace (coordinate server downtime), confirm the archive
is verified, and then:

```sh
orbit backup restore ARCHIVE --dry-run --replace
orbit backup restore ARCHIVE --replace --confirm REPLACE
```

`--confirm` accepts `REPLACE` or the exact target database path. Replacement
refuses an active workspace and a live-database overwrite, stages and verifies
the restored workspace before switching, preserves the previous copy (database,
vault and recovery journal) for rollback, and never deletes the preserved copy
automatically. The CLI prints the preserved path and the rollback instruction.

On a connected deployment the server database is canonical: a connected client
replica cannot be replaced from an authority backup. Restore on the server host
and re-enroll or refresh clients instead.

### Connected clients after a restore

A server backup does not contain pending commands or staged uploads held by
disconnected clients. Restoring an older server snapshot can therefore lose
accepted history published after that snapshot, while clients may still hold
newer revisions or unresolved outbox commands. The server must fail closed on
incompatible replica state and require explicit reconciliation; pending
commands are never silently replayed. Do not delete replica or outbox data to
make a warning disappear.

A connected replica owns its own accepted snapshot, its unresolved outbox and
its `attachment-staging/` directory for **unflushed uploads** the server has not
accepted. Replica snapshots are not an accepted recovery point in this build:
back up the **server attachment object store** and database on the server host,
and preserve each replica (its accepted snapshot, unresolved outbox and staging
directory) with that replica. A database restored without the object store
lists attachments whose bytes are missing; an object store restored without the
database lists no associations. Credentials and enrollment material are separate
secrets and are never in either backup.

## Web GUI: Settings → Backup & Restore

The page is at `/settings/backup`, reachable from the sidebar footer and the
command palette (**Go to Backup & Restore**). It uses the same Rust service and
authorization architecture as the CLI; see
[the web app guide](../web-app.md#backup-and-restore) for the screen-by-screen
contract.

- **Last verified backup**, **Server backup folder** (writable/not writable),
  **Automatic backups** policy editor, **Create a backup** with an optional
  label, **Backup history** (Created / Label / Size / Format / Verification with
  Inspect, Verify and Download), an inspect drawer, and a **Restore** panel.
- Restore offers **Isolated copy** (recommended; type `RESTORE`, then **Create
  isolated copy**) or **Replace authority** (maintenance; administrator key,
  **Prepare replacement plan**). Replacement only produces an offline
  maintenance plan — the browser never replaces the live database.
- The browser never supplies a server filesystem path: it works with backup ids
  that the server resolves inside `ORBIT_BACKUP_DIR`, and product responses do
  not leak absolute server paths. Ordinary product authorization covers
  list/create/inspect/verify/download/schedule and isolated restore; only the
  replacement plan requires the administrator credential (local-trusted without
  an admin bearer gets `403`).
- Downloads are memory-buffered and capped by
  `ORBIT_BACKUP_DOWNLOAD_MAX_BYTES` (default 512 MiB): a larger archive answers
  `413 Archive too large for a browser download`, and large archives should be
  copied from the server's backup folder instead. Expect states for backup in
  progress (`409`), verification failed, incompatible schema, insufficient
  space, and recovery failed with the original workspace preserved.

## Automatic backups and retention

Automatic backups are optional and configured in `policy.json` inside the
backup directory, so the CLI and a server that shares the directory read one
policy file:

```sh
orbit backup schedule show
orbit backup schedule set --mode daily --max-retained 14 --min-retention-days 7
orbit backup schedule set --mode weekly --max-total-bytes 5000000000
orbit backup retention apply --dry-run
orbit backup retention apply
```

- **Schedule** — `disabled` (the default), `daily`, `weekly` or
  `before-upgrade`. The long-running `orbit-server` process runs the scheduler:
  it re-reads `<backup directory>/policy.json` on every tick (every 15 minutes,
  first check about 30 seconds after startup), so a CLI or Web GUI policy change
  takes effect without a restart. It creates a verified
  `orbit-<stamp>-automatic-<schedule>.orbitbak` when a `daily`/`weekly` backup is
  due (a daily backup is not duplicated within the same UTC day), then applies
  retention while protecting the new archive. `disabled` and `before-upgrade`
  never run on a timer; `before-upgrade` is covered by `orbit update`. A
  local-only CLI installation without a running server has no background
  scheduler, so run `orbit backup create` manually (or from your own timer) if
  you do not run a server.
- **Retention** — `max_retained`, `min_retention_days` and optional
  `max_total_bytes`. Cleanup runs only after a verified creation and never
  removes the newest verified recovery point or a user-managed external copy.
  `retention apply --dry-run` reports what would be removed without deleting
  anything.

A failed automatic backup is logged and never affects the live workspace; if a
workspace has a missing attachment object the automatic backup fails closed
rather than publishing an incomplete archive. A backup on the same disk is not
protection against disk failure: configure the destination on a different
physical device (or copy verified archives there) if you care about the data.

## Before you upgrade

`orbit update` creates a **complete, verified workspace recovery point** — the
whole `.orbitbak` (database, vault, referenced attachments and normalized
configuration), labelled `before-update-<from>-to-<to>` — before it replaces
binaries. It is a hard gate: if the recovery point cannot be created, verified,
inspected or deep-verified, the update is refused and no binaries are replaced.
With no workspace database present (a fresh installation) there is nothing to
recover and the step is skipped. `orbit update --check` remains read-only, and
retention runs after verification while protecting the new archive.

```sh
orbit update --check
orbit update
```

The update output names the recovery point path and size, confirms it was
verified before binaries were replaced, and reports whether the stored retention
policy removed older backups (the new recovery point is always protected). To
check the recovery point yourself, use `orbit backup list` and
`orbit backup verify`. See
[upgrades and recovery](../releasing/upgrades-and-recovery.md) for the update
and rollback rules.

## Manual fallback (offline)

Use this only when the built-in commands are unavailable (for example, on an
older build), or to take a copy with external tooling you already trust. It is a
last resort: it has no manifest, no cross-store consistency guarantee beyond
stopping writers, and no automatic verification that the three parts match.

Stop every process that may write to the database first. Then use SQLite's
online backup API, which produces a consistent snapshot even in WAL mode:

```sh
BACKUP="$HOME/orbit-backups/$(date +%Y-%m-%d)"
mkdir -p "$BACKUP"
sqlite3 "$HOME/.orbit/orbit.db" ".backup '$BACKUP/orbit.db'"
cp -a "$HOME/.orbit/orbit.knowledge" "$BACKUP/"
cp -a "$HOME/.orbit/attachments" "$BACKUP/"
cp -a "$HOME/.orbit/config.toml" "$BACKUP/"
sqlite3 "$BACKUP/orbit.db" "PRAGMA integrity_check;"
```

`sqlite3` is an external tool; Orbit does not require it for normal operation.
**Never copy only the main database file while a writer is active.** If a
SQLite backup API is unavailable, stop all writers and copy the database file
**and** its `-wal`/`-shm` sidecars as a set. That is the only case where the
sidecars are part of a valid manual copy; the built-in backup avoids the
problem entirely by using the online backup API and never copying a live file.

Attachment metadata lives in the database and content lives in
`attachments/objects/`; copy both or the restored workspace lists attachments
whose bytes are missing. Object files are immutable and content-addressed, so a
recursive copy is a valid store snapshot as long as no writer is still staging a
file. `ORBIT_ATTACHMENT_MAX_BYTES` changes the import limit, not the layout.

Restore a manual copy into an **isolated** path first, verify it with
`sqlite3 … "PRAGMA integrity_check;"`, `orbit doctor` and `orbit status`, open
one attachment, and exercise representative reads and one disposable write
before moving anything into place.

### Attachment garbage collection

Removing an attachment association deletes its content object only when no other
attachment references it. The TUI's **Clean up unreferenced attachment content**
action performs the same sweep for objects that crashed imports or failed writes
left behind. It never deletes content a metadata row still references.

### MCP note

The MCP server in this tree does not expose a `sqlite.backup` tool. MCP-created
records are still covered by a workspace backup because they live in the
authoritative SQLite snapshot; a workspace backup does not need MCP cooperation.

## Downgrades and migrations

- Binary rollback is **not** database schema rollback.
- This build does not automatically downgrade a database schema.
- If an older binary cannot open a newer schema, stop and restore the verified
  backup instead of editing schema tables.
- A backup is the recovery path, not a downgrade mechanism.
- Keep the old binary and the verified backup until the new installation has
  been accepted.

## What not to do

- Do not copy a live WAL-mode database with `cp` alone.
- Do not treat verification as restoration, or a restore plan as a rehearsal.
- Do not export Markdown notes and call that a workspace backup.
- Do not back up the database without `attachments/objects/`; metadata would
  point at missing content.
- Do not edit SQLite rows or attachment object files by hand to "repair" state.
- Do not restore a server snapshot over newer accepted history without an
  explicit disaster-recovery plan.
- Do not discard unresolved outbox commands.
- Do not assume external Herdr sessions, processes, Git worktrees, repositories
  or credentials come back with a workspace.


## What is verified in this build

M34 acceptance recorded: `cargo test -p orbit-backup --offline` 123 passed;
`cargo test -p orbit-cli --offline` 164 passed; `cargo test -p orbit-server
--offline` 95 passed; web type-check clean, Vitest 490 passed, terminology 286
files, production build clean; Playwright `backup-restore.spec.ts` 4/4. A real
recovery rehearsal passed 14/14 checks on a deterministic dataset (3 projects,
3 milestones, 103 tasks, 206 acceptance criteria, 6 Work Items, 6 notes, 5
attachments, reserved agent jobs, capability install/grant/real execution
receipt, an MCP-authored task update and 124 activity rows; all 64 database
tables equal before/after, archive SHA-256 recorded), and the CLI
failure-injection matrix passed 21 checks with 1 not applicable, with the
original workspace recoverable in every case. Manual live-server backup also
succeeded and a project created through the server API appeared in the restored
copy. Evidence: `web/evidence/m34-recovery/`.

## Storage practice

- Keep copies on a different physical device or a trusted remote location.
  **A backup on the same disk is not protection against disk failure.**
- Verify before you rely on an archive, and re-verify a copy after moving it
  between machines; the sidecar is not transferred proof.
- Keep the newest verified recovery point for each deployment, not only the
  scheduled rotation.
- Orbit makes no zero-data-loss or zero-downtime claim. Restoring a snapshot
  always loses work accepted after the snapshot instant.

Next: [switching projects and workspaces](switching-projects-and-workspaces.md),
[troubleshooting](troubleshooting.md), or
[upgrades and recovery](../releasing/upgrades-and-recovery.md).
