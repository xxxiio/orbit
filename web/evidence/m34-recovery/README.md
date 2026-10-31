# M34 — real recovery rehearsal and failure-injection evidence

Evidence for MILESTONE-0034 exit criteria **EC-013** (real recovery rehearsal) and **EC-014** (failure injection), produced by `tests/acceptance/m34_recovery_rehearsal.py` and `tests/acceptance/m34_backup_cli.py`. Everything below was executed against the real built binaries on real SQLite files; no row was fabricated with SQL and no external agent was launched.

## Rehearsal

### Build identity (ties this evidence to one tree)

- Source tree: commit `f82e3fa13d4f1fe4`, branch `main`, dirty `True` (74 changed path(s))
- CLI: `/home/xliu/dev/orbit/target/debug/orbit` 217098400 bytes, mtime `2026-10-31T10:06:41+00:00`, sha256 `ad31c0360b1dc16ee946caf996ae2f88bcd6a967245d0035f332f267b5a83c80`, version `orbit 0.1.0`
- Server: `/home/xliu/dev/orbit/target/debug/orbit-server` 182233640 bytes, mtime `2026-10-31T10:06:40+00:00`, sha256 `9bbf52de263fc1cbcc149bddf8f082df0f54954a89c4d86bfd96f90bb0db92f8`
- MCP: `/home/xliu/dev/orbit/target/debug/orbit-mcp` 180374616 bytes, mtime `2026-10-27T19:59:57+00:00`, sha256 `3c805e95202ad2ff84637d8483e5d78646081bcd3c29ec31189eed5c37d15104`

- Result: **✅ PASS**
- Generated: `2026-10-31T10:36:59.511+00:00`
- Orbit version: `orbit 0.1.0`
- Host: `Linux 6.6.87.1-microsoft-standard-WSL2 x86_64`
- Authority: real `orbit-server` in `local-trusted` mode on `127.0.0.1:34405`
- Isolated restore instance: real `orbit-server` on the restored database, plus CLI reads via `orbit --db <restored> status`

### Archive

- Archive: `/home/xliu/dev/orbit/.tmp-e2e-rehearsal/orbit-m34-rehearsal-5bnycw8z/backups/orbit-20261031T103657Z-m34-rehearsal.orbitbak`
- Bytes: `1495552`
- SHA-256: `953cb9b22b4ca43be1b45115d111bceb8d64b95247416da8fbeece124c62c874`
- Format/schema: `v1` / `schema 20`
- Components: `database, knowledge, attachments, configuration`
- Verified at creation: `True`
- Captured from a **live server**: pid `1869559`, journal_mode `wal`, WAL file present `True`, server answered HTTP `True` (EC-002 evidence)

### Deterministic dataset (created through supported surfaces)

| Element | Count / state | Created through |
|---|---|---|
| Projects | 3 | `POST /v1/authoritative/project-commands` |
| Milestones | 3 | `POST /v1/authoritative/milestone-commands` |
| Tasks | 103 | `POST /v1/authoritative/task-commands` (create) |
| Work Items | 6 | `POST /v1/authoritative/blueprints/items` (bundled Development v3 + capability-reference) |
| Markdown notes | 6 | `POST /v1/authoritative/markdown-commands` (wiki links in body) |
| Typed references | 3 | `POST /v1/authoritative/markdown-reference-commands` |
| Attachments | 5 | `POST /v1/authoritative/attachment-import` (incl. 8 KiB binary) |
| Agent-job records | 2 | `POST /v1/authoritative/agent-jobs/reserve` (reserved-only, never launched) |
| Capability install + grant + receipt | `succeeded` / `succeeded` | `capabilities/install`, `capabilities/authorize`, `capability-actions/request`, real `orbit capability serve --once` |
| MCP-made change | accepted (`authority.flush` → `Accepted(1)`) | real `orbit-mcp` connected session: `task-beta-002` |

### Rehearsal steps

| # | Step | Status | Detail |
|---|---|---|---|
| 1 | start a real Orbit instance (server, local-trusted, loopback) | ✅ PASS |  |
| 2 | populate the deterministic realistic dataset | ✅ PASS |  |
| 3 | create a complete backup | ✅ PASS |  |
| 4 | verify the archive | ✅ PASS |  |
| 5 | make additional disposable changes | ✅ PASS |  |
| 6 | restore the backup into a separate location | ✅ PASS |  |
| 7 | start an isolated Orbit instance on the restored data | ✅ PASS |  |
| 8 | confirm earlier tasks/milestones/notes/attachments exist | ✅ PASS |  |
| 9 | verify task relationships and activity history | ✅ PASS |  |
| 10 | confirm later disposable changes are absent | ✅ PASS |  |
| 11 | verify agent jobs and capability receipts are readable | ✅ PASS |  |
| 12 | confirm no external agent/capability auto-executes | ✅ PASS |  |
| 13 | perform an allowed disposable write in the restored instance | ✅ PASS |  |
| 14 | restart the restored instance and verify persistence | ✅ PASS |  |

