#!/usr/bin/env bash
# Create an offline, checksum-verifiable archive of Orbit binaries.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FROM_DIR=""; OUTPUT="$ROOT/dist"; COMPONENTS="cli"; TARGET=""; VERSION=""; BUILD=0
usage() { cat <<'HELP'
Usage: scripts/package-release.sh --from-dir BIN_DIR [--output DIR] [--components cli,mcp,tui,server] [--target RUST_TARGET] [--version VERSION]
       scripts/package-release.sh --build [--output DIR] [--components ...] [--target ...]
Creates orbit-VERSION-TARGET.tar.gz and matching .sha256 without downloading dependencies.
When --target is specified, the provided binaries MUST be built for that target.
HELP
}
while (($#)); do
 case "$1" in
 --from-dir|--output|--components|--target|--version) (($# >= 2)) || exit 2; case "$1" in
 --from-dir) FROM_DIR="$2";; --output) OUTPUT="$2";; --components) COMPONENTS="$2";; --target) TARGET="$2";; --version) VERSION="$2";; esac; shift 2;;
 --build) BUILD=1; shift;; -h|--help) usage; exit 0;; *) usage >&2; exit 2;; esac
done
[[ "$BUILD" == 0 || -z "$FROM_DIR" ]] || { echo 'choose --build or --from-dir' >&2; exit 2; }
[[ "$BUILD" == 1 || -n "$FROM_DIR" ]] || { echo 'provide --build or --from-dir' >&2; exit 2; }
if [[ -z "$VERSION" ]]; then VERSION="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"; fi
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][a-zA-Z0-9.-]+)?$ ]] || { echo 'invalid version' >&2; exit 2; }
if [[ -z "$TARGET" ]]; then
 command -v rustc >/dev/null || { echo 'specify --target when Rust is unavailable' >&2; exit 2; }
 TARGET="$(rustc -vV | sed -n 's/^host: //p')"
fi
[[ "$TARGET" =~ ^[A-Za-z0-9_]+(-[A-Za-z0-9_]+)+$ ]] || { echo 'invalid target triple' >&2; exit 2; }
IFS=, read -ra selected <<< "$COMPONENTS"
((${#selected[@]})) || exit 2
packages=(); sources=(); names=(); declare -A seen=()
for comp in "${selected[@]}"; do
 case "$comp" in cli) src=orbit; dst=orbit;; mcp|tui|server) src="orbit-$comp"; dst="$src";; *) echo "unknown component: $comp" >&2; exit 2;; esac
 [[ ! -v seen[$comp] ]] || { echo "duplicate component: $comp" >&2; exit 2; }
 seen[$comp]=1; sources+=("$src"); names+=("$dst"); packages+=(-p "orbit-$comp")
done
if (( BUILD )); then
 command -v cargo >/dev/null || { echo 'Cargo unavailable' >&2; exit 2; }
 cargo build --locked --release --target "$TARGET" "${packages[@]}" --manifest-path "$ROOT/Cargo.toml"
 FROM_DIR="$ROOT/target/$TARGET/release"
fi
[[ -d "$FROM_DIR" ]] || { echo "missing binary directory: $FROM_DIR" >&2; exit 2; }
# Fail before creating a partially populated archive.
for src in "${sources[@]}"; do
 [[ -f "$FROM_DIR/$src" && -x "$FROM_DIR/$src" && ! -L "$FROM_DIR/$src" ]] || { echo "missing or unsafe executable: $FROM_DIR/$src" >&2; exit 2; }
done
command -v sha256sum >/dev/null || { echo 'sha256sum is required' >&2; exit 2; }
mkdir -p "$OUTPUT"; OUTPUT="$(cd "$OUTPUT" && pwd -P)"
name="orbit-$VERSION-$TARGET"
tmp="$(mktemp -d)"; trap 'rm -rf -- "$tmp"' EXIT
mkdir -p "$tmp/$name/bin"
for i in "${!names[@]}"; do install -m 0755 "$FROM_DIR/${sources[$i]}" "$tmp/$name/bin/${names[$i]}"; done
printf 'Orbit %s\nTarget: %s\nComponents: %s\n' "$VERSION" "$TARGET" "$COMPONENTS" > "$tmp/$name/RELEASE.txt"
(cd "$tmp/$name" && sha256sum bin/* RELEASE.txt > SHA256SUMS)
archive="$OUTPUT/$name.tar.gz"
tar -C "$tmp" -czf "$tmp/$name.tar.gz" "$name"
(cd "$tmp" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
# Move files only after successful archive creation and checksum generation.
mv -f -- "$tmp/$name.tar.gz" "$archive"
mv -f -- "$tmp/$name.tar.gz.sha256" "$archive.sha256"
printf '%s\n%s\n' "$archive" "$archive.sha256"
