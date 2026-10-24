---
"@convt/desktop": minor
"@convt/cli": minor
"@convt/web": minor
---

Compress a file without changing its format (`convt clip.mp4 --to mp4 --quality 60`, or the file's own format in Quick convert), and fit output under a size with `--target-size 10MB` or the new Max file size control. WebP is now lossy by default and follows `--quality` (100 stays lossless), so photos no longer come out larger than their JPEG. New `--strip-metadata`, `--trim-start`, `--trim-end` and `--fps` options, and a Strip metadata checkbox in Quick convert. Photos with an EXIF rotation now keep it.
