#!/bin/sh
# Optional foreground periodic reconnect for a connected Orbit replica.
# Stop with Ctrl-C/systemd.
set -eu
: "${ORBIT_DB:?Set ORBIT_DB to the replica database path}"
: "${ORBIT_SYNC_HTTP_TOKEN:?Set device token in environment}"
: "${ORBIT_AUTHORITY_DEVICE:?Set enrolled device identity in environment}"
: "${ORBIT_SYNC_SERVER:?Set HTTPS or loopback authority URL}"
INTERVAL="${ORBIT_SYNC_INTERVAL_SECONDS:-300}"
case "$INTERVAL" in ''|*[!0-9]*) echo 'interval must be an integer >= 30' >&2; exit 2;; esac
[ "$INTERVAL" -ge 30 ] || { echo 'interval must be >= 30 seconds' >&2; exit 2; }
# One sequential foreground worker per replica. Failed reconnects retain the
# durable local outbox and accepted snapshot; the next pass retries with the
# same frozen command identities.
while :; do
    if ! orbit sync replica --db "$ORBIT_DB" --server "$ORBIT_SYNC_SERVER" reconnect; then
        echo 'orbit replica reconnect failed; retrying after interval' >&2
    fi
    sleep "$INTERVAL"
done
