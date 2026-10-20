#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python -m unittest discover -s deploy/server -p 'test_*.py' -v
python -m pytest -q tests/contracts tests/packaging tests/documentation
