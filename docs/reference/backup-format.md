# Workspace backup format (`.orbitbak`)

Status: **M34 implemented format contract** (format version 1). This document is
normative for the archive format and manifest. It is referenced by the backup
guide, the CLI reference and the Web GUI guide.

## Purpose

One `.orbitbak` file is a complete, portable, verifiable snapshot of one
authoritative Orbit workspace:

- the authoritative SQLite database (local-only mode) or server database
  (server-authoritative mode);
- the Markdown knowledge vault that belongs to that database;
- the content-addressed attachment objects that the snapshot references;
- normalized, non-secret application configuration;
- the manifest that describes all of the above.

The archive is a recovery artifact. It is **not** a sync payload, a merge unit,
or a replacement for the authoritative command ledger.

## Container

- A POSIX **ustar** archive (uncompressed). This build writes deterministic
  headers: mode `0644`, uid/gid `0`, mtime `0`. Archives with identical content
  are byte-identical.
- Extension: `.orbitbak`.
- Size: the archive reports its own exact byte size in `manifest.json`.

## Archive layout

All paths are relative to the archive root. Absolute paths are never written.

```text
manifest.json                                  first entry
database/orbit.db                              SQLite online-backup snapshot
knowledge/<vault-relative path>.md             Markdown notes
config/config.json                             normalized non-secret config (optional)
attachments/objects/<aa>/<sha256>              referenced attachment objects
```

Empty vault directories are recorded in `manifest.directories` and recreated on
restore. No other entries are permitted.

## Manifest

`manifest.json` is the first archive entry and is a UTF-8 JSON object:

| Field | Type | Meaning |
|---|---|---|
| `format_version` | integer | Archive format version written by the producer. |
| `min_reader_version` | integer | Oldest reader that may open the archive. |
| `orbit_version` | string | Orbit package version that produced the backup. |
| `orbit_build` | string? | Optional build identifier for provenance. |
| `schema_version` | integer | SQLite schema version of the snapshot. |
| `created_at_ms` / `created_at` | integer / string | Creation time (epoch ms, RFC 3339 UTC). |
| `label` | string? | Optional operator label. |
| `complete` | boolean | Whether every claimed component is present. |
| `source.mode` | `local` \| `server` | Authority mode of the snapshot. `replica` is reserved by the format for a future client-replica snapshot; current builds reject it (see below). |
| `source.workspace_id` | string? | Stable workspace identity (`authority_identity.generation`). |
| `source.revision` | integer? | Authority revision at the snapshot instant. |
| `source.described_as` | string | Human description; never an absolute path. |
| `source.excluded_client_pending_data` | boolean | True for server snapshots (disconnected clients may hold unaccepted commands/uploads) and for any future replica snapshot; false only for a local-only authority workspace. |
| `config` | object? | `config_version`, `authority_mode`, optional `default_agent_provider`, optional `connected_server`. |
| `exclusions[]` | array | `{category, detail}` for every component deliberately not archived. |
| `inventory[]` | array | One `{path, kind, size_bytes, sha256}` per file (`kind`: `database`, `knowledge_note`, `attachment_object`, `configuration`). |
| `directories[]` | array | Vault-relative directory paths, including empty folders. |
| `totals` | object | `database_bytes`, `knowledge_bytes`, `attachment_bytes`, `config_bytes`, `content_bytes`, `archive_bytes`. |
| `counts` | object | `knowledge_notes`, `knowledge_folders`, `attachment_objects`, `attachment_associations`, and a `tables` map of SQLite row counts. |

`inventory` never includes `manifest.json` itself (a file cannot hash itself).
`totals.content_bytes` must equal the sum of inventory sizes.

## Versioning and compatibility

- A producer writes `format_version = 1`, `min_reader_version = 1`.
- A reader rejects an archive whose `format_version` is newer than it supports
  with an actionable message ("upgrade Orbit"). It likewise rejects an archive
  that requires a newer minimum reader.
