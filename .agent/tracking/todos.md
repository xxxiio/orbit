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
