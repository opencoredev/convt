#!/usr/bin/env bash
# Standard package tools run in digest-pinned containers, never on the host.
set -euo pipefail
umask 022
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"
family=${1:-all}
[[ $family == deb || $family == rpm || $family == all ]] || exit 2
out=$(realpath "${CONVT_BUNDLE_OUT:-$repo/packaging/out}")
bundle_epoch=$(cat "$out/source-date-epoch")
[[ -z ${SOURCE_DATE_EPOCH:-} || $SOURCE_DATE_EPOCH == "$bundle_epoch" ]] || { echo 'SOURCE_DATE_EPOCH differs from the payload' >&2; exit 1; }
export SOURCE_DATE_EPOCH=$bundle_epoch
version=${CONVT_PACKAGE_VERSION:-0.1.0}
release=${CONVT_PACKAGE_RELEASE:-1}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && $release =~ ^[0-9]+$ ]] || { echo 'Invalid package version or release' >&2; exit 2; }
work=$(mktemp -d /tmp/convt-pkg-package-XXXXXX)
images=()
cleanup() { for kind in deb rpm; do docker rm -f "convt-pkg-package-$kind-$$" >/dev/null 2>&1 || true; done; for image in "${images[@]}"; do docker image rm "$image" >/dev/null 2>&1 || true; done; rm -rf "$work"; }
trap cleanup EXIT
mkdir -p "$work/root/opt" "$work/root/usr/bin" "$work/artifacts"
cp -a "$out/convt" "$work/root/opt/convt"
ln -s /opt/convt/convt "$work/root/usr/bin/convt"
ln -s /opt/convt/convt-app "$work/root/usr/bin/convt-app"
python3 packaging/linux/system-menus.py "$work/root" "$out/convt/convt"
install -m755 packaging/linux/menu-migration-message.sh "$work/root/usr/share/convt/menu-migration-message.sh"
mkdir -p "$work/root/usr/share/doc/convt"
license_flags=()
[[ ${CONVT_RELEASE:-0} != 1 ]] || license_flags+=(--release)
python3 packaging/linux/license-metadata.py "$work/root/opt/convt" "$work/root/usr/share/doc/convt" "${license_flags[@]}"
mkdir -p "$work/root/usr/share/man/man1"
for page in convt convt-app; do gzip -n -9 -c "packaging/linux/$page.1" > "$work/root/usr/share/man/man1/$page.1.gz"; done
printf 'convt (%s-%s) unstable; urgency=medium\n\n  * Package the audited Linux payload and system integration.\n\n -- Convt <support@convt.app>  %s\n' "$version" "$release" "$(date -u -d "@$SOURCE_DATE_EPOCH" -R)" | gzip -n -9 > "$work/root/usr/share/doc/convt/changelog.Debian.gz"
# Normalize all modes and times, and omit generated Python caches.
find "$work/root" -type d -name __pycache__ -exec rm -rf {} +
find "$work/root" -type d -exec chmod 755 {} +
find "$work/root" -type f -perm /111 -exec chmod 755 {} +
find "$work/root" -type f ! -perm /111 -exec chmod 644 {} +
find "$work/root/opt/convt/lib" -type f -name '*.so*' -exec chmod 755 {} +
find "$work/root" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
for kind in deb rpm; do
  [[ $family == all || $family == "$kind" ]] || continue
  base=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))[sys.argv[2]])' packaging/linux/package-images.lock.json "$kind")
  image="convt-pkg-package-$kind-$$"
  images+=("$image")
  mkdir "$work/$kind"
  cache="${CONVT_BUNDLE_CACHE:-$repo/packaging/.cache}/package-tools-$kind"
  python3 packaging/linux/fetch.py "$cache" "packaging/linux/package-tools-$kind.lock.json"
  python3 - "$cache" "$work/$kind" "packaging/linux/package-tools-$kind.lock.json" <<'TOOLS'
import json,pathlib,shutil,sys
for item in json.load(open(sys.argv[3])):
    shutil.copyfile(pathlib.Path(sys.argv[1])/item['name'],pathlib.Path(sys.argv[2])/item['name'])
TOOLS
  if [[ $kind == deb ]]; then
    printf 'FROM %s\nCOPY *.deb /tmp/tools/\nRUN dpkg -i /tmp/tools/*.deb && rm -rf /tmp/tools\n' "$base" > "$work/$kind/Dockerfile"
  else
    printf 'FROM %s\nCOPY *.rpm /tmp/tools/\nRUN dnf --disablerepo=\"*\" install -y /tmp/tools/*.rpm && rm -rf /tmp/tools\n' "$base" > "$work/$kind/Dockerfile"
  fi
  docker build --network none -t "$image" "$work/$kind"
  docker run --rm --label app=convt --label purpose=release-build --name "convt-pkg-package-$kind-$$" --network none \
    -v "$repo:/repo:ro" -v "$work:/work" -e SOURCE_DATE_EPOCH -e "CONVT_BUILD_UID=$(id -u)" -e "CONVT_BUILD_GID=$(id -g)" \
    -e "CONVT_PACKAGE_VERSION=$version" -e "CONVT_PACKAGE_RELEASE=$release" "$image" \
    bash /repo/packaging/linux/container-package.sh "$kind"
done
cp "$work/artifacts/"* "$out/"
(cd "$out"; sha256sum convt_*.deb convt-*.rpm 2>/dev/null > packages.sha256) || true
# P11 signing hook: sign copies of completed artifacts in release CI. This
# builder intentionally never invokes dpkg-sig, debsigs or rpmsign.
ls -lh "$out/"*.deb "$out/"*.rpm 2>/dev/null || true
