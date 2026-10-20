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

## Completed

- None.
