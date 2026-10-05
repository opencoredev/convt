#!/usr/bin/env bash
# Repository libraries are explicit test inputs, never release discovery paths.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
export CONVT_LICENSE_STORE=file
export CONVT_PDFIUM_DIR=${CONVT_PDFIUM_DIR:-$root/vendor/pdfium/lib}

# convt only accepts the patched libheif the Linux bundle ships, so HEIC and
# AVIF cases need it. Use the built bundle unless the caller chose a libheif.
bundle=$root/packaging/out/convt
if [ -z "${CONVT_LIBHEIF_DIR:-}" ] && [ -e "$bundle/lib/libheif.so.1" ]; then
  export CONVT_LIBHEIF_DIR=$bundle/lib
  export CONVT_LIBHEIF_PLUGIN_DIR=$bundle/lib/libheif/plugins
  export LIBHEIF_PLUGIN_PATH=$bundle/lib/libheif/plugins
  export LD_LIBRARY_PATH=$bundle/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
  export CONVT_FFMPEG=${CONVT_FFMPEG:-$bundle/ffmpeg}
  export CONVT_FFPROBE=${CONVT_FFPROBE:-$bundle/ffprobe}
fi
if [ -z "${CONVT_LIBHEIF_DIR:-}" ] && [ "${CONVT_MATRIX_WITHOUT_HEIF:-}" != 1 ]; then
  echo "No patched libheif: HEIC and AVIF cases would be skipped." >&2
  echo "Run 'bun run bundle:linux' first, set CONVT_LIBHEIF_DIR, or set CONVT_MATRIX_WITHOUT_HEIF=1 to skip them on purpose." >&2
  exit 1
fi

cd "$root"
exec cargo test -p convt-engines "$@"
