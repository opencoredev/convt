# Pinned third-party inputs for convt.app, sourced by bundle.sh. Every
# download is checked against its SHA-256 before use.
#
# FFmpeg 9.0.2 per architecture:
#   source  built from the sources pinned in
#           packaging/release/macos-source-ffmpeg.lock.json by
#           packaging/release/macos-source-ffmpeg-build.sh. bundle.sh checks the
#           build's receipt against that lock. Release-ready.
#   riedl   Martin Riedl's prebuilt binary. Its exact corresponding source is
#           unknown (see the lock's remaining_source_gaps), so it is NOT READY:
#           bundle.sh uses it only with CONVT_MAC_UNSOURCED_FFMPEG=1 and marks
#           the bundle, and release.sh refuses such a bundle.
FFMPEG_VERSION=9.0.2
FFMPEG_SOURCE_arm64=source
# NOT READY: no x86_64 source build yet.
FFMPEG_SOURCE_x86_64=riedl
FFMPEG_URL_x86_64=https://ffmpeg.martin-riedl.de/download/macos/amd64/1789931006_9.0.2
FFMPEG_SHA256_x86_64=7c6b4125b191cbf773832dc51f424cf2b6bb7da43007d1e066f95909e47cacd4
FFPROBE_SHA256_x86_64=2322438ed2f6319a691291b247d09c69dcaa3a982460d1f269a7e1af335cfdfd

# PDFium chromium/8076 from bblanchon/pdfium-binaries, the same build as Linux.
PDFIUM_VERSION=8076
PDFIUM_URL=https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8076
PDFIUM_SHA256_arm64=0d6781fe08906baff3d82c90953e519fbc4eb253fe76431e5ed53b157763b97c
PDFIUM_SHA256_x86_64=40865f34642c34d82cc336132df9e0347133f4692cd46776647af160f9a5cca9
PDFIUM_SHA256_universal=3bdb93e229298dfdf083dc8ccc7d1a8cf87790b6917e5073335504fe2ff0bdc1
