#!/usr/bin/env bash
# Install the optional machine-side Python worker without installing a public CLI.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
PREFIX="${HOME}/.local"
ACTION=install
usage() {
  cat <<'HELP'
Usage: scripts/install-worker.sh [--prefix DIR] [--check|--uninstall]
Installs only the optional agent machine worker into PREFIX/libexec/orbit/agent-machine-worker.
Requires Python 3 for execution, not for local Orbit. Does not install Herdr, enroll a
machine, start a background service, or create/disclose credentials.
  --check       Check installed files; make no changes.
  --uninstall   Remove only worker files tracked by the worker installer.
HELP
}
while (($#)); do
  case "$1" in
    --prefix) (($# >= 2)) || { echo 'missing prefix' >&2; exit 2; }; PREFIX="$2"; shift 2;;
    --check) ACTION=check; shift;;
    --uninstall) ACTION=uninstall; shift;;
    -h|--help) usage; exit 0;;
    *) echo "unknown argument: $1" >&2; exit 2;;
  esac
done
[[ -n "$PREFIX" && "$PREFIX" != / ]] || { echo 'invalid prefix' >&2; exit 2; }
# Do not create directories for --check/--uninstall.
if [[ "$ACTION" != install ]]; then
  [[ -d "$PREFIX" ]] || { echo 'installation prefix missing' >&2; exit 1; }
  PREFIX="$(cd "$PREFIX" && pwd -P)"
else
  mkdir -p -- "$PREFIX"
  PREFIX="$(cd "$PREFIX" && pwd -P)"
fi
[[ "$PREFIX" != / ]] || exit 2
TARGET="$PREFIX/libexec/orbit/agent-machine-worker"
MANIFEST="$PREFIX/share/orbit/installed-worker-files.txt"
FILES=(agent-machine-worker.py agent_worker_evidence.py agent_worker_journal.py worker_transport.py)
if [[ "$ACTION" == check ]]; then
  [[ -f "$MANIFEST" ]] || { echo 'worker not installed' >&2; exit 1; }
  for f in "${FILES[@]}"; do
    [[ -f "$TARGET/$f" ]] || { echo "missing worker component: $f" >&2; exit 1; }
  done
  echo "Agent worker files installed at $TARGET"
  exit 0
fi
if [[ "$ACTION" == uninstall ]]; then
  [[ -f "$MANIFEST" ]] || { echo 'worker not installed' >&2; exit 1; }
  # Reject an unexpected manifest rather than deleting arbitrary files.
  mapfile -t recorded < "$MANIFEST"
  ((${#recorded[@]} == ${#FILES[@]})) || { echo 'unexpected worker manifest' >&2; exit 1; }
  for f in "${FILES[@]}"; do
    printf '%s\n' "${recorded[@]}" | grep -Fxq -- "$f" || { echo 'invalid worker manifest' >&2; exit 1; }
  done
  for f in "${FILES[@]}"; do rm -f -- "$TARGET/$f"; done
  rm -f -- "$MANIFEST"
  rmdir "$TARGET" 2>/dev/null || true
  echo 'Worker files removed; journals, credentials, and user data preserved.'
  exit 0
fi
command -v python3 >/dev/null || { echo 'python3 required for optional worker' >&2; exit 2; }
for f in "${FILES[@]}"; do [[ -f "$ROOT/deploy/server/$f" ]] || { echo "missing source: $f" >&2; exit 1; }; done
# Existing untracked files are not ours to replace.
for f in "${FILES[@]}"; do
  if [[ -e "$TARGET/$f" || -L "$TARGET/$f" ]]; then
    [[ -f "$MANIFEST" ]] && grep -Fxq -- "$f" "$MANIFEST" || { echo "refusing unmanaged file: $TARGET/$f" >&2; exit 1; }
  fi
done
mkdir -p "$TARGET" "$PREFIX/share/orbit"
for f in "${FILES[@]}"; do
  install -m 0644 "$ROOT/deploy/server/$f" "$TARGET/.$f.new"
  mv -f "$TARGET/.$f.new" "$TARGET/$f"
done
printf '%s\n' "${FILES[@]}" > "$MANIFEST.tmp"
mv "$MANIFEST.tmp" "$MANIFEST"
echo "Installed optional machine worker into $TARGET"
echo "Run with: ORBIT_SYNC_HTTP_TOKEN=<secret> python3 $TARGET/agent-machine-worker.py --server <URL> --device <ENROLLED-ID> --journal-dir <PERSISTENT-DIR>"
echo 'Keep the token outside shell history and never remove a journal to retry an uncertain launch.'
