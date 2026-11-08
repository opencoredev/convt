# Conversion audit, October 2026

Which conversions convt offers, which of them work, and what was wrong. Every
pair that `convt targets` offers was run through the real CLI and its output
was opened and checked with tools that share no code with convt.

Audited on 2026-10-30, from `fe03401` plus the fixes on the `conversion-audit`
branch, on Linux x86_64 with all nine engines ready: image, svg, markdown,
data, subtitle, libheif (the patched 1.17.6 from the Linux bundle, with the
x265, libde265 and libaom plugins), FFmpeg 6.1.1, PDFium 158.0.8125
(pdfium-binaries) and LibreOffice 24.2.7. macOS and Windows were not tested
(see "Not tested").

## Result

All 751 offered pairs pass on the fixed branch, across 49 input formats. Before
the fixes, 6 pairs failed for a real bug: every document or PDF converted to
GIF opened as a solid black image (bug 1 below). The other 13 baseline
failures were mistakes in the audit's own checker or samples (a one-frame GIF
sample, float TIFF that Pillow can't read, page counts for files without page
breaks, a too-strict ink threshold for icon-sized pages); each was corrected
and every pair re-run. The repository's own semantic
matrix, run through the same CLI, passes all 862 of its cases, including the
same-format re-encodes, AV1 video inputs and the 440 Hz tone checks; before its
harness was repaired it could not finish (bug 4).

| Input family |   Pairs |    Pass |  Fail | Skipped |
| ------------ | ------: | ------: | ----: | ------: |
| image        |     174 |     174 |     0 |       0 |
| vector       |      14 |      14 |     0 |       0 |
| video        |     147 |     147 |     0 |       0 |
| audio        |      90 |      90 |     0 |       0 |
| pdf          |      20 |      20 |     0 |       0 |
| document     |     164 |     164 |     0 |       0 |
| presentation |      48 |      48 |     0 |       0 |
| spreadsheet  |      94 |      94 |     0 |       0 |
| **all**      | **751** | **751** | **0** |   **0** |

No pair was skipped: every engine and every validator was present. "Pass"
means the output decoded as the target format and kept its content, not only
that convt exited 0. What each family was checked for:

- Images: the file signature must match the target (checked with `file` on
  all 650 image, PDF and SVG outputs), then decoded with Pillow, or FFmpeg,
  libtiff or libheif when Pillow can't read the format; size must be 64×48
  and each quadrant's color must be within 40 of the source.
- PDF and document pages to images: one image per page (3 for the PDF, DOCX,
  DOC, ODT, RTF and HTML samples, 2 for the slide decks), each with rendered
  text on it.
- Video and audio: ffprobe streams and duration (2 s), plus a full FFmpeg
  decode that fails on any corrupt packet.
- Anything to PDF: page count from `pdfinfo`, and `pdftotext` must find each
  page's marker word. Images to PDF must render with all four colors.
- Office outputs: ZIP or OLE structure checked, then reopened by a separate
  LibreOffice run, printed to PDF and searched for the marker words or
  spreadsheet cells.
- Text outputs: parsed as JSON, YAML, CSV or TSV where they are data; SRT and
  VTT must keep cue timing and text.
- Video to TXT, SRT and VTT extract the video's own subtitle track; the
  samples carry one.

## Bugs found

1. **Documents and PDFs to GIF came out black. Fixed** (`cef6d0d`). A
   grayscale image using all 256 gray levels, which every anti-aliased
   rendered page is, was written with its palette on the frame and a padded
   two-entry black global palette, because of how the `image` and `gif`
   crates write GIFs. Pillow, and anything built on it, reads that file as
   solid black. FFmpeg read it correctly, which is why it went unnoticed.
   convt now writes the palette as the global color table. Baseline output:

   ```
   pdf -> gif   sample.gif: dark-pixel ratio 1.0000; page text not rendered
   docx -> gif  (same); doc, odt, rtf, html -> gif (same)
   ```

   Regression test: `image::tests::gif_palette_is_global_so_full_gray_ramps_stay_gray`.
   It fails without the fix ("palette written on the frame").

2. **Password-protected and damaged PDFs gave a debug dump. Fixed**
   (`2261d4f`). Every PDF conversion of a locked or broken file failed with

   ```
   locked.pdf: pdfium failed: PdfiumLibraryInternalError(
       PasswordError,
   )
   ```

   in the CLI and its `--json` events, and the app shows the same engine
   message (not opened in this audit). It now reads "pdfium failed:
   the PDF is password-protected; remove the password and try again" or
   "pdfium failed: the file is damaged or is not a PDF". Regression test:
   `pdfium::tests::open_errors_are_plain_words`, which fails without the fix.

3. **HEIC with transparency and a rotation keeps its alpha unrotated. Not
   fixed.** `orientation_tests::heic_orientation_irot_imir_to_every_image_target`
   fails on this machine. This branch doesn't touch the HEIC or JPEG paths,
   and the cause is upstream of convt:

   ```
   jpeg EXIF 3: red at (47,35) is [247, 139, 139, 255]
   ```

   Decoding the same file directly through libheif (the
   `scripts/matrix-fixtures.py heif-pixels` helper, no convt code) shows the
   cause: with default decode options, the bundled libheif 1.17.6 rotates and
   mirrors the color planes but not the alpha plane when only the primary
   image carries `irot` or `imir`. The red quadrant gets the half-transparent
   corner's alpha and flattens to pink in JPEG. Going by those decoded
   pixels, formats that keep alpha (PNG, WebP) get the transparency in the
   wrong corner too; the test only checks colors, so it doesn't catch that. iPhone photos are opaque, so this needs
   an unusual file. A fix means decoding with transformations off and
   applying rotation, mirroring and `clap` cropping in convt, or a libheif
   patch.

4. **The semantic matrix could not pass on a fully equipped machine. Fixed**
   (`d320a64`, test code only). `bun run test:matrix` checked PDF to Markdown
   against the wrong fixture's text, panicked on AIFF, CAF and M4R (added
   with the Apple audio formats but never given fixtures), expected an M4R's
   container to be named "m4r", and had no subtitle track for the video to
   TXT, SRT and VTT targets. Its `CONVT_BIN` mode also expected
   `convt targets` to list the file's own format, which the CLI deliberately
   leaves out. None of these were conversion bugs; each failing pair was
   confirmed working by the independent audit.

5. **A CLI test broke wherever LibreOffice is installed. Fixed in the test**
   (`c6bf311`). `folder_inputs_report_each_expanded_file` expected a
   `readme.txt` in a photo folder to be skipped by a WebP batch, but text
   reaches WebP through PDF. The test now uses an MP3 as the file to skip.
   The behavior behind it is listed below as a product question.

6. **PPM output was PAM. Fixed.** Every conversion to PPM wrote a PAM file
   (`P7` header) under a `.ppm` name, because `ImageFormat::Pnm` defaults to
   PAM. Decoders that read the real bytes accepted it, so the first audit
   passed; a byte-signature check against the requested target, added after
   review, caught all 30 pairs. convt now writes a binary pixmap (`P6`).
   Regression test: `image::tests::ppm_is_a_binary_pixmap_not_pam`, which
   fails without the fix.

## Offered, working, but worth a decision

These all pass, and none was changed, because each is a product choice rather
than a defect.

- **PDF to DOCX, ODT, RTF, DOC, HTML and Markdown keep only the text.** These
  routes go through PDFium's text extraction: headings become plain
  paragraphs, tables become lines of text, and images are dropped (a PDF with
  an embedded picture gave a DOCX with no `word/media`). PDF to DOCX is one
  of the four entries in a PDF's right-click menu, where people will expect
  an editable copy of the layout. LibreOffice's PDF import or a dedicated
  converter would keep more. Image-only (scanned) PDFs fail with "the
  selected pages have no text to extract; the PDF may be a scan", which is
  clear.
- **A workbook to CSV keeps only the first sheet, without saying so.** XLSX to
  CSV dropped the second sheet of a two-sheet test file; XLSX to PDF kept
  both. XLS and ODS go through the same LibreOffice CSV export but were not
  tried with two sheets. The registry already refuses to go further
  from that CSV (no XLSX to JSON, HTML or TSV) for this reason, but the CSV
  itself gives no warning.
- **Folder batches convert text documents into images.**
  `convt photos/ -r --to webp` also writes `readme.webp`, a picture of the
  README's page, whenever LibreOffice (the document pack) is available.
  Folder files are skipped only when they can't reach the target at all.
- **Every document, slide deck and spreadsheet offers every raster format.**
  That includes OpenEXR (floating-point HDR), ICO (a page shrunk to 181×256),
  QOI, TGA, PPM and HEIC. They render correctly but are unlikely to be wanted;
  they are not in the right-click menus, only in Quick convert.
- **EXR to TIFF writes 32-bit floating-point TIFF.** libtiff reads it with the
  right colors, by design (the image engine keeps precision). Pillow and
  FFmpeg 6.1 can't open it, and some viewers may not either.
- **A still GIF to video makes a 0.1 s clip.** Animated GIFs convert with
  their full duration; a single-frame GIF becomes a one-frame video.
