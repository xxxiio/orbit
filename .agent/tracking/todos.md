---
template: todo-register
template_version: 2.0.0
template_digest: "6c2f3e43ed9194113fb98463ffa65e35d3146bdf32da8b7b1369a830c800b49e"
project_owned: true
record_contract: todo-register
record_schema_version: 1
status: current
---

# TODO Register

## Active

### TODO-0015 — Local-trusted per-device agent-job visibility
- **Status:** ready
- **Priority:** P2
- **Milestone:** MILESTONE-0033
- **Reason:** Agent jobs keep the pre-existing per-device ownership rule; after switching an existing database to local-trusted mode, jobs started by a device are not listed for the local principal. They return when signing back in as that device, so the data is not lost, but the local-trusted experience hides prior agent work.
- **Depends on:** an accepted ownership model for agent jobs across the local principal and enrolled devices
- **Blocks:** complete local-trusted agent workflow visibility
- **Unblock condition:** none
- **Acceptance:** The local-trusted principal can list agent jobs created by any enrolled device according to an explicit, tested ownership rule, with no privilege escalation to machine or executor routes.
- **Source:** M33 residual risk (2026-10-31 internal acceptance)

### TODO-0016 — Explicit read-only `/mcp` boundary in local-trusted mode
- **Status:** ready
- **Priority:** P2
- **Milestone:** MILESTONE-0033
- **Reason:** `/mcp` writes need an enrolled device, so in local-trusted mode with the admin key the endpoint is effectively read-only. The refusal is documented, but the boundary follows from the guard split rather than an explicit read-only contract.
- **Depends on:** a server authorization policy for MCP reads and writes in local-trusted mode
- **Blocks:** an explicit, testable MCP boundary for personal installs
- **Unblock condition:** none
- **Acceptance:** Local-trusted MCP reads are explicitly allowed and mutations are refused with a stable, documented error; the boundary is covered by tests.
- **Source:** M33 residual risk (2026-10-31 internal acceptance)

### TODO-0017 — Residual M33 visual/comfort polish
- **Status:** ready
- **Priority:** P3
- **Milestone:** MILESTONE-0033
- **Reason:** The internal-use acceptance judged the app "generally good enough to begin internal use" and explicitly did not certify comfort or visual taste; screenshots cannot judge comfort. Residual visual/comfort observations should be collected during real internal use rather than reopening the accepted M33 design.
- **Depends on:** a short real-use review list from internal dogfooding
- **Blocks:** none
- **Unblock condition:** none
- **Acceptance:** A recorded internal-use review names any concrete visual or comfort issue with its screen and viewport; each issue is fixed in its owning layer without reopening typography, Milestone, Epic-removal, unified Tasks, compact Board or authentication decisions.
- **Source:** M33 residual risk (2026-10-31 internal acceptance)

### TODO-0018 — Restored-database re-enrollment of an existing device returns HTTP 500
- **Status:** ready
- **Priority:** P3
- **Milestone:** MILESTONE-0034
- **Reason:** Post-restore acceptance found that re-enrolling an existing device against the restored database returns HTTP 500 instead of a 409/idempotent answer. Recovery is unaffected; the device enrollment path handles the restored state poorly.
- **Depends on:** none
- **Blocks:** a clean post-restore device re-enrollment flow
- **Unblock condition:** none
- **Acceptance:** Re-enrolling an existing device against a restored workspace returns a stable 409 (or an explicit idempotent success) with a clear message, covered by a regression test; no server error is logged as a crash.
- **Source:** M34 I5 acceptance/failure-injection evidence (`web/evidence/m34-recovery/`)

