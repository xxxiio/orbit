#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"; trap 'rm -rf -- "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/dist"
for component in orbit orbit-mcp orbit-tui; do
  printf '#!/bin/sh\necho %s\n' "$component" > "$tmp/bin/$component"
  chmod +x "$tmp/bin/$component"
done
"$ROOT/scripts/package-release.sh" --from-dir "$tmp/bin" --output "$tmp/dist" --version 0.1.0 --target x86_64-unknown-linux-gnu --components cli,mcp,tui >/dev/null
archive="$tmp/dist/orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz"
"$ROOT/scripts/install.sh" --prefix "$tmp/prefix" --from-archive "$archive" >/dev/null
"$ROOT/scripts/install.sh" --prefix "$tmp/prefix" --components cli,mcp,tui --check >/dev/null
[[ "$("$tmp/prefix/bin/orbit")" == orbit ]]
[[ "$("$tmp/prefix/bin/orbit-mcp")" == orbit-mcp ]]
# Missing optional component fails without altering an installation.
if "$ROOT/scripts/install.sh" --prefix "$tmp/prefix" --from-archive "$archive" --components cli,server >/dev/null 2>&1; then exit 1; fi
[[ "$("$tmp/prefix/bin/orbit")" == orbit ]]
# A corrupted archive must not alter prior binaries.
printf 'corruption' >> "$archive"
if "$ROOT/scripts/install.sh" --prefix "$tmp/prefix" --from-archive "$archive" >/dev/null 2>&1; then exit 1; fi
[[ "$("$tmp/prefix/bin/orbit")" == orbit ]]
echo 'verified release archive installation contract passed'
