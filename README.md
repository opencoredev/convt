<p align="center">
  <a href="https://github.com/opencoredev/convt/stargazers"><img alt="GitHub stars" src="https://shieldcn.dev/github/opencoredev/convt/stars.svg?variant=branded&mode=dark" /></a>
  <a href="https://x.com/leodev"><img alt="Follow @leodev on X" src="https://shieldcn.dev/x/follow/leodev.svg?variant=branded&mode=dark" /></a>
</p>

# convt

Local file conversion for your own machine. Right-click a file, pick a format, and convt writes the result next to the original. Images, video, audio, PDFs and documents stay on disk: nothing is uploaded.

- 40 formats, routed through native engines (FFmpeg, PDFium, LibreOffice, image, resvg)
- A desktop app built with [GPUI](https://www.gpui.rs), a Finder menu on macOS, an Explorer menu on Windows, and file manager menus on Linux
- The `convt` CLI, shipped in the same install
- Multi-hop routes of at most three steps when no engine can convert directly
- Optional document pack for Word, Excel and PowerPoint, installed only when you ask
- A paid cloud API and TypeScript SDK for converting files on convt's servers
- Accounts, checkout and downloads on [convt.app](https://convt.app)

## Install

Download the app from [convt.app/download](https://convt.app/download). Release 0.2.0 has a disk image for macOS on Apple silicon and an MSI installer for Windows x86_64. The Linux packages (AppImage, .deb, .rpm and a tarball) build from `packaging/linux` but are not in a release yet. Packaged builds start a 7-day trial on the first conversion.

To build from source:

```bash
bun run setup
cargo run -p convt-cli -- photo.png --to webp
```

`bun run setup` installs system packages (asks for sudo on Linux and macOS), the Rust toolchain, Bun dependencies and PDFium. On Windows run `scripts/setup-windows.ps1` in PowerShell. Ordinary source builds are unrestricted.

## Usage

```bash
convt clip.mov --to mp4
convt photos/ --to webp -r
convt lease.pdf --to png --pages 1-3
```

From a checkout, prefix those with `cargo run -p convt-cli --`. Output lands next to each input unless you pass `--out-dir`.

## Engines

convt registers whatever can run on this machine and picks a route:

- **FFmpeg** for video and audio (MP4, MOV, WebM, MKV, AVI, MP3, WAV, FLAC, AAC, M4A, OGG, Opus)
- **LibreOffice** for Word, Excel, PowerPoint and their open formats, and for saving any of them as PDF
- **PDFium** to render PDF pages to PNG or JPEG
- **image** and **resvg** for photos and SVG (JPEG, PNG, WebP, AVIF, GIF, TIFF, BMP, ICO, TGA, PPM, QOI, OpenEXR)
- **libheif**, or `sips` on macOS, for HEIC

Office files need LibreOffice on `PATH`. Released builds can also run `convt pack install documents`. Ordinary source builds need a system LibreOffice, or an explicit `--source` and `--sha256`. The engines never start that download themselves.

## CLI

```bash
convt engines              # backends available here
convt targets photo.heic   # formats this file can reach
convt targets photo.heic --menu
convt formats
convt presets
convt pack status documents
```

`convt --help` lists quality, size, pages, DPI, video and job flags. See [AGENTS.md](AGENTS.md) for the rest of the commands in this repo.

## Cloud API

The cloud API converts files on convt's servers for a paid plan. You upload a file, start a job, and download the result. [`@convt/sdk`](packages/sdk) wraps those calls in TypeScript; see the [API docs](https://convt.app/docs) for keys, the host to call and the full reference. The API is `crates/convt-server`, and conversions run in sandboxed `crates/convt-worker` processes using the same engines as the app.

## Documentation

- **[convt.app](https://convt.app)**: product site, pricing and sign-in
- **[Download](https://convt.app/download)**: desktop builds and matching source archives
- **[Formats](https://convt.app/formats)**: every format and the targets it can reach
- **[API docs](https://convt.app/docs)**: the cloud API, the SDK and the API reference
- **[AGENTS.md](AGENTS.md)**: repo layout, conventions, and how to check, test and build

## License

convt is open source under the [GNU AGPL v3](LICENSE). The signed desktop builds, convt Cloud and the API are paid. "convt" and the convt logo are trademarks and are not covered by the code license.

## Sponsors

Want your logo here? **[Become a sponsor →](https://github.com/sponsors/opencoredev)**

<p align="center">
  <a href="https://github.com/sponsors/opencoredev">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/sponsors/opencoredev.svg?special=resend,instatushq,primitivedotdev,lettermint&title=false&mode=dark&preset=surface" />
      <source media="(prefers-color-scheme: light)" srcset="https://shieldcn.dev/sponsors/opencoredev.svg?special=resend,instatushq,primitivedotdev,lettermint&title=false&mode=light&preset=surface" />
      <img alt="Sponsors" src="https://shieldcn.dev/sponsors/opencoredev.svg?special=resend,instatushq,primitivedotdev,lettermint&title=false&mode=dark&preset=surface" width="820" />
    </picture>
  </a>
</p>

## Star History

<p align="center">
  <a href="https://github.com/opencoredev/convt/stargazers"><img alt="Star history" src="https://shieldcn.dev/chart/github/stars/opencoredev/convt.svg?mode=dark" /></a>
</p>

<p align="center"><sub><a href="./LICENSE">AGPL-3.0</a> · Built by <a href="https://x.com/leodev">@leodev</a></sub></p>
