# Orbit milestone implementation snapshot — 112

> **Historical snapshot.** This table predates the M18–M25 refactors and the
> M15 acceptance run. Current M15/M24/M25 status and executable evidence live in
> `docs/roadmap.md`, `README.md`, and the M15 runner evidence bundle
> (`docs/m15-runner-coverage.md`).

**Revised architectural contract:** one host-side runtime and authoritative
SQLite implementation in embedded and remote deployment; local/remote CLI and
TUI; local stdio MCP and remote HTTP MCP. See `unified-runtime-roadmap.md`.

Implementation estimates are provisional, not verified acceptance. The expanded
M18 scope is rebaselined to 75%; prior 99% referred to the older two-implementation
architecture and is not valid for the newly agreed design. M15 acceptance remains
unverified. Flutter is deferred to Atomic App.

| Milestone | Estimate |
|---|---:|
| M1 Domain foundation | 94% |
| M2 Local persistence | 96% |
| M3 CLI core | 92% |
| M4 Planning & scheduling | 94% |
| M5 Progress & forecasting | 94% |
| M6 Habits | 94% |
| M7 Retired Git sync (historical) | 92% |
| M8 Knowledge foundation | 94% |
| M9 Knowledge retrieval | 97% |
| M10 Embedded Rust client | 94% |
| M11 Local AI/BYOK | 94% |
| M12 MCP/agents | 95% |
| M13 Terminal UI | 94% |
| M14 Project Modes/AI Agent | 97% |
| M15 Release readiness | 8% |
| M16 Self-hosted server | 95% |
| M17 Remote agents/web | 98% |
| M18 Unified host runtime/refactor | 79% |

M18's next highest priority is U2: replace separate authoritative server
business implementations with shared runtime/application services. This is
more important than additional security hardening, release packaging or
new features at this stage. No internal-testing-ready claim until a runnable
build, core smoke acceptance, and no blocking data-loss issue.

Development 115 (provisional): M18 unified runtime 81%; native embedded
canonical command adapter added, with source regression tests. M15 remains 8%:
Rust compilation and end-to-end acceptance have not run. Overall equal-weight
estimated progress approximately 88.9%. Flutter remains separate Atomic App
work and does not block M18/M15.


Development 116 (provisional): M18 unified runtime 82%; removed the
`AuthoritativeHost` raw-store dereference escape hatch and routed server device
management and backup through explicit host APIs. Rust compilation and runtime
parity remain unverified; M15 stays at 8%.

Development 117 (provisional): M18 83%, focused on unified canonical
embedded/remote client entry point plus remote enrolled-device authentication
header wiring in CLI/TUI. The older feature-rich local SQLite runtime remains
separate (U2 is **not** complete), local MCP parity is unfinished, and no Rust
compilation/runtime acceptance was run. M15 stays at 8%.

Development 118 (provisional): M18 84%. First knowledge-item write/read path in canonical
`AuthoritativeHost` SQLite, using the same revision, immutable semantic change,
command idempotency receipt, and replay-projection transaction as tasks/projects.
Rust regression tests added for accepted writes, replay, actor conflict, and
stale revisions, but not executed (no Rust toolchain). This is **not** U2
completion: local `SqliteStore` knowledge/planning repositories remain separate;
knowledge is not yet connected to HTTP/MCP and knowledge updates/deletes are
not implemented. No internal-testing-ready claim; M15 remains 8%.
