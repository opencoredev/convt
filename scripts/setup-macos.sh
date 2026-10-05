#!/usr/bin/env bash
# Homebrew packages for building convt on macOS. GPUI needs Xcode (Metal), not
# just the command-line tools.
set -euo pipefail

if ! xcrun -f metal >/dev/null 2>&1; then
  echo "Install Xcode from the App Store, then run: sudo xcode-select -s /Applications/Xcode.app" >&2
  exit 1
fi
command -v brew >/dev/null || { echo "Install Homebrew first: https://brew.sh" >&2; exit 1; }
brew install cmake pkg-config ffmpeg vips libheif
[ "${WITH_LIBREOFFICE:-1}" = 1 ] && brew install --cask libreoffice
