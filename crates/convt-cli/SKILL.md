---
name: convt
description: Convert images, video, audio, PDFs and documents locally with the convt CLI. Use when an agent needs to convert files on the user's machine without uploading them, list formats or engines, or run a batch from a script.
---

# convt

Local file conversion. Right-click or run the CLI; files stay on disk. Nothing is uploaded. Routes through engines on this machine (FFmpeg, PDFium, LibreOffice, image, resvg, libheif). Multi-hop routes are at most three steps. A still image cannot become video or audio through a multi-hop route.

Print this file: `convt --skill` or `convt skill`.

## Install check

```sh
command -v convt
convt --version
convt engines
```

`convt engines` lists each ready backend and each unavailable one with its reason. A "no route" error usually means an engine is missing, not a bad flag. Packaged installs come from https://convt.app/download. From this repository: `cargo run -p convt-cli -- …` or `./target/debug/convt`. A `convt` on `PATH` may be an older install.

## Convert

```
convt <files or folders> --to <fmt> [options]
```

`--to` is required unless `--preset` names a preset that sets `to`. Output lands next to each input unless `--out-dir` is set. With `--out-dir`, files found in subfolders keep their relative folder.

{{FLAGS}}
## Commands

{{COMMANDS}}
## Formats

Ids are what `--to` and `convt formats` use. Extensions select the input format (no content sniffing). Availability depends on `convt engines`; Office formats need LibreOffice or `convt pack install documents`. Raster-to-SVG is not offered.

{{FORMATS}}
## Examples

```sh
convt photo.png --to webp
convt photos/ --to webp -r
convt lease.pdf --to png --pages 1-3
convt clip.mov --to mp4 --video-height 720
convt photo.png --to jpeg -q 60 --max-size 64 --background black
convt clip.mov --to mp4 --no-audio --video-codec hevc
convt photos/ -r --to webp -j 4 --json --out-dir out/
convt targets photo.heic
convt targets photo.heic --menu
convt formats --json
convt pack status documents
```

## Exit codes and errors

- `0`: success (including `--skill`, `--help`, and list commands)
- `1`: a conversion failed, or the run was rejected before it started (no `--to`, unknown format, nothing to convert, license)
- `2`: usage error from the parser (unknown flag, bad value)
- `130`: cancelled (`SIGINT`; a second interrupt exits immediately)

`--json` prints one event per line on stdout: `started`, `progress`, `done`, `failed`, then `summary`. A `failed` event has a stable `kind`: `unsupported_input`, `no_route`, `invalid_option`, `engine_missing`, `engine_failed`, `output_exists`, `cancelled`, `io`.

Folder inputs skip files that cannot reach the target (or already are it). Files named directly always run, so their errors show.

## Tips for agents

- Non-interactive: pass every option on the command line. The CLI does not prompt.
- Prefer `--json` when parsing progress. Human `input -> output` lines go to stderr unless `--json`.
- Outputs never overwrite existing files; a taken name gets a numbered suffix. Use `--out-dir` to keep results apart from the inputs.
- Discover before converting: `convt engines`, `convt formats`, `convt targets path/to/file`.
- `formats`, `targets`, `engines`, `presets` and `--skill` never need a license. Only conversions do. Source builds skip the license check; packaged builds start a 7-day trial on the first conversion.
- `convt license activate` reads the key from stdin when KEY is omitted. Do not put a license key in a shared command line.
- `convt pack install documents` is the only pack command that may use the network. `pack status` is offline.
- Presets are TOML (`to`, `quality`, `max_size`, …) under the config presets directory, or a path to a `.toml` file. List them with `convt presets`. Point `CONVT_CONFIG_DIR` at a private directory instead of the user's real presets.
