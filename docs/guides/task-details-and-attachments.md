# Task details, acceptance criteria and attachments

Orbit tasks carry more than a title and status. A task can have a
Markdown-compatible description, a priority, labels and an ordered list of
acceptance criteria. Tasks, projects, workflow items, knowledge notes and agent
evidence can also own first-class attachments.

These capabilities are **local-authoritative in this build**. Connected mode
fails closed for them instead of pretending a queued command succeeded; see
[connected-mode limitations](#connected-mode-limitations).

## Task details

| Detail | Values | Notes |
|---|---|---|
| Description | Multi-line text | Markdown-compatible; line breaks are preserved |
| Priority | `low`, `medium`, `high`, `urgent` | Optional; clearing it is explicit |
| Labels | Normalised string list | Up to 32 labels, 64 characters each |

The domain enforces a maximum description of 100,000 characters. Labels are
trimmed and de-duplicated by the task aggregate, so the stored list is the
canonical set rather than the caller's raw input. A details update that changes
nothing writes no activity record.

In the TUI, select a task and press `e` to open the task form. Description is a
multi-line field; `Enter` inserts a newline while that field is focused and
`Ctrl+S` saves the form. The same data is available through the MCP tools
`task.get` and `task.update`.

## Acceptance criteria

Acceptance criteria are a separate ordered aggregate, not a task field list.
Each criterion has:

- a stable identifier that survives editing and reordering;
- text (up to 2,000 characters);
- an optional completion timestamp.

One task can hold up to 100 criteria. Operations are explicit:

| Operation | TUI | MCP |
|---|---|---|
| List | `K` opens the criteria modal | `task.acceptance.list` |
| Add | `c` in the criteria modal | `task.acceptance.add` |
| Edit text | `r` in the criteria modal | `task.acceptance.update` |
| Reorder | `J` / `K` in the criteria modal | `task.acceptance.reorder` |
| Check / uncheck | `Space` in the criteria modal | `task.acceptance.check` |
| Remove | `x` in the criteria modal | `task.acceptance.remove` |

**Checking every criterion never completes the task implicitly.** Completion is
still an explicit task transition. `all_completed` is informational only, and a
later task status change neither reopens nor rewrites the criterion list. Each
accepted change appends a semantic activity record, and task activity is
available with `H` in the TUI or `task.activity` over MCP.

Use the criteria list for short, verifiable statements. Use a linked note or the
task description when the acceptance argument needs longer prose.

## Attachment model

An attachment is a metadata association between an owner entity and immutable
content:

- metadata (owner, display filename, content type, size, checksum, object hash
  and creation time) lives in SQLite;
- content lives in a content-addressed object store outside SQLite;
- the object name **is** the lowercase SHA-256 digest of the bytes, so identical
  content is stored once and shared by every association.

Supported owner kinds are `task`, `project`, `work_item`, `knowledge` and
`agent_job`. A closed owner list means a client cannot ask Orbit to attach to an
arbitrary table or path.

### Storage layout

For the default database `~/.orbit/orbit.db`:

```text
~/.orbit/orbit.db                          # metadata + the rest of the workspace
~/.orbit/attachments/objects/<xx>/<sha256> # immutable attachment content
~/.orbit/orbit.knowledge/                  # separate Markdown vault
```

The general rule for a database at `DB` is:

```text
<database directory>/attachments/objects/<first two hex>/<sha256>
```

An in-progress write is staged under `attachments/objects/.staging/` and then
atomically renamed into its bucket. TUI exports and external-viewer handoff use
an `exports` directory beneath the same root; the store root itself is
diagnostic-only and clients are expected to read content through the API, not by
path.

The Markdown vault and this object store are different things. Vault assets
belong to notes and are addressed inside the vault; first-class attachments use
the object store described here.

## Attachment operations

| Operation | TUI | MCP |
|---|---|---|
| Add a local file | `i` (or `A` inside the attachments modal) | `attachment.add` |
| List an owner's attachments | `F` | `attachment.list` |
| Inspect metadata | `Enter` on a row | `attachment.inspect` |
| Read/export content | `E` in the modal | `attachment.read` |
| Open with a viewer | `o` in the modal | not exposed |
| Remove one association | `x` in the modal | `attachment.remove` |
| Associate existing content elsewhere | not exposed in the TUI | `attachment.associate` |
| Garbage-collect unreferenced objects | **Clean up unreferenced attachment content** action | not exposed |

`attachment.read` returns base64 content and has an explicit byte limit; it
defaults to 1 MiB and refuses more than 4 MiB per call. Opening an attachment is
two explicit steps: export inert bytes to a new file, then hand that path to a
configured external viewer. Orbit never runs a shell, never executes attachment
content, and never imports or interprets it.

## Content deduplication and garbage collection

Two attachments with the same bytes share one object. Removing one association
deletes the metadata row and the activity link, but deletes content only when no
other attachment references the same object. If a store deletion fails, the
object is left for a later cleanup instead of losing an association another
entity still owns.

The cleanup action removes objects that no attachment references any more. It
only considers objects the metadata layer has already marked unreferenced, so it
cannot delete live content.

## Safety limits and guarantees

- **Size limit** — 64 MiB by default
  (`DEFAULT_ATTACHMENT_MAX_BYTES`). Set `ORBIT_ATTACHMENT_MAX_BYTES` to a
  positive byte count to change the local limit.
- **Display-only filenames** — a filename is validated for display and is never
  used to build a path. Slashes, backslashes, control characters, `.` and `..`
  are rejected.
- **Regular files only** — symbolic links, directories and devices are refused
  at import. Import follows no links.
- **Atomic writes** — content is written to a unique staging file, flushed, then
  atomically renamed into the content-addressed path. A crash can leave only an
  unreferenced object, never metadata whose content is missing.
- **Integrity on read** — the store re-computes the SHA-256 and checks the size
  before returning bytes. A mismatch is an error, not silently accepted content.
- **Never executed** — attachment bytes are inert. Orbit does not execute,
  import or interpret them, and viewer handoff uses a known viewer binary with
  the exported path as a single argument, never a shell.
- **No overwrite on export** — exports open the destination with create-new
  semantics, so an existing file is not replaced.
- **Last-reference deletion** — unreferenced content is deleted only after the
  last association is removed.
- **Metadata checks** — content types are normalised, descriptions are bounded
  (1,000 characters), and every change is recorded as semantic activity where
  the owner kind supports activity.

## Connected mode limitations

In this build, attachment operations, task descriptions, priority, labels and
acceptance criteria are **local-authoritative only**. Connected mode does not
queue them and does not claim success. The client returns a typed
`Unavailable` failure with a clear message, for example:

- `attachments are not available in connected mode in this build; the server does not yet accept attachment uploads`
- `connected mode does not yet support task descriptions or acceptance criteria; use the local workspace`
- `connected mode does not yet project per-task activity history; use the local workspace`
- `connected mode does not yet support changing a project workflow; use the local workspace`

Server-side attachment upload and storage are **not implemented**. Do not expect
an attachment or rich task edit to appear in the accepted server snapshot. Use a
local workspace for these workflows until connected parity is implemented.

## Backup and restore

A local workspace now has **three parts**:

1. the authoritative SQLite database;
2. the Markdown knowledge vault (`<database>.knowledge/`);
3. the attachment object store (`attachments/objects/` beside the database).

Back up all three together. A database-only backup preserves attachment metadata
but not the bytes, and an object-store-only copy has no associations. See
[backup and recovery](backup-and-recovery.md) for the snapshot and restore
procedure, including the attachment directory and
`ORBIT_ATTACHMENT_MAX_BYTES`.

## Troubleshooting

- **An attachment is too large** — check `ORBIT_ATTACHMENT_MAX_BYTES`; the
  default limit is 64 MiB.
- **A file cannot be attached** — resolve symlinks and select the real regular
  file. Directories and devices are rejected by design.
- **Content checksum mismatch** — the stored object was modified or corrupted.
  Restore the object store from backup; do not edit object files by hand.
- **Removed an attachment but disk use did not fall** — another association may
  reference the same content. Run the cleanup action to remove genuinely
  unreferenced objects.
- **Connected mode reports "not available"** — expected in this build. Use the
  local workspace.

Related: [TUI workspace](tui-workspace.md),
[workflow views](blueprint-views.md),
[MCP tools reference](../reference/mcp.md), and
[backup and recovery](backup-and-recovery.md).