- **Markdown offers PNG and JPEG but not WebP or other rasters**, because the
  route (md, html, pdf, png, webp) would be four hops and routes stop at
  three. Plain text, one hop shorter, offers them all.
- **Video to TXT, SRT and VTT are offered for every MP4, MOV, MKV and WebM.**
  A video without subtitles fails with "this video has no subtitle track",
  which is clear.

Probes that found nothing wrong: accented, CJK and euro-sign text through
TXT, Markdown and CSV to PDF, DOCX, XLSX, HTML and JSON; images embedded in a
DOCX survive to PDF; a PDF page with `/Rotate 90` renders the same as Poppler;
XLSX to CSV is UTF-8 even under the C locale; Office to PDF embeds its fonts;
an image to PDF makes a page the image's size at 96 DPI.

## Not tested

- macOS and Windows. HEIC there goes through `sips` (macOS) and the Windows
  build, Office through each platform's document pack, and FFmpeg through
  each platform's build. LibreOffice's CSV export charset on Windows was not
  checked.
- The shipped document pack and the bundled FFmpeg 7.0.2. Conversions used
  this machine's LibreOffice 24.2.7 and FFmpeg 6.1.1; the pack's fonts may
  render CJK differently.
- Password-protected Office files, very large inputs and conversion speed.
- The desktop app, the Finder and Explorer menus, the Linux file-manager
  menus and the browser extension. The CLI uses the same
  `default_registry()`, so the conversions are the same, but none of those
  surfaces was opened.

## Rerun

```sh
work=$(mktemp -d)
python3 -I scripts/conversion-audit/fixtures.py "$work/in"
# HEIC sample: libheif with an HEVC encoder (CONVT_LIBHEIF_DIR from a Linux bundle)
python3 -I -c 'import sys; from PIL import Image; open(sys.argv[2], "wb").write(Image.open(sys.argv[1]).convert("RGBA").tobytes())' \
  "$work/in/sample.png" "$work/pattern.rgba"
python3 -I scripts/matrix-fixtures.py heic "$work" && mv "$work/sample.heic" "$work/in/"
python3 -I scripts/conversion-audit/audit.py "$PWD/target/debug/convt" "$work/in" "$work/run" "$work/report.json" 4
python3 -I scripts/conversion-audit/report.py "$work/report.json"   # the tables below
bun run test:matrix                                                  # the repository's matrix
```

The libheif environment variables are the ones in
`.agents/skills/test-convt-cli/SKILL.md`. Most of the audit's time goes to
LibreOffice, both converting and re-reading outputs.

## Every input

Targets are what `convt targets` offers (the file's own format, which only
re-encodes, is not listed). The menu column is `convt targets --menu`.

| Input | Family       | Targets offered | Passed                                                                                                                           | Failed | Right-click menu  |
| ----- | ------------ | --------------: | -------------------------------------------------------------------------------------------------------------------------------- | ------ | ----------------- |
| jpeg  | image        |              13 | avif bmp gif heic ico exr png ppm qoi tga tiff webp pdf                                                                          | -      | png webp pdf      |
| png   | image        |              13 | avif bmp gif heic ico jpeg exr ppm qoi tga tiff webp pdf                                                                         | -      | jpeg webp pdf     |
| webp  | image        |              13 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff pdf                                                                          | -      | jpeg png pdf      |
| heic  | image        |              13 | avif bmp gif ico jpeg exr png ppm qoi tga tiff webp pdf                                                                          | -      | jpeg png webp pdf |
| avif  | image        |              13 | bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf                                                                          | -      | jpeg png webp pdf |
| gif   | image        |              18 | avif bmp heic ico jpeg exr png ppm qoi tga tiff webp avi mkv mov mp4 webm pdf                                                    | -      | mp4 webp png      |
| tiff  | image        |              13 | avif bmp gif heic ico jpeg exr png ppm qoi tga webp pdf                                                                          | -      | jpeg png webp pdf |
| bmp   | image        |              13 | avif gif heic ico jpeg exr png ppm qoi tga tiff webp pdf                                                                         | -      | jpeg png webp pdf |
| ico   | image        |              13 | avif bmp gif heic jpeg exr png ppm qoi tga tiff webp pdf                                                                         | -      | jpeg png webp pdf |
| tga   | image        |              13 | avif bmp gif heic ico jpeg exr png ppm qoi tiff webp pdf                                                                         | -      | jpeg png webp pdf |
| ppm   | image        |              13 | avif bmp gif heic ico jpeg exr png qoi tga tiff webp pdf                                                                         | -      | jpeg png webp pdf |
| qoi   | image        |              13 | avif bmp gif heic ico jpeg exr png ppm tga tiff webp pdf                                                                         | -      | jpeg png webp pdf |
| exr   | image        |              13 | avif bmp gif heic ico jpeg png ppm qoi tga tiff webp pdf                                                                         | -      | jpeg png webp pdf |
| svg   | vector       |              14 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf                                                                     | -      | png jpeg          |
| mp4   | video        |              30 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp avi mkv mov webm aac aiff caf flac m4a m4r mp3 ogg opus wav txt srt vtt | -      | mov gif mp3       |
| mov   | video        |              30 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp avi mkv mp4 webm aac aiff caf flac m4a m4r mp3 ogg opus wav txt srt vtt | -      | mp4 gif mp3       |
| webm  | video        |              30 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp avi mkv mov mp4 aac aiff caf flac m4a m4r mp3 ogg opus wav txt srt vtt  | -      | mp4 mov gif mp3   |
| mkv   | video        |              30 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp avi mov mp4 webm aac aiff caf flac m4a m4r mp3 ogg opus wav txt srt vtt | -      | mp4 mov gif mp3   |
| avi   | video        |              27 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp mkv mov mp4 webm aac aiff caf flac m4a m4r mp3 ogg opus wav             | -      | mp4 mov gif mp3   |
| mp3   | audio        |               9 | aac aiff caf flac m4a m4r ogg opus wav                                                                                           | -      | m4a wav           |
| wav   | audio        |               9 | aac aiff caf flac m4a m4r mp3 ogg opus                                                                                           | -      | mp3 m4a           |
| flac  | audio        |               9 | aac aiff caf m4a m4r mp3 ogg opus wav                                                                                            | -      | mp3 m4a wav       |
| aac   | audio        |               9 | aiff caf flac m4a m4r mp3 ogg opus wav                                                                                           | -      | mp3 m4a wav       |
| m4a   | audio        |               9 | aac aiff caf flac m4r mp3 ogg opus wav                                                                                           | -      | mp3 wav           |
| ogg   | audio        |               9 | aac aiff caf flac m4a m4r mp3 opus wav                                                                                           | -      | mp3 m4a wav       |
| opus  | audio        |               9 | aac aiff caf flac m4a m4r mp3 ogg wav                                                                                            | -      | mp3 m4a wav       |
| aiff  | audio        |               9 | aac caf flac m4a m4r mp3 ogg opus wav                                                                                            | -      | mp3 m4a wav       |
| caf   | audio        |               9 | aac aiff flac m4a m4r mp3 ogg opus wav                                                                                           | -      | mp3 m4a wav       |
| m4r   | audio        |               9 | aac aiff caf flac m4a mp3 ogg opus wav                                                                                           | -      | mp3 m4a wav       |
| pdf   | pdf          |              20 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp doc docx html md odt txt rtf                                            | -      | png jpeg docx txt |
| docx  | document     |              20 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf doc html md odt txt rtf                                             | -      | pdf txt           |
| doc   | document     |              20 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf docx html md odt txt rtf                                            | -      | pdf docx txt      |
| odt   | document     |              20 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf doc docx html md txt rtf                                            | -      | pdf docx txt      |
| rtf   | document     |              20 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf doc docx html md odt txt                                            | -      | pdf docx txt      |
| txt   | document     |              19 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf doc docx html odt rtf                                               | -      | pdf docx          |
| html  | document     |              20 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf doc docx md odt txt rtf                                             | -      | pdf docx txt      |
| md    | document     |               9 | jpeg png pdf doc docx html odt txt rtf                                                                                           | -      | html pdf docx     |
| json  | document     |              16 | jpeg png pdf doc docx html md odt txt rtf yaml csv ods tsv xls xlsx                                                              | -      | yaml csv          |
| yaml  | document     |              16 | jpeg png pdf doc docx html json md odt txt rtf csv ods tsv xls xlsx                                                              | -      | json csv          |
| srt   | document     |               2 | txt vtt                                                                                                                          | -      | vtt txt           |
| vtt   | document     |               2 | txt srt                                                                                                                          | -      | srt txt           |
| pptx  | presentation |              16 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf odp ppt                                                             | -      | pdf               |
| ppt   | presentation |              16 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf odp pptx                                                            | -      | pdf pptx          |
| odp   | presentation |              16 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf ppt pptx                                                            | -      | pdf pptx          |
| xlsx  | spreadsheet  |              17 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf csv ods xls                                                         | -      | pdf csv           |
| xls   | spreadsheet  |              17 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf csv ods xlsx                                                        | -      | pdf xlsx csv      |
| ods   | spreadsheet  |              17 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf csv xls xlsx                                                        | -      | pdf xlsx csv      |
| csv   | spreadsheet  |              27 | avif bmp gif heic ico jpeg exr png ppm qoi tga tiff webp pdf doc docx html json md odt txt rtf yaml ods tsv xls xlsx             | -      | pdf xlsx          |
| tsv   | spreadsheet  |              16 | jpeg png pdf doc docx html json md odt txt rtf yaml csv ods xls xlsx                                                             | -      | csv xlsx json     |

