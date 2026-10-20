#!/usr/bin/env bash
# Exit non-zero unless complete source compilation and executable acceptance pass.
set -euo pipefail
cd "$(dirname "$0")/../.."
if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
  echo 'BLOCKED: Rust/Cargo unavailable; no M18 acceptance verdict.' >&2
  exit 2
fi
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
# Run real user-facing workflows with fresh SQLite databases in subprocesses.
cargo build -p orbit-cli --locked
python3 scripts/legacy-contracts/test-m18-e2e-workflows.py --binary target/debug/orbit
python3 scripts/legacy-contracts/test-m18-all-local-writes-accepted.py
python3 scripts/legacy-contracts/test-m18-plan-command-acceptance.py
python3 scripts/legacy-contracts/test-m18-rich-mutation-cutover.py
python3 scripts/legacy-contracts/test-m18-command-engine.py
python3 scripts/legacy-contracts/test-m18-transaction-bridge.py
python3 scripts/legacy-contracts/test-m18-runtime-boundaries.py
python3 scripts/legacy-contracts/test-m18-cli-task-project-boundary.py
python3 scripts/legacy-contracts/test-m18-query-unification.py
python3 scripts/legacy-contracts/test-m18-application-services.py
python3 scripts/legacy-contracts/test-m18-retired-relay.py
python3 scripts/legacy-contracts/test-m18-markdown-cutover.py
python3 scripts/legacy-contracts/test-m18-no-sql-knowledge.py
python3 scripts/legacy-contracts/test-m18-folder-contract.py
python3 scripts/legacy-contracts/test-m18-host-composition.py
python3 scripts/legacy-contracts/test-m18-unified-entities.py
python3 scripts/legacy-contracts/test-m18-single-project-task-store.py
python3 scripts/legacy-contracts/test-m18-single-sqlite-owner.py
python3 -m pytest -q deploy/server/test_agent_worker.py deploy/server/test_automation_worker.py
python3 scripts/test-authority-offline.py
printf '\nRust and source checks PASS. Full M18 runtime/consolidation acceptance still requires a dedicated feature-parity review.\n'
# Independent progress-report gate. A source scan never certifies completeness.
python3 scripts/legacy-contracts/test-m18-progress.py
python3 scripts/legacy-contracts/m18-progress.py # Evidence only; milestone release gate remains separate
# Verify the underlying mutable authority repository stays private to the runtime.
python3 scripts/legacy-contracts/test-m18-private-authority-store.py
