#!/usr/bin/env bash
# Test-only payload. No host tool directories, vendor tree or sockets enter the containers.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH" CONVT_LICENSE_STORE=file
bundle=${1:-$repo/packaging/out/convt}
report=${CONVT_BUNDLE_REPORT:-$(mktemp -d /tmp/convt-bundle-validation-XXXXXX)}
mkdir -p "$report"
work=$(mktemp -d)
created_images=()
cleanup() { rm -rf "$work"; for image in "${created_images[@]}"; do docker image rm "$image" >/dev/null 2>&1 || true; done; }
trap cleanup EXIT
# Use tools built in the same glibc 2.28 container. A host-built runner or
# interpreter would make an older-distro proof depend on the host baseline.
tools=${CONVT_VALIDATION_TOOLS:-$(dirname "$bundle")/validation-tools}
[[ -x "$tools/matrix" && -x "$tools/python/bin/python3" ]] || { echo "Missing baseline validation tools: $tools" >&2; exit 1; }
cp "$tools/matrix" "$work/matrix"
cp -a "$tools/python" "$work/python"
cp -a "$tools/validator-lib" "$work/validator-lib"
cp scripts/matrix-fixtures.py "$work/"
cat > "$work/python3" <<'EOF'
#!/bin/sh
export PYTHONHOME=/validation/python
export LD_LIBRARY_PATH="/validation/validator-lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec /validation/python/bin/python3 "$@"
EOF
chmod +x "$work/python3"
cat > "$work/convt-clean-env" <<'EOF'
#!/bin/sh
exec env -u CONVT_LIBHEIF_DIR -u CONVT_LIBHEIF_PLUGIN_DIR -u CONVT_PDFIUM_DIR -u CONVT_FFMPEG -u CONVT_FFPROBE -u LIBHEIF_PLUGIN_PATH -u LD_LIBRARY_PATH /bundle/convt "$@"
EOF
chmod +x "$work/convt-clean-env"
cat > "$work/run.sh" <<'EOF'
#!/bin/sh
set -eu
export PATH=/bundle:/validation:/usr/bin:/bin
export LD_LIBRARY_PATH=/bundle/lib
export LIBHEIF_PLUGIN_PATH=/bundle/lib/libheif/plugins
export CONVT_LIBHEIF_PLUGIN_DIR=/bundle/lib/libheif/plugins
export CONVT_LIBHEIF_DIR=/bundle/lib CONVT_PDFIUM_DIR=/bundle/lib
export CONVT_FFMPEG=/bundle/ffmpeg CONVT_FFPROBE=/bundle/ffprobe
export CONVT_BIN=/validation/convt-clean-env CONVT_MATRIX_HELPER=/validation/matrix-fixtures.py
export CONVT_LICENSE_STORE=file CONVT_CONFIG_DIR=/tmp/config CONVT_DATA_DIR=/tmp/data
export CONVT_MATRIX_REPORT=/reports/matrix.json
if command -v soffice >/dev/null; then echo "Unexpected system Office" >&2; exit 1; fi
if [ -e /archive/documents.tar.gz ]; then
    /bundle/convt pack status documents
    /bundle/convt pack install documents --source file:///archive/documents.tar.gz --sha256 "$(cat /archive/documents.sha256)"
    /bundle/convt pack status documents
    hash=$(cat "$CONVT_DATA_DIR/packs/documents/current")
    mkdir /tmp/validator-bin
    printf '#!/bin/sh\nexec "%s" "$@"\n' "$CONVT_DATA_DIR/packs/documents/$hash/soffice" > /tmp/validator-bin/soffice
    chmod +x /tmp/validator-bin/soffice
    export PATH="/tmp/validator-bin:$PATH"
elif [ -e /pack/soffice ]; then export CONVT_SOFFICE=/pack/soffice; fi
if test -e /usr/bin/ffmpeg; then echo "Unexpected system FFmpeg" >&2; exit 1; fi
if test -e /usr/lib/x86_64-linux-gnu/libheif.so.1; then echo "Unexpected system libheif" >&2; exit 1; fi
cat /etc/os-release
/validation/convt-clean-env pack --help > /reports/pack-help.txt
if /usr/bin/grep -q 'convt.bin' /reports/pack-help.txt; then echo 'Binary name leaked into help' >&2; exit 1; fi
/bundle/convt-app --help > /reports/app-help.txt
if /usr/bin/grep -q 'convt-app.bin' /reports/app-help.txt; then echo 'App binary name leaked into help' >&2; exit 1; fi
/validation/convt-clean-env engines
/validation/convt-clean-env targets sample.png
ldd /bundle/convt-app.bin > /reports/gui-ldd.txt
if /usr/bin/grep -q 'not found' /reports/gui-ldd.txt; then cat /reports/gui-ldd.txt; exit 1; fi
if [ ! -e /pack/soffice ] && [ ! -e /archive/documents.tar.gz ]; then test -z "$(/bundle/convt targets sample.docx)"; fi
# Resolve every plugin against the private closure and document paths actually loaded.
for lib in /bundle/lib/libheif.so.1 /bundle/lib/libheif/plugins/*.so /bundle/lib/libpdfium.so; do ldd "$lib"; done
LD_DEBUG=libs /bundle/convt engines 2>/reports/loader.log
/validation/matrix --ignored --nocapture full_matrix
EOF
chmod +x "$work/run.sh"
read -ra distros <<< "${CONVT_TEST_DISTROS:-ubuntu:22.04 debian:12 debian:stable-slim ubuntu:24.04 ${CONVT_TEST_FEDORA:+fedora:latest}}"
for distro in "${distros[@]}"; do
  label=${distro//[:\/]/-}
  image="convt-pkg-$label-$$"
  # Pin the resolved test base by image ID and record its registry digest.
  if [[ ${CONVT_TEST_USE_LOCAL_IMAGES:-0} != 1 ]]; then
    docker pull "$distro" > "$report/$label-pull.log"
  fi
  base=$(docker image inspect "$distro" --format '{{.Id}}')
  docker tag "$base" "$image"
  created_images+=("$image")
  docker image inspect "$image" --format '{{json .RepoDigests}}' > "$report/$label-digest.json"
  mkdir -p "$report/$label"
  mounts=(-v "$bundle:/bundle:ro" -v "$work:/validation:ro" -v "$report/$label:/reports")
  if [[ -n ${CONVT_DOCUMENT_PACK:-} ]]; then mounts+=(-v "$CONVT_DOCUMENT_PACK:/pack:ro"); fi
  if [[ -n ${CONVT_DOCUMENT_ARCHIVE:-} ]]; then mounts+=(-v "$CONVT_DOCUMENT_ARCHIVE:/archive:ro"); fi
  docker run --rm --name "convt-pkg-$label-$$" --network none "${mounts[@]}" "$image" /validation/run.sh > "$report/$label.log" 2>&1
  tail -18 "$report/$label.log"
done
printf 'Reports: %s\n' "$report"
