#!/usr/bin/env bash
# Linux x86_64, built in pinned AlmaLinux 8 / manylinux (glibc 2.28).
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
[[ $(uname -sm) == 'Linux x86_64' ]] || { echo 'Only Linux x86_64 is locked' >&2; exit 1; }
[[ $(rustc --version) == 'rustc 1.95.0 '* ]] || { echo 'Rust 1.95.0 is required' >&2; exit 1; }
# One build date drives the licence update cutoff embedded by
# crates/convt-license/build.rs, every timestamp inside the build and the
# tarball mtimes. Give SOURCE_DATE_EPOCH or CONVT_BUILD_DATE (YYYY-MM-DD, UTC
# midnight); if both are set they must name the same day. Release builds
# (CONVT_RELEASE=1 or CONVT_LICENSE_ENFORCE=1) fail without one. Dev builds
# default to today, at UTC midnight so a same-day rebuild matches.
if [[ -n ${SOURCE_DATE_EPOCH:-} ]]; then
  [[ $SOURCE_DATE_EPOCH =~ ^[0-9]+$ ]] || { echo "SOURCE_DATE_EPOCH is not a Unix time: $SOURCE_DATE_EPOCH" >&2; exit 1; }
  epoch_date=$(date -u -d "@$SOURCE_DATE_EPOCH" +%F)
  [[ -z ${CONVT_BUILD_DATE:-} || $CONVT_BUILD_DATE == "$epoch_date" ]] || { echo "CONVT_BUILD_DATE $CONVT_BUILD_DATE disagrees with SOURCE_DATE_EPOCH ($epoch_date)" >&2; exit 1; }
  CONVT_BUILD_DATE=$epoch_date
