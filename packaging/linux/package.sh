#!/usr/bin/env bash
# Standard package tools run in digest-pinned containers, never on the host.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"
family=${1:-all}
[[ $family == deb || $family == rpm || $family == all ]] || exit 2
out=$(realpath "${CONVT_BUNDLE_OUT:-$repo/packaging/out}")
export SOURCE_DATE_EPOCH=$(cat "$out/source-date-epoch")
version=${CONVT_PACKAGE_VERSION:-0.1.0}
release=${CONVT_PACKAGE_RELEASE:-1}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && $release =~ ^[0-9]+$ ]] || { echo 'Invalid package version or release' >&2; exit 2; }
work=$(mktemp -d /tmp/convt-pkg-package-XXXXXX)
images=()
cleanup() { rm -rf "$work"; for image in "${images[@]}"; do docker image rm "$image" >/dev/null 2>&1 || true; done; }
trap cleanup EXIT
mkdir -p "$work/root/opt" "$work/root/usr/bin" "$work/artifacts"
cp -a "$out/convt" "$work/root/opt/convt"
ln -s /opt/convt/convt "$work/root/usr/bin/convt"
ln -s /opt/convt/convt-app "$work/root/usr/bin/convt-app"
python3 packaging/linux/system-menus.py "$work/root" "$out/convt/convt"
mkdir -p "$work/root/usr/share/doc/convt"
cp LICENSE "$work/root/usr/share/doc/convt/copyright"
# Normalize all modes and times, and omit generated Python caches.
find "$work/root" -type d -name __pycache__ -exec rm -rf {} +
find "$work/root" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
for kind in deb rpm; do
  [[ $family == all || $family == "$kind" ]] || continue
  base=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))[sys.argv[2]])' packaging/linux/package-images.lock.json "$kind")
  image="convt-pkg-package-$kind-$$"
  images+=("$image")
  mkdir "$work/$kind"
  if [[ $kind == deb ]]; then
    printf 'FROM %s\nRUN apt-get update && apt-get install -y --no-install-recommends dpkg-dev && rm -rf /var/lib/apt/lists/*\n' "$base" > "$work/$kind/Dockerfile"
  else
    printf 'FROM %s\nRUN dnf install -y rpm-build && dnf clean all\n' "$base" > "$work/$kind/Dockerfile"
  fi
  docker build -t "$image" "$work/$kind"
  docker run --rm --name "convt-pkg-package-$kind-$$" --network none \
    -v "$repo:/repo:ro" -v "$work:/work" -e SOURCE_DATE_EPOCH \
    -e "CONVT_PACKAGE_VERSION=$version" -e "CONVT_PACKAGE_RELEASE=$release" "$image" \
    bash /repo/packaging/linux/container-package.sh "$kind"
done
cp "$work/artifacts/"* "$out/"
(cd "$out"; sha256sum convt_*.deb convt-*.rpm 2>/dev/null > packages.sha256) || true
# P11 signing hook: sign copies of completed artifacts in release CI. This
# builder intentionally never invokes dpkg-sig, debsigs or rpmsign.
ls -lh "$out/"*.deb "$out/"*.rpm 2>/dev/null || true