### TODO-0019 — Connected client replica snapshot support
- **Status:** ready
- **Priority:** P2
- **Milestone:** MILESTONE-0034
- **Reason:** The `.orbitbak` format reserves `source.mode: replica`, but current builds fail closed and refuse replica snapshots as a recovery point. A safe create/restore story must be designed before enabling it.
- **Depends on:** an accepted restore semantics for client replicas (outbox/staged-upload reconciliation; authority relationship)
- **Blocks:** connected-mode client-side recovery points
- **Unblock condition:** a documented replica create/restore design is approved
- **Acceptance:** A design records when a replica snapshot is a valid recovery artifact and how outbox/staged uploads reconcile; if enabled, creation and restore are tested end to end; otherwise the refusal stays documented and tested.
- **Source:** M34 I5 acceptance evidence and `docs/reference/backup-format.md`

### TODO-0020 — Streaming backup download instead of a 512 MiB memory buffer
- **Status:** ready
- **Priority:** P3
- **Milestone:** MILESTONE-0034
- **Reason:** The browser download endpoint buffers the whole archive in memory and caps it with `ORBIT_BACKUP_DOWNLOAD_MAX_BYTES` (default 512 MiB); larger archives must be copied from the server's backup folder.
- **Depends on:** a streaming response helper without adding an external dependency
- **Blocks:** browser download of large workspaces
- **Unblock condition:** none
- **Acceptance:** The download streams the archive with bounded memory and produces identical bytes/checksum for a large archive; the cap/`413` limitation is removed or explicitly retained with a reasoned policy.
- **Source:** M34 I3 acceptance evidence

### TODO-0021 — Optional live-server replacement activation on restart
- **Status:** ready
- **Priority:** P2
- **Milestone:** MILESTONE-0034
- **Reason:** Server replacement is deliberately offline: the Web GUI only writes `<db>.restore-pending.json` and an operator runs the maintenance commands. A reviewed activation-on-restart path could make recovery more convenient without pretending to be a live overwrite.
- **Depends on:** an accepted maintenance-window/authorization design
- **Blocks:** smoother operator recovery on a server deployment
- **Unblock condition:** a design is approved that keeps the maintenance boundary and rollback preservation
- **Acceptance:** A server restart detects an approved pending replacement plan, refuses if writers are active, stages/verifies before switching, preserves the previous workspace and journal, and is covered by tests.
- **Source:** M34 I3 acceptance evidence

### TODO-0022 — Preflight free-space measurement and early disk-full refusal
- **Status:** ready
- **Priority:** P3
- **Milestone:** MILESTONE-0034
- **Reason:** Restore preflight reports free space as `None`; a full disk surfaces during extraction. Extraction fails closed and the original workspace is preserved, but an early refusal would be clearer.
- **Depends on:** a portable free-space check for the target filesystem
- **Blocks:** clearer large-restore preflight
- **Unblock condition:** none
- **Acceptance:** Preflight reports free space when measurable and blocks an insufficient target with a clear message; when unmeasurable it states why and extraction still fails closed.
- **Source:** M34 I2/I5 acceptance evidence (`free_space_unknown` warning)

### TODO-0023 — `knowledge_folders` counts include internal `.orbit-operations` directories
- **Status:** ready
- **Priority:** P4
- **Milestone:** MILESTONE-0034
- **Reason:** The backup manifest's `knowledge_folders` count includes internal `.orbit-operations` directories, so the reported folder count can exceed the user-visible folders.
- **Depends on:** none
- **Blocks:** none
- **Unblock condition:** none
- **Acceptance:** Manifest folder counts either exclude internal operational directories or label them separately, with a test proving the user-visible folder count.
- **Source:** M34 I5 rehearsal evidence

### TODO-0013 — Per-device project authorization for connected attachments
- **Status:** ready
- **Priority:** P1
- **Milestone:** MILESTONE-0028
- **Reason:** The authority is single-tenant; any enrolled device may access any existing attachment owner. Required for multi-user deployments.
- **Depends on:** a device/project ACL model shared by all entity commands
- **Blocks:** multi-tenant attachment authorization claims
- **Unblock condition:** none
- **Acceptance:** A device can only list/read/write attachments for projects it is scoped to; denial is tested for task, project, work-item, knowledge and agent-job owners.
- **Source:** NEXT-0009

