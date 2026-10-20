#!/usr/bin/env bash
# Orbit installer: source build or install prebuilt binaries, no sudo and no network fetches.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="${HOME}/.local"; SOURCE_DIR=""; COMPONENTS="cli"; PROFILE="release"; DRY_RUN=0; CHECK=0; ARCHIVE=""; COMPONENTS_SET=0; EXTRACT_DIR=""
usage() { cat <<'EOF'
Usage: scripts/install.sh [--prefix DIR] [--components cli,mcp,tui,server] [--from-dir DIR | --from-archive FILE.tar.gz] [--debug] [--dry-run] [--check]
Default installs only `orbit` into ~/.local/bin using cargo build --locked --release.
--from-dir installs prebuilt binaries without needing Rust; no software is downloaded.
--from-archive verifies the archive and checksums, discovers bundled components, then installs offline.
Run scripts/uninstall.sh --prefix DIR to remove binaries recorded by this installer.
EOF
}
while (($#)); do
  case "$1" in
    --prefix|--components|--from-dir|--from-archive) (($# >= 2)) || { echo "missing value: $1" >&2; exit 2; }; case "$1" in --prefix) PREFIX="$2";; --components) COMPONENTS="$2"; COMPONENTS_SET=1;; --from-dir) SOURCE_DIR="$2";; --from-archive) ARCHIVE="$2";; esac; shift 2;;
    --debug) PROFILE=debug; shift;;
    --dry-run) DRY_RUN=1; shift;;
    --check) CHECK=1; shift;;
    -h|--help) usage; exit 0;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2;;
  esac