### Entity counts and content hashes

| Signal | Before backup | Restored |
|---|---|---|
| Tasks (`tasks`) | 103 | 103 |
| Task prerequisites | 10 | 10 |
| Acceptance criteria | 206 | 206 |
| Work items | 6 | 6 |
| Markdown notes | 6 | 6 |
| Attachment objects | 5 | 5 |
| Agent jobs | 2 | 2 |
| Capability receipts | 1 | 1 |
| Tables with differing counts | `none` | (must be none) |

The backup was taken from the **live server instance** (WAL mode, active connection) and the isolated restore reproduced **all 64** database tables with equal row counts; the markdown vault tree and the attachment object tree are byte-identical (hashes below). No automatic external action ran.

Content hashes (SHA-256), before backup → after isolated restore:

- Database snapshot: `abc0cd02868cdd880a83c9d41b9b39bd1e339f15c9a497fee65091551a17080f` → `abc0cd02868cdd880a83c9d41b9b39bd1e339f15c9a497fee65091551a17080f` (match: yes)
- Markdown vault tree: `00f999dcd6d6aa6ba46c773fbb9968a0e089e4e029a48ea62a71a4430e002935` → `00f999dcd6d6aa6ba46c773fbb9968a0e089e4e029a48ea62a71a4430e002935` (match: yes)
- Attachment object tree: `83ba77e22b683c19e73665af1ac23978ff692e78ae894fb361595ae16565d85c` → `83ba77e22b683c19e73665af1ac23978ff692e78ae894fb361595ae16565d85c` (match: yes)
- Note files match: `yes`
- Attachment content match: `yes`
- Attachment `att-alpha-spec`: `e86f6d5ba5630980349d139a7848ad3f494ab9d6265c04bd37efeaebfe7e1ada`
- Attachment `att-alpha-binary`: `1f5a16c4456f34c5459e4e66d8650d4dbb6b298810d0c21cef6571db21d69c81`
- Attachment `att-beta-notes`: `f96b34e7a4190141c5be6fc3e5f2de10cb795a5f9cb4bd70d48432a23d9a25ca`
- Attachment `att-gamma-feature`: `c97384f191595c29f36e39694ac2c238b0dcddf150737ca28c16b0aab25ca110`
- Attachment `att-agent-brief`: `45725f46d1b0d1ec6111e62af3f9c00cc69f487ba775b9be7d243f8daabce494`
- Note `note-alpha-plan`: `da5149ac7260f9ec7eb8e9a3a45edb29125a95064957cecd7fff9495c4d727c7`
- Note `note-alpha-log`: `5bc7e0599d7812a34323cde9ee1f422e9b1d45da0c8844efb8b919be3352e062`
- Note `note-beta-design`: `b7d14bae22b6b66665d00bbeb09e3e1be1f49fb685a6a3e327e1c763ac63c885`
- Note `note-gamma-capability`: `12b7dbace89e4ad9fb5c03b53f5a9da504a99c7dc4d276c7e04db02e6b5e36fa`
- Note `note-restore-checklist`: `432994f4b7a46d15c7676a0de782568e133fda1331ec9661b5d3aa69d5b3035b`
- Note `note-backup-practice`: `a5314c2c77ee164ec89637f538907bc9eb4248062fa27610b0c203d9877739f5`

### MCP-authored change (EC-012)

A real `orbit-mcp` stdio session in connected mode changed a task; `authority.flush` was accepted by the authority and the change is present after restore. Tool calls: `authority.refresh`, `task.update`, `authority.flush`, `task.get`.

- Task: `task-beta-002`
- Accepted change: `{"description": "MCP-authored description recorded before the backup", "priority": "medium", "labels": ["mcp", "rehearsal"]}`
- Flush outcome: `Accepted(1)`
- Verified after restore: description/priority/labels match the accepted server state.

### Capability execution receipt (EC-012)