### TODO-0014 — Server-side attachment garbage collection
- **Status:** ready
- **Priority:** P2
- **Milestone:** MILESTONE-0028
- **Reason:** Connected clients cannot purge unreferenced attachment content; only local purge exists.
- **Depends on:** server attachment routes
- **Blocks:** storage reclamation for server deployments
- **Unblock condition:** none
- **Acceptance:** An authorized admin operation removes unreferenced objects and reports counts without deleting referenced content.
- **Source:** NEXT-0009


### TODO-0011 — Server-authoritative attachment upload and retrieval
- **Status:** completed
- **Priority:** P1
- **Milestone:** MILESTONE-0028
- **Reason:** Connected attachment operations currently fail closed; required scenario D needs durable upload intents, integrity verification and cross-client retrieval.
- **Depends on:** none
- **Blocks:** connected attachment parity and scenario D acceptance
- **Unblock condition:** none
- **Acceptance:** A file attached while connected is server-accepted, retrievable by another authorized client, and an interrupted/offline upload retries exactly once with matching checksum and no duplicate content.
- **Source:** NEXT-0009

### TODO-0012 — Live Herdr Agent Development acceptance
- **Status:** blocked
- **Priority:** P2
- **Milestone:** MILESTONE-0028
- **Reason:** Agent Development workflow composition exists, but launch/monitor/review/resume could not be exercised without an authorized runtime.
- **Depends on:** an explicitly configured, authorized Herdr runtime and disposable repository
- **Blocks:** scenario C acceptance evidence
- **Unblock condition:** A maintainer provides an authorized runtime configuration
- **Acceptance:** Launch, monitor, handoff review, correction request, resume and explicit completion are exercised with disposable data and no implicit launch from MCP or Basic.
- **Source:** TASK-0005


### TODO-0001 — Connected MCP blueprint writes use the canonical outbox and receipts
- **Status:** ready
- **Priority:** P1
- **Milestone:** MILESTONE-0025
- **Reason:** Connected MCP currently posts directly to blueprint endpoints, so it lacks offline staging and lost-ack reconciliation.
- **Depends on:** none
- **Blocks:** NEXT-0001's M25-I5 follow-up
- **Unblock condition:** none
- **Acceptance:** MCP-initiated connected blueprint mutation is queued in the canonical outbox, replayed with the same command ID, and reconciled after a lost acknowledgement.
- **Source:** NEXT-0001

### TODO-0002 — Add an explicit blueprint version upgrade/preview path
- **Status:** ready
- **Priority:** P2
- **Milestone:** MILESTONE-0025
- **Reason:** Binding is first-binding-wins and changing versions returns conflict; the roadmap requires explicit, previewable upgrades.
- **Depends on:** none
- **Blocks:** none
- **Unblock condition:** none
- **Acceptance:** A bound project can preview and atomically upgrade to a newer immutable blueprint version without silent remapping.
- **Source:** NEXT-0001

### TODO-0007 — Publish an authorized test release and verify the real installer/update path
- **Status:** blocked
- **Priority:** P1
- **Milestone:** MILESTONE-0027
- **Reason:** M27 fixture tests cannot prove the GitHub-hosted installer or update command works against a real release.
- **Depends on:** explicit maintainer authorization to create a tag/release
- **Blocks:** public installation readiness and EC-002/EC-005 real-release confidence
- **Unblock condition:** A maintainer authorizes publishing a draft or test release from this repository
- **Acceptance:** The workflow publishes the canonical assets, and `install.sh` plus `orbit update --check` succeed against the real release on Linux x86_64.
- **Source:** RISK-0010

