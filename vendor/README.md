# vendor

Third-party binaries fetched at setup time. Everything here except this file is gitignored.

- `pdfium/`: prebuilt PDFium from [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries), fetched by `scripts/fetch-pdfium.sh`. The PDF engine loads `pdfium/lib/` in dev builds and the copy next to the executable in release builds. Set `CONVT_PDFIUM_DIR` to override.

Release builds will also bundle FFmpeg here. Until then the FFmpeg engine uses `CONVT_FFMPEG` or `ffmpeg` on `PATH`.
