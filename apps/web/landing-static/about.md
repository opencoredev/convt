# About convt

convt converts files on your own computer. You right-click a file, pick a format, and the converted file appears next to the original. Nothing is uploaded, so it works offline and on files you would not hand to a website.

## Why it exists

Most people convert files by searching for "heic to jpg", uploading a photo to a site they have never heard of, and downloading the result. That is slow, it leaks private files to strangers, and it rarely handles a folder at once. The tools that do the work well, like FFmpeg and LibreOffice, are free and excellent, but they live on the command line and each one only covers part of the job.

convt puts those tools behind one menu. It knows 40 formats across images, video, audio and documents, picks the right engine for each conversion, and chains up to three steps when no single tool can do the job directly.

## How it is built

The conversion engine is written in Rust. Video and audio go through FFmpeg, office documents through LibreOffice, PDF pages through PDFium, and photos and SVG through pure Rust libraries. The desktop app is built with GPUI, and the right-click menu plugs into the file manager: Finder on macOS, and Nautilus, Dolphin, Nemo and Thunar on Linux, with Explorer on Windows to follow. The same engine powers the `convt` command line tool.

convt is open source under the GNU Affero General Public License v3.0.

## Status

convt is not released yet. Downloads, accounts and checkout are coming soon. When it launches, the desktop app will cost $29 once with a 7-day free trial, and an optional Pro plan will add cloud conversions.
