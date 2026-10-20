#!/usr/bin/env bash
# Remove only Orbit binaries recorded by scripts/install.sh. Data/config untouched.
set -euo pipefail
PREFIX="${HOME}/.local"
while (($#)); do
  case "$1" in --prefix) (($# >= 2)) || exit 2; PREFIX="$2"; shift 2;; -h|--help) echo 'Usage: scripts/uninstall.sh [--prefix DIR]'; exit 0;; *) echo "unknown argument: $1" >&2; exit 2;; esac
done
prefix_abs="$(cd "$PREFIX" && pwd -P)"
manifest="$prefix_abs/share/orbit/installed-binaries.txt"
[[ -f "$manifest" ]] || { echo 'No Orbit installation manifest at requested prefix' >&2; exit 1; }
[[ ! -L "$manifest" ]] || { echo "refusing symlinked installation manifest" >&2; exit 1; }
# Validate the entire manifest before removing anything.
while IFS= read -r name; do
  case "$name" in orbit|orbit-mcp|orbit-server|orbit-tui) ;; *) echo "invalid manifest entry: $name" >&2; exit 1;; esac
done < "$manifest"
while IFS= read -r name; do
  [[ ! -L "$prefix_abs/bin/$name" ]] || { echo "refusing symlinked executable: $name" >&2; exit 1; }
done < "$manifest"
while IFS= read -r name; do
  rm -f -- "$prefix_abs/bin/$name"
done < "$manifest"
rm -f "$manifest"
echo 'Orbit binaries removed. User databases, settings and other files were preserved.'
