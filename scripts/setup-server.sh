#!/usr/bin/env bash
# Prepare an opt-in, loopback-bound Orbit server deployment. Never starts services by default.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
COMPOSE="$ROOT/deploy/server/compose.yaml"
ENV_FILE="$ROOT/deploy/server/.env"
ACTION=init
usage() { cat <<'HELP'
Usage: scripts/setup-server.sh [--env-file PATH] [--check|--init|--up|--down]
  --init   Create a private .env file if absent; do not start any service (default).
  --check  Verify deployment inputs and Docker Compose syntax; do not modify files.
  --up     Verify inputs, then build and start the local-loopback server.
  --down   Stop the configured Compose service; preserve the data volume.
The server binds to 127.0.0.1:8787 by default. For remote access, configure TLS ingress.
HELP
}
while (($#)); do
  case "$1" in
    --env-file) (($# >= 2)) || { echo 'Missing path' >&2; exit 2; }; ENV_FILE="$2"; shift 2;;
    --check) ACTION=check; shift;;
    --init) ACTION=init; shift;;
    --up) ACTION=up; shift;;
    --down) ACTION=down; shift;;
    --help|-h) usage; exit 0;;
    *) echo "Unknown option: $1" >&2; exit 2;;
  esac
done
case "$ENV_FILE" in /*) ;; *) ENV_FILE="$PWD/$ENV_FILE";; esac
if [[ "$ACTION" == init ]]; then
  if [[ -e "$ENV_FILE" ]]; then
    echo "Configuration exists; refusing overwrite: $ENV_FILE"
    exit 0
  fi
  command -v openssl >/dev/null || { echo 'openssl required to create a random admin key' >&2; exit 2; }
  mkdir -p -- "$(dirname "$ENV_FILE")"
  umask 077
  tmp="$(mktemp "${ENV_FILE}.tmp.XXXXXXXX")"
  trap 'rm -f -- "$tmp"' EXIT
  key="$(openssl rand -hex 32)"
  [[ ${#key} == 64 ]] || { echo 'Key generation failed' >&2; exit 1; }
  printf 'ORBIT_SERVER_ADMIN_KEY=%s\n' "$key" > "$tmp"
  chmod 600 "$tmp"
  # Refuse clobbering a file created by a concurrent invocation.
  if ln "$tmp" "$ENV_FILE" 2>/dev/null; then
    chmod 600 "$ENV_FILE"
  else
    echo "Configuration appeared concurrently; refusing overwrite: $ENV_FILE" >&2
    exit 1
  fi
  echo "Created private server configuration: $ENV_FILE"
  echo 'Next: scripts/setup-server.sh --check; scripts/setup-server.sh --up'
  exit 0
fi
command -v docker >/dev/null || { echo 'docker required for --check, --up or --down' >&2; exit 2; }
docker compose version >/dev/null 2>&1 || { echo 'Docker Compose v2 required' >&2; exit 2; }
[[ -f "$ENV_FILE" ]] || { echo "Missing environment file: $ENV_FILE; run --init" >&2; exit 2; }
# Do not source untrusted shell text. Compose loads the env file itself.
if [[ "$ACTION" != down ]]; then
  key="$(sed -n 's/^ORBIT_SERVER_ADMIN_KEY=//p' "$ENV_FILE" | head -n 1)"
  [[ "$key" =~ ^[[:xdigit:]]{64}$ ]] || { echo 'Invalid server admin key; expected 64 hex characters' >&2; exit 2; }
  # Compose config may contain expanded secrets: discard output completely.
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE" config --quiet >/dev/null
fi
case "$ACTION" in
  check) echo 'Compose configuration valid; no service started.';;
  up) docker compose --env-file "$ENV_FILE" -f "$COMPOSE" up -d --build; echo 'Server bound to loopback; configure TLS for remote clients.';;
  down) docker compose --env-file "$ENV_FILE" -f "$COMPOSE" down; echo 'Server stopped; persistent volume preserved.';;
esac