Menu entries not in `convt targets`: none. Inputs with an empty menu: none.

<details><summary>Every pair, with the evidence the checker recorded</summary>

| From | To   | Result | Evidence                                   |
| ---- | ---- | ------ | ------------------------------------------ |
| avif | bmp  | pass   | BMP (64, 48)                               |
| avif | exr  | pass   | PNG (64, 48)                               |
| avif | gif  | pass   | GIF (64, 48)                               |
| avif | heic | pass   | decoded (64, 48)                           |
| avif | ico  | pass   | ICO (64, 48)                               |
| avif | jpeg | pass   | JPEG (64, 48)                              |
| avif | pdf  | pass   | 1 page, image present                      |
| avif | png  | pass   | PNG (64, 48)                               |
| avif | ppm  | pass   | PNG (64, 48)                               |
| avif | qoi  | pass   | QOI (64, 48)                               |
| avif | tga  | pass   | TGA (64, 48)                               |
| avif | tiff | pass   | TIFF (64, 48)                              |
| avif | webp | pass   | WEBP (64, 48)                              |
| bmp  | avif | pass   | PNG (64, 48)                               |
| bmp  | exr  | pass   | PNG (64, 48)                               |
| bmp  | gif  | pass   | GIF (64, 48)                               |
| bmp  | heic | pass   | decoded (64, 48)                           |
| bmp  | ico  | pass   | ICO (64, 48)                               |
| bmp  | jpeg | pass   | JPEG (64, 48)                              |
| bmp  | pdf  | pass   | 1 page, image present                      |
| bmp  | png  | pass   | PNG (64, 48)                               |
| bmp  | ppm  | pass   | PNG (64, 48)                               |
| bmp  | qoi  | pass   | QOI (64, 48)                               |
| bmp  | tga  | pass   | TGA (64, 48)                               |
| bmp  | tiff | pass   | TIFF (64, 48)                              |
| bmp  | webp | pass   | WEBP (64, 48)                              |
| exr  | avif | pass   | PNG (64, 48)                               |
| exr  | bmp  | pass   | BMP (64, 48)                               |
| exr  | gif  | pass   | GIF (64, 48)                               |
| exr  | heic | pass   | decoded (64, 48)                           |
| exr  | ico  | pass   | ICO (64, 48)                               |
| exr  | jpeg | pass   | JPEG (64, 48)                              |
| exr  | pdf  | pass   | 1 page, image present                      |
| exr  | png  | pass   | PNG (64, 48)                               |
| exr  | ppm  | pass   | PNG (64, 48)                               |
| exr  | qoi  | pass   | QOI (64, 48)                               |
| exr  | tga  | pass   | TGA (64, 48)                               |
| exr  | tiff | pass   | TIFF 32-bit float (libtiff) (64, 48)       |
| exr  | webp | pass   | WEBP (64, 48)                              |
| gif  | avi  | pass   | avi v:mpeg4 2.00s                          |
| gif  | avif | pass   | PNG (64, 48)                               |
| gif  | bmp  | pass   | BMP (64, 48)                               |
| gif  | exr  | pass   | PNG (64, 48)                               |
| gif  | heic | pass   | decoded (64, 48)                           |
| gif  | ico  | pass   | ICO (64, 48)                               |
| gif  | jpeg | pass   | JPEG (64, 48)                              |
| gif  | mkv  | pass   | matroska,webm v:h264 2.00s                 |
| gif  | mov  | pass   | mov,mp4,m4a,3gp,3g2,mj2 v:h264 2.00s       |
| gif  | mp4  | pass   | mov,mp4,m4a,3gp,3g2,mj2 v:h264 2.00s       |
| gif  | pdf  | pass   | 1 page, image present                      |
| gif  | png  | pass   | PNG (64, 48)                               |
| gif  | ppm  | pass   | PNG (64, 48)                               |
| gif  | qoi  | pass   | QOI (64, 48)                               |
| gif  | tga  | pass   | TGA (64, 48)                               |
| gif  | tiff | pass   | TIFF (64, 48)                              |
| gif  | webm | pass   | matroska,webm v:vp9 2.00s                  |
| gif  | webp | pass   | WEBP (64, 48)                              |
| heic | avif | pass   | PNG (64, 48)                               |
| heic | bmp  | pass   | BMP (64, 48)                               |
| heic | exr  | pass   | PNG (64, 48)                               |
| heic | gif  | pass   | GIF (64, 48)                               |
| heic | ico  | pass   | ICO (64, 48)                               |
| heic | jpeg | pass   | JPEG (64, 48)                              |
| heic | pdf  | pass   | 1 page, image present                      |
| heic | png  | pass   | PNG (64, 48)                               |
| heic | ppm  | pass   | PNG (64, 48)                               |
| heic | qoi  | pass   | QOI (64, 48)                               |
| heic | tga  | pass   | TGA (64, 48)                               |
| heic | tiff | pass   | TIFF (64, 48)                              |
| heic | webp | pass   | WEBP (64, 48)                              |
| ico  | avif | pass   | PNG (64, 48)                               |
| ico  | bmp  | pass   | BMP (64, 48)                               |
| ico  | exr  | pass   | PNG (64, 48)                               |
| ico  | gif  | pass   | GIF (64, 48)                               |
| ico  | heic | pass   | decoded (64, 48)                           |
| ico  | jpeg | pass   | JPEG (64, 48)                              |
| ico  | pdf  | pass   | 1 page, image present                      |
| ico  | png  | pass   | PNG (64, 48)                               |
| ico  | ppm  | pass   | PNG (64, 48)                               |
| ico  | qoi  | pass   | QOI (64, 48)                               |
| ico  | tga  | pass   | TGA (64, 48)                               |
| ico  | tiff | pass   | TIFF (64, 48)                              |
| ico  | webp | pass   | WEBP (64, 48)                              |
| jpeg | avif | pass   | PNG (64, 48)                               |
| jpeg | bmp  | pass   | BMP (64, 48)                               |
| jpeg | exr  | pass   | PNG (64, 48)                               |
| jpeg | gif  | pass   | GIF (64, 48)                               |
| jpeg | heic | pass   | decoded (64, 48)                           |
| jpeg | ico  | pass   | ICO (64, 48)                               |
| jpeg | pdf  | pass   | 1 page, image present                      |
| jpeg | png  | pass   | PNG (64, 48)                               |
| jpeg | ppm  | pass   | PNG (64, 48)                               |
| jpeg | qoi  | pass   | QOI (64, 48)                               |
| jpeg | tga  | pass   | TGA (64, 48)                               |
| jpeg | tiff | pass   | TIFF (64, 48)                              |
| jpeg | webp | pass   | WEBP (64, 48)                              |
| png  | avif | pass   | PNG (64, 48)                               |
| png  | bmp  | pass   | BMP (64, 48)                               |
| png  | exr  | pass   | PNG (64, 48)                               |
| png  | gif  | pass   | GIF (64, 48)                               |
| png  | heic | pass   | decoded (64, 48)                           |
| png  | ico  | pass   | ICO (64, 48)                               |
| png  | jpeg | pass   | JPEG (64, 48)                              |
| png  | pdf  | pass   | 1 page, image present                      |
| png  | ppm  | pass   | PNG (64, 48)                               |
| png  | qoi  | pass   | QOI (64, 48)                               |
| png  | tga  | pass   | TGA (64, 48)                               |
| png  | tiff | pass   | TIFF (64, 48)                              |
| png  | webp | pass   | WEBP (64, 48)                              |
| ppm  | avif | pass   | PNG (64, 48)                               |
| ppm  | bmp  | pass   | BMP (64, 48)                               |
| ppm  | exr  | pass   | PNG (64, 48)                               |
| ppm  | gif  | pass   | GIF (64, 48)                               |
| ppm  | heic | pass   | decoded (64, 48)                           |
| ppm  | ico  | pass   | ICO (64, 48)                               |
| ppm  | jpeg | pass   | JPEG (64, 48)                              |
| ppm  | pdf  | pass   | 1 page, image present                      |
| ppm  | png  | pass   | PNG (64, 48)                               |
| ppm  | qoi  | pass   | QOI (64, 48)                               |
| ppm  | tga  | pass   | TGA (64, 48)                               |
| ppm  | tiff | pass   | TIFF (64, 48)                              |
| ppm  | webp | pass   | WEBP (64, 48)                              |
| qoi  | avif | pass   | PNG (64, 48)                               |
| qoi  | bmp  | pass   | BMP (64, 48)                               |
| qoi  | exr  | pass   | PNG (64, 48)                               |
| qoi  | gif  | pass   | GIF (64, 48)                               |
| qoi  | heic | pass   | decoded (64, 48)                           |
| qoi  | ico  | pass   | ICO (64, 48)                               |
| qoi  | jpeg | pass   | JPEG (64, 48)                              |
| qoi  | pdf  | pass   | 1 page, image present                      |
| qoi  | png  | pass   | PNG (64, 48)                               |
| qoi  | ppm  | pass   | PNG (64, 48)                               |
| qoi  | tga  | pass   | TGA (64, 48)                               |
| qoi  | tiff | pass   | TIFF (64, 48)                              |
| qoi  | webp | pass   | WEBP (64, 48)                              |
| tga  | avif | pass   | PNG (64, 48)                               |
| tga  | bmp  | pass   | BMP (64, 48)                               |
| tga  | exr  | pass   | PNG (64, 48)                               |
| tga  | gif  | pass   | GIF (64, 48)                               |
| tga  | heic | pass   | decoded (64, 48)                           |
| tga  | ico  | pass   | ICO (64, 48)                               |
| tga  | jpeg | pass   | JPEG (64, 48)                              |
| tga  | pdf  | pass   | 1 page, image present                      |
| tga  | png  | pass   | PNG (64, 48)                               |
| tga  | ppm  | pass   | PNG (64, 48)                               |
| tga  | qoi  | pass   | QOI (64, 48)                               |
| tga  | tiff | pass   | TIFF (64, 48)                              |
| tga  | webp | pass   | WEBP (64, 48)                              |
| tiff | avif | pass   | PNG (64, 48)                               |
| tiff | bmp  | pass   | BMP (64, 48)                               |
| tiff | exr  | pass   | PNG (64, 48)                               |
| tiff | gif  | pass   | GIF (64, 48)                               |
| tiff | heic | pass   | decoded (64, 48)                           |
| tiff | ico  | pass   | ICO (64, 48)                               |
| tiff | jpeg | pass   | JPEG (64, 48)                              |
| tiff | pdf  | pass   | 1 page, image present                      |
| tiff | png  | pass   | PNG (64, 48)                               |
| tiff | ppm  | pass   | PNG (64, 48)                               |
| tiff | qoi  | pass   | QOI (64, 48)                               |
| tiff | tga  | pass   | TGA (64, 48)                               |
| tiff | webp | pass   | WEBP (64, 48)                              |
| webp | avif | pass   | PNG (64, 48)                               |
| webp | bmp  | pass   | BMP (64, 48)                               |
| webp | exr  | pass   | PNG (64, 48)                               |
| webp | gif  | pass   | GIF (64, 48)                               |
| webp | heic | pass   | decoded (64, 48)                           |
| webp | ico  | pass   | ICO (64, 48)                               |
| webp | jpeg | pass   | JPEG (64, 48)                              |
| webp | pdf  | pass   | 1 page, image present                      |
| webp | png  | pass   | PNG (64, 48)                               |
| webp | ppm  | pass   | PNG (64, 48)                               |
| webp | qoi  | pass   | QOI (64, 48)                               |
| webp | tga  | pass   | TGA (64, 48)                               |
| webp | tiff | pass   | TIFF (64, 48)                              |
| svg  | avif | pass   | PNG (64, 48)                               |
| svg  | bmp  | pass   | BMP (64, 48)                               |
| svg  | exr  | pass   | PNG (64, 48)                               |
| svg  | gif  | pass   | GIF (64, 48)                               |
| svg  | heic | pass   | decoded (64, 48)                           |
| svg  | ico  | pass   | ICO (64, 48)                               |
| svg  | jpeg | pass   | JPEG (64, 48)                              |
| svg  | pdf  | pass   | 1 page, image present                      |
| svg  | png  | pass   | PNG (64, 48)                               |
| svg  | ppm  | pass   | PNG (64, 48)                               |
| svg  | qoi  | pass   | QOI (64, 48)                               |
| svg  | tga  | pass   | TGA (64, 48)                               |
| svg  | tiff | pass   | TIFF (64, 48)                              |
| svg  | webp | pass   | WEBP (64, 48)                              |
| avi  | aac  | pass   | aac a:aac 2.05s                            |
| avi  | aiff | pass   | aiff a:pcm_s16be 2.04s                     |
| avi  | avif | pass   | frame (160, 120)                           |
| avi  | bmp  | pass   | frame (160, 120)                           |
| avi  | caf  | pass   | caf a:alac 2.04s                           |
| avi  | exr  | pass   | frame (160, 120)                           |
| avi  | flac | pass   | flac a:flac 2.04s                          |
| avi  | gif  | pass   | gif v:gif 2.08s                            |
| avi  | heic | pass   | frame (160, 120)                           |
| avi  | ico  | pass   | frame (160, 120)                           |
| avi  | jpeg | pass   | frame (160, 120)                           |
| avi  | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.04s        |
| avi  | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.04s        |
| avi  | mkv  | pass   | matroska,webm v:h264,a:aac 2.07s           |
| avi  | mov  | pass   | mov,mp4,m4a,3gp,3g2,mj2 v:h264,a:aac 2.07s |
| avi  | mp3  | pass   | mp3 a:mp3 2.06s                            |
| avi  | mp4  | pass   | mov,mp4,m4a,3gp,3g2,mj2 v:h264,a:aac 2.07s |
| avi  | ogg  | pass   | ogg a:vorbis 2.04s                         |
| avi  | opus | pass   | ogg a:opus 2.04s                           |
| avi  | png  | pass   | frame (160, 120)                           |
| avi  | ppm  | pass   | frame (160, 120)                           |
| avi  | qoi  | pass   | frame (160, 120)                           |
| avi  | tga  | pass   | frame (160, 120)                           |
| avi  | tiff | pass   | frame (160, 120)                           |
| avi  | wav  | pass   | wav a:pcm_s16le 2.04s                      |
| avi  | webm | pass   | matroska,webm v:vp9,a:opus 2.07s           |
| avi  | webp | pass   | frame (160, 120)                           |
| mkv  | aac  | pass   | aac a:aac 2.04s                            |
| mkv  | aiff | pass   | aiff a:pcm_s16be 2.00s                     |
| mkv  | avi  | pass   | avi v:mpeg4,a:mp3 2.07s                    |
| mkv  | avif | pass   | frame (160, 120)                           |
| mkv  | bmp  | pass   | frame (160, 120)                           |
| mkv  | caf  | pass   | caf a:alac 2.04s                           |
| mkv  | exr  | pass   | frame (160, 120)                           |
| mkv  | flac | pass   | flac a:flac 2.00s                          |
| mkv  | gif  | pass   | gif v:gif 2.00s                            |
| mkv  | heic | pass   | frame (160, 120)                           |
| mkv  | ico  | pass   | frame (160, 120)                           |
| mkv  | jpeg | pass   | frame (160, 120)                           |
| mkv  | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| mkv  | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| mkv  | mov  | pass   | mov,mp4,m4a,3gp,3g2,mj2 v:h264,a:aac 2.00s |
| mkv  | mp3  | pass   | mp3 a:mp3 2.04s                            |
| mkv  | mp4  | pass   | mov,mp4,m4a,3gp,3g2,mj2 v:h264,a:aac 2.00s |
| mkv  | ogg  | pass   | ogg a:vorbis 2.00s                         |
| mkv  | opus | pass   | ogg a:opus 2.01s                           |
| mkv  | png  | pass   | frame (160, 120)                           |
| mkv  | ppm  | pass   | frame (160, 120)                           |
| mkv  | qoi  | pass   | frame (160, 120)                           |
| mkv  | srt  | pass   | 111 bytes, content ok                      |
| mkv  | tga  | pass   | frame (160, 120)                           |
| mkv  | tiff | pass   | frame (160, 120)                           |
| mkv  | txt  | pass   | 45 bytes, content ok                       |
| mkv  | vtt  | pass   | 102 bytes, content ok                      |
| mkv  | wav  | pass   | wav a:pcm_s16le 2.00s                      |
| mkv  | webm | pass   | matroska,webm v:vp9,a:opus,s:webvtt 2.01s  |
| mkv  | webp | pass   | frame (160, 120)                           |
| mov  | aac  | pass   | aac a:aac 2.04s                            |
| mov  | aiff | pass   | aiff a:pcm_s16be 2.02s                     |
| mov  | avi  | pass   | avi v:mpeg4,a:mp3 2.07s                    |
| mov  | avif | pass   | frame (160, 120)                           |
| mov  | bmp  | pass   | frame (160, 120)                           |
| mov  | caf  | pass   | caf a:alac 2.04s                           |
| mov  | exr  | pass   | frame (160, 120)                           |
| mov  | flac | pass   | flac a:flac 2.02s                          |
| mov  | gif  | pass   | gif v:gif 2.00s                            |
| mov  | heic | pass   | frame (160, 120)                           |
| mov  | ico  | pass   | frame (160, 120)                           |
| mov  | jpeg | pass   | frame (160, 120)                           |
| mov  | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.02s        |
| mov  | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.02s        |
| mov  | mkv  | pass   | matroska,webm v:h264,a:aac,s:ass 2.04s     |
| mov  | mp3  | pass   | mp3 a:mp3 2.06s                            |
| mov  | mp4  | pass   | mov,mp4,m4a,3gp,3g2,mj2 v:h264,a:aac 2.02s |
| mov  | ogg  | pass   | ogg a:vorbis 2.02s                         |
| mov  | opus | pass   | ogg a:opus 2.03s                           |
| mov  | png  | pass   | frame (160, 120)                           |
| mov  | ppm  | pass   | frame (160, 120)                           |
| mov  | qoi  | pass   | frame (160, 120)                           |
| mov  | srt  | pass   | 111 bytes, content ok                      |
| mov  | tga  | pass   | frame (160, 120)                           |
| mov  | tiff | pass   | frame (160, 120)                           |
| mov  | txt  | pass   | 45 bytes, content ok                       |
| mov  | vtt  | pass   | 102 bytes, content ok                      |
| mov  | wav  | pass   | wav a:pcm_s16le 2.02s                      |
| mov  | webm | pass   | matroska,webm v:vp9,a:opus,s:webvtt 2.03s  |
| mov  | webp | pass   | frame (160, 120)                           |
| mp4  | aac  | pass   | aac a:aac 2.04s                            |
| mp4  | aiff | pass   | aiff a:pcm_s16be 2.02s                     |
| mp4  | avi  | pass   | avi v:mpeg4,a:mp3 2.07s                    |
| mp4  | avif | pass   | frame (160, 120)                           |
| mp4  | bmp  | pass   | frame (160, 120)                           |
| mp4  | caf  | pass   | caf a:alac 2.04s                           |
| mp4  | exr  | pass   | frame (160, 120)                           |
| mp4  | flac | pass   | flac a:flac 2.02s                          |
| mp4  | gif  | pass   | gif v:gif 2.00s                            |
| mp4  | heic | pass   | frame (160, 120)                           |
| mp4  | ico  | pass   | frame (160, 120)                           |
| mp4  | jpeg | pass   | frame (160, 120)                           |
| mp4  | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.02s        |
| mp4  | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.02s        |
| mp4  | mkv  | pass   | matroska,webm v:h264,a:aac,s:ass 2.04s     |
| mp4  | mov  | pass   | mov,mp4,m4a,3gp,3g2,mj2 v:h264,a:aac 2.02s |
| mp4  | mp3  | pass   | mp3 a:mp3 2.06s                            |
| mp4  | ogg  | pass   | ogg a:vorbis 2.02s                         |
| mp4  | opus | pass   | ogg a:opus 2.03s                           |
| mp4  | png  | pass   | frame (160, 120)                           |
| mp4  | ppm  | pass   | frame (160, 120)                           |
| mp4  | qoi  | pass   | frame (160, 120)                           |
| mp4  | srt  | pass   | 111 bytes, content ok                      |
| mp4  | tga  | pass   | frame (160, 120)                           |
| mp4  | tiff | pass   | frame (160, 120)                           |
| mp4  | txt  | pass   | 45 bytes, content ok                       |
| mp4  | vtt  | pass   | 102 bytes, content ok                      |
| mp4  | wav  | pass   | wav a:pcm_s16le 2.02s                      |
| mp4  | webm | pass   | matroska,webm v:vp9,a:opus,s:webvtt 2.03s  |
| mp4  | webp | pass   | frame (160, 120)                           |
| webm | aac  | pass   | aac a:aac 2.02s                            |
| webm | aiff | pass   | aiff a:pcm_s16be 2.00s                     |
| webm | avi  | pass   | avi v:mpeg4,a:mp3 2.07s                    |
| webm | avif | pass   | frame (160, 120)                           |
| webm | bmp  | pass   | frame (160, 120)                           |
| webm | caf  | pass   | caf a:alac 2.05s                           |
| webm | exr  | pass   | frame (160, 120)                           |
| webm | flac | pass   | flac a:flac 2.00s                          |
| webm | gif  | pass   | gif v:gif 2.00s                            |
| webm | heic | pass   | frame (160, 120)                           |
| webm | ico  | pass   | frame (160, 120)                           |
| webm | jpeg | pass   | frame (160, 120)                           |
| webm | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.01s        |
| webm | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.01s        |
| webm | mkv  | pass   | matroska,webm v:h264,a:aac,s:ass 2.03s     |
| webm | mov  | pass   | mov,mp4,m4a,3gp,3g2,mj2 v:h264,a:aac 2.01s |
| webm | mp3  | pass   | mp3 a:mp3 2.04s                            |
| webm | mp4  | pass   | mov,mp4,m4a,3gp,3g2,mj2 v:h264,a:aac 2.01s |
| webm | ogg  | pass   | ogg a:vorbis 2.00s                         |
| webm | opus | pass   | ogg a:opus 2.01s                           |
| webm | png  | pass   | frame (160, 120)                           |
| webm | ppm  | pass   | frame (160, 120)                           |
| webm | qoi  | pass   | frame (160, 120)                           |
| webm | srt  | pass   | 111 bytes, content ok                      |
| webm | tga  | pass   | frame (160, 120)                           |
| webm | tiff | pass   | frame (160, 120)                           |
| webm | txt  | pass   | 45 bytes, content ok                       |
| webm | vtt  | pass   | 102 bytes, content ok                      |
| webm | wav  | pass   | wav a:pcm_s16le 2.00s                      |
| webm | webp | pass   | frame (160, 120)                           |
| aac  | aiff | pass   | aiff a:pcm_s16be 2.04s                     |
| aac  | caf  | pass   | caf a:alac 2.04s                           |
| aac  | flac | pass   | flac a:flac 2.04s                          |
| aac  | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.04s        |
| aac  | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.04s        |
| aac  | mp3  | pass   | mp3 a:mp3 2.09s                            |
| aac  | ogg  | pass   | ogg a:vorbis 2.04s                         |
| aac  | opus | pass   | ogg a:opus 2.05s                           |
| aac  | wav  | pass   | wav a:pcm_s16le 2.04s                      |
| aiff | aac  | pass   | aac a:aac 2.04s                            |
| aiff | caf  | pass   | caf a:alac 2.04s                           |
| aiff | flac | pass   | flac a:flac 2.00s                          |
| aiff | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| aiff | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| aiff | mp3  | pass   | mp3 a:mp3 2.04s                            |
| aiff | ogg  | pass   | ogg a:vorbis 2.00s                         |
| aiff | opus | pass   | ogg a:opus 2.01s                           |
| aiff | wav  | pass   | wav a:pcm_s16le 2.00s                      |
| caf  | aac  | pass   | aac a:aac 2.04s                            |
| caf  | aiff | pass   | aiff a:pcm_s16be 2.00s                     |
| caf  | flac | pass   | flac a:flac 2.00s                          |
| caf  | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| caf  | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| caf  | mp3  | pass   | mp3 a:mp3 2.04s                            |
| caf  | ogg  | pass   | ogg a:vorbis 2.00s                         |
| caf  | opus | pass   | ogg a:opus 2.01s                           |
| caf  | wav  | pass   | wav a:pcm_s16le 2.00s                      |
| flac | aac  | pass   | aac a:aac 2.04s                            |
| flac | aiff | pass   | aiff a:pcm_s16be 2.00s                     |
| flac | caf  | pass   | caf a:alac 2.04s                           |
| flac | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| flac | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| flac | mp3  | pass   | mp3 a:mp3 2.04s                            |
| flac | ogg  | pass   | ogg a:vorbis 2.00s                         |
| flac | opus | pass   | ogg a:opus 2.01s                           |
| flac | wav  | pass   | wav a:pcm_s16le 2.00s                      |
| m4a  | aac  | pass   | aac a:aac 2.04s                            |
| m4a  | aiff | pass   | aiff a:pcm_s16be 2.02s                     |
| m4a  | caf  | pass   | caf a:alac 2.04s                           |
| m4a  | flac | pass   | flac a:flac 2.02s                          |
| m4a  | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.02s        |
| m4a  | mp3  | pass   | mp3 a:mp3 2.06s                            |
| m4a  | ogg  | pass   | ogg a:vorbis 2.02s                         |
| m4a  | opus | pass   | ogg a:opus 2.03s                           |
| m4a  | wav  | pass   | wav a:pcm_s16le 2.02s                      |
| m4r  | aac  | pass   | aac a:aac 2.04s                            |
| m4r  | aiff | pass   | aiff a:pcm_s16be 2.02s                     |
| m4r  | caf  | pass   | caf a:alac 2.04s                           |
| m4r  | flac | pass   | flac a:flac 2.02s                          |
| m4r  | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.02s        |
| m4r  | mp3  | pass   | mp3 a:mp3 2.06s                            |
| m4r  | ogg  | pass   | ogg a:vorbis 2.02s                         |
| m4r  | opus | pass   | ogg a:opus 2.03s                           |
| m4r  | wav  | pass   | wav a:pcm_s16le 2.02s                      |
| mp3  | aac  | pass   | aac a:aac 2.04s                            |
| mp3  | aiff | pass   | aiff a:pcm_s16be 2.00s                     |
| mp3  | caf  | pass   | caf a:alac 2.04s                           |
| mp3  | flac | pass   | flac a:flac 2.00s                          |
| mp3  | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| mp3  | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| mp3  | ogg  | pass   | ogg a:vorbis 2.00s                         |
| mp3  | opus | pass   | ogg a:opus 2.01s                           |
| mp3  | wav  | pass   | wav a:pcm_s16le 2.00s                      |
| ogg  | aac  | pass   | aac a:aac 2.04s                            |
| ogg  | aiff | pass   | aiff a:pcm_s16be 2.00s                     |
| ogg  | caf  | pass   | caf a:alac 2.04s                           |
| ogg  | flac | pass   | flac a:flac 2.00s                          |
| ogg  | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| ogg  | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| ogg  | mp3  | pass   | mp3 a:mp3 2.04s                            |
| ogg  | opus | pass   | ogg a:opus 2.01s                           |
| ogg  | wav  | pass   | wav a:pcm_s16le 2.00s                      |
| opus | aac  | pass   | aac a:aac 2.02s                            |
| opus | aiff | pass   | aiff a:pcm_s16be 2.00s                     |
| opus | caf  | pass   | caf a:alac 2.05s                           |
| opus | flac | pass   | flac a:flac 2.00s                          |
| opus | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| opus | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| opus | mp3  | pass   | mp3 a:mp3 2.04s                            |
| opus | ogg  | pass   | ogg a:vorbis 2.00s                         |
| opus | wav  | pass   | wav a:pcm_s16le 2.00s                      |
| wav  | aac  | pass   | aac a:aac 2.04s                            |
| wav  | aiff | pass   | aiff a:pcm_s16be 2.00s                     |
| wav  | caf  | pass   | caf a:alac 2.04s                           |
| wav  | flac | pass   | flac a:flac 2.00s                          |
| wav  | m4a  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| wav  | m4r  | pass   | mov,mp4,m4a,3gp,3g2,mj2 a:aac 2.00s        |
| wav  | mp3  | pass   | mp3 a:mp3 2.04s                            |
| wav  | ogg  | pass   | ogg a:vorbis 2.00s                         |
| wav  | opus | pass   | ogg a:opus 2.01s                           |
| pdf  | avif | pass   | 3 page image(s), [(2000, 2828)]            |
| pdf  | bmp  | pass   | 3 page image(s), [(2000, 2828)]            |
| pdf  | doc  | pass   | re-read by LibreOffice, text ok            |
| pdf  | docx | pass   | re-read by LibreOffice, text ok            |
| pdf  | exr  | pass   | 3 page image(s), [(2000, 2828)]            |
| pdf  | gif  | pass   | 3 page image(s), [(2000, 2828)]            |
| pdf  | heic | pass   | 3 page image(s), [(2000, 2828)]            |
| pdf  | html | pass   | 494 bytes, content ok                      |
| pdf  | ico  | pass   | 3 page image(s), [(181, 256)]              |
| pdf  | jpeg | pass   | 3 page image(s), [(2000, 2828)]            |
| pdf  | md   | pass   | 246 bytes, content ok                      |
| pdf  | odt  | pass   | re-read by LibreOffice, text ok            |
| pdf  | png  | pass   | 3 page image(s), [(2000, 2828)]            |
| pdf  | ppm  | pass   | 3 page image(s), [(2000, 2828)]            |
| pdf  | qoi  | pass   | 3 page image(s), [(2000, 2828)]            |
| pdf  | rtf  | pass   | re-read by LibreOffice, text ok            |
| pdf  | tga  | pass   | 3 page image(s), [(2000, 2828)]            |
| pdf  | tiff | pass   | 3 page image(s), [(2000, 2828)]            |
| pdf  | txt  | pass   | 242 bytes, content ok                      |
| pdf  | webp | pass   | 3 page image(s), [(2000, 2828)]            |
| doc  | avif | pass   | 3 page image(s), [(2000, 2828)]            |
| doc  | bmp  | pass   | 3 page image(s), [(2000, 2828)]            |
| doc  | docx | pass   | re-read by LibreOffice, text ok            |
| doc  | exr  | pass   | 3 page image(s), [(2000, 2828)]            |
| doc  | gif  | pass   | 3 page image(s), [(2000, 2828)]            |
| doc  | heic | pass   | 3 page image(s), [(2000, 2828)]            |
| doc  | html | pass   | 4229 bytes, content ok                     |
| doc  | ico  | pass   | 3 page image(s), [(181, 256)]              |
| doc  | jpeg | pass   | 3 page image(s), [(2000, 2828)]            |
| doc  | md   | pass   | 257 bytes, content ok                      |
| doc  | odt  | pass   | re-read by LibreOffice, text ok            |
| doc  | pdf  | pass   | 3 pages, text ok                           |
| doc  | png  | pass   | 3 page image(s), [(2000, 2828)]            |
| doc  | ppm  | pass   | 3 page image(s), [(2000, 2828)]            |
| doc  | qoi  | pass   | 3 page image(s), [(2000, 2828)]            |
| doc  | rtf  | pass   | re-read by LibreOffice, text ok            |
| doc  | tga  | pass   | 3 page image(s), [(2000, 2828)]            |
| doc  | tiff | pass   | 3 page image(s), [(2000, 2828)]            |
| doc  | txt  | pass   | 244 bytes, content ok                      |
| doc  | webp | pass   | 3 page image(s), [(2000, 2828)]            |
| docx | avif | pass   | 3 page image(s), [(2000, 2828)]            |
| docx | bmp  | pass   | 3 page image(s), [(2000, 2828)]            |
| docx | doc  | pass   | re-read by LibreOffice, text ok            |
| docx | exr  | pass   | 3 page image(s), [(2000, 2828)]            |
| docx | gif  | pass   | 3 page image(s), [(2000, 2828)]            |
| docx | heic | pass   | 3 page image(s), [(2000, 2828)]            |
| docx | html | pass   | 4120 bytes, content ok                     |
| docx | ico  | pass   | 3 page image(s), [(181, 256)]              |
| docx | jpeg | pass   | 3 page image(s), [(2000, 2828)]            |
| docx | md   | pass   | 257 bytes, content ok                      |
| docx | odt  | pass   | re-read by LibreOffice, text ok            |
| docx | pdf  | pass   | 3 pages, text ok                           |
| docx | png  | pass   | 3 page image(s), [(2000, 2828)]            |
| docx | ppm  | pass   | 3 page image(s), [(2000, 2828)]            |
| docx | qoi  | pass   | 3 page image(s), [(2000, 2828)]            |
| docx | rtf  | pass   | re-read by LibreOffice, text ok            |
| docx | tga  | pass   | 3 page image(s), [(2000, 2828)]            |
| docx | tiff | pass   | 3 page image(s), [(2000, 2828)]            |
| docx | txt  | pass   | 244 bytes, content ok                      |
| docx | webp | pass   | 3 page image(s), [(2000, 2828)]            |
| html | avif | pass   | 3 page image(s), [(2000, 2828)]            |
| html | bmp  | pass   | 3 page image(s), [(2000, 2828)]            |
| html | doc  | pass   | re-read by LibreOffice, text ok            |
| html | docx | pass   | re-read by LibreOffice, text ok            |
| html | exr  | pass   | 3 page image(s), [(2000, 2828)]            |
| html | gif  | pass   | 3 page image(s), [(2000, 2828)]            |
| html | heic | pass   | 3 page image(s), [(2000, 2828)]            |
| html | ico  | pass   | 3 page image(s), [(181, 256)]              |
| html | jpeg | pass   | 3 page image(s), [(2000, 2828)]            |
| html | md   | pass   | 257 bytes, content ok                      |
| html | odt  | pass   | re-read by LibreOffice, text ok            |
| html | pdf  | pass   | 3 pages, text ok                           |
| html | png  | pass   | 3 page image(s), [(2000, 2828)]            |
| html | ppm  | pass   | 3 page image(s), [(2000, 2828)]            |
| html | qoi  | pass   | 3 page image(s), [(2000, 2828)]            |
| html | rtf  | pass   | re-read by LibreOffice, text ok            |
| html | tga  | pass   | 3 page image(s), [(2000, 2828)]            |
| html | tiff | pass   | 3 page image(s), [(2000, 2828)]            |
| html | txt  | pass   | 244 bytes, content ok                      |
| html | webp | pass   | 3 page image(s), [(2000, 2828)]            |
| json | csv  | pass   | 28 bytes, content ok                       |
| json | doc  | pass   | re-read by LibreOffice, text ok            |
| json | docx | pass   | re-read by LibreOffice, text ok            |
| json | html | pass   | 1119 bytes, content ok                     |
| json | jpeg | pass   | 1 page image(s), [(2000, 2828)]            |
| json | md   | pass   | 60 bytes, content ok                       |
| json | ods  | pass   | re-read by LibreOffice, text ok            |
| json | odt  | pass   | re-read by LibreOffice, text ok            |
| json | pdf  | pass   | 1 pages, text ok                           |
| json | png  | pass   | 1 page image(s), [(2000, 2828)]            |
| json | rtf  | pass   | re-read by LibreOffice, text ok            |
| json | tsv  | pass   | 28 bytes, content ok                       |
| json | txt  | pass   | 32 bytes, content ok                       |
| json | xls  | pass   | re-read by LibreOffice, text ok            |
| json | xlsx | pass   | re-read by LibreOffice, text ok            |
| json | yaml | pass   | 52 bytes, content ok                       |
| md   | doc  | pass   | re-read by LibreOffice, text ok            |
| md   | docx | pass   | re-read by LibreOffice, text ok            |
| md   | html | pass   | 1429 bytes, content ok                     |
| md   | jpeg | pass   | 1 page image(s), [(2000, 2828)]            |
| md   | odt  | pass   | re-read by LibreOffice, text ok            |
| md   | pdf  | pass   | 1 pages, text ok                           |
| md   | png  | pass   | 1 page image(s), [(2000, 2828)]            |
| md   | rtf  | pass   | re-read by LibreOffice, text ok            |
| md   | txt  | pass   | 250 bytes, content ok                      |
| odt  | avif | pass   | 3 page image(s), [(2000, 2828)]            |
| odt  | bmp  | pass   | 3 page image(s), [(2000, 2828)]            |
| odt  | doc  | pass   | re-read by LibreOffice, text ok            |
| odt  | docx | pass   | re-read by LibreOffice, text ok            |
| odt  | exr  | pass   | 3 page image(s), [(2000, 2828)]            |
| odt  | gif  | pass   | 3 page image(s), [(2000, 2828)]            |
| odt  | heic | pass   | 3 page image(s), [(2000, 2828)]            |
| odt  | html | pass   | 2947 bytes, content ok                     |
| odt  | ico  | pass   | 3 page image(s), [(181, 256)]              |
| odt  | jpeg | pass   | 3 page image(s), [(2000, 2828)]            |
| odt  | md   | pass   | 257 bytes, content ok                      |
| odt  | pdf  | pass   | 3 pages, text ok                           |
| odt  | png  | pass   | 3 page image(s), [(2000, 2828)]            |
| odt  | ppm  | pass   | 3 page image(s), [(2000, 2828)]            |
| odt  | qoi  | pass   | 3 page image(s), [(2000, 2828)]            |
| odt  | rtf  | pass   | re-read by LibreOffice, text ok            |
| odt  | tga  | pass   | 3 page image(s), [(2000, 2828)]            |
| odt  | tiff | pass   | 3 page image(s), [(2000, 2828)]            |
| odt  | txt  | pass   | 244 bytes, content ok                      |
| odt  | webp | pass   | 3 page image(s), [(2000, 2828)]            |
| rtf  | avif | pass   | 3 page image(s), [(2000, 2828)]            |
| rtf  | bmp  | pass   | 3 page image(s), [(2000, 2828)]            |
| rtf  | doc  | pass   | re-read by LibreOffice, text ok            |
| rtf  | docx | pass   | re-read by LibreOffice, text ok            |
| rtf  | exr  | pass   | 3 page image(s), [(2000, 2828)]            |
| rtf  | gif  | pass   | 3 page image(s), [(2000, 2828)]            |
| rtf  | heic | pass   | 3 page image(s), [(2000, 2828)]            |
| rtf  | html | pass   | 4469 bytes, content ok                     |
| rtf  | ico  | pass   | 3 page image(s), [(181, 256)]              |
| rtf  | jpeg | pass   | 3 page image(s), [(2000, 2828)]            |
| rtf  | md   | pass   | 257 bytes, content ok                      |
| rtf  | odt  | pass   | re-read by LibreOffice, text ok            |
| rtf  | pdf  | pass   | 3 pages, text ok                           |
| rtf  | png  | pass   | 3 page image(s), [(2000, 2828)]            |
| rtf  | ppm  | pass   | 3 page image(s), [(2000, 2828)]            |
| rtf  | qoi  | pass   | 3 page image(s), [(2000, 2828)]            |
| rtf  | tga  | pass   | 3 page image(s), [(2000, 2828)]            |
| rtf  | tiff | pass   | 3 page image(s), [(2000, 2828)]            |
| rtf  | txt  | pass   | 244 bytes, content ok                      |
| rtf  | webp | pass   | 3 page image(s), [(2000, 2828)]            |
| srt  | txt  | pass   | 45 bytes, content ok                       |
| srt  | vtt  | pass   | 115 bytes, content ok                      |
| txt  | avif | pass   | 1 page image(s), [(2000, 2828)]            |
| txt  | bmp  | pass   | 1 page image(s), [(2000, 2828)]            |
| txt  | doc  | pass   | re-read by LibreOffice, text ok            |
| txt  | docx | pass   | re-read by LibreOffice, text ok            |
| txt  | exr  | pass   | 1 page image(s), [(2000, 2828)]            |
| txt  | gif  | pass   | 1 page image(s), [(2000, 2828)]            |
| txt  | heic | pass   | 1 page image(s), [(2000, 2828)]            |
| txt  | html | pass   | 1012 bytes, content ok                     |
| txt  | ico  | pass   | 1 page image(s), [(181, 256)]              |
| txt  | jpeg | pass   | 1 page image(s), [(2000, 2828)]            |
| txt  | odt  | pass   | re-read by LibreOffice, text ok            |
| txt  | pdf  | pass   | 1 pages, text ok                           |
| txt  | png  | pass   | 1 page image(s), [(2000, 2828)]            |
| txt  | ppm  | pass   | 1 page image(s), [(2000, 2828)]            |
| txt  | qoi  | pass   | 1 page image(s), [(2000, 2828)]            |
| txt  | rtf  | pass   | re-read by LibreOffice, text ok            |
| txt  | tga  | pass   | 1 page image(s), [(2000, 2828)]            |
| txt  | tiff | pass   | 1 page image(s), [(2000, 2828)]            |
| txt  | webp | pass   | 1 page image(s), [(2000, 2828)]            |
| vtt  | srt  | pass   | 111 bytes, content ok                      |
| vtt  | txt  | pass   | 45 bytes, content ok                       |
| yaml | csv  | pass   | 28 bytes, content ok                       |
| yaml | doc  | pass   | re-read by LibreOffice, text ok            |
| yaml | docx | pass   | re-read by LibreOffice, text ok            |
| yaml | html | pass   | 1119 bytes, content ok                     |
| yaml | jpeg | pass   | 1 page image(s), [(2000, 2828)]            |
| yaml | json | pass   | 95 bytes, content ok                       |
| yaml | md   | pass   | 60 bytes, content ok                       |
| yaml | ods  | pass   | re-read by LibreOffice, text ok            |
| yaml | odt  | pass   | re-read by LibreOffice, text ok            |
| yaml | pdf  | pass   | 1 pages, text ok                           |
| yaml | png  | pass   | 1 page image(s), [(2000, 2828)]            |
| yaml | rtf  | pass   | re-read by LibreOffice, text ok            |
| yaml | tsv  | pass   | 28 bytes, content ok                       |
| yaml | txt  | pass   | 32 bytes, content ok                       |
| yaml | xls  | pass   | re-read by LibreOffice, text ok            |
| yaml | xlsx | pass   | re-read by LibreOffice, text ok            |
| odp  | avif | pass   | 2 page image(s), [(2000, 1125)]            |
| odp  | bmp  | pass   | 2 page image(s), [(2000, 1125)]            |
| odp  | exr  | pass   | 2 page image(s), [(2000, 1125)]            |
| odp  | gif  | pass   | 2 page image(s), [(2000, 1125)]            |
| odp  | heic | pass   | 2 page image(s), [(2000, 1125)]            |
| odp  | ico  | pass   | 2 page image(s), [(256, 144)]              |
| odp  | jpeg | pass   | 2 page image(s), [(2000, 1125)]            |
| odp  | pdf  | pass   | 2 pages, text ok                           |
| odp  | png  | pass   | 2 page image(s), [(2000, 1125)]            |
| odp  | ppm  | pass   | 2 page image(s), [(2000, 1125)]            |
| odp  | ppt  | pass   | re-read by LibreOffice, text ok            |
| odp  | pptx | pass   | re-read by LibreOffice, text ok            |
| odp  | qoi  | pass   | 2 page image(s), [(2000, 1125)]            |
| odp  | tga  | pass   | 2 page image(s), [(2000, 1125)]            |
| odp  | tiff | pass   | 2 page image(s), [(2000, 1125)]            |
| odp  | webp | pass   | 2 page image(s), [(2000, 1125)]            |
| ppt  | avif | pass   | 2 page image(s), [(2000, 1125)]            |
| ppt  | bmp  | pass   | 2 page image(s), [(2000, 1125)]            |
| ppt  | exr  | pass   | 2 page image(s), [(2000, 1125)]            |
| ppt  | gif  | pass   | 2 page image(s), [(2000, 1125)]            |
| ppt  | heic | pass   | 2 page image(s), [(2000, 1125)]            |
| ppt  | ico  | pass   | 2 page image(s), [(256, 144)]              |
| ppt  | jpeg | pass   | 2 page image(s), [(2000, 1125)]            |
| ppt  | odp  | pass   | re-read by LibreOffice, text ok            |
| ppt  | pdf  | pass   | 2 pages, text ok                           |
| ppt  | png  | pass   | 2 page image(s), [(2000, 1125)]            |
| ppt  | ppm  | pass   | 2 page image(s), [(2000, 1125)]            |
| ppt  | pptx | pass   | re-read by LibreOffice, text ok            |
| ppt  | qoi  | pass   | 2 page image(s), [(2000, 1125)]            |
| ppt  | tga  | pass   | 2 page image(s), [(2000, 1125)]            |
| ppt  | tiff | pass   | 2 page image(s), [(2000, 1125)]            |
| ppt  | webp | pass   | 2 page image(s), [(2000, 1125)]            |
| pptx | avif | pass   | 2 page image(s), [(2000, 1125)]            |
| pptx | bmp  | pass   | 2 page image(s), [(2000, 1125)]            |
| pptx | exr  | pass   | 2 page image(s), [(2000, 1125)]            |
| pptx | gif  | pass   | 2 page image(s), [(2000, 1125)]            |
| pptx | heic | pass   | 2 page image(s), [(2000, 1125)]            |
| pptx | ico  | pass   | 2 page image(s), [(256, 144)]              |
| pptx | jpeg | pass   | 2 page image(s), [(2000, 1125)]            |
| pptx | odp  | pass   | re-read by LibreOffice, text ok            |
| pptx | pdf  | pass   | 2 pages, text ok                           |
| pptx | png  | pass   | 2 page image(s), [(2000, 1125)]            |
| pptx | ppm  | pass   | 2 page image(s), [(2000, 1125)]            |
| pptx | ppt  | pass   | re-read by LibreOffice, text ok            |
| pptx | qoi  | pass   | 2 page image(s), [(2000, 1125)]            |
| pptx | tga  | pass   | 2 page image(s), [(2000, 1125)]            |
| pptx | tiff | pass   | 2 page image(s), [(2000, 1125)]            |
| pptx | webp | pass   | 2 page image(s), [(2000, 1125)]            |
| csv  | avif | pass   | 1 page image(s), [(2000, 2828)]            |
| csv  | bmp  | pass   | 1 page image(s), [(2000, 2828)]            |
| csv  | doc  | pass   | re-read by LibreOffice, text ok            |
| csv  | docx | pass   | re-read by LibreOffice, text ok            |
| csv  | exr  | pass   | 1 page image(s), [(2000, 2828)]            |
| csv  | gif  | pass   | 1 page image(s), [(2000, 2828)]            |
| csv  | heic | pass   | 1 page image(s), [(2000, 2828)]            |
| csv  | html | pass   | 1120 bytes, content ok                     |
| csv  | ico  | pass   | 1 page image(s), [(181, 256)]              |
| csv  | jpeg | pass   | 1 page image(s), [(2000, 2828)]            |
| csv  | json | pass   | 99 bytes, content ok                       |
| csv  | md   | pass   | 60 bytes, content ok                       |
| csv  | ods  | pass   | re-read by LibreOffice, text ok            |
| csv  | odt  | pass   | re-read by LibreOffice, text ok            |
| csv  | pdf  | pass   | 1 pages, text ok                           |
| csv  | png  | pass   | 1 page image(s), [(2000, 2828)]            |
| csv  | ppm  | pass   | 1 page image(s), [(2000, 2828)]            |
| csv  | qoi  | pass   | 1 page image(s), [(2000, 2828)]            |
| csv  | rtf  | pass   | re-read by LibreOffice, text ok            |
| csv  | tga  | pass   | 1 page image(s), [(2000, 2828)]            |
| csv  | tiff | pass   | 1 page image(s), [(2000, 2828)]            |
| csv  | tsv  | pass   | 28 bytes, content ok                       |
| csv  | txt  | pass   | 32 bytes, content ok                       |
| csv  | webp | pass   | 1 page image(s), [(2000, 2828)]            |
| csv  | xls  | pass   | re-read by LibreOffice, text ok            |
| csv  | xlsx | pass   | re-read by LibreOffice, text ok            |
| csv  | yaml | pass   | 56 bytes, content ok                       |
| ods  | avif | pass   | 1 page image(s), [(2000, 2828)]            |
| ods  | bmp  | pass   | 1 page image(s), [(2000, 2828)]            |
| ods  | csv  | pass   | 28 bytes, content ok                       |
| ods  | exr  | pass   | 1 page image(s), [(2000, 2828)]            |
| ods  | gif  | pass   | 1 page image(s), [(2000, 2828)]            |
| ods  | heic | pass   | 1 page image(s), [(2000, 2828)]            |
| ods  | ico  | pass   | 1 page image(s), [(181, 256)]              |
| ods  | jpeg | pass   | 1 page image(s), [(2000, 2828)]            |
| ods  | pdf  | pass   | 1 pages, text ok                           |
| ods  | png  | pass   | 1 page image(s), [(2000, 2828)]            |
| ods  | ppm  | pass   | 1 page image(s), [(2000, 2828)]            |
| ods  | qoi  | pass   | 1 page image(s), [(2000, 2828)]            |
| ods  | tga  | pass   | 1 page image(s), [(2000, 2828)]            |
| ods  | tiff | pass   | 1 page image(s), [(2000, 2828)]            |
| ods  | webp | pass   | 1 page image(s), [(2000, 2828)]            |
| ods  | xls  | pass   | re-read by LibreOffice, text ok            |
| ods  | xlsx | pass   | re-read by LibreOffice, text ok            |
| tsv  | csv  | pass   | 28 bytes, content ok                       |
| tsv  | doc  | pass   | re-read by LibreOffice, text ok            |
| tsv  | docx | pass   | re-read by LibreOffice, text ok            |
| tsv  | html | pass   | 1120 bytes, content ok                     |
| tsv  | jpeg | pass   | 1 page image(s), [(2000, 2828)]            |
| tsv  | json | pass   | 99 bytes, content ok                       |
| tsv  | md   | pass   | 60 bytes, content ok                       |
| tsv  | ods  | pass   | re-read by LibreOffice, text ok            |
| tsv  | odt  | pass   | re-read by LibreOffice, text ok            |
| tsv  | pdf  | pass   | 1 pages, text ok                           |
| tsv  | png  | pass   | 1 page image(s), [(2000, 2828)]            |
| tsv  | rtf  | pass   | re-read by LibreOffice, text ok            |
| tsv  | txt  | pass   | 32 bytes, content ok                       |
| tsv  | xls  | pass   | re-read by LibreOffice, text ok            |
| tsv  | xlsx | pass   | re-read by LibreOffice, text ok            |
| tsv  | yaml | pass   | 56 bytes, content ok                       |
| xls  | avif | pass   | 1 page image(s), [(2000, 2828)]            |
| xls  | bmp  | pass   | 1 page image(s), [(2000, 2828)]            |
| xls  | csv  | pass   | 28 bytes, content ok                       |
| xls  | exr  | pass   | 1 page image(s), [(2000, 2828)]            |
| xls  | gif  | pass   | 1 page image(s), [(2000, 2828)]            |
| xls  | heic | pass   | 1 page image(s), [(2000, 2828)]            |
| xls  | ico  | pass   | 1 page image(s), [(181, 256)]              |
| xls  | jpeg | pass   | 1 page image(s), [(2000, 2828)]            |
| xls  | ods  | pass   | re-read by LibreOffice, text ok            |
| xls  | pdf  | pass   | 1 pages, text ok                           |
| xls  | png  | pass   | 1 page image(s), [(2000, 2828)]            |
| xls  | ppm  | pass   | 1 page image(s), [(2000, 2828)]            |
| xls  | qoi  | pass   | 1 page image(s), [(2000, 2828)]            |
| xls  | tga  | pass   | 1 page image(s), [(2000, 2828)]            |
| xls  | tiff | pass   | 1 page image(s), [(2000, 2828)]            |
| xls  | webp | pass   | 1 page image(s), [(2000, 2828)]            |
| xls  | xlsx | pass   | re-read by LibreOffice, text ok            |
| xlsx | avif | pass   | 1 page image(s), [(2000, 2828)]            |
| xlsx | bmp  | pass   | 1 page image(s), [(2000, 2828)]            |
| xlsx | csv  | pass   | 28 bytes, content ok                       |
| xlsx | exr  | pass   | 1 page image(s), [(2000, 2828)]            |
| xlsx | gif  | pass   | 1 page image(s), [(2000, 2828)]            |
| xlsx | heic | pass   | 1 page image(s), [(2000, 2828)]            |
| xlsx | ico  | pass   | 1 page image(s), [(181, 256)]              |
| xlsx | jpeg | pass   | 1 page image(s), [(2000, 2828)]            |
| xlsx | ods  | pass   | re-read by LibreOffice, text ok            |
| xlsx | pdf  | pass   | 1 pages, text ok                           |
| xlsx | png  | pass   | 1 page image(s), [(2000, 2828)]            |
| xlsx | ppm  | pass   | 1 page image(s), [(2000, 2828)]            |
| xlsx | qoi  | pass   | 1 page image(s), [(2000, 2828)]            |
| xlsx | tga  | pass   | 1 page image(s), [(2000, 2828)]            |
| xlsx | tiff | pass   | 1 page image(s), [(2000, 2828)]            |
| xlsx | webp | pass   | 1 page image(s), [(2000, 2828)]            |
| xlsx | xls  | pass   | re-read by LibreOffice, text ok            |

</details>
