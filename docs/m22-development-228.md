# Orbit Development 228 — Interactive planning handoff

## Scope

Continue the M22 action-menu functionality from Development 227. A reviewed
selection now opens a **read-only destination** instead of returning an inert
navigation label. This uses the accepted task/project planning brief and a
single shared deterministic Rust dispatcher.

- `planning_handoff` validates the chosen indexed action against a recomputed
  accepted menu, then constructs a scope-limited task or group drilldown or a
  typed preview instruction. Every result has `authoritative_write_performed=false`.
- Both terminal modes render numbered menu entries and allow selecting 1–9.
  Completion selections also open the existing completion-impact simulator.
  The remaining entries are bounded view/preview handoffs, not mutations.
- Embedded and connected shared Rust clients expose typed handoff methods and
  enforce the accepted revision boundary. Connected outbox intentions do not
  change accepted navigation.
- Local CLI and both MCP backends offer task/group handoff commands; authenticated
  server endpoints expose the same read-only dispatch.
- Offline connected mode does not offer authoritative write or approval handoffs.
  Calendar review and dependency changes still require the separate explicit
  command / plan review / approval workflow.

## Boundaries and follow-up

The user must re-review after a changed authoritative revision. A displayed
menu is not a write permission. Typed preview handoffs are instructions to
use existing preview flows; this iteration does not silently create calendar
blocks or dependency edges. The interactive terminal supports the first nine
entries, while CLI/MCP support the full indexed menu.

Source-level tests check revision guards, hidden identities, forged selections,
offline availability, approval boundaries, and deterministic rendering. Rust
compilation and executable integration tests are deferred to M15 and must not
be marked as passed without running them.
