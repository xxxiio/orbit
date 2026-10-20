# Orbit 160: M18 handoff and M19 physical knowledge CLI

## Scope and priority

M18 application runtime implementation is treated as functionally scoped for handoff. Its authoritative store is private, and project/task/planning/habit/metric mutations use accepted commands. The single remaining `with_legacy_knowledge_cli` adapter is **not an M18 acceptance exception silently ignored**: it is legacy SQL-backed knowledge functionality to be retired by M19. M18 executable validation is deliberately deferred to M15 at the user's request, and therefore is not asserted as passed.

M19 is now the highest-priority feature milestone. Existing physical Markdown and folder primitives have durable SQLite intent and receipt protocols, but the CLI previously exposed only the SQL knowledge subcommands. This iteration gives users a distinct, usable physical-knowledge interface, without destroying the legacy compatibility path until a feature-complete replacement is implemented.

## Commands

Use `orbit --db workspace.db --json ...` for machine-readable output.

| Workflow | New command |
|---|---|
| Create empty physical folder | `folder create Work --command-id folder-work` |
| Display folder hierarchy | `folder tree` |
| Rename physical folder | `folder move Work Archive --command-id move-work` |
| Create note with generated YAML frontmatter | `note create Archive/plan.md --id plan --body '# Plan' --command-id note-plan` |
| Import full Markdown from a file | `note create Archive/plan.md --id plan --file content.md` |
| Inspect a note by immutable ID | `note read plan` |
| List all current Markdown note IDs and paths | `note list` |
| Search physical Markdown content | `note search planning --limit 50` |
| View backlinks and outbound references | `note relations plan` |
| Read a document by current path | `note path Archive/plan.md` |
| Update with compare-and-swap | `note update plan --file revised.md --expected-file previous.md --command-id update-plan` |
| Move by ID with expected bytes | `note move plan Work/plan.md --from Archive/plan.md --expected-file previous.md --command-id move-plan` |

For updating, the replacement file must contain the same `orbit_id` YAML frontmatter. With `--expected-file` the operation is stable across retries even after the current file has changed. Without it, the command reads the current version; use that shorthand for a one-time interactive edit, not for robust retry pipelines. Likewise, specify both `--from` and `--expected-file` for idempotent move retries. The `--command-id` is optional for one-shot CLI interactions; provide a fixed value for retryable automation.

The format of `--json` responses is an outer `{"type":"authority","result":...}` wrapper containing receipts, note objects, lists, and folder trees. Human-readable output uses the existing authority response formatting.

## Safety and existing architecture

* Markdown files and frontmatter are authoritative. The index is a disposable SQLite projection.
* Existing command handlers reject duplicate IDs and conflicting command receipts. Document updates validate stable identity and expected bytes; folder rename uses durable intent and restart replay.
* Note/folder writes pass through `AuthoritativeHost`, not an opened database store.
* The legacy `knowledge` command family remains available for compatibility during M19; its SQL-backed content must not be presented as authoritative Markdown.
* Native Rust test cases cover note creation, command retries, restarts, folder moves, updates, conflicts, search, and backlinks. They require `cargo test` when M15 verification runs.

## Next feature-oriented M19 work

Retire the SQL-backed `knowledge` command family only after mapping its resource/attachment, relation, import, and item-status workflows to file-authoritative semantics. Add attachments and resource lifecycle behind vetted vault-relative paths; do not quietly keep absolute local SQL resources as canonical knowledge. Extend markdown file/SQLite coordinated recovery where required. M20 then establishes matching native features over remote HTTP/MCP; M21 establishes replica and offline handling.

## Validation policy

Source guard and selected Python checks are useful smoke evidence, not proof of Rust compilation. Full Rust compilation, integration verification, and release readiness are assigned to M15. Do not label this archive executable-accepted or claim the new CLI has passed binary tests.

## Bulk physical import/export

`orbit --db orbit.db note import-dir /path/to/source --into Imported` recursively imports `.md` files into the Markdown vault. It preserves valid `orbit_id` YAML frontmatter, and deterministically generates one from the destination relative path when absent. It preflights duplicate IDs, existing-path conflicts, and content disagreements before writing. Destination subfolders are created through durable folder commands; each note uses a stable, content-specific command ID and a durable Markdown command receipt. Reissuing the same import is therefore safe after partial success, provided no external edits altered the imported content. A directory import is not an all-or-nothing multi-note transaction; if one note fails after others succeed, rerun the command after correcting the failure. The import explicitly rejects symlink entries and skips dot-prefixed source paths.

`orbit --db orbit.db note export-dir /path/to/new-export` writes the physical Markdown notes and their original frontmatter IDs into an entirely new directory, preserving the current folder hierarchy. It fails if the target directory exists, avoiding destructive overwrites. Export is a portable knowledge copy, **not** a consistent SQL + vault backup or remote replication protocol. For that, follow the self-hosted backup workflow. An interrupted export may leave a partial output directory that must be removed or moved aside before retrying.

Added Rust CLI-level tests cover a round-trip of plain and existing-frontmatter notes, deterministic import receipts on retry, duplicate identity rejection before publication, and export-overwrite protection. Like the other Rust tests, these require execution during M15.