elif [[ -n ${CONVT_BUILD_DATE:-} ]]; then
  [[ $CONVT_BUILD_DATE =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] && SOURCE_DATE_EPOCH=$(date -u -d "$CONVT_BUILD_DATE 00:00:00" +%s 2>/dev/null) \
    && [[ $(date -u -d "@$SOURCE_DATE_EPOCH" +%F) == "$CONVT_BUILD_DATE" ]] || { echo "CONVT_BUILD_DATE is not YYYY-MM-DD: $CONVT_BUILD_DATE" >&2; exit 1; }
elif [[ ${CONVT_RELEASE:-} == 1 || ${CONVT_LICENSE_ENFORCE:-} == 1 ]]; then
  echo 'A release build needs SOURCE_DATE_EPOCH or CONVT_BUILD_DATE: it is the licence update cutoff' >&2
  exit 1
else
  CONVT_BUILD_DATE=$(date -u +%F)
  SOURCE_DATE_EPOCH=$(date -u -d "$CONVT_BUILD_DATE 00:00:00" +%s)
  echo "Dev build dated $CONVT_BUILD_DATE; set SOURCE_DATE_EPOCH for a reproducible build" >&2
fi
export SOURCE_DATE_EPOCH CONVT_BUILD_DATE
echo "Build date $CONVT_BUILD_DATE (SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH)"
cache=${CONVT_BUNDLE_CACHE:-$repo/packaging/.cache}
mkdir -p "$cache"
cache=$(realpath "$cache")
python3 packaging/linux/fetch.py "$cache"
python3 packaging/linux/fetch.py "$cache" packaging/linux/ffmpeg-source-inputs.lock.json
CONVT_BUNDLE_CACHE="$cache" python3 packaging/release/pdfium-source-verify.py packaging/release/pdfium-source.lock.json --fetch
python3 packaging/linux/fetch.py "$cache" packaging/linux/build-rpms.lock.json
work=$(mktemp -d)
image="convt-pkg-builder-$$"
container="convt-pkg-build-$$"
cleanup() { docker rm -f "$container" >/dev/null 2>&1 || true; docker image rm "$image" >/dev/null 2>&1 || true; rm -rf "$work"; }
trap cleanup EXIT
base=$(python3 -c 'import json; print(json.load(open("packaging/linux/build-image.json"))["image"])')
mkdir "$work/context"
python3 - "$cache" "$work/context" "$base" <<'DOCKER'
import json,pathlib,shutil,sys
cache,context=map(pathlib.Path,sys.argv[1:3])
for item in json.load(open('packaging/linux/build-rpms.lock.json')):
    shutil.copy2(cache/item['name'],context/item['name'])
(context/'Dockerfile').write_text('FROM '+sys.argv[3]+'\nCOPY *.rpm /tmp/build-rpms/\nRUN dnf --disablerepo="*" install -y /tmp/build-rpms/*.rpm && rm -rf /tmp/build-rpms\n')
DOCKER
docker build --network none -t "$image" "$work/context"
# The host toolchain/cache are build inputs only. Cargo resolution is locked and
# offline; no host library or target directories enter the builder.
rust=$(dirname "$(dirname "$(rustup which rustc)")")
registry=${CARGO_HOME:-$HOME/.cargo}/registry
build_work=${CONVT_BUILD_WORK:-$work/build}
mkdir -p "$build_work"
build_work=$(realpath "$build_work")
docker run --rm --label app=convt --label purpose=release-build --name "$container" --network none \
  -v "$repo:/repo:ro" -v "$cache:/inputs:ro" -v "$rust:/rust:ro" \
  -v "$registry:/cargo/registry:ro" -v "$build_work:/work" \
  -e "CONVT_DOCUMENT_PACK_URL=${CONVT_DOCUMENT_PACK_URL:-}" \
  -e "CONVT_DOCUMENT_PACK_SHA256=${CONVT_DOCUMENT_PACK_SHA256:-}" \
  -e "CONVT_DOCUMENT_PACK_VERSION=${CONVT_DOCUMENT_PACK_VERSION:-unconfigured}" \
  -e "SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH" -e "CONVT_BUILD_DATE=$CONVT_BUILD_DATE" \
  -e "CONVT_UPDATE_PUBKEY=${CONVT_UPDATE_PUBKEY:-}" \
  -e "CONVT_LICENSE_PUBKEY=${CONVT_LICENSE_PUBKEY:-}" -e "CONVT_LICENSE_ENFORCE=${CONVT_LICENSE_ENFORCE:-}" \
  -e "CONVT_BUILD_CLOCK_OFFSET_DAYS=${CONVT_BUILD_CLOCK_OFFSET_DAYS:-}" \
  -e "CONVT_BUILD_JOBS=${CONVT_BUILD_JOBS:-8}" -e "CONVT_BUILD_UID=$(id -u)" -e "CONVT_BUILD_GID=$(id -g)" "$image" \
  bash /repo/packaging/linux/container-build.sh
out=${CONVT_BUNDLE_OUT:-$repo/packaging/out}
mkdir -p "$out"
[[ ! -e "$out/convt" ]] || { echo "Output already exists: $out/convt" >&2; exit 1; }
cp -a "$build_work/convt" "$out/convt"
cp -a "$build_work/validation-tools" "$out/validation-tools"
license_flags=()
[[ ${CONVT_RELEASE:-0} != 1 ]] || license_flags+=(--release)
python3 packaging/linux/license-metadata.py "$out/convt" "$out/license-metadata" "${license_flags[@]}"
# appimage.sh reads this so the AppImage carries the same timestamps.
echo "$SOURCE_DATE_EPOCH" > "$out/source-date-epoch"
python3 - "$out/convt" "$out/components.json" <<'SIZES'
import json,pathlib,sys
root=pathlib.Path(sys.argv[1])
def size(p):
    return sum(x.stat().st_size for x in p.rglob('*') if x.is_file() and not x.is_symlink())
parts={p.name:(size(p) if p.is_dir() else p.stat().st_size) for p in root.iterdir()}
parts['lib']={p.name:(size(p) if p.is_dir() else p.stat().st_size) for p in (root/'lib').iterdir() if not p.is_symlink()}
pathlib.Path(sys.argv[2]).write_text(json.dumps(parts,indent=2)+'\n')
SIZES
tar --sort=name --mtime="@$SOURCE_DATE_EPOCH" --owner=0 --group=0 --numeric-owner -C "$out" -cf - convt | gzip -n > "$out/convt-linux-x86_64.tar.gz"
(cd "$out" && sha256sum convt-linux-x86_64.tar.gz > convt-linux-x86_64.tar.gz.sha256)
du -sh "$out/convt" "$out/convt-linux-x86_64.tar.gz"
