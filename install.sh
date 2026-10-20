#!/bin/sh
# Orbit GitHub-hosted installer.
#
# Public usage:
#   curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh
#
# This script is POSIX sh compatible because it is normally piped into `sh`.
# It downloads a published release archive, verifies its checksum, and then
# reuses the transactional installer from scripts/install.sh.
set -eu

die() {
  printf 'orbit installer: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage: install.sh [--version VERSION_OR_TAG] [--prefix DIR] [--dry-run] [--help]

Install Orbit prebuilt binaries from GitHub releases into DIR/bin.
The default prefix is ~/.local and only the `orbit` CLI is installed.

Options:
  --version VERSION_OR_TAG  Install a specific release (v0.1.0 or 0.1.0).
                            Without this option the latest stable release is used.
  --prefix DIR              Installation prefix (default: ~/.local).
  --dry-run                 Print the resolved tag, target, asset URL and prefix
                            without downloading or installing anything.
  -h, --help                Show this help.

Environment:
  ORBIT_VERSION             Same as --version.
  ORBIT_INSTALL_PREFIX      Same as --prefix.
  ORBIT_TARGET              Override the detected target triple (tests only).
  ORBIT_REPO                GitHub repository (default: xxxiio/orbit).
  ORBIT_API_BASE            GitHub API base URL.
  ORBIT_DOWNLOAD_BASE       Release asset download base URL. The installer
                            also fetches its helper scripts from this release.
EOF
}

VERSION="${ORBIT_VERSION:-}"
PREFIX="${ORBIT_INSTALL_PREFIX:-}"
DRY_RUN=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || die 'missing value for --version'
      VERSION="$2"
      shift 2
      ;;
    --prefix)
      [ "$#" -ge 2 ] || die 'missing value for --prefix'
      PREFIX="$2"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

if [ -z "$PREFIX" ]; then
  [ -n "${HOME:-}" ] || die 'HOME is not set; pass --prefix or set ORBIT_INSTALL_PREFIX'
  PREFIX="$HOME/.local"
fi
[ -n "$PREFIX" ] || die 'installation prefix is empty'
[ "$PREFIX" != / ] || die 'refusing to install into /'
case "$PREFIX" in
  /*) ;;
  *) PREFIX="$(pwd)/$PREFIX" ;;
esac

# Resolve the target triple. ORBIT_TARGET is an explicit test/unsupported-host
# override and must still name a release that Orbit publishes.
if [ -n "${ORBIT_TARGET:-}" ]; then
  TARGET="$ORBIT_TARGET"
else
  uname_s="$(uname -s 2>/dev/null || true)"
  uname_m="$(uname -m 2>/dev/null || true)"
  case "$uname_s" in
    Linux) os_part='unknown-linux-gnu' ;;
    Darwin) os_part='apple-darwin' ;;
    *) die "unsupported operating system: ${uname_s:-unknown}; supported: Linux and Darwin" ;;
  esac
  case "$uname_m" in
    x86_64|amd64) arch_part='x86_64' ;;
    arm64|aarch64) arch_part='aarch64' ;;
    *) die "unsupported CPU architecture: ${uname_m:-unknown}; supported: x86_64/amd64 and arm64/aarch64" ;;
  esac
  TARGET="$arch_part-$os_part"
fi
case "$TARGET" in
  x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|x86_64-apple-darwin|aarch64-apple-darwin) ;;
  *) die "unsupported target: $TARGET; supported: x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu, x86_64-apple-darwin, aarch64-apple-darwin" ;;
esac

# Fail before creating any temporary state or touching the prefix.
for tool in curl tar gzip mktemp bash grep sed tr head cat sort uniq awk install cp mv chmod rm mkdir basename dirname; do
  command -v "$tool" >/dev/null 2>&1 \
    || die "required tool '$tool' was not found in PATH; install it and retry"
done
if ! command -v sha256sum >/dev/null 2>&1 && ! command -v shasum >/dev/null 2>&1; then
  die 'sha256sum or shasum is required to verify downloads'
fi

ORBIT_REPO="${ORBIT_REPO:-xxxiio/orbit}"
ORBIT_API_BASE="${ORBIT_API_BASE:-https://api.github.com/repos/$ORBIT_REPO}"
ORBIT_DOWNLOAD_BASE="${ORBIT_DOWNLOAD_BASE:-https://github.com/$ORBIT_REPO/releases/download}"
ORBIT_API_BASE="${ORBIT_API_BASE%/}"
ORBIT_DOWNLOAD_BASE="${ORBIT_DOWNLOAD_BASE%/}"

resolved_latest=0

if [ -n "$VERSION" ]; then
  requested_version="$VERSION"
  case "$requested_version" in
    v*) version="${requested_version#v}" ;;
    *) version="$requested_version" ;;
  esac
  case "$version" in
    ''|*[!0-9A-Za-z.+-]*) die "invalid version: $requested_version" ;;
    [0-9]*) ;;
    *) die "invalid version: $requested_version" ;;
  esac
  tag="v$version"
  case "$version" in
    *-*) printf 'orbit installer: warning: %s looks like a prerelease; installing it because it was requested explicitly.\n' "$version" >&2 ;;
  esac
else
  resolved_latest=1
  api_url="$ORBIT_API_BASE/releases/latest"
  json="$(curl --fail --silent --show-error --location --retry 3 --retry-delay 1 "$api_url")" \
    || die "could not resolve the latest stable release from $api_url"

  tag="$(printf '%s\n' "$json" \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n 1)"
  [ -n "$tag" ] || die "could not find tag_name in the latest release response from $api_url"

  # Collapse whitespace so both compact and pretty-printed JSON are handled.
  compact="$(printf '%s' "$json" | tr -d '[:space:]')"
  case "$compact" in
    *'"prerelease":true'*) die "latest release '$tag' is a prerelease; refusing to install it as stable" ;;
  esac
  case "$compact" in
    *'"prerelease":false'*) ;;
    *) die "latest release '$tag' does not report prerelease: false; refusing to install it as stable" ;;
  esac
  case "$compact" in
    *'"draft":true'*) die "latest release '$tag' is a draft; refusing to install it" ;;
  esac
  case "$compact" in
    *'"draft":false'*) ;;
    *) die "latest release '$tag' does not report draft: false; refusing to install it" ;;
  esac

  case "$tag" in
    *[!0-9A-Za-z._+-]*) die "latest release tag has unsupported characters: $tag" ;;
  esac
  version="${tag#v}"
  case "$version" in
    ''|*[!0-9A-Za-z.+-]*) die "latest release tag is not a usable version: $tag" ;;
  esac
fi

archive_name="orbit-$version-$TARGET.tar.gz"
asset_url="$ORBIT_DOWNLOAD_BASE/$tag/$archive_name"
sha_url="$asset_url.sha256"

# For a remote latest lookup, confirm the checksum asset exists as part of
# resolution. For explicit versions, only do this when actually installing.
if [ "$resolved_latest" -eq 1 ] || [ "$DRY_RUN" -eq 0 ]; then
  curl --fail --silent --show-error --location --head --retry 3 --retry-delay 1 "$sha_url" >/dev/null 2>&1 \
    || die "checksum asset is not reachable: $sha_url"
fi

if [ "$DRY_RUN" -eq 1 ]; then
  printf 'Tag: %s\n' "$tag"
  printf 'Version: %s\n' "$version"
  printf 'Target: %s\n' "$TARGET"
  printf 'Asset URL: %s\n' "$asset_url"
  printf 'Checksum URL: %s\n' "$sha_url"
  printf 'Installer helper URL: %s/%s/orbit-install-core.sh\n' "$ORBIT_DOWNLOAD_BASE" "$tag"
  printf 'Prefix: %s\n' "$PREFIX"
  exit 0
fi

saved_umask="$(umask)"
umask 077
tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/orbit-install.XXXXXXXX")" || {
  umask "$saved_umask"
  die 'could not create a temporary directory'
}
umask "$saved_umask"

cleanup() {
  [ -z "${tmpdir:-}" ] || rm -rf -- "$tmpdir"
}
trap cleanup 0
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

download_dir="$tmpdir/download"
scripts_dir="$tmpdir/scripts"
mkdir -p "$download_dir" "$scripts_dir"
archive_path="$download_dir/$archive_name"
sha_path="$archive_path.sha256"

curl --fail --location --retry 3 --retry-delay 1 --silent --show-error \
  --output "$archive_path" "$asset_url" \
  || die "failed to download $asset_url"
curl --fail --location --retry 3 --retry-delay 1 --silent --show-error \
  --output "$sha_path" "$sha_url" \
  || die "failed to download the checksum from $sha_url"

[ -s "$archive_path" ] || die "downloaded archive is empty: $archive_name"
[ -s "$sha_path" ] || die "downloaded checksum file is empty: $archive_name.sha256"

# Reject empty or unrelated checksum files before handing them to `-c`.
awk -v name="$archive_name" '
  { hash = $1; file = $2; sub(/^\*/, "", file) }
  length(hash) == 64 && hash ~ /^[0-9a-fA-F]+$/ && file == name { found = 1 }
  END { exit found ? 0 : 1 }
