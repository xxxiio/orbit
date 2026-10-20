#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT
prefix="$tmp/prefix"
mkdir -p "$prefix/bin" "$prefix/share/orbit"
printf 'orbit\n' > "$prefix/share/orbit/installed-binaries.txt"
printf '#!/bin/sh\nexit 0\n' > "$prefix/bin/orbit"
chmod +x "$prefix/bin/orbit"
args=(--prefix "$prefix" --data-home "$tmp/data" --config-home "$tmp/config" --state-home "$tmp/state")
"$ROOT/scripts/orbit-env.sh" "${args[@]}" --check >/dev/null
"$ROOT/scripts/orbit-env.sh" "${args[@]}" --init-local >/dev/null
for dir in "$tmp/data/orbit" "$tmp/config/orbit" "$tmp/state/orbit"; do
  [[ -d "$dir" ]] || exit 1
  [[ "$(stat -c %a "$dir" 2>/dev/null || stat -f %Lp "$dir")" == 700 ]] || exit 1
done
[[ ! -e "$tmp/data/orbit/orbit.db" ]] || exit 1
rm "$prefix/bin/orbit"
if "$ROOT/scripts/orbit-env.sh" "${args[@]}" --check >/dev/null 2>&1; then echo 'expected missing binary failure' >&2; exit 1; fi
printf '../bad\n' > "$prefix/share/orbit/installed-binaries.txt"
if "$ROOT/scripts/orbit-env.sh" "${args[@]}" --check >/dev/null 2>&1; then echo 'expected invalid manifest failure' >&2; exit 1; fi
printf 'orbit\n' > "$prefix/share/orbit/installed-binaries.txt"
rmdir "$tmp/config/orbit"
mkdir -p "$tmp/elsewhere"
ln -s "$tmp/elsewhere" "$tmp/config/orbit"
if "$ROOT/scripts/orbit-env.sh" "${args[@]}" --init-local >/dev/null 2>&1; then echo 'expected symlink rejection' >&2; exit 1; fi
echo 'PASS: installation inspection and local directory preparation'

# Doctor is read-only and optional dependencies are informational.
printf '#!/bin/sh\nexit 0\n' > "$prefix/bin/orbit"
chmod +x "$prefix/bin/orbit"
mkdir -p "$tmp/db"
export ORBIT_DB="$tmp/db/orbit.db"
"$ROOT/scripts/orbit-env.sh" "${args[@]}" --doctor > "$tmp/doctor.log"
grep -q 'not yet created' "$tmp/doctor.log"
[[ ! -e "$ORBIT_DB" ]] || exit 1
: > "$ORBIT_DB"
"$ROOT/scripts/orbit-env.sh" "${args[@]}" --doctor > "$tmp/doctor.log"
grep -q 'Existing local database' "$tmp/doctor.log"
rm "$ORBIT_DB"
ln -s "$tmp/not-a-database" "$ORBIT_DB"
if "$ROOT/scripts/orbit-env.sh" "${args[@]}" --doctor >/dev/null 2>&1; then echo 'expected database symlink error' >&2; exit 1; fi
rm "$ORBIT_DB"
# Valid installation is required before diagnostic execution.
rm "$prefix/bin/orbit"
if "$ROOT/scripts/orbit-env.sh" "${args[@]}" --doctor >/dev/null 2>&1; then echo 'expected broken executable error' >&2; exit 1; fi
