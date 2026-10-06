#!/usr/bin/env bash
set -euo pipefail
umask 022
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH" CONVT_LICENSE_STORE=file PYTHONDONTWRITEBYTECODE=1
repo=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo"
verification=0
case ${1:---dry-run} in
  --dry-run) shift || true ;;
  --verification-only) verification=1; shift ;;
  *) echo 'usage: bun run release:linux [--dry-run|--verification-only] [OUTPUT_ROOT]' >&2; exit 2 ;;
esac
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml","rb"))["workspace"]["package"]["version"])')
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2
: "${SOURCE_DATE_EPOCH:?Set the release Unix timestamp in UTC}"
[[ $SOURCE_DATE_EPOCH =~ ^[0-9]+$ ]] || exit 2
build_date=$(date -u -d "@$SOURCE_DATE_EPOCH" +%F)
[[ -z ${CONVT_BUILD_DATE:-} || $CONVT_BUILD_DATE == "$build_date" ]] || { echo 'Build date disagrees with epoch' >&2; exit 2; }
export CONVT_BUILD_DATE=$build_date CONVT_PACKAGE_VERSION=$version
output_root=${1:-$repo/packaging/out/releases}
mkdir -p "$output_root"
output_root=$(realpath "$output_root")
out="$output_root/$version"
[[ ! -e $out ]] || { echo "Refusing existing version directory: $out" >&2; exit 2; }
mkdir "$out"
work=$(mktemp -d /tmp/convt-release-XXXXXX)
trap 'rm -rf "$work"' EXIT
key=${CONVT_UPDATE_SIGNING_KEY:-}
if (( verification )); then
  export CONVT_VERIFICATION_ONLY=1 CONVT_RELEASE=0
  # A dry-run key is intentionally temporary and distinct from licenses.
  if [[ -z $key ]]; then key="$work/update.seed"; bun scripts/release/manifest.ts keygen "$key" > "$out/update-public-key.txt"; fi
else
  export CONVT_RELEASE=1 CONVT_LICENSE_ENFORCE=1
  : "${CONVT_LICENSE_PUBKEY:?Production license public key required}"
  : "${CONVT_UPDATE_PUBKEY:?Production update public key required}"
  : "${CONVT_UPDATE_SIGNING_KEY:?External update seed required}"
fi
if (( verification )); then
  export CONVT_UPDATE_PUBKEY=$(bun scripts/release/manifest.ts public-key "$key")
fi
export CONVT_BUNDLE_CACHE=${CONVT_BUNDLE_CACHE:-$repo/packaging/.cache}
if [[ -n ${CONVT_RELEASE_SOURCE_TREE:-} ]]; then
  python3 scripts/release/source.py check "$CONVT_RELEASE_SOURCE_TREE"
  cp -a "$CONVT_RELEASE_SOURCE_TREE" "$work/convt-source"
else
  python3 scripts/release/source.py snapshot "$work/convt-source"
fi
source_tree="$work/convt-source"
export CONVT_BUNDLE_OUT="$out"
# Work is confined to this snapshot. Optional cache is compilation only, never
# a substitute for rebuilding and auditing the actual payload.
export CONVT_BUILD_WORK=${CONVT_RELEASE_BUILD_WORK:-$work/build}
(
  cd "$source_tree"
  bash packaging/linux/build.sh
  bash packaging/linux/appimage.sh
  bash packaging/linux/package.sh
)
flags=()
(( ! verification )) || flags+=(--verification-only)
python3 scripts/release/source.py archive "$source_tree" "$out" "$version" "$SOURCE_DATE_EPOCH" "${flags[@]}"
timeout --foreground 900 bash scripts/release/rebuild-cli.sh "$out/convt-$version-source.tar.gz"
bun scripts/release/manifest.ts generate "$out" "$version" "$build_date" "${CONVT_RELEASE_BASE_URL:-https://downloads.convt.app}" ${CONVT_RELEASE_HISTORY:+"$CONVT_RELEASE_HISTORY"}
bun scripts/release/manifest.ts sign "$out/release-manifest.json" "$key" "$out/update-manifest.json"
bun scripts/release/manifest.ts verify "$out/release-manifest.json" "$out/update-manifest.json"
python3 scripts/release/templates.py "$out/release-manifest.json" "$out/repositories"
# No network writes here. Upload is a separate, gated command.
bash scripts/release/upload.sh --dry-run "$out"
echo "Release dry run: $out"
