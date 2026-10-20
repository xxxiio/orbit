# Development 271 — Worker transport and ownership consolidation

M24 source implementation estimate: 71% → 77% (+6 percentage points).

Reuses a single internal authenticated worker transport across all three optional
Python workers without changing their distinct roles, external execution safety,
or public `orbit` CLI. See `docs/m24-worker-architecture.md` for boundaries and
`docs/m24-architecture-consolidation.md` for priority and scope decisions.

Feature-level checks: 46 Python unit/contract tests passed in this environment.
Rust-wide compilation, real Rust authority/Herdr integration and any fixes found
by those checks remain M15 acceptance scope. Internal-testing readiness has not
been established (runnable build and core smoke workflows not yet confirmed).

Remaining M24-I4: targeted obsolete implementation/documentation cleanup. Do not
replace the worker's crash journal with the connected client's offline outbox;
they fence different kinds of side effects.
