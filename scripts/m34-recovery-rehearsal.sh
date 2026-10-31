#!/usr/bin/env bash
# M34 I5 — one-command recovery rehearsal and failure-injection evidence run.
#
# Builds the CLI and server (offline, locked where possible), then runs both
# acceptance suites against the real binaries. Every run uses disposable
# directories under the system temporary directory; the repository workspace,
# `orbit-local.db`, `~/.orbit` and `web/orbit-server.db` are never touched.
#
# Usage:
#   scripts/m34-recovery-rehearsal.sh [--skip-build] [--keep-workdir]
#
# Evidence is written under web/evidence/m34-recovery/:
#   rehearsal-evidence.json  machine-readable 14-step rehearsal result
#   failure-injection.json   machine-readable CLI acceptance/failure matrix
#   README.md                human summary of both
#
# Exit codes: 0 = both suites passed; 1 = a failure; 2 = a product surface is
# still not implemented (blocked, not failed).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

SKIP_BUILD=0
KEEP=()
for argument in "$@"; do
  case "$argument" in
    --skip-build) SKIP_BUILD=1 ;;
    --keep-workdir) KEEP+=(--keep-workdir) ;;
    -h|--help)
      sed -n '2,20p' "${BASH_SOURCE[0]}"
      exit 0
      ;;
    *)
      echo "unknown argument: $argument" >&2
      exit 64
      ;;
  esac
done

CLI="$REPO_ROOT/target/debug/orbit"
SERVER="$REPO_ROOT/target/debug/orbit-server"
MCP="$REPO_ROOT/target/debug/orbit-mcp"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  echo "== building orbit-cli and orbit-server =="
  if ! cargo build --offline -p orbit-cli -p orbit-server; then
    echo "build failed; fix the tree before running acceptance" >&2
    exit 1
  fi
fi

for binary in "$CLI" "$SERVER" "$MCP"; do
  if [[ ! -x "$binary" ]]; then
    echo "required binary missing or not executable: $binary" >&2
    exit 1
  fi
done

status=0

echo
echo "== CLI acceptance + failure injection =="
python3 tests/acceptance/m34_backup_cli.py \
  --binary "$CLI" --server-binary "$SERVER" "${KEEP[@]}"
cli_status=$?
if [[ "$cli_status" -ne 0 ]]; then
  status="$cli_status"
fi

echo
echo "== real recovery rehearsal (14 steps) =="
python3 tests/acceptance/m34_recovery_rehearsal.py \
  --binary "$CLI" --server-binary "$SERVER" --mcp-binary "$MCP" "${KEEP[@]}"
rehearsal_status=$?
if [[ "$rehearsal_status" -ne 0 && "$status" -eq 0 ]]; then
  status="$rehearsal_status"
fi

echo
echo "== evidence =="
ls -l web/evidence/m34-recovery/ 2>/dev/null || true
echo
if [[ "$status" -eq 0 ]]; then
  echo "M34 recovery acceptance: PASS"
elif [[ "$status" -eq 2 ]]; then
  echo "M34 recovery acceptance: BLOCKED (product surface not implemented yet)"
else
  echo "M34 recovery acceptance: FAIL"
fi
exit "$status"
