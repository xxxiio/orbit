# Orbit Development 275 — M15 acceptance: Python contracts after M24 refactoring

## Scope
This is an M15 release-acceptance iteration based on Development 274. M24 implementation remains closed.

## Acceptance executed
- `python -m unittest discover -s deploy/server -p 'test_*.py' -v` — **46 passed**.
- `pytest -q tests` — **34 passed** after correcting 7 stale source-location assertions.
- Rust workspace build/test — **blocked: cargo and rustc absent**.
- Live Herdr and full Rust HTTP integration — **not run**.

## Fixes made
The M24 CLI extraction moved task handlers into `crates/orbit-cli/src/task_commands.rs`; planning query implementation moved into `crates/orbit-client/src/planning_queries.rs`. Seven historical M22 source-contract assertions still inspected the earlier `lib.rs` or `unified.rs` locations, producing false negatives. Updated:
- `tests/test_m22_approved_plan_contract.py`
- `tests/test_m22_handoff_contract.py`
- `tests/test_m22_navigation_contract.py`
- `tests/test_m22_portfolio_contract.py`

These changes update tests to inspect the currently owning modules; no business logic or runtime API was changed. They do not establish that the Rust application compiles.

## Next M15 acceptance gates
1. Install or supply a suitable Rust toolchain (edition 2024), then run `cargo test --workspace --locked` and fix compilation/test failures.
2. Start an actual Orbit server, verify authenticated task and agent command flows.
3. Verify local and connected sync, duplicate-launch fencing and reconnection with real Herdr.
4. Internal-testing readiness requires a runnable build, core smoke workflows and no blocking data-loss issue.
