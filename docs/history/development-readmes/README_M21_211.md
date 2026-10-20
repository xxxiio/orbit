# M21 Development 211 — Cross-domain synchronization visibility

## Purpose

One accepted-state and outbox reporting contract powers the connected Rust
client, CLI, desktop MCP, and terminal TUI. Status must never imply that
local pending intentions have been accepted by the server.

## Implemented

- `SyncHealth` describes incomplete accepted snapshot, corrupt outbox, conflicts,
  pending commands, incomplete Markdown mirror, unobserved live authority, and
  live-history divergence as separate conditions with suggested actions.
- `DomainQueueReport` reads the canonical outbox across all ten command kinds,
  preserving the globally earliest unresolved intention; accepted/superseded
  receipts remain distinct from outstanding work.
- `WorkspaceSyncReport` links the two read-only views and checks consistency
  between domain-aggregated queue counts and the overall convergence status.
- Offline and live-observed reports are separate operations. Offline reports
  never assert convergence with a server that has not been observed.
- The CLI exposes `workspace-sync`, `observe-workspace-sync`, `sync-health`,
  `observe-sync-health`; desktop MCP exposes corresponding authority tools.
- The connected TUI exposes `sync.status` and `sync.observe`, including the
  first globally blocking command.

## Boundaries

Reports do not submit pending commands, rewrite accepted snapshots, auto-resolve
conflicts, or include command payloads/Markdown content. A live report is valid
only at the instant of the observation. A clean queue and mirrored notes do
not prove that another connected client cannot write subsequently.

## Verification

Focused source-contract checks and regression test definitions accompany this
change. Compilation and executable multi-client acceptance remain explicitly
under M15. Internal-testing readiness is not yet established.
