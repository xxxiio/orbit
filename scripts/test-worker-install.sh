#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
TEMP="$(mktemp -d)"
trap 'rm -rf -- "$TEMP"' EXIT
PREFIX="$TEMP/prefix"
"$ROOT/scripts/install-worker.sh" --prefix "$PREFIX" >/dev/null
"$ROOT/scripts/install-worker.sh" --prefix "$PREFIX" --check >/dev/null
for name in agent-machine-worker.py worker_transport.py agent_worker_evidence.py agent_worker_journal.py; do
  cmp "$ROOT/deploy/server/$name" "$PREFIX/libexec/orbit/agent-machine-worker/$name"
done
# User-managed data must not be removed.
mkdir -p "$PREFIX/var/orbit"; echo 'keep' > "$PREFIX/var/orbit/journal"
"$ROOT/scripts/install-worker.sh" --prefix "$PREFIX" --uninstall >/dev/null
[[ -f "$PREFIX/var/orbit/journal" ]]
if "$ROOT/scripts/install-worker.sh" --prefix "$PREFIX" --check >/dev/null 2>&1; then exit 1; fi
# Refuse clobbering untracked scripts.
mkdir -p "$PREFIX/libexec/orbit/agent-machine-worker"
echo unmanaged > "$PREFIX/libexec/orbit/agent-machine-worker/worker_transport.py"
if "$ROOT/scripts/install-worker.sh" --prefix "$PREFIX" >/dev/null 2>&1; then exit 1; fi
grep -qx unmanaged "$PREFIX/libexec/orbit/agent-machine-worker/worker_transport.py"
echo 'worker installation lifecycle: PASS'
