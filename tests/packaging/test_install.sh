#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
base="$(mktemp -d)"
trap 'rm -rf "$base"' EXIT
mkdir -p "$base/build"
for b in orbit orbit-mcp; do
  printf '#!/bin/sh\nexit 0\n' > "$base/build/$b"; chmod +x "$base/build/$b"
done
"$root/scripts/install.sh" --from-dir "$base/build" --prefix "$base/app" --components cli,mcp >/dev/null
[[ -x "$base/app/bin/orbit" && -x "$base/app/bin/orbit-mcp" ]]
"$base/app/bin/orbit"; "$base/app/bin/orbit-mcp"
# Ownership manifest must refuse to clobber an unmanaged executable.
printf '#!/bin/sh\n' > "$base/app/bin/orbit-server"
chmod +x "$base/app/bin/orbit-server"
if "$root/scripts/install.sh" --from-dir "$base/build" --prefix "$base/app" --components server 2>/dev/null; then
  echo 'unexpected installation succeeded with missing binary' >&2; exit 1
fi
"$root/scripts/uninstall.sh" --prefix "$base/app" >/dev/null
[[ ! -e "$base/app/bin/orbit" && ! -e "$base/app/bin/orbit-mcp" ]]
[[ -e "$base/app/bin/orbit-server" ]]
echo 'installation contract passed'