- Capability: `orbit.reference.work-item-note`
- Work item: `item-gamma-001`
- Action request state: `succeeded`
- Receipt outcome: `succeeded`
- Executed by the real connected `orbit capability serve` executor (exit 0); the server never runs provider code.

### Agent-job records

Reserved through the authority but deliberately never launched:

- `job-alpha-001` on `task-alpha-001`: state `queued`, claim_state null, runtime_session_ref null
- `job-alpha-002` on `task-alpha-004`: state `queued`, claim_state null, runtime_session_ref null

### Limitations (external boundary)

- No Herdr session and no external agent was launched anywhere in this rehearsal. Agent-job records are durable reservations in state 'queued' with claim_state=null and runtime_session_ref=null; the rehearsal asserts they stay that way and never auto-execute.
- No external agent session or repository is part of the archive: they are documented exclusions. Restore therefore cannot restore them, and the rehearsal does not pretend it does.
- Capability code was executed by the real connected `orbit capability serve` executor against a throwaway replica; the server itself never runs provider code. The receipt is durable evidence of that one execution, not an automatic re-execution after restore.
- Milestone creation and MCP tool calls use the real server HTTP/MCP surfaces; the local CLI has no milestone command. Nested note paths require a folder-create command first, so notes use root-level paths.
- The bundled Basic workflow declares no work-item kinds; bundled Development v3 does, so workflow Work Items use Development.
- A backup on the same disk is not protection against disk failure. Orbit does not claim zero data loss or zero downtime.

### Reproduce

```sh
cargo build --offline -p orbit-cli -p orbit-server
python3 tests/acceptance/m34_recovery_rehearsal.py --binary target/debug/orbit --server-binary target/debug/orbit-server
python3 tests/acceptance/m34_backup_cli.py --binary target/debug/orbit --server-binary target/debug/orbit-server
```

## Failure injection

- Result: **✅ PASS**
- Generated: `2026-10-31T10:36:52.292+00:00`

