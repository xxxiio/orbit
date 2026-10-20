# Development 215 — Accepted schedule-aware day planning

Adds a deterministic plan of task slots in real free intervals of the accepted schedule. This is a read-only proposal; no schedule blocks or tasks are created or modified.

## Surfaces

- `AuthoritativeHost::schedule_plan` uses accepted tasks, prerequisite edges, and accepted schedule blocks.
- `AuthorityConnection::schedule_plan` evaluates embedded state or an offline accepted snapshot, never pending commands.
- `orbit task schedule-plan --starts-at-ms ... --ends-at-ms ... [--max-tasks ...] [--project ...] [--default-minutes ...]`.
- `GET /v1/authoritative/schedule-plan?starts_at_ms=...&ends_at_ms=...`.

## Semantics

- Fixed busy intervals are unioned before capacity calculations, with horizon limited to seven days.
- Each accepted schedule block occupies time even when linked to a task; no implicit double booking.
- Tasks have to fit in one uninterrupted free interval; no automatic splitting.
- Dependency readiness and ranking are reused from existing pure planning logic.
- Non-actionable and unplaceable tasks are explained explicitly.
- Snapshot-based remote results are offline accepted-state proposals, not online convergence claims.
- Windows and slot times are Unix milliseconds; capacity and allocations are whole minutes.

## Verification

Static source integration assertions pass. Focused Rust unit tests added for overlapping busy blocks, invalid input, blocked tasks, time-window fitting, project filters, deterministic order, and duration estimates. Cargo/rustc are unavailable in this development environment: Rust compile and executable regression verification remain outstanding for M15.
