# Notes and folders

Orbit stores Markdown notes in a file-authoritative vault next to the database:

```text
~/.orbit/orbit.db
~/.orbit/orbit.knowledge/
```

For a custom database path, the vault is `<database>.knowledge/`. The SQLite
database is authoritative for projects, tasks and other entities; inside the
vault, the Markdown bytes are authoritative. Do not edit the database to change
note content.

## Create a note

```sh
orbit note create acceptance.md \
  --id note-acceptance \
  --body "# API acceptance
- Typed JSON responses
- Documented error cases"
```

Output includes a command receipt, the stable note ID and the revision:

```json
{
  "command_id": "command-1792480205281-1054841-0",
  "note_id": "note-acceptance",
  "revision": 1
}
```

`--id` is required and must be stable. `--file PATH` reads the initial body
from a file instead of `--body`. A note can be created at a nested path only if
the folder already exists:

```sh
orbit folder create docs
orbit note create docs/design.md --id note-design --body "# Design"
```

The error `markdown publication failed` usually means the parent folder does
not exist, the path escapes the vault, or a file already exists at that path.

## Read and list

```sh
orbit note read note-design
orbit note list
orbit folder tree
orbit note path docs/design.md
```

`note path` reads by vault-relative path; `note read` uses the stable identity.
`note list` returns IDs and current physical paths. Paths can change without
changing identity.

## Update, move and lifecycle

```sh
orbit note update NOTE_ID --file revised.md
orbit note move NOTE_ID docs/revised.md
orbit note kind NOTE_ID decision
orbit note status NOTE_ID resolved
```

`update` checks that the saved document has not changed since it was read. If it
has, refresh and retry rather than overwriting another writer.

Kinds: `note`, `decision`, `finding`, `assumption`, `risk`, `reference`,
`concept`, `resource`, `meeting-note`, `daily-note`, `project-context`.
Statuses: `active`, `resolved`, `superseded`, `archived`.

Filter using Markdown bytes only:

```sh
orbit note catalog --kind decision --status active
orbit note search "typed JSON"
```

`note search` searches note content; the top-level `orbit search` searches
entity titles/IDs and notes.

## Link notes and reference entities

Link two notes by stable ID:

```sh
orbit note link NOTE_ID OTHER_NOTE_ID
orbit note unlink NOTE_ID OTHER_NOTE_ID
orbit note relations NOTE_ID
```

Attach a typed reference to an entity:

```sh
orbit note reference note-acceptance task:TASK_ID
orbit note reference note-api project:PROJECT_ID
orbit note reference note-readings habit:HABIT_ID
orbit note reference-backlinks task:TASK_ID
orbit note unreference note-acceptance task:TASK_ID
```

Target grammar is `project:ID`, `task:ID` or `habit:ID` for typed references.
References are explicit; link-looking text inside the Markdown body is not a
durable reference. `relations` reports note-to-note links and backlinks;
`reference-backlinks` reports notes that reference an external entity.

## Attachments

Attachments are immutable and content-addressed:

```sh
orbit note asset add NOTE_ID ./diagram.png
orbit note asset list NOTE_ID
orbit note asset verify NOTE_ID
orbit note asset unlink NOTE_ID ASSET
orbit note asset-audit
```

`asset verify` checks referenced files against their digest. Unlinking removes
the note's link while preserving shared immutable content. `asset-audit` reports
missing, corrupt and unreferenced files without deleting anything.

## Import and export

```sh
orbit note import-dir ./notes --into imported
orbit note export-dir ./notes-export
```

Import preserves stable IDs where present and into an existing vault. Export
writes a complete copy to a new directory without modifying originals. Review
the destination before running either command.

## Use notes for task acceptance criteria

There is no task-level acceptance-criteria field. A linked note is a supported
way to record them:

```sh
orbit note create acceptance.md --id note-acceptance --body "# Acceptance
- [ ] Error cases documented
- [ ] Integration test recorded"
orbit note reference note-acceptance task:TASK_ID
orbit note reference-backlinks task:TASK_ID
```

Alternatively, use a project success criterion. State which approach a project
uses so reviewers know where to look.

## Backup and limits

Back up the Markdown vault together with the SQLite database. Exporting notes is
not a database backup, and copying the database alone is not a vault backup.
See [backup and recovery](backup-and-recovery.md).

This build has no knowledge command group. The note and folder commands above
replace the retired structured-knowledge interface. Notes are Markdown files,
not rows that should be edited by hand.

## Common errors

- **`markdown publication failed`** — create the parent folder first and check
  that the path is inside the vault.
- **Note not found** — list IDs; do not use a filename as an ID.
- **Stale update** — re-read the note; another writer changed it.
- **Unresolved reference** — the target entity does not exist in this
  workspace, or the target grammar is wrong.
- **Missing attachment** — run `note asset-audit` and restore from backup.

Next: [search and navigation](search-and-navigation.md) or
[backup and recovery](backup-and-recovery.md).
