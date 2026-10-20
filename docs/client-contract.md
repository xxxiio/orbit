# Orbit embedded client contract

`orbit-client` is the stable Rust boundary intended for generated Flutter/FFI bindings. Flutter must not bind directly to repositories, SQLite handles, domain aggregate internals, sync transport internals, or AI-provider implementations.

## Version

The current bridge contract is `ORBIT_CLIENT_API_VERSION = 3`. V2 extended the frozen V1 project DTO with project mode, objective, supported progress signals, explicit success criteria, and criterion-status mutations.

Breaking DTO/command changes require a client API version bump. Internal domain/storage refactors that preserve the client DTO contract do not.

## Runtime model

The normal Flutter application embeds Orbit Rust in-process:

```text
Flutter / Atomic Apps
        ↓ generated bridge
orbit-client
        ↓
orbit-application / planner / storage / sync
```

No separately running Orbit backend is required for normal mobile or desktop use.

## V2 read surfaces

- workspace summary and capability negotiation
- projects (including mode/objective/progress signals/success criteria) and tasks
- structured knowledge
- habits and habit history
- schedule blocks for caller-defined windows
- `TodaySnapshotDto`
- durable activity-backed change cursor

## V2 mutation surfaces

- create/update project status
- configure project mode/objective/progress signals/success criteria and update criterion status
- create/update task status
- update task planning metadata (estimate / earliest start / deadline)
- create/update structured knowledge
- create/pause/resume/check habits
- create schedule blocks
- deterministic task-plan preview/apply
- deterministic habit-plan preview/apply

Plan apply never trusts the DTO just because it came from Flutter. Rust reconstructs the proposal and validates it against current canonical state before applying it atomically.

## Time and timezone boundary

Client DTOs use Unix milliseconds and explicit caller-resolved time windows. Flutter/platform code owns timezone and local-day/DST resolution; the deterministic core does not guess the user's timezone.

## Secrets

`SecureSecretStore` is a platform adapter. Raw BYOK credentials must remain in platform secure storage and must not be stored in normal Orbit SQLite or semantic sync data.

## Change notifications

`ClientChangeCursor` is backed by the durable semantic activity journal, so it observes activity written by embedded Flutter mutations as well as CLI/MCP processes using the same workspace.

Inbound sync materialization deliberately does not create semantic activity records. Therefore the embedded client **requires a query-surface refresh after a successful sync pull**. `ClientBridgeContract.refresh_after_sync_pull_required` exposes this explicitly rather than implying that the activity cursor covers remote materialization.

## Platform capabilities

- desktop: embedded runtime, local AI endpoints where platform/network policy permits, optional MCP server
- mobile: embedded runtime, no MCP package/registration
- every platform: external secure-secret adapter required

## FFI rule

The generated bridge should expose primitive/owned DTOs only. Do not expose Rust lifetimes, repository traits, SQLite connections, domain references, or provider-specific SDK types across FFI.


V3 adds project evidence-binding and metric target/observation methods plus explicit-window deterministic project evidence DTOs. These Rust facade APIs need FFI/Dart bridge smoke validation before release.
