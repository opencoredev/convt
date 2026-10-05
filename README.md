# convt

Right-click any file and convert it. Images, video, audio, PDFs and documents, converted on your own machine with native engines (FFmpeg, PDFium, LibreOffice, resvg) instead of a browser or an upload.

Works from Finder on macOS, Explorer on Windows, and Nautilus, Dolphin, Nemo or Thunar on Linux, plus a desktop app and a `convt` CLI.

## Status

Early. The CLI converts real files today; the desktop app and context menus are scaffolding.

## Quick start

```sh
bun run setup                                  # installs system packages (asks for sudo), Rust, Bun deps, PDFium
cargo run -p convt-cli -- clip.mov --to mp4
cargo run -p convt-cli -- engines              # what's available on this machine
```

See [AGENTS.md](AGENTS.md) for the repo layout and commands.

## License

convt is open source under the [GNU AGPL v3](LICENSE). The signed desktop builds, convt Cloud and the API are paid. "convt" and the convt logo are trademarks and aren't covered by the code license.