### TODO-0008 — Execute the installer and update portability checks on macOS
- **Status:** blocked
- **Priority:** P2
- **Milestone:** MILESTONE-0027
- **Reason:** The repository environment is Linux x86_64; shell portability is statically checked but not executed on macOS bash 3.2/BSD tar.
- **Depends on:** macOS runner or machine
- **Blocks:** macOS installation support claim
- **Unblock condition:** A macOS runner is available
- **Acceptance:** Root installer and update install/upgrade a release on macOS x86_64 or arm64.
- **Source:** RISK-0007

### TODO-0003 — Extract habit recurrence/streak behavior into an opt-in capability package
- **Status:** ready
- **Priority:** P2
- **Milestone:** MILESTONE-0025
- **Reason:** Recurrence/streak remains core despite I4's domain-extraction goal.
- **Depends on:** TODO-0001
- **Blocks:** none
- **Unblock condition:** none
- **Acceptance:** Core Orbit operates without recurrence/streak capabilities; the opt-in package owns the behavior and has executable acceptance.
- **Source:** NEXT-0001

## Deferred

### TODO-0004 — TLS/Caddy ingress and browser deployment acceptance
- **Status:** deferred
- **Priority:** P1
- **Milestone:** MILESTONE-0015
- **Reason:** Docker is unavailable; reconsider when a self-hosted deployment environment exists.
- **Depends on:** none
- **Blocks:** public release
- **Unblock condition:** Docker plus a disposable domain/certificate environment
- **Acceptance:** A loopback-to-TLS ingress smoke passes without exposing secrets.
- **Source:** RISK-0001

### TODO-0005 — Remote-machine Herdr execution against the real Rust server
- **Status:** deferred
- **Priority:** P1
- **Milestone:** MILESTONE-0015
- **Reason:** Requires a second enrolled machine; reconsider before claiming remote-agent readiness.
- **Depends on:** none
- **Blocks:** remote agent readiness
- **Unblock condition:** A second disposable machine/worker is available
- **Acceptance:** Real remote launch/reconnect/review passes with a durable journal.
- **Source:** RISK-0002

### TODO-0006 — Distribution signing, cross-platform and installed-version upgrade evidence
- **Status:** deferred
- **Priority:** P1
- **Milestone:** MILESTONE-0026
- **Reason:** No signing channel, macOS/Windows toolchains or older release fixture; reconsider before public release.
- **Depends on:** none
- **Blocks:** public release
- **Unblock condition:** Signing key/channel and target platforms/fixtures exist
- **Acceptance:** Signed archives install and upgrade with rollback evidence on each supported target.
- **Source:** RISK-0003

### TODO-0007 — Adopt the shared Orbit UI renderer in connected mode
- **Status:** completed
- **Priority:** P1
- **Milestone:** MILESTONE-0028
- **Reason:** Read-only connected project views are implemented over the accepted snapshot; the full connected interactive shell and mutation surface remain command-centric.
- **Depends on:** connected snapshot-to-view-data adapter
- **Blocks:** full local/connected presentation parity
- **Unblock condition:** connected replica data can be mapped to `WorkspaceData`/`ProjectData` without bypassing acceptance rules
- **Acceptance:** Connected mode renders project views through `orbit-ui` and preserves pending/accepted distinctions.
- **Source:** NEXT-0005
- **Completion:** Unified `App`/renderer over `backend.rs`; connected project/task pending→flush→accepted PTY test passes.

### TODO-0008 — Local agent handoff review surface
- **Status:** open
- **Priority:** P2
- **Milestone:** MILESTONE-0028
- **Reason:** Local client exposes job inspection but no accepted review command surface; the TUI action explains this.
- **Depends on:** accepted local review API
- **Blocks:** complete local agent review workflow
- **Unblock condition:** review decisions are exposed through a typed authorized client call
- **Acceptance:** Review decision is explicit, durable and does not auto-complete the task.
- **Source:** M28 EC-004/EC-006

## Completed

- None.
