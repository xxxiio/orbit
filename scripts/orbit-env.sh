#!/usr/bin/env bash
# Inspect and prepare a user-scoped Orbit installation without starting services.
set -euo pipefail
PREFIX="${HOME}/.local"
MODE="status"
DATA_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}"
CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"
STATE_HOME="${XDG_STATE_HOME:-${HOME}/.local/state}"
usage() {
  cat <<'HELP'
Usage: scripts/orbit-env.sh [--prefix DIR] [--status|--init-local|--check|--doctor] [--data-home DIR] [--config-home DIR] [--state-home DIR]
--status      Print installed components and user directory locations (default; read-only).
--init-local  Create private local data/config/state directories. Does not create a database or start a service.
--check       Validate installation manifest and executables; nonzero on issues.
--doctor      Diagnose installation, PATH, local database access and optional runtime availability (read-only).
This script neither installs Herdr/Python nor configures the optional server.
HELP
}
while (($#)); do
  case "$1" in
    --prefix|--data-home|--config-home|--state-home)
      (($# >= 2)) || { echo "missing value for $1" >&2; exit 2; }
      case "$1" in --prefix) PREFIX="$2";; --data-home) DATA_HOME="$2";; --config-home) CONFIG_HOME="$2";; --state-home) STATE_HOME="$2";; esac
      shift 2;;
    --status) MODE=status; shift;;
    --init-local) MODE=init; shift;;
    --check) MODE=check; shift;;
    --doctor) MODE=doctor; shift;;
    -h|--help) usage; exit 0;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2;;
  esac
done
for dir in "$PREFIX" "$DATA_HOME" "$CONFIG_HOME" "$STATE_HOME"; do
  [[ -n "$dir" && "$dir" != / ]] || { echo 'refusing empty or root directory' >&2; exit 2; }
done
manifest="$PREFIX/share/orbit/installed-binaries.txt"
case "$MODE" in
  init)
    umask 077
    for dir in "$DATA_HOME/orbit" "$CONFIG_HOME/orbit" "$STATE_HOME/orbit"; do
      [[ ! -L "$dir" ]] || { echo "refusing symbolic-link directory: $dir" >&2; exit 1; }
      mkdir -p -- "$dir"
      chmod 700 -- "$dir"
      echo "Prepared: $dir"
    done
    echo 'No database or application configuration was generated.'
    ;;
  check|status|doctor)
    if [[ ! -f "$manifest" || -L "$manifest" ]]; then
      echo "No valid installation manifest: $manifest" >&2
      [[ "$MODE" != check && "$MODE" != doctor ]] || exit 1
    else
      declare -A seen=()
      while IFS= read -r name || [[ -n "$name" ]]; do
        case "$name" in orbit|orbit-mcp|orbit-tui|orbit-server) ;; *) echo "Invalid manifest entry: $name" >&2; exit 1;; esac
        [[ ! -v seen[$name] ]] || { echo "Duplicate manifest entry: $name" >&2; exit 1; }
        seen[$name]=1
        bin="$PREFIX/bin/$name"
        if [[ -L "$bin" || ! -f "$bin" || ! -x "$bin" ]]; then
          echo "BROKEN: $name ($bin)" >&2
          [[ "$MODE" != check && "$MODE" != doctor ]] || exit 1
        else
          echo "OK: $name ($bin)"
        fi
      done < "$manifest"
    fi
    printf 'Local paths: data=%s config=%s state=%s\n' "$DATA_HOME/orbit" "$CONFIG_HOME/orbit" "$STATE_HOME/orbit"
    if [[ "$MODE" == doctor ]]; then
      problems=0
      cli="$PREFIX/bin/orbit"
      if [[ -f "$cli" && -x "$cli" && ! -L "$cli" ]]; then
        found="$(command -v orbit 2>/dev/null || true)"
        if [[ -z "$found" ]]; then
          echo 'ADVICE: Orbit is installed but not on PATH. Add the installation bin directory to PATH.'
        elif [[ "$found" != "$cli" ]]; then
          echo "ADVICE: PATH resolves orbit to $found instead of $cli"
        else
          echo 'OK: PATH resolves to this Orbit CLI'
        fi
      fi
      db="${ORBIT_DB:-${HOME}/.orbit/orbit.db}"
      if [[ -L "$db" ]]; then
        echo "ERROR: Orbit database is a symbolic link: $db" >&2
        problems=$((problems+1))
      elif [[ -e "$db" && ! -f "$db" ]]; then
        echo "ERROR: Orbit database path is not a regular file: $db" >&2
        problems=$((problems+1))
      elif [[ -f "$db" && ! -r "$db" ]]; then
        echo "ERROR: Orbit database is not readable: $db" >&2
        problems=$((problems+1))
      elif [[ -f "$db" ]]; then
        echo "INFO: Existing local database: $db (not opened or modified)"
      else
        echo "INFO: Local database not yet created: $db"
      fi
      dbdir="$(dirname -- "$db")"
      if [[ ! -d "$dbdir" ]]; then
        echo "ADVICE: Database parent directory does not exist: $dbdir"
      elif [[ ! -w "$dbdir" ]]; then
        echo "ERROR: Database parent directory is not writable: $dbdir" >&2
        problems=$((problems+1))
      fi
      for runtime in herdr docker python3; do
        if command -v "$runtime" >/dev/null 2>&1; then
          echo "OPTIONAL: $runtime available"
        else
          echo "OPTIONAL: $runtime not installed (not required for local CLI)"
        fi
      done
      ((problems == 0)) || exit 1
      echo 'Diagnostics complete. No services started, executables run, or databases opened.'
    fi
    ;;
esac