' "$sha_path" \
  || die "checksum file does not contain a valid hash for $archive_name"

if command -v sha256sum >/dev/null 2>&1; then
  (cd "$download_dir" && sha256sum -c -- "$archive_name.sha256") \
    || die "sha256 checksum verification failed for $archive_name"
elif command -v shasum >/dev/null 2>&1; then
  (cd "$download_dir" && shasum -a 256 -c -- "$archive_name.sha256") \
    || die "sha256 checksum verification failed for $archive_name"
else
  die 'sha256sum or shasum is required to verify downloads'
fi
printf 'Checksum verified. Note: same-origin checksums catch corruption in transit but are not publisher authenticity.\n'

# The transactional installer and verifier are attached to the same release as
# the archives. Fetching them from the release asset namespace avoids raw source
# hosting while keeping the public entry point on github.com.
download_helper() {
  asset="$1"
  destination="$2"
  url="$ORBIT_DOWNLOAD_BASE/$tag/$asset"
  curl --fail --location --retry 3 --retry-delay 1 --silent --show-error \
    --output "$destination" "$url" \
    || die "failed to download release installer asset $asset for tag $tag"
  [ -s "$destination" ] || die "downloaded release installer asset $asset is empty"
  chmod 0755 "$destination"
}

download_helper orbit-install-core.sh "$scripts_dir/install.sh"
download_helper orbit-verify-release.sh "$scripts_dir/verify-release.sh"

# Reuse the M26 transactional installer. It verifies the archive again, stages
# every binary, and rolls back on failure. Install the default CLI only.
bash "$scripts_dir/install.sh" --from-archive "$archive_path" --prefix "$PREFIX" --components cli \
  || die 'Orbit installation failed; any existing installation was left unchanged'

case ":${PATH:-}:" in
  *":$PREFIX/bin:"*) ;;
  *) printf 'Add %s/bin to PATH to run orbit from any directory (for example: export PATH="%s/bin:$PATH").\n' "$PREFIX" "$PREFIX" ;;
esac
printf 'Orbit %s installed into %s/bin\n' "$version" "$PREFIX"
