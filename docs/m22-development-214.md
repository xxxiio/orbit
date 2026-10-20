# M22 Development 214 — Deterministic capacity-aware daily planning

This source implementation extends dependency-aware next actions with a capacity-aware daily plan.

## Contract

- The sole inputs are authoritative task records, dependency edges, an accepted revision, and an explicit request (time, project, capacity, count cap, and estimate policy).
- Greedy placement respects the existing next-action ordering, never guesses task durations, and never splits an oversized task.
- Excluded tasks remain visible with an explanation: unfinished prerequisites, workflow state, future earliest start, project mismatch, absent estimates, inadequate capacity, or task-count cap.
- Projects are summarized by actionable, blocked, selected tasks, and allocated minutes.
- Local-only host and connected client's accepted SQLite snapshot provide the same planning implementation. Pending remote intents never alter accepted readiness.
- Planning is read-only. It does not create calendar blocks, reserve time, advance tasks, or invoke AI.
- The remote server exposes `/v1/authoritative/day-plan`; the local CLI exposes `orbit task day-plan`.
- A plan is a deterministic suggestion for a supplied moment, not a reservation or proof of the server's *current* state when evaluated offline.

## CLI usage

```sh
orbit task day-plan --now-ms 1800000000000 --available-minutes 180 --max-tasks 5
orbit task day-plan --now-ms 1800000000000 --available-minutes 90 --project my-project --default-minutes 25
```

## Verification status

Focused Rust tests were authored for capacity fitting, dependencies, project filtering, deterministic row ordering, unsized tasks, overdue work, and invalid requests. Cargo execution and broad release acceptance remain pending M15. Source-only checks do not establish compilation or runtime correctness.
