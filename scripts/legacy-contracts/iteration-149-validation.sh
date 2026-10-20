#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 scripts/legacy-contracts/test-m18-routine-host.py
python3 scripts/legacy-contracts/test-m18-application-services.py
python3 scripts/legacy-contracts/test-m18-application-queries.py
python3 scripts/legacy-contracts/test-m18-cli-task-project-boundary.py
python3 scripts/test-authority-offline.py
if command -v cargo >/dev/null; then cargo test --workspace; else echo 'Rust toolchain unavailable: cargo test not executed'; fi
