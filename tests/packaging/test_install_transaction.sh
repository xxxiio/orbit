#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/build" "$TMP/prefix/bin" "$TMP/prefix/share/orbit"
printf '#!/bin/sh\necho old\n' > "$TMP/prefix/bin/orbit"
chmod +x "$TMP/prefix/bin/orbit"
printf 'orbit\n' > "$TMP/prefix/share/orbit/installed-binaries.txt"
printf '#!/bin/sh\necho new\n' > "$TMP/build/orbit"
chmod +x "$TMP/build/orbit"
if "$ROOT/scripts/install.sh" --prefix "$TMP/prefix" --from-dir "$TMP/build" --components cli,mcp >/dev/null 2>&1; then echo 'unexpected success' >&2; exit 1; fi
[[ "$("$TMP/prefix/bin/orbit")" == old ]]
[[ "$(cat "$TMP/prefix/share/orbit/installed-binaries.txt")" == orbit ]]
"$ROOT/scripts/install.sh" --prefix "$TMP/prefix" --from-dir "$TMP/build" --components cli >/dev/null
[[ "$("$TMP/prefix/bin/orbit")" == new ]]
"$ROOT/scripts/install.sh" --prefix "$TMP/prefix" --components cli --check >/dev/null
if "$ROOT/scripts/install.sh" --prefix "$TMP/prefix" --components cli,mcp --check >/dev/null 2>&1; then exit 1; fi
printf 'orbit\nUNEXPECTED-ENTRY\n' > "$TMP/prefix/share/orbit/installed-binaries.txt"
if "$ROOT/scripts/uninstall.sh" --prefix "$TMP/prefix" >/dev/null 2>&1; then exit 1; fi
[[ -x "$TMP/prefix/bin/orbit" ]]
echo 'install transaction contract: PASS'