- A reader rejects a snapshot whose `schema_version` is newer than the running
  build's supported schema. An older schema is opened through the existing
  documented database migration path when the restored workspace is opened.
- An older Orbit binary must never be assumed to open a newer database schema.
  Backups are the recovery path, not a downgrade mechanism.

## Path safety

- Producers write only relative, slash-separated paths.
- Readers reject absolute paths, `..` components, backslashes, NUL bytes and
  empty paths, and independently verify that a joined path stays inside the
  extraction root.
- A single archive entry may not be written twice.

## What is excluded by default

Never written: credentials, private keys, environment files, administrator keys,
device tokens, provider API keys, SSH material, external repository contents.
The database snapshot itself contains rows that Orbit stores, including
credential **hashes** for enrolled devices and session records; those are hashes
and metadata, not usable secrets, and are required for a faithful restore of the
authority. Administrator keys are environment configuration and stay out.

Also outside the backup boundary, listed in `exclusions`:

- unaccepted changes and staged uploads on disconnected connected-mode clients;
- external Herdr agent sessions and their working directories;
- running processes and operating-system state;
- Git worktrees, branches and external repositories;
- credentials held by external agents or providers;
- actions that already executed outside Orbit.

## Authority modes and connected clients

- `local`: a local-only authoritative workspace. There is no connected-client
  pending state to exclude, so `source.excluded_client_pending_data` is false.
- `server`: a self-hosted server's authoritative database. It is complete as the
  *authority* snapshot, but it cannot contain unaccepted changes, pending outbox
  commands or staged uploads held by disconnected clients. A server backup
  therefore records `source.excluded_client_pending_data: true` and a
  `client_pending_data` exclusion entry. Restoring a server snapshot never
  replays client commands; clients must be reconciled explicitly.
- Replica snapshots: a connected client's replica is **not** a restorable
  authority backup in this build. Its schema, outbox and attachment staging are
  client-local, and an older authority snapshot restored over newer client state
  is precisely the case a backup must fail closed on. Current builds reject
  `--source-mode replica` with an actionable message; keep each replica's own
  pending outbox and staging directory with that replica. The `replica` value
  remains reserved in the format so a future build can add a clearly labelled
  client snapshot without changing the container.

## Verification semantics

`orbit backup verify` checks, and fails closed on:

1. archive structure and the end-of-archive marker;
2. manifest presence, JSON validity, version compatibility and internal
   consistency;
3. every inventory path safety and uniqueness;
4. every file's size and SHA-256;
5. SQLite integrity (`PRAGMA integrity_check`) and schema compatibility;
6. presence and hash of every attachment object referenced by the snapshot
   database;
7. vault inventory consistency (recorded notes/directories exist, no
   undeclared entries).

Verification is not restoration. A verified archive can still fail to become a
working application (for example, a missing external agent session); the restore
preflight and rehearsal cover that separately.

## Verification sidecar

After a successful create-and-verify, the CLI writes
`<archive>.verify.json` beside the archive (not inside it) recording
`verified_at_ms`, `ok`, `archive_bytes`, `format_version`, `verified_files`.
`orbit backup list` shows `verified` for archives with a passing sidecar,
`failed` for a failing one, and `unknown` for copied archives without one. The
sidecar never substitutes for verifying a copy that travelled between machines.

## Restore expectations

- Isolated restore extracts into a new directory and verifies before it is
  opened; it never touches the active workspace.
- Replacement restore requires an explicit confirmation, refuses an active
  workspace, stages and verifies first, preserves the previous database, vault
  and recovery journal, and never deletes the preserved copy automatically.
- Attachment objects are immutable and content-addressed; replacement merges
  verified objects into the workspace attachment store instead of deleting
  other workspaces' objects.
- Connected clients may hold newer accepted revisions or pending commands after
  an authority restore; the server must fail closed and require explicit
  reconciliation. Pending commands are never silently replayed.

## Storage practice

A backup on the same disk is **not** protection against disk failure. Copy
verified archives to a different physical device or trusted remote location.
Orbit does not claim zero data loss or zero downtime.
