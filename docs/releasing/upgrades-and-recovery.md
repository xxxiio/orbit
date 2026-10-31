# Upgrades, database backups and recovery

Orbit has two independent authoritative configurations. **Local-only mode** uses
local SQLite as canonical state. **Connected mode** uses server SQLite as
canonical state; client SQLite is an accepted-state replica plus outbox. Choose
the correct authority before backing up or restoring anything.

Orbit's built-in workspace backup produces one portable, verifiable `.orbitbak`
archive covering the authoritative SQLite snapshot, the Markdown vault, the
referenced attachment objects and normalized non-secret configuration. See
[backup and recovery](../guides/backup-and-recovery.md) and
[the backup format reference](../reference/backup-format.md).

> **Status in this build.** M34 is implemented: `orbit backup
> create|list|inspect|verify|restore`, the schedule/retention policy and the
> server-side scheduler, the Web GUI Backup & Restore page, and the complete
> pre-upgrade recovery point from `orbit update`. Remaining limits: connected
> client pending data is excluded from server backups, replica snapshots are
> rejected as a recovery point, browser downloads are memory-buffered and
> capped, and external agent/git/credential boundaries are never restored.

## Before any upgrade

1. Stop Orbit processes that may write to the authoritative database. Coordinate
   server downtime for connected deployments.
2. Record the installed Orbit version, binary checksums, target platform,
   selected components and schema compatibility notes.
3. Take or confirm a **complete workspace recovery point**, not only a database
   copy. `orbit update` now creates one automatically before replacing binaries
   and refuses to proceed if it cannot; you can also take one yourself:

   ```sh
   orbit backup create --label "pre-upgrade"
   orbit backup list
   orbit backup verify ~/.orbit/backups/NEWEST.orbitbak
   ```

   On a server host, run it against the server workspace:
   `orbit backup create --db /var/lib/orbit/server.db --source-mode server --label "pre-upgrade"`.
4. Record the location of pending client outboxes. Do not discard unresolved
   offline commands. Backing up only the server does not preserve pending edits
   on disconnected devices.
5. Test recovery with a **disposable copy** — isolated restore is the intended
   path (`orbit backup restore ARCHIVE --to DIR`), or the manual fallback in the
   backup guide for an older build without the built-in commands. Keep the old
   binary and the verified backup until the upgraded installation is accepted.

## Update the CLI

```sh
orbit update --check
orbit update
```

`orbit update`:

- checks the stable channel by default (`--channel prerelease` is explicit);
- verifies release checksums before replacing anything;
- refuses an installation without the M26 manifest;
- creates a complete, verified workspace recovery point under `~/.orbit/backups/`
  before replacing binaries: the whole `.orbitbak` (database, Markdown vault,
  referenced attachments, normalized non-secret configuration), labelled
  `before-update-<from>-to-<to>`, inspected and deep-verified after publish; if
  that cannot be done, **the update fails closed and no binaries are replaced**
  (a fresh installation with no workspace is skipped because there is nothing to
  recover);
- reports the recovery point path and size, and applies the stored retention
  policy only after a verified recovery point, always protecting the new one;
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
- Restore into an isolated path first, verify it, and exercise representative
  reads and a disposable write before replacing anything.
- Replacement is a maintenance operation: stop writers, confirm the preserved
  copy and the recovery journal, and keep the previous workspace until the
  restored one is accepted.
- Do not treat exporting Markdown notes as a workspace backup, and do not copy
  only a live WAL-mode database file.
- Do not delete replica/outbox data to resolve conflicts. Use reviewed conflict
  and convergence tools.
- After a manual restore, run `sqlite3 DB "PRAGMA integrity_check;"`,
  `orbit doctor` and `orbit status`, then exercise representative reads and a
  disposable write.

## Connected deployments

Server SQLite is authoritative. Preserve:

- a consistent server backup including committed WAL data — the built-in
  archive does this with SQLite's online backup API;
- credentials and enrollment material in their own secret store (never in the
  archive);
- each client replica and its unresolved outbox and staged uploads.

A restored older server snapshot may be missing events published after the
backup. Clients can hold newer cursors or accepted revisions. The server must
fail closed on incompatible replica state and require explicit reconciliation
through the supported convergence and conflict-review workflow; do not silently
wipe client state and do not replay pending commands automatically.

## Release acceptance still required

Real version-to-version schema upgrades, downgrade behavior, crash recovery,
multi-device convergence and full release binary tests belong to M15 acceptance.
This guide does **not** claim automatic database migrations or downgrade
support. It also makes no zero-data-loss or zero-downtime claim: a backup on the
same disk is not protection against disk failure.

See [binary distribution](binary-distribution.md),
[platforms and components](platforms-and-components.md),
[installation](../getting-started/installation.md),
[backup and recovery](../guides/backup-and-recovery.md),
[self-hosting](../guides/self-hosting.md) and
[offline sync](../guides/offline-operation.md).
