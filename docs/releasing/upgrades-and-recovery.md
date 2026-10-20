# Upgrades, database backups and recovery

Orbit has two independent authoritative configurations. **Local-only mode** uses
local SQLite as canonical state. **Connected mode** uses server SQLite as
canonical state; client SQLite is an accepted-state replica plus outbox. Choose
the correct authority before backing up or restoring anything.

## Before any upgrade

1. Stop Orbit processes that may write to the authoritative database. Coordinate
   server downtime for connected deployments.
2. Record the installed Orbit version, binary checksums, target platform,
   selected components and schema compatibility notes.
3. Take a consistent SQLite backup using SQLite's online backup API
   (`sqlite3 ... '.backup ...'` if available), not an unsafe copy of a live
   WAL-mode database. For server containers, back up the canonical server store
   and credentials separately.
4. Record the location of pending client outboxes. Do not discard unresolved
   offline commands. Backing up only the server does not preserve pending edits
   on disconnected devices.
5. Back up the Markdown vault (`<database>.knowledge/`) with the database.
6. Test recovery with a **disposable copy**. Keep the old binary and verified
   backup until the upgraded installation is accepted.

## Update the CLI

```sh
orbit update --check
orbit update
```

`orbit update`:

- checks the stable channel by default (`--channel prerelease` is explicit);
- verifies release checksums before replacing anything;
- refuses an installation without the M26 manifest;
- backs up the SQLite database under `~/.orbit/backups/`;
- refuses downgrades unless `--allow-downgrade`;
- preserves the prior binary and data if the update fails.

`--check` never mutates. If the installed build does not list `update`, use the
installer with `--version` and the same prefix.

A verified archive can also be installed directly:

```sh
./scripts/install.sh --from-archive /path/to/orbit-VERSION-TARGET.tar.gz
./scripts/orbit-env.sh --check
```

Installed-file checks do **not** establish database schema compatibility. Start
one client or server and exercise core read/write paths only after confirming
its database target and authority mode.

## Schema and binary rollback are different

- The installer can restore prior **binaries** after an installation failure.
- It cannot reverse an already applied SQLite schema change.
- This build does not automatically downgrade a database schema.
- If an older binary cannot open a newer schema, stop and restore the verified
  backup instead of editing schema tables.
- Real version-to-version schema upgrades and crash recovery remain release
  gates under M15.

## Recovery principles

- Never restore an older server snapshot over newer accepted authoritative
  history without an explicit disaster-recovery plan. Clients may hold newer
  accepted revisions or unresolved outboxes.
- Do not treat exporting Markdown notes as a full SQLite database backup.
- Do not delete replica/outbox data to resolve conflicts. Use reviewed conflict
  and convergence tools.
- Restore into an isolated path first when schema compatibility is unknown.
- After restore, run `sqlite3 DB "PRAGMA integrity_check;"`, `orbit doctor` and
  `orbit status`, then exercise representative reads and a disposable write.

## Connected deployments

Server SQLite is authoritative. Preserve:

- a consistent server database backup including committed WAL data;
- credentials and enrollment material in their own secret store;
- each client replica and its unresolved outbox.

A restored older server snapshot may be missing events published after the
backup. Clients can hold newer cursors or accepted revisions. Reconcile through
the supported convergence and conflict-review workflow; do not silently wipe
client state.

## Release acceptance still required

Real version-to-version schema upgrades, downgrade behavior, crash recovery,
multi-device convergence and full release binary tests belong to M15 acceptance.
This guide does **not** claim automatic database migrations or downgrade
support.

See [binary distribution](binary-distribution.md),
[platforms and components](platforms-and-components.md),
[installation](../getting-started/installation.md),
[backup and recovery](../guides/backup-and-recovery.md),
[self-hosting](../guides/self-hosting.md) and
[offline sync](../guides/offline-operation.md).
