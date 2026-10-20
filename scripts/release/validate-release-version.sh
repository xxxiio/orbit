#!/usr/bin/env bash
# Validate a release tag against [workspace.package] version in Cargo.toml.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CARGO_TOML="$ROOT/Cargo.toml"
TAG=""
PRINT_VERSION=0

usage() {
  cat <<'HELP'
Usage: scripts/release/validate-release-version.sh TAG [--cargo-toml PATH]
       scripts/release/validate-release-version.sh --print-version [--cargo-toml PATH]

TAG must be vX.Y.Z or vX.Y.Z-suffix and must exactly match the
[workspace.package] version in Cargo.toml (without the leading "v").

Options:
  --cargo-toml PATH  Read the workspace version from PATH instead of the
                     repository Cargo.toml.
  --print-version    Print the workspace version and exit.
HELP
}

fail() {
  echo "error: $*" >&2
  exit 2
}

while (($#)); do
  case "$1" in
    --cargo-toml)
      (($# >= 2)) || fail "--cargo-toml requires a path"
      CARGO_TOML="$2"
      shift 2
      ;;
    --print-version)
      PRINT_VERSION=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      usage >&2
      fail "unknown option: $1"
      ;;
    *)
      [[ -z "$TAG" ]] || fail "unexpected extra argument: $1"
      TAG="$1"
      shift
      ;;
  esac
done

[[ -f "$CARGO_TOML" ]] || fail "Cargo.toml not found: $CARGO_TOML"

version="$(sed -n '/^\[workspace\.package\]/,/^\[/s/^[[:space:]]*version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$CARGO_TOML" | head -n 1)"
[[ -n "$version" ]] || fail "could not read [workspace.package] version from $CARGO_TOML"

validate_tag() {
  local tag="$1" tag_version
  [[ -n "$tag" ]] || fail "missing release tag; expected vX.Y.Z or vX.Y.Z-suffix"
  [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z][0-9A-Za-z.-]*)?$ ]] ||
    fail "invalid release tag '$tag'; expected vX.Y.Z or vX.Y.Z-suffix"
  tag_version="${tag#v}"
  [[ "$tag_version" == "$version" ]] ||
    fail "release tag '$tag' does not match [workspace.package] version '$version' in $CARGO_TOML"
}

if ((PRINT_VERSION)); then
  if [[ -n "$TAG" ]]; then
    validate_tag "$TAG"
  fi
  printf '%s\n' "$version"
  exit 0
fi

validate_tag "$TAG"
printf 'release version %s matches tag %s\n' "$version" "$TAG"
