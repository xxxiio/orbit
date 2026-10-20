#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"; trap 'rm -rf -- "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/output"
for name in orbit orbit-mcp orbit-tui orbit-server; do printf '#!/bin/sh\necho test\n' > "$tmp/bin/$name"; chmod +x "$tmp/bin/$name"; done
"$ROOT/scripts/package-release.sh" --from-dir "$tmp/bin" --output "$tmp/output" --target x86_64-unknown-linux-gnu --version 0.1.0 --components cli,mcp,tui,server >/dev/null
archive="$tmp/output/orbit-0.1.0-x86_64-unknown-linux-gnu.tar.gz"
"$ROOT/scripts/verify-release.sh" "$archive" >/dev/null
[[ "$(tar -tzf "$archive" | grep -E '/bin/[^/]+$' | wc -l)" -eq 4 ]]
# A properly rehashed outer archive still needs an exact internal manifest.
mkdir -p "$tmp/repacked"
tar -xzf "$archive" -C "$tmp/repacked"
root="orbit-0.1.0-x86_64-unknown-linux-gnu"
printf 'not-hashed\n' > "$tmp/repacked/$root/bin/orbit-server"
tar -C "$tmp/repacked" -czf "$tmp/output/$root.tar.gz" "$root"
(cd "$tmp/output" && sha256sum "$root.tar.gz" > "$root.tar.gz.sha256")
if "$ROOT/scripts/verify-release.sh" "$archive" >/dev/null 2>&1; then echo 'accepted altered executable' >&2; exit 1; fi
# Restore the valid package for corruption coverage below.
"$ROOT/scripts/package-release.sh" --from-dir "$tmp/bin" --output "$tmp/output" --target x86_64-unknown-linux-gnu --version 0.1.0 --components cli,mcp,tui,server >/dev/null
# Adding unlisted files with a valid outer digest must also fail.
mkdir -p "$tmp/repacked/$root"
printf 'unexpected' > "$tmp/repacked/$root/EXTRA.txt"
tar -C "$tmp/repacked" -czf "$archive" "$root"
(cd "$tmp/output" && sha256sum "$root.tar.gz" > "$root.tar.gz.sha256")
if "$ROOT/scripts/verify-release.sh" "$archive" >/dev/null 2>&1; then echo 'accepted extra member' >&2; exit 1; fi
"$ROOT/scripts/package-release.sh" --from-dir "$tmp/bin" --output "$tmp/output" --target x86_64-unknown-linux-gnu --version 0.1.0 --components cli,mcp,tui,server >/dev/null
# The producer must reject missing components before producing a partial archive.
rm "$tmp/bin/orbit-server"
if "$ROOT/scripts/package-release.sh" --from-dir "$tmp/bin" --output "$tmp/output" --target x86_64-unknown-linux-gnu --version 0.2.0 --components cli,server > /dev/null 2>&1; then exit 1; fi
[[ ! -e "$tmp/output/orbit-0.2.0-x86_64-unknown-linux-gnu.tar.gz" ]]
# A modified archive must fail verification even if the checksum file is unchanged.
printf 'corruption' >> "$archive"
if "$ROOT/scripts/verify-release.sh" "$archive" >/dev/null 2>&1; then exit 1; fi
# Reject invalid target and duplicate components.
if "$ROOT/scripts/package-release.sh" --from-dir "$tmp/bin" --output "$tmp/output" --target '../unsafe' --components cli >/dev/null 2>&1; then exit 1; fi
if "$ROOT/scripts/package-release.sh" --from-dir "$tmp/bin" --output "$tmp/output" --target x86_64-unknown-linux-gnu --components cli,cli >/dev/null 2>&1; then exit 1; fi
echo 'release packaging contract passed'
