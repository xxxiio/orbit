#!/usr/bin/env bash
# Fully noninteractive M15 acceptance; no dependency installation and no real user data.
set -euo pipefail
cd "$(dirname "$0")/.."
exec python3 scripts/collect-release-evidence.py --profile full --no-setup-prompt "$@"
