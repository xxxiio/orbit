#!/usr/bin/env bash
# Verify an Orbit release archive without extracting untrusted archive paths.
set -euo pipefail
sha256_check() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c -- "$1"
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -c -- "$1"
  else
    echo 'sha256sum or shasum is required' >&2
    return 127
  fi
}
(($# == 1)) || { echo 'Usage: scripts/verify-release.sh PATH.tar.gz' >&2; exit 2; }
archive="$1"; [[ -f "$archive" && -f "$archive.sha256" ]] || { echo 'archive/checksum missing' >&2; exit 2; }
command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 || { echo 'sha256sum or shasum is required' >&2; exit 2; }
base="$(basename "$archive")"
[[ "$base" =~ ^orbit-[0-9]+\.[0-9]+\.[0-9]+([.-][a-zA-Z0-9.-]+)?-[A-Za-z0-9_-]+(-[A-Za-z0-9_-]+)+\.tar\.gz$ ]] || { echo 'unexpected release archive name' >&2; exit 2; }
(cd "$(dirname "$archive")" && sha256_check "$base.sha256")
root="${base%.tar.gz}"
# Check the complete member inventory before extraction. Duplicate members and
# multiple spellings of a path can otherwise overwrite verified content.
entries="$(tar -tzf "$archive")" || { echo 'cannot list archive' >&2; exit 1; }
[[ -n "$entries" ]] || { echo 'empty archive' >&2; exit 1; }
if printf '%s\n' "$entries" | LC_ALL=C sort | uniq -d | grep -q .; then
  echo 'duplicate archive members are not supported' >&2; exit 1
fi
while IFS= read -r entry; do
  case "$entry" in "$root/"|"$root/bin/"|"$root/RELEASE.txt"|"$root/SHA256SUMS"|"$root/bin/orbit"|"$root/bin/orbit-mcp"|"$root/bin/orbit-tui"|"$root/bin/orbit-server") ;;
    *) echo "unexpected archive path: $entry" >&2; exit 1;; esac
done <<< "$entries"
# Exactly one top-level directory, bin directory, metadata file and hash manifest.
for required in "$root/" "$root/bin/" "$root/RELEASE.txt" "$root/SHA256SUMS"; do
  grep -Fxq -- "$required" <<< "$entries" || { echo "missing release member: $required" >&2; exit 1; }
done
# GNU tar verbose type character: only regular files and directories allowed.
if tar -tvzf "$archive" | awk 'substr($1,1,1)!="-" && substr($1,1,1)!="d" {bad=1} END {exit !bad}'; then
 echo 'archive contains unsupported entry type' >&2; exit 1
fi
tmp="$(mktemp -d)"; trap 'rm -rf -- "$tmp"' EXIT
tar -xzf "$archive" -C "$tmp" --no-same-owner --no-same-permissions
# Hash coverage must be exact, not simply a passing subset. Do not permit
# absolute paths, traversal or metadata names that are not part of the bundle.
# A newline-delimited string keeps this working on bash 3.2 (macOS), which has
# no associative arrays.
manifest="$tmp/$root/SHA256SUMS"
hashed=""
has_hash() {
  case "
$hashed" in *"
$1
"*) return 0 ;; *) return 1 ;; esac
}
while IFS= read -r line || [[ -n "$line" ]]; do
  if [[ "$line" =~ ^[[:xdigit:]]{64}[[:space:]]+\*?(bin/(orbit|orbit-mcp|orbit-tui|orbit-server)|RELEASE.txt)$ ]]; then
    path="${BASH_REMATCH[1]}"
    if has_hash "$path"; then
      echo "duplicate checksum target: $path" >&2; exit 1
    fi
    hashed="${hashed}${path}
"
  else
    echo "invalid checksum entry" >&2; exit 1
  fi
done < "$manifest"
has_hash RELEASE.txt || { echo 'missing RELEASE.txt checksum' >&2; exit 1; }
found=0
for name in orbit orbit-mcp orbit-tui orbit-server; do
  if [[ -f "$tmp/$root/bin/$name" ]]; then
    has_hash "bin/$name" || { echo "unhashed executable: $name" >&2; exit 1; }
    ((found+=1))
  elif has_hash "bin/$name"; then
    echo "missing checksummed executable: $name" >&2; exit 1
  fi
done
(( found > 0 )) || { echo 'release has no executables' >&2; exit 1; }
(cd "$tmp/$root" && sha256_check SHA256SUMS)
echo 'Release archive verified'
