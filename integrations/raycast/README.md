<div align="center">
    <br/>
    <br/>
    <img src="./assets/icon.png" alt="Convt" width="100"/>
    <h3>Convt</h3>
    <p>Convert images, videos, audio, PDFs, and documents on your computer</p>
    <br/>
    <br/>
</div>

Convert images, videos, audio, PDFs, and documents on your computer using [Convt](https://github.com/opencoredev/convt). Files stay on disk; this extension does not use Convt's cloud API.

## Setup

Install Convt from [convt.app/download](https://convt.app/download). On Apple silicon Macs, you can also use Homebrew:

```sh
brew tap opencoredev/convt https://github.com/opencoredev/convt
brew install --cask convt
```

The extension detects the macOS app in `/Applications` or `~/Applications`, the Windows installation in `%LOCALAPPDATA%\Programs\convt`, and executables on `PATH`. For a custom installation, set **Convt CLI Path** in the extension preferences to the executable's absolute path.

Use Convt 0.5.0 or newer. Older versions don't report which file each progress event belongs to, which folder conversion needs. Packaged builds require a Convt trial or license; ordinary source builds follow Convt's own licensing behavior. This extension does not manage licenses. Available conversions depend on your installed engines; see [Convt's engine documentation](https://github.com/opencoredev/convt#engines).

## Convert Files

1. Select files or folders in Finder and run **Convert Files**, or choose them in the command's file picker. On Windows, use the file picker.
2. Review the detected file types, identified by their filename extensions. Choose an output format from Convt's **Suggested** section or the remaining formats. Every listed format works for **every** selected file.
3. Optionally choose an output folder, a preset, and the options that apply to the output format. Options appear only where they apply.
4. Convert, then open, reveal, or copy the resulting files and paths.

Files are saved beside each original unless you choose an output folder. Convt preserves originals and uses numbered suffixes when output names already exist. PDF pages produce separate images.

### Options

| Option                | Shown for                                                                     | Convt flag         |
| --------------------- | ----------------------------------------------------------------------------- | ------------------ |
| Preset                | When you have saved presets (`convt presets`)                                 | `--preset`         |
| Quality               | Image and video outputs                                                       | `--quality`        |
| Maximum Image Size    | Image outputs                                                                 | `--max-size`       |
| PDF Pages, Render DPI | Image outputs from PDFs (DPI also from SVGs)                                  | `--pages`, `--dpi` |
| Background            | Image outputs: Transparent, White, Black, or a custom color such as `#ff8800` | `--background`     |
| Maximum Video Height  | Video outputs: 2160p to 360p; never enlarges                                  | `--video-height`   |
| Video Codec           | MP4, MOV, and MKV outputs: H.264 or HEVC                                      | `--video-codec`    |
| Audio Bitrate         | Audio and video outputs: 64 to 320 kbps                                       | `--audio-bitrate`  |
| Remove audio          | Video outputs                                                                 | `--no-audio`       |

Choosing a preset selects its output format when the files support it. Options you set in the form override the preset's. Transparent is hidden for formats that can't store it, such as JPEG; Raycast AI requests for it are refused with an explanation.

### Folders

A folder converts every file directly inside it. Turn on **Include subfolders** to convert files in its subfolders too; with an output folder, Convt recreates the subfolder structure there. Hidden files are ignored.

The output formats offered for folders are the ones at least one file inside can reach, combined with the formats every file you selected directly can reach. When you convert, files in a folder that can't become the chosen format are skipped, as `convt` does on the command line. The results list each converted file. To keep the form responsive, target discovery reads up to 10,000 files per folder; Convt still converts every file.

**Parallel Conversions** in the extension preferences sets how many files convert at once. The default, 1, converts one file at a time. **Auto** lets Convt use one per CPU core. Videos always convert one at a time.

### Office documents

Converting Word, Excel, PowerPoint, OpenDocument, and RTF files needs LibreOffice or Convt's document pack. When neither is installed, the form explains this and offers **Install Document Pack**, which runs `convt pack install documents` and shows the download in a toast, then refreshes the formats. **Open Convt.app** links to the download page.

The first suggestion is selected automatically. An output you choose stays selected when it remains available for the next files. Suggestions and supported formats come from your installed Convt CLI. Mixed file selections may have no shared suggestions or output formats. See [Convt's supported formats](https://convt.app/formats) for an overview.

Conversion progress and cancellation are available while the command is running. A cancelled or failed batch can still contain completed files; the results list keeps those outputs. Check the results before retrying.

## Raycast AI

Ask Raycast AI to find available formats or convert files using their absolute paths:

- “What formats can I convert `/Users/me/photo.heic` to?”
- “What file type is `/Users/me/clip.mov`, and which output formats do you suggest?”
- “Convert `/Users/me/photo.png` to JPEG at quality 85.”
- “Convert pages 1–3 of `/Users/me/report.pdf` to PNG in `/Users/me/Exports`.”
- “Convert everything in `/Users/me/Screenshots`, including subfolders, to WebP.”
- “Convert `/Users/me/talk.mov` to a 720p MP4 without audio.”

Format discovery returns detected source types, folder summaries, all common supported targets, suggested target IDs, and your saved presets. When Office documents can't convert because the document pack is missing, it returns a `hint` that Raycast AI passes on. The conversion tool accepts the same options as the form, shows a confirmation before writing, and returns the actual output paths and individual failures.

## Development

```sh
npm install
npm run dev
```

Validate with `npm test`, `npx tsc --noEmit`, `npm run lint`, and `npm run build`.

## Syncing with raycast/extensions

This folder lives in the [convt repository](https://github.com/opencoredev/convt) at `integrations/raycast` and mirrors [`extensions/convt`](https://github.com/raycast/extensions/tree/main/extensions/convt) in [raycast/extensions](https://github.com/raycast/extensions), which is what the Raycast Store publishes. Make changes here first, then copy them over in a pull request to raycast/extensions.

1. Fork raycast/extensions on GitHub, then clone only this extension:

   ```sh
   git clone --filter=blob:none --no-checkout https://github.com/<you>/extensions.git raycast-extensions
   cd raycast-extensions
   git sparse-checkout set --no-cone /extensions/convt
   git checkout main
   git checkout -b convt-update
   ```

2. Pull any changes made directly in the store repository, so the copy doesn't overwrite them:

   ```sh
   (cd extensions/convt && npx @raycast/api@latest pull-contributions)
   ```

   Bring anything new back into `integrations/raycast` in the convt repository before continuing.

3. Copy this folder over, leaving out installed packages and generated files:

   ```sh
   rsync -a --delete --exclude node_modules --exclude raycast-env.d.ts \
     /path/to/convt/integrations/raycast/ extensions/convt/
   ```

4. Check the copy:

   ```sh
   cd extensions/convt
   npm install && npm run build && npm run lint && npm test
   ```

5. Add a `CHANGELOG.md` entry titled `## [Title] - {PR_MERGE_DATE}` if this change doesn't have one, commit, and open a pull request against raycast/extensions. [0xdhrv](https://github.com/0xdhrv) created and owns this extension; request their review. Keep `author` and `contributors` in `package.json` as they are.

Once the pull request merges, Raycast publishes the update to the store.

## Credits

[Convt](https://github.com/opencoredev/convt) is created and maintained by [Leo](https://github.com/leoisadev1) and [OpenCore](https://github.com/opencoredev). This extension is an independent Raycast integration that uses Convt's local CLI.

The icon comes from [Convt's brand assets](https://github.com/opencoredev/convt/tree/main/apps/web/public/brand). Convt and its logo belong to their respective owners.
