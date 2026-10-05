#!/usr/bin/env bash
# Downloads a prebuilt PDFium from bblanchon/pdfium-binaries into vendor/pdfium.
set -euo pipefail
cd "$(dirname "$0")/.."

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) asset=pdfium-linux-x64 ;;
  Linux-aarch64) asset=pdfium-linux-arm64 ;;
  Darwin-arm64) asset=pdfium-mac-arm64 ;;
  Darwin-x86_64) asset=pdfium-mac-x64 ;;
  MINGW*|MSYS*|CYGWIN*) asset=pdfium-win-x64 ;;
  *) echo "no PDFium build for $(uname -sm)" >&2; exit 1 ;;
esac

version="${PDFIUM_VERSION:-latest}"
if [ "$version" = latest ]; then
  url="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/$asset.tgz"
else
  url="https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F$version/$asset.tgz"
fi

rm -rf vendor/pdfium && mkdir -p vendor/pdfium
echo "fetching $url"
curl -fsSL "$url" | tar -xz -C vendor/pdfium
echo "PDFium $(cat vendor/pdfium/VERSION 2>/dev/null | tr '\n' ' ')installed in vendor/pdfium"
