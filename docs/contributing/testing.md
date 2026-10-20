# Tests and acceptance

Run the maintained M15 gate with `./scripts/m15-full-automated.sh --profile full`. The runner creates a ZIP evidence bundle with each check and its status.

| Suite | Scope |
|---|---|
| `cargo test --workspace --locked` | Rust logic and crate integration |
| `tests/contracts/test_*.py` | Lightweight source boundary checks, not proof of runtime functionality |
| `tests/acceptance/local_cli_workflows.py` | Real compiled CLI with fresh, isolated SQLite databases |
| `tests/packaging/` | Install/uninstall, setup, and release-archive contracts |
| `deploy/server/test_*.py` | Optional Python worker behavior |
| `deploy/server/smoke-*.py` | Authenticated server/client smoke scenarios |

Milestone-numbered source checks and historic wrappers are intentionally removed: their assumptions about Rust file layout were brittle after M24 and should not gate a release. The retained executable local CLI scenario is now included in the full profile after cargo build.

No tests should touch users' real data. Rust and deployed-server acceptance remains M15 scope.
