# Backup and recovery

Backing up Orbit means backing up **three things together**:

1. the authoritative SQLite database;
2. its Markdown vault, `<database>.knowledge/`;
3. its attachment object store, `attachments/objects/` beside the database.

Configuration (`~/.orbit/config.toml`) should also be kept with them. In
connected mode, the server's SQLite database is authoritative and each client
replica has its own pending-command outbox. Server-side attachment storage is not
implemented yet, so attachments remain a local-workspace concern in this build.

For the default database `~/.orbit/orbit.db`, the three local parts are:

```text
~/.orbit/orbit.db
~/.orbit/orbit.knowledge/
~/.orbit/attachments/objects/<first two hex>/<sha256>
```

There is no dedicated backup subcommand in this build. Use the procedures below
rather than inventing one.

## Automatic backup before update

`orbit update` backs up the SQLite database under `~/.orbit/backups/` before it
replaces a binary. This protects against a failed or incompatible update, but it
does **not** replace a planned backup of the vault, the attachment object store
and configuration.

`orbit update --check` never mutates anything.

## Manual local backup

Stop every process that may write to the database first. Then use SQLite's
online backup API, which produces a consistent snapshot even in WAL mode:

```sh
mkdir -p "$HOME/orbit-backups/2026-10-20"
sqlite3 "$HOME/.orbit/orbit.db" \
  ".backup '$HOME/orbit-backups/2026-10-20/orbit.db'"
```

`sqlite3` is an external tool; Orbit does not require it for normal operation.
If you do not have it, use another implementation of SQLite's backup API, or
stop all writers and copy the database **and** its `-wal`/`-shm` sidecars as a
set. Never copy only the main file while a writer is active.

Copy the vault, attachment object store and configuration separately:

```sh
cp -a "$HOME/.orbit/orbit.knowledge" "$HOME/orbit-backups/2026-10-20/"
cp -a "$HOME/.orbit/attachments" "$HOME/orbit-backups/2026-10-20/"
cp -a "$HOME/.orbit/config.toml" "$HOME/orbit-backups/2026-10-20/"
```

Attachment metadata lives in the database and content lives in
`attachments/objects/`. Copy both or the restored workspace will list
attachments whose bytes are missing. Object files are immutable and
content-addressed, so an ordinary recursive copy is a valid snapshot of the
store as long as no writer is still staging a file; stop writers first.
`ORBIT_ATTACHMENT_MAX_BYTES` changes the import limit, not the backup layout.

Verify the snapshot:

```sh
sqlite3 "$HOME/orbit-backups/2026-10-20/orbit.db" "PRAGMA integrity_check;"
```

Expected result: `ok`. Also open representative projects, tasks, notes and one
attachment from a **disposable copy** before relying on the backup.

### Attachment garbage collection

Removing an attachment association deletes its content object only when no other
attachment references it. The TUI's **Clean up unreferenced attachment content**
action performs the same sweep for objects that crashed imports or failed writes
left behind. Run it before taking a backup if you want to avoid copying orphaned
objects; it never deletes content that a metadata row still references.

### MCP backup tool

Where the optional MCP server is installed and exposes `sqlite.backup`, that
tool can create a database backup through the supported MCP surface. Use the
tool discovery response to confirm its arguments. It still does not back up the
Markdown vault or the attachment object store for you.

## Connected-mode backup

In connected mode, the server SQLite database is canonical. Arrange a
consistent server-side backup (including committed WAL data) using the
deployment's supported procedure. Separately preserve each client replica and
its unresolved outbox.

- Backing up only the server does not preserve pending edits on disconnected
  devices.
- Never delete replica/outbox data to resolve a conflict.
- Restoring an older server snapshot can lose accepted history published after
  that snapshot. Clients may hold newer revisions.
- Credentials and enrollment material are separate secrets; a database backup
  does not automatically include them.

## Recovery

1. Stop every Orbit process that can write to the target database. For a
   connected deployment, coordinate server downtime.
2. Move the current database **and** its `-wal`/`-shm` files aside while
   stopped. Do not overwrite a live database.
3. Copy the verified backup into place.
4. Restore the matching Markdown vault, attachment object store and
   configuration.
5. Run `sqlite3 DB "PRAGMA integrity_check;"` and then `orbit doctor` and
   `orbit status`.
6. Open one attached file to confirm the content object is present and its
   checksum verifies.
7. Exercise representative reads and one disposable write before resuming
   normal work.

`orbit doctor` performs a read-only integrity check and reports schema state,
but a successful doctor run is not a complete recovery test. Restore into an
isolated path first when the schema version is uncertain.

## Downgrades and migrations

- Binary rollback is **not** database schema rollback.
- This build does not automatically downgrade a database schema.
- If an older binary cannot open a newer schema, stop and restore the verified
  backup instead of editing schema tables.
- Keep the old binary and the verified backup until the new installation has
  been accepted.
- Real version-to-version upgrade and downgrade remain release gates; see
  [upgrades and recovery](../releasing/upgrades-and-recovery.md).

## What not to do

- Do not copy a live WAL-mode database with `cp` alone.
- Do not export Markdown notes and call that a database backup.
- Do not back up the database without `attachments/objects/`; metadata would
  point at missing content.
- Do not edit SQLite rows or attachment object files by hand to "repair" state.
- Do not restore a server snapshot over newer accepted history without an
  explicit disaster-recovery plan.
- Do not discard unresolved outbox commands.

## Quick checklist

```sh
# 1. Stop writers.
# 2. Snapshot database.
sqlite3 "$DB" ".backup '$BACKUP/orbit.db'"
# 3. Copy vault, attachment store and config.
cp -a "${DB}.knowledge" "$BACKUP/"
cp -a "$(dirname "$DB")/attachments" "$BACKUP/"
cp -a "$HOME/.orbit/config.toml" "$BACKUP/"
# 4. Verify.
sqlite3 "$BACKUP/orbit.db" "PRAGMA integrity_check;"
orbit --db "$BACKUP/orbit.db" status
```

Next: [switching projects and workspaces](switching-projects-and-workspaces.md)
or [troubleshooting](troubleshooting.md).
