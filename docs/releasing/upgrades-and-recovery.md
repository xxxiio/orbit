# Upgrades, database backups, and recovery

Orbit supports two independent authoritative configurations. **Local-only mode** uses local SQLite as canonical state. **Connected mode** uses server SQLite as canonical state; client SQLite is an accepted-state replica plus outbox. Choose the correct authority before backing up or restoring anything.

## Before any upgrade

1. Stop Orbit processes that may write to the authoritative database; coordinate server downtime for connected deployments.
2. Record the installed Orbit version, binary checksums, target platform, selected components and database schema compatibility notes.
3. Take a consistent SQLite backup using SQLite's online backup API (`sqlite3 ... '.backup ...'` if the CLI is available), not an unsafe copy of a live WAL-mode database. For server containers, arrange a consistent backup of the canonical server store and credentials separately.
4. Record the location of pending client outboxes; do not discard unresolved offline commands. Backing up only the server does not preserve pending edits on disconnected devices.
5. Test recovery with a **disposable copy**, confirming SQLite integrity and representative project/task reads. Keep the old binaries and verified backup until the upgraded installation is accepted.

## Install and validate

A verified binary bundle can be installed using `./scripts/install.sh --from-archive /path/to/orbit-archive.tar.gz`. Check status using `./scripts/orbit-env.sh --check`. This validates installed files, **not** database schema compatibility. Start one client or server instance and exercise core read/write paths only after confirming its database target and authority mode. For the full suite use the M15 runner in an isolated acceptance environment.

## Recovery principles

- The installer can restore prior **binaries** after an installation failure; it cannot reverse an already applied SQLite schema change.
- Never restore an older server snapshot over newer accepted authoritative history without explicit disaster-recovery planning. Clients may possess newer accepted revisions or unresolved outboxes.
- Do not treat exporting Markdown knowledge as a full SQLite database backup.
- If schema compatibility is unknown, stop and restore the verified backup into an isolated environment before proceeding.
- Never delete replica/outbox data in an attempt to resolve conflicts. Use the reviewed conflict and convergence tools.

## Release acceptance still required

Real version-to-version schema upgrades, crash recovery, multi-device convergence and full release binary tests belong in M15 acceptance. This guide does **not** claim automatic database migrations or downgrade support.

See [binary distribution](binary-distribution.md), [installation](../getting-started/installation.md), [self-hosting](../guides/self-hosting.md), and [offline sync](../guides/offline-operation.md).
