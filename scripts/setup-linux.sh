#!/usr/bin/env bash
# System packages for building convt on Debian/Ubuntu or Fedora.
# GPUI needs the X11/Wayland/font libraries; the engines need FFmpeg and
# optionally LibreOffice, libvips and libheif.
set -euo pipefail

SUDO=""; [ "$(id -u)" -ne 0 ] && SUDO="sudo"

if command -v apt-get >/dev/null; then
  $SUDO apt-get update
  $SUDO apt-get install -y \
    build-essential clang cmake pkg-config curl git \
    libxcb1-dev libx11-xcb-dev libxkbcommon-dev libxkbcommon-x11-dev \
    libwayland-dev libvulkan-dev libfontconfig-dev libfreetype-dev \
    libasound2-dev libssl-dev libzstd-dev \
    ffmpeg libvips-dev libheif-dev
  [ "${WITH_LIBREOFFICE:-1}" = 1 ] && $SUDO apt-get install -y --no-install-recommends libreoffice-writer libreoffice-calc libreoffice-impress
elif command -v dnf >/dev/null; then
  $SUDO dnf install -y \
    gcc gcc-c++ clang cmake pkgconf curl git \
    libxcb-devel libxkbcommon-devel libxkbcommon-x11-devel \
    wayland-devel vulkan-loader-devel fontconfig-devel freetype-devel \
    alsa-lib-devel openssl-devel libzstd-devel \
    ffmpeg-free vips-devel libheif-devel
  [ "${WITH_LIBREOFFICE:-1}" = 1 ] && $SUDO dnf install -y libreoffice-writer libreoffice-calc libreoffice-impress
else
  echo "Unsupported distro. Install the equivalents of the packages in this script by hand." >&2
  exit 1
fi
