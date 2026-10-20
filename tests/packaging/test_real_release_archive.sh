#!/usr/bin/env bash
# Build real release binaries, package them, verify the archive and install it
# into a disposable prefix. This is the end-to-end packaging gate that fixture
# tests cannot provide.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"; trap 'rm -rf -- "$tmp"' EXIT

command -v cargo >/dev/null || { echo 'cargo required' >&2; exit 2; }
command -v sha256sum >/dev/null || { echo 'sha256sum required' >&2; exit 2; }

"$ROOT/scripts/package-release.sh" --build --components cli --output "$tmp/dist" >/dev/null
archive="$(find "$tmp/dist" -maxdepth 1 -name 'orbit-*-*.tar.gz' -print -quit)"
[[ -n "$archive" && -f "$archive.sha256" ]] || { echo 'release archive not produced' >&2; exit 1; }

"$ROOT/scripts/verify-release.sh" "$archive" >/dev/null
"$ROOT/scripts/install.sh" --from-archive "$archive" --prefix "$tmp/prefix" >/dev/null
"$ROOT/scripts/install.sh" --prefix "$tmp/prefix" --check >/dev/null

"$tmp/prefix/bin/orbit" --version >/dev/null

# The installed binary must perform a real local write against a disposable DB.
ORBIT_DB="$tmp/orbit.db" "$tmp/prefix/bin/orbit" --json project add 'Packaged CLI smoke' >/dev/null
ORBIT_DB="$tmp/orbit.db" "$tmp/prefix/bin/orbit" --json project list >/dev/null
[[ -f "$tmp/orbit.db" ]] || { echo 'installed CLI did not create its database' >&2; exit 1; }

"$ROOT/scripts/uninstall.sh" --prefix "$tmp/prefix" >/dev/null
[[ ! -e "$tmp/prefix/bin/orbit" ]] || { echo 'uninstall left orbit behind' >&2; exit 1; }
echo 'real release build/package/install smoke passed'