| Case | Expected | Observed | Original workspace intact | Status |
|---|---|---|---|---|
| create — default destination | a verified archive lands in the configured backups directory with a verification sidecar | created orbit-20261031T103648Z-backup.orbitbak (732672 bytes), verified=True, sidecar present | yes | ✅ PASS |
| create — --output and --label | the archive is written to the external directory and the manifest records the label, format and inventory | archive orbit-20261031T103649Z-nightly-acceptance.orbitbak labelled 'nightly acceptance', format 1, inventory kinds ['attachment_object', 'configuration', 'database', 'knowledge_note'] | yes | ✅ PASS |
| list — default and external directories | both listings derive entries from the archives themselves and report verification status | default dir: 1 archive(s); external dir: 2 archive(s), 2 verified | yes | ✅ PASS |
| verify — passing archive | verify exits zero, reports ok and lists checks | ok=true, 8 checks: {'structure': 'pass', 'manifest': 'pass', 'paths': 'pass', 'file_hashes': 'pass', 'sqlite_integrity': 'pass', 'schema_compatibility': 'pass', 'attachment_presence': 'pass', 'vault_consistency': 'pass'} | yes | ✅ PASS |
| verify — corrupted archive | verify exits nonzero and the corruption is reported | nonzero exit 1, corrupt byte at offset 366336 detected; original workspace unchanged | yes | ✅ PASS |
| restore --dry-run | a preflight plan is reported and nothing in the workspace changes (strict file list and hashes) | admissible preflight, no destination created, workspace file list and hashes unchanged (12 files) | yes | ✅ PASS |
| restore --to — isolated workspace opens | a separate workspace is created, verifies and opens through the CLI with the same tasks/notes | restored orbit.db, CLI status/list open it, 5 tasks and 2 notes readable, active workspace unchanged | yes | ✅ PASS |
| restore --replace — confirmation required | replace is refused without the exact confirmation, then activates and preserves the previous workspace | without/wrong --confirm refused; REPLACE activated, preserved .orbit-preserved-20261031T103650Z and journal restore-journal.json; preserved copy opens (workspace_status) | yes | ✅ PASS |
| restore --replace — active target refused | a workspace held by a writer is not replaced and stays intact | nonzero exit 4 while a writer held the target; target intact | yes | ✅ PASS |
| schedule + retention | the policy round-trips and retention never removes the last verified recovery point | schedule stored 'daily'; retention kept 1 archive(s), last verified present | yes | ✅ PASS |
| interrupted backup | SIGKILL mid-create leaves no partial archive, no staging artifacts and an unchanged workspace | SIGKILL after staging activity (rc -9); no partial .orbitbak published, no staging artifacts, workspace intact | yes | ✅ PASS |
| corrupted archive | byte corruption is rejected by verify and restore; workspace intact | verify exit 1, restore exit 1; target not populated; workspace intact | yes | ✅ PASS |
| truncated archive | a truncated archive is rejected; workspace intact | truncated archive rejected with exit 1; workspace intact | yes | ✅ PASS |
| missing attachment object | a referenced attachment object missing from the archive is detected; workspace intact | dropped attachments/objects/ae/ae8cb81f47d93dd2e91ef76759364f0aee40c2d1a98dfcf0b2f03f08d8d7cfb9; verify exit 1, restore exit 1; workspace intact | yes | ✅ PASS |
| unreadable vault file | an unreadable Markdown file fails creation cleanly with no partial archive | note note-acceptance-log.md made unreadable; create failed with exit 4; no archive published; workspace intact | yes | ✅ PASS |
| disk-full / permission failure | a destination that cannot be created fails cleanly with no partial archive | permission-denied create failed with exit 4; no destination, no archive; workspace intact | yes | ✅ PASS |
| destination permissions preserved | an existing non-writable destination is never silently widened and fails cleanly | non-writable destination refused cleanly (exit 4), mode 0o500 preserved | yes | ✅ PASS |
| unsupported manifest version | a newer format is rejected with an actionable upgrade message | format v999 rejected by verify/inspect/restore with an upgrade message; workspace intact | yes | ✅ PASS |
| wrong destination | restoring onto the active workspace, a file, or with mixed modes is refused | active-workspace target exit 1, file target exit 1, mixed modes exit 2; workspace intact | yes | ✅ PASS |
| interrupted restoration | SIGKILL mid-restore leaves the original workspace intact and never produces a silently usable partial target | killed mid-restore (attempts=1, activity_detected=True); no complete target produced; follow-up restore succeeded | yes | ✅ PASS |
| replica snapshots unsupported | a client-replica snapshot is refused with an actionable 'use the server workspace' message from both local and connected deployments, publishing nothing | local deployment exit 2, connected deployment exit 4; both refused as unsupported recovery points with no archive/staging; workspace intact | yes | ✅ PASS |
| missing external agent session | external-boundary scenario: restored reserved jobs stay queued and no launch is attempted | external-boundary scenario: the rehearsal's restored instance keeps the reserved agent jobs queued with runtime_session_ref=null and makes no launch attempt (rehearsal steps 11 and 12) | yes | ✅ PASS |
| pending connected-client outbox | connected-client reconciliation is an external boundary for this CLI suite | The CLI accepts neither a connected replica as a restore target nor a server outbox; pending connected-client reconciliation is enforced by the server/preflight (I2 restore-engine's pending-outbox blocker test) and is outside this CLI failure suite. A replica-only workspace is not a supported recovery point in this build, so it is never presented as one. | yes | NOT_APPLICABLE |

Failure-injection limitations:

- Disk-full is not simulated with a size-limited tmpfs (that requires mount privileges this harness does not have). Permission denial is injected two real ways instead: a destination under a read-only parent, and an existing non-writable destination directory (which also asserts the operator's mode is preserved).
- Interrupted backup/restore use SIGKILL at the first observable staging activity, retried up to four times; if every attempt lands after the operation completed, the case is reported INCONCLUSIVE instead of a false pass. The recorded attempts make the timing visible.
- A failure *after* activation started (preserved-copy rollback path) has no supported fault-injection hook from outside the product. The successful replacement path is asserted to activate, write a recovery journal and preserve the previous workspace, and the active-target/refused paths are asserted to leave the target intact; deeper post-activation rollback injection belongs to the restore service's own tests.
- Pending connected-client outbox reconciliation needs a connected replica target, which the CLI deliberately refuses to replace; it is enforced by the server/preflight and is outside this CLI-only suite.
- The suite touches no repository state: every workspace, archive and kill target is created under a fresh temporary directory and removed afterwards.
- KNOWN LIMITATION (non-blocking, P3 backlog): re-enrolling an already-enrolled device on a database restored from an archive returns HTTP 500 {"error":"storage error"} instead of a clean conflict or idempotent success. Repro: restore an archive containing an enrolled device, start orbit-server on the restored database, then POST /v1/devices with the same device id and token using the administrator key => 500. The restored bearer credential itself authenticates (200), so recovery is unaffected; this harness therefore relies on the restored credential and never re-enrolls.