done
[[ -z "$ARCHIVE" || -z "$SOURCE_DIR" ]] || { echo 'choose --from-dir or --from-archive' >&2; exit 2; }
[[ -z "$ARCHIVE" || "$PROFILE" == release ]] || { echo '--debug cannot be used with --from-archive' >&2; exit 2; }
if [[ -n "$ARCHIVE" && "$CHECK" == 0 && "$DRY_RUN" == 0 ]]; then
  "$ROOT/scripts/verify-release.sh" "$ARCHIVE" || exit 1
  EXTRACT_DIR="$(mktemp -d)"
  trap '[[ -z "$EXTRACT_DIR" ]] || rm -rf -- "$EXTRACT_DIR"' EXIT
  tar -xzf "$ARCHIVE" -C "$EXTRACT_DIR" --no-same-owner --no-same-permissions
  archive_root="$(basename "$ARCHIVE" .tar.gz)"
  bundle="$EXTRACT_DIR/$archive_root/bin"
  [[ -d "$bundle" ]] || { echo 'release bundle has no bin directory' >&2; exit 1; }
  selected=()
  for comp in cli mcp tui server; do
    name="orbit-$comp"; [[ "$comp" != cli ]] || name=orbit
    if [[ -f "$bundle/$name" ]]; then selected+=("$comp"); fi
  done
  ((${#selected[@]} > 0)) || { echo 'release contains no Orbit executables' >&2; exit 1; }
  if (( COMPONENTS_SET )); then
    IFS=',' read -ra requested <<< "$COMPONENTS"
    for comp in "${requested[@]}"; do
      found=0; for available in "${selected[@]}"; do [[ "$comp" != "$available" ]] || found=1; done
      (( found )) || { echo "requested component missing from archive: $comp" >&2; exit 2; }
    done
  else
    COMPONENTS="$(IFS=,; echo "${selected[*]}")"
  fi
  SOURCE_DIR="$EXTRACT_DIR/prebuilt"
  mkdir -p "$SOURCE_DIR"
  for comp in "${selected[@]}"; do
    name="orbit-$comp"; [[ "$comp" != cli ]] || name=orbit
    src="$name"
    cp -- "$bundle/$name" "$SOURCE_DIR/$src"
    chmod 0755 "$SOURCE_DIR/$src"
  done
fi
[[ -n "$PREFIX" && "$PREFIX" != / ]] || { echo 'invalid installation prefix' >&2; exit 2; }
mkdir_components=(); IFS=',' read -ra names <<< "$COMPONENTS"
((${#names[@]} > 0)) || { echo 'empty component list' >&2; exit 2; }
for n in "${names[@]}"; do
  case "$n" in cli|mcp|server|tui) ;; *) echo "unknown component: $n" >&2; exit 2;; esac
  for existing in "${mkdir_components[@]}"; do [[ "$n" != "$existing" ]] || { echo "duplicate component: $n" >&2; exit 2; }; done
  mkdir_components+=("$n")
done
if (( CHECK )); then
  [[ -d "$PREFIX" ]] || { echo "prefix does not exist: $PREFIX" >&2; exit 1; }
  prefix_abs="$(cd "$PREFIX" && pwd -P)"
else
  prefix_abs="$(mkdir -p -- "$PREFIX" && cd "$PREFIX" && pwd -P)"
fi
[[ "$prefix_abs" != / ]] || exit 2
packages=(); binaries=()
for n in "${mkdir_components[@]}"; do
  packages+=("-p" "orbit-$n")
  if [[ "$n" == cli ]]; then binaries+=(orbit); else binaries+=("orbit-$n"); fi
done
if [[ -z "$SOURCE_DIR" && "$CHECK" == 0 ]]; then
  command -v cargo >/dev/null || { echo 'cargo not found; install Rust or use --from-dir' >&2; exit 2; }
  if (( ! DRY_RUN )); then
    args=(build --locked "${packages[@]}"); [[ "$PROFILE" != release ]] || args+=(--release)
    (cd "$ROOT" && cargo "${args[@]}")
  fi
  SOURCE_DIR="$ROOT/target/$PROFILE"
fi
[[ "$CHECK" == 1 || -d "$SOURCE_DIR" || "$DRY_RUN" == 1 ]] || { echo "binary directory missing: $SOURCE_DIR" >&2; exit 2; }
install_names=()
for i in "${!binaries[@]}"; do
  src="$SOURCE_DIR/${binaries[$i]}"
  dest="${binaries[$i]}"
  if (( ! DRY_RUN && ! CHECK )); then
    [[ -f "$src" && -x "$src" ]] || { echo "required executable missing: $src" >&2; exit 2; }
  fi
  install_names+=("$dest")
done
manifest="$prefix_abs/share/orbit/installed-binaries.txt"
if [[ -L "$manifest" ]]; then echo "refusing symlinked installation manifest: $manifest" >&2; exit 1; fi
if [[ -f "$manifest" ]]; then
  declare -A installed_seen=()
  while IFS= read -r previous || [[ -n "$previous" ]]; do
    case "$previous" in orbit|orbit-mcp|orbit-tui|orbit-server) ;; *) echo "invalid installation manifest entry: $previous" >&2; exit 1;; esac
    [[ ! -v installed_seen[$previous] ]] || { echo "duplicate installation manifest entry: $previous" >&2; exit 1; }
    installed_seen[$previous]=1
  done < "$manifest"
fi
if (( CHECK )); then
  [[ -f "$manifest" ]] || { echo "no installation manifest: $manifest" >&2; exit 1; }
  for name in "${install_names[@]}"; do
    grep -Fxq -- "$name" "$manifest" || { echo "not installed: $name" >&2; exit 1; }
    [[ -x "$prefix_abs/bin/$name" ]] || { echo "missing executable: $name" >&2; exit 1; }
  done
  printf 'Installed and executable: %s\n' "${install_names[*]}"
  exit 0
fi
if (( DRY_RUN )); then printf 'Would install %s into %s/bin\n' "${install_names[*]}" "$prefix_abs"; exit 0; fi
mkdir -p "$prefix_abs/bin" "$prefix_abs/share/orbit"
# Do not overwrite a binary not owned by a prior Orbit manifest.
for dest in "${install_names[@]}"; do
  [[ ! -L "$prefix_abs/bin/$dest" ]] || { echo "refusing symlinked executable: $dest" >&2; exit 1; }
  if [[ -e "$prefix_abs/bin/$dest" ]]; then
    if [[ ! -f "$manifest" ]] || ! grep -Fxq -- "$dest" "$manifest"; then
      echo "refusing to overwrite unmanaged executable: $prefix_abs/bin/$dest" >&2; exit 1
    fi
  fi
done
# Stage all binaries before touching an existing installation.
stage="$(mktemp -d "$prefix_abs/bin/.orbit-stage.XXXXXXXX")"
backup="$(mktemp -d "$prefix_abs/bin/.orbit-backup.XXXXXXXX")"
committed=0
applied_names=()
cleanup() {
  if (( ! committed )); then
    for name in "${applied_names[@]}"; do
      if [[ -f "$backup/$name" ]]; then
        mv -f -- "$backup/$name" "$prefix_abs/bin/$name"
      else
        rm -f -- "$prefix_abs/bin/$name"
      fi
    done
  fi
  rm -rf -- "$stage" "$backup"
  [[ -z "$EXTRACT_DIR" ]] || rm -rf -- "$EXTRACT_DIR"
}
trap cleanup EXIT
for i in "${!binaries[@]}"; do
  install -m 0755 "${SOURCE_DIR}/${binaries[$i]}" "$stage/${install_names[$i]}"
done
for name in "${install_names[@]}"; do
  [[ ! -e "$prefix_abs/bin/$name" ]] || cp -p -- "$prefix_abs/bin/$name" "$backup/$name"
done
for name in "${install_names[@]}"; do
  mv -f -- "$stage/$name" "$prefix_abs/bin/$name"
  applied_names+=("$name")
done
# Record all installed components only after all binaries are in place.
{ [[ ! -f "$manifest" ]] || cat "$manifest"; printf '%s\n' "${install_names[@]}"; } | LC_ALL=C sort -u > "$stage/manifest"
mv -f -- "$stage/manifest" "$manifest"
committed=1
printf 'Installed: %s\nLocation: %s/bin\n' "${install_names[*]}" "$prefix_abs"
printf 'Add %s/bin to PATH if necessary. Try: orbit --help\n' "$prefix_abs"
