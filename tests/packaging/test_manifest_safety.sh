#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"; trap 'rm -rf -- "$tmp"' EXIT
mkdir -p "$tmp/prefix/bin" "$tmp/prefix/share/orbit" "$tmp/build"
printf '#!/bin/sh\nexit 0\n' > "$tmp/build/orbit"
chmod +x "$tmp/build/orbit"
man="$tmp/prefix/share/orbit/installed-binaries.txt"
printf 'orbit\n../escape\n' > "$man"
if "$ROOT/scripts/install.sh" --from-dir "$tmp/build" --prefix "$tmp/prefix" >/dev/null 2>&1; then echo 'installer accepted malformed manifest' >&2; exit 1; fi
if "$ROOT/scripts/uninstall.sh" --prefix "$tmp/prefix" >/dev/null 2>&1; then echo 'uninstaller accepted malformed manifest' >&2; exit 1; fi
printf 'orbit\n' > "$man"
printf 'keep\n' > "$tmp/victim"
ln -s "$tmp/victim" "$tmp/prefix/bin/orbit"
if "$ROOT/scripts/install.sh" --from-dir "$tmp/build" --prefix "$tmp/prefix" >/dev/null 2>&1; then echo 'installer accepted symlinked binary' >&2; exit 1; fi
if "$ROOT/scripts/uninstall.sh" --prefix "$tmp/prefix" >/dev/null 2>&1; then echo 'uninstaller accepted symlinked binary' >&2; exit 1; fi
[[ "$(cat "$tmp/victim")" == keep ]] || { echo 'external file modified' >&2; exit 1; }
echo 'PASS: installer/uninstaller reject untrusted manifest entries and symlinked executables'
