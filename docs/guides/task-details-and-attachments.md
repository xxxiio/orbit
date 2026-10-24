# Task details, acceptance criteria and attachments

Orbit tasks carry more than a title and status. A task can have a
Markdown-compatible description, a priority, labels and an ordered list of
acceptance criteria. Tasks, projects, workflow items, knowledge notes and agent
evidence can also own first-class attachments.

These capabilities work in both authoritative deployments. In a local
workspace the embedded authority accepts them synchronously. In a connected
workspace the accepted snapshot carries each task's description, priority and
labels, so detail reads for a task in that snapshot work offline. A task the
snapshot does not contain is fetched from the live authoritative task-detail
route instead. Ordered acceptance criteria and per-task activity are not in the
snapshot: they are read from dedicated authenticated routes and therefore need
a server connection. Rich task writes are queued as durable commands and are
**not accepted** until a flush is acknowledged by the server. Attachments have a
connected staging model described under
[connected attachments](#connected-attachments).

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

Connected detail reads are snapshot-first: description, priority and labels come
from the accepted replica snapshot, so they work offline for any task the last
snapshot contains. If the snapshot does not contain the task, the client falls
back to the live authoritative task-detail route; when the server is unreachable
that read fails explicitly rather than inventing values. Connected writes queue
a canonical `SetDetails` command. The response's `server_state` is the last
accepted projection and the queued mutation is marked `pending` (or `conflict`),
never accepted merely because it was queued; flush the outbox
(`f` in the connected TUI, `orbit sync replica ... flush`, or
`authority.flush` over MCP) to submit it. A queued details edit never appears as
an accepted description, priority or label before the server accepts it.

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

In connected mode, `task.acceptance.list` and the TUI read the ordered criteria
from the authenticated
`GET /v1/authoritative/tasks/{id}/acceptance-criteria` route. Criteria are not
part of the accepted snapshot, so this read needs a server connection. Add, edit,
reorder, check and remove operations queue a canonical command; the returned list
is still the **accepted** list and excludes the queued change until a flush is
accepted. Activity (`H` in the TUI or `task.activity`) is likewise read from the
authoritative `GET /v1/authoritative/tasks/{id}/activity` route, newest first,
and also requires the server. Checking every criterion still never completes the
task, locally or connected.

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

An attachment can be in one of two states:

- **staged** — the bytes exist only in the local replica's staging area and a
  durable `attachment_uploads` row records the intent. A staged upload has no
  server existence, is not returned as accepted, and may still fail;
- **accepted** — the server committed the immutable metadata row and the
  content object. Only an accepted attachment can be listed or read by another
  enrolled device.

The two states are never merged. A pending upload is always presented separately
(for example `[pending]` or in a `staged` list) so a local intention is never
mistaken for authoritative content.

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

A connected replica has its own durable staging area beside the replica
database:

```text
<replica directory>/attachment-staging/<sha256>-<attachment_id>
```

Staged files are written with owner-only permissions, fsynced, then renamed into
place. The replica database records the owning `attachment_uploads` row. The
staging directory is user intent, not an accepted cache; back it up with the
replica if pending uploads matter. Accepted content on the server uses the same
`<database directory>/attachments/objects/...` layout as a local workspace.

The Markdown vault and this object store are different things. Vault assets
belong to notes and are addressed inside the vault; first-class attachments use
the object store described here.

## Attachment operations

| Operation | TUI | MCP | CLI |
|---|---|---|---|
| Add a local file | `i` (or `A` inside the attachments modal) | `attachment.add` | `orbit attachment add` |
| List an owner's attachments | `F` | `attachment.list` | `orbit attachment list` |
| Inspect metadata | `Enter` on a row | `attachment.inspect` | `orbit attachment inspect` |
| Read/export content | `E` in the modal | `attachment.read` | `orbit attachment export` |
| Open with a viewer | `o` in the modal | not exposed | not exposed |
| Remove one association | `x` in the modal | `attachment.remove` | `orbit attachment remove` |
| Associate existing content elsewhere | not exposed in the TUI | `attachment.associate` | not exposed |
| Flush staged connected uploads | `f` (flush pending commands) | not exposed | `orbit attachment flush` |
| Garbage-collect unreferenced objects | **Clean up unreferenced attachment content** action | not exposed | not exposed |

`attachment.read` returns base64 content and has an explicit byte limit; it
defaults to 1 MiB and refuses more than 4 MiB per call. Opening an attachment is
two explicit steps: export inert bytes to a new file, then hand that path to a
configured external viewer. Orbit never runs a shell, never executes attachment
content, and never imports or interprets it.

The MCP attachment tools exist only in the local registry. Connected rich task
tools are advertised, but connected attachment workflows use the CLI or TUI;
there is no connected MCP attachment tool in this build.

## Connected attachments

A connected attachment import is a two-phase operation:

1. the CLI reads the regular file, computes the SHA-256 digest and writes the
   bytes into the replica's staging directory with a durable
   `attachment_uploads` row in state `pending`;
2. it attempts one upload to the authenticated server route. Only a validated
   receipt can move the row to `accepted`.

If the attempt fails with a transport or retryable revision error, the row stays
`pending` and the command reports `accepted: false, pending: true`. If the server
returns a permanent metadata, size or receipt conflict (for example `400`,
`409`, `413` or `422`), the row becomes `conflict` and stops automatic retries
until the user removes or replaces it. A `401`/`403` is treated as an
authentication failure, not as a verdict on the content.

`orbit attachment flush` first submits the ordered command outbox and then the
staged uploads in creation order. It is idempotent: a retry reuses the same
command identity, and a duplicate server receipt never creates a second
attachment or activity row. `flush` prints `uploads_pending_before` and
`uploads_pending_after` so an operator can see whether staging was actually
accepted.

Other guarantees of the connected model:

- **Cross-client retrieval** — after acceptance, any enrolled device can list
  the attachment and download its content. Downloads are verified against the
  server's `x-orbit-checksum` and `x-orbit-size` headers, so corrupted bytes are
  refused instead of returned.
- **Removal is online-only** — removing an accepted attachment requires the
  server. Removing a staged (pending or conflicted) upload discards it locally
  without contacting the server. This build does not queue an offline removal.
- **Idempotent removal** — replaying a removal command returns the original
  accepted receipt. A fresh removal command for an attachment another client
  already removed is a successful no-op: the server route reports
  `already_removed: true` and `content_removed: false` rather than a permanent
  `404`.
- **No server-side garbage collection endpoint** — the server removes an object
  when the last association is removed, but there is no client-visible GC route
  for orphaned objects left by failed imports. Treat server attachment storage
  as server-owned.
- **Pending is never accepted** — `list` returns accepted metadata separately
  from `staged`, and human output marks staged rows `[pending]`.
- **Metadata-only ledger** — the canonical command payload and accepted history
  record identity, owner, filename, content type, size, SHA-256, object hash,
  description and activity identity. They do not embed the attachment bytes, so
  a large upload does not become a large ledger record.

Server-side imports accept a raw body only above a bounded, authenticated route.
The device is authenticated before the body is buffered, and the body is
rejected with `413` when it exceeds `ORBIT_ATTACHMENT_MAX_BYTES`. Successful
downloads set `Content-Disposition` and `X-Content-Type-Options: nosniff`.


## Content deduplication and garbage collection

Two attachments with the same bytes share one object. Removing one association
deletes the metadata row and the activity link, but deletes content only when no
other attachment references the same object. If a store deletion fails, the
object is left for a later cleanup instead of losing an association another
entity still owns.

The cleanup action removes objects that no attachment references any more. It
only considers objects the metadata layer has already marked unreferenced, so it
cannot delete live content.

The cleanup action is local-only. In connected mode the server owns attachment
garbage collection and exposes no client GC endpoint; the TUI refuses the action
with an explanation instead of pretending to clean server storage.

## Safety limits and guarantees

- **Size limit** — 64 MiB by default
  (`DEFAULT_ATTACHMENT_MAX_BYTES`). Set `ORBIT_ATTACHMENT_MAX_BYTES` to a
  positive byte count to change the local limit; the server also reads it as the
  maximum import body length and rejects a larger body with `413`.
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

## Connected authorization and limitations

Orbit connected mode is **single-tenant**. Every enrolled device shares one
authority and one set of canonical entities; there is no per-project ACL and no
per-user repository isolation. Project scope in an MCP policy or a token's
project allowlist is a least-privilege convention for what a client asks to do,
not a security boundary between devices. Issue device tokens only to parties
that may act on the whole deployment, and revoke them when that trust changes.

What is supported in this build:

- task description, priority and labels from the accepted snapshot, with a live
  task-detail fallback when the snapshot lacks the task;
- ordered acceptance criteria and per-task activity through dedicated
  authenticated read routes, plus queued canonical writes for every rich task
  mutation;
- connected attachments, including offline staging, ordered flush, idempotent
  retry, cross-client retrieval and completed `Content-Disposition`/`nosniff`
  download headers;
- task, project and planning mutations through the existing connected command
  path.

Honest limitations:

- Changing a project's workflow/work-item configuration is still
  local-authoritative; connected mode reports that clearly instead of queueing
  an unsupported mutation.
- Accepted attachment removal is online-only.
- There is no client-visible server-side attachment garbage-collection endpoint.
- Connected MCP does not expose attachment tools. Use the CLI or TUI for
  connected attachments.
- Live Herdr/model acceptance remains opt-in and is not proven by the scripted
  lifecycle tests.
- Acceptance criteria and activity are not projected into the accepted snapshot,
  so they cannot be read offline; rich task mutations stay queued and pending
  until a flush is accepted, and a snapshot that predates an accepted rich edit
  does not show it until the next refresh.

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

In connected mode the same three parts exist on the **server**: its authoritative
SQLite database, its Markdown vault and its attachment object store beside the
server database. Each client replica additionally holds its own
`attachment-staging/` directory of uploads the server has not accepted yet.
Back up the server attachment store with the server database, and preserve a
replica's staging directory with that replica when unflushed uploads matter.

## Troubleshooting

- **An attachment is too large** — check `ORBIT_ATTACHMENT_MAX_BYTES`; the
  default limit is 64 MiB.
- **A file cannot be attached** — resolve symlinks and select the real regular
  file. Directories and devices are rejected by design.
- **Content checksum mismatch** — the stored object was modified or corrupted.
  Restore the object store from backup; do not edit object files by hand.
- **Removed an attachment but disk use did not fall** — another association may
  reference the same content. Run the cleanup action to remove genuinely
  unreferenced objects. In connected mode the server owns cleanup and exposes no
  client GC endpoint.
- **A connected upload stays `[pending]`** — run `orbit attachment flush` while
  online. If it reports a conflict, remove the staged upload and import a
  corrected file. A pending upload is not accepted state.
- **A connected attachment says it is unavailable offline** — accepted metadata
  and content require the server. Staged local bytes remain listable, but
  accepted reads, export of accepted content, and accepted removal fail
  explicitly until connectivity returns.

Related: [TUI workspace](tui-workspace.md),
[workflow views](blueprint-views.md),
[MCP tools reference](../reference/mcp.md), and
[backup and recovery](backup-and-recovery.md).
