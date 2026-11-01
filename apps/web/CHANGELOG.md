# @convt/web

## 0.6.0

### Minor Changes

- [#140](https://github.com/opencoredev/convt/pull/140) [`8003d19`](https://github.com/opencoredev/convt/commit/8003d196f9dfde6e666bd36e9c252b4940d2a811) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Cloud conversions now use your conversion options. In the desktop app, Quick convert shows the same quality, size, codec and background controls in the cloud as on your computer. The cloud converter on convt.app lets you pick a quality and a size, and the API takes an optional `options` object when you create a job.

- [#141](https://github.com/opencoredev/convt/pull/141) [`64037a1`](https://github.com/opencoredev/convt/commit/64037a1c68aa5776fa84df5d43ecf40484def34b) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Compress a file without changing its format (`convt clip.mp4 --to mp4 --quality 60`, or the file's own format in Quick convert), and fit output under a size with `--target-size 10MB` or the new Max file size control. WebP is now lossy by default and follows `--quality` (100 stays lossless), so photos no longer come out larger than their JPEG. New `--strip-metadata`, `--trim-start`, `--trim-end` and `--fps` options, and a Strip metadata checkbox in Quick convert. Photos with an EXIF rotation now keep it.

- [#150](https://github.com/opencoredev/convt/pull/150) [`6cf388c`](https://github.com/opencoredev/convt/commit/6cf388c580763da1f76827da3b3dfc66355ea22f) Thanks [@leoisadev1](https://github.com/leoisadev1)! - convt is now sold once, with no subscription. Desktop is $39 once, and Desktop + Cloud is $69 once: a Desktop license plus a year of cloud conversions from your phone, your browser or the app, which doesn't renew. Buying it again adds another year. Licenses bought at $29 keep working, and existing Pro subscriptions keep renewing, switching and canceling as before. In the desktop app, signing in installs your Desktop license, so there is no key to paste, and Cloud works while the account has cloud conversions.

- [#140](https://github.com/opencoredev/convt/pull/140) [`8003d19`](https://github.com/opencoredev/convt/commit/8003d196f9dfde6e666bd36e9c252b4940d2a811) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Nine new formats. Markdown converts to HTML and plain text, and HTML, Word and other documents convert to Markdown. CSV, TSV, JSON and YAML convert between each other, and CSV and TSV become HTML, Markdown or text tables, all without the document pack. SRT and WebVTT subtitles convert to each other and to plain text, and MP4, MOV, MKV and WebM videos give up their subtitle track as SRT or VTT. AIFF, CAF and M4R (iPhone ringtone) audio join the audio formats.

### Patch Changes

- [#148](https://github.com/opencoredev/convt/pull/148) [`2d4bbd6`](https://github.com/opencoredev/convt/commit/2d4bbd660928f31d411f74ef55e7235f6b1ff12e) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Cloud conversion works on convt.app. Pro subscribers can convert files from the cloud converter on the dashboard and from the desktop app, which used to say cloud conversion was still being connected.

- [#134](https://github.com/opencoredev/convt/pull/134) [`7e7bba3`](https://github.com/opencoredev/convt/commit/7e7bba38f9c9e39c21464d09d080266dda9cccab) Thanks [@leoisadev1](https://github.com/leoisadev1)! - The email code boxes on convt.app submit as soon as the last digit is typed or pasted, show when the code is being checked, and shake and clear on a wrong code.

## 0.5.0

### Patch Changes

- [#121](https://github.com/opencoredev/convt/pull/121) [`cf10d01`](https://github.com/opencoredev/convt/commit/cf10d01426f1bde22f53041dd7f06c4d6eefdcb8) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Dashboard tabs switch faster: a tab click no longer checks the session again before loading its page. The dashboard header is now one bar with the logo, the tabs and your avatar, and the underline slides to the tab you click.

## 0.4.0

### Minor Changes

- [#110](https://github.com/opencoredev/convt/pull/110) [`4a3df83`](https://github.com/opencoredev/convt/commit/4a3df835a9e6ab09cdfc7574cec7d5a72347d491) Thanks [@leoisadev1](https://github.com/leoisadev1)! - The desktop app now installs updates itself. While update checks are on, a new version your license covers downloads in the background, is checked against the signed list of releases, and waits for you to click Restart to update. This works for the Mac app, the Windows installer and the Linux AppImage; the deb, rpm and tarball still link to the download page.

- [#111](https://github.com/opencoredev/convt/pull/111) [`fa09406`](https://github.com/opencoredev/convt/commit/fa09406faafb6a36e3a327738c25eb4fefb10b40) Thanks [@leoisadev1](https://github.com/leoisadev1)! - The CLI prints a SKILL.md for AI coding agents. Run `convt --skill` or `convt skill`.

### Patch Changes

- [#104](https://github.com/opencoredev/convt/pull/104) [`49d15c8`](https://github.com/opencoredev/convt/commit/49d15c832daf0e545608db2803c66c2c1053ca8f) Thanks [@leoisadev1](https://github.com/leoisadev1)! - On Linux desktops that leave window decorations to the app, such as GNOME on Wayland, convt's windows now have a title bar: drag it to move the window, double-click it to maximize, right-click it for the window menu, and use its minimize, maximize and close buttons. Desktops that draw their own title bars keep them.

## 0.3.0

### Minor Changes

- [#90](https://github.com/opencoredev/convt/pull/90) [`807a94f`](https://github.com/opencoredev/convt/commit/807a94fa1043b98224e6db3c1876be6bf5edd927) Thanks [@leoisadev1](https://github.com/leoisadev1)! - New installs convert photos to JPEG and other images (including screenshots) to PNG. Automations now watch the real screenshot and screen-recording folders — one directory each, never recursively — with separate presets and an option to copy the result.

### Patch Changes

- [#88](https://github.com/opencoredev/convt/pull/88) [`379397c`](https://github.com/opencoredev/convt/commit/379397c30e57cd2c65e342ced5f0800f96a015fa) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Record Polar $0 / 100%-off and pre-signup Desktop orders, attach them on signup or sign-in by email, show them on the Billing tab, and add a dry-run Polar order backfill.

- [#41](https://github.com/opencoredev/convt/pull/41) [`29831b6`](https://github.com/opencoredev/convt/commit/29831b635d243d9a49fc7f5b4d464ef426e81fd6) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Linux downloads: an AppImage, `.deb`, `.rpm` and a tarball for x86_64, built from pinned sources with the release's corresponding-source archive.

- [#93](https://github.com/opencoredev/convt/pull/93) [`57f13c0`](https://github.com/opencoredev/convt/commit/57f13c0ef1a3b08bb67d958551fde3114dad91de) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Photos keep the rotation and mirroring you see in Preview when converted to WebP, PNG, JPEG and the other image formats. HEIC irot/imir and JPEG EXIF orientation are baked into the pixels so a later hop cannot flip the picture again.

- [#96](https://github.com/opencoredev/convt/pull/96) [`0833b0a`](https://github.com/opencoredev/convt/commit/0833b0a65d7fe4200d191614081baa8945dc0927) Thanks [@leoisadev1](https://github.com/leoisadev1)! - The Windows desktop app no longer opens a black console window when launched from the Start menu, and conversion tools no longer flash one. Linux .desktop entries set Terminal=false so a launching terminal does not need to stay open.

- [#92](https://github.com/opencoredev/convt/pull/92) [`9214ba0`](https://github.com/opencoredev/convt/commit/9214ba0e898ebc3fa8acb1733861a70c03914823) Thanks [@leoisadev1](https://github.com/leoisadev1)! - The Windows installer no longer embeds the LibreOffice document pack or codec source trees, so the MSI is much closer to the Mac and Linux download size. Document support downloads once from the GitHub release, same as the existing Install flow on Mac and Linux.

- Updated dependencies [[`379397c`](https://github.com/opencoredev/convt/commit/379397c30e57cd2c65e342ced5f0800f96a015fa)]:
  - @convt/billing@0.1.1

## 0.2.0

### Minor Changes

- [#8](https://github.com/opencoredev/convt/pull/8) [`2d21073`](https://github.com/opencoredev/convt/commit/2d21073be78c737210a014b12f84a397587453eb) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Prepare the desktop launch with local file conversion, a shared desktop, CLI and website version, and GitHub release artifacts. macOS builds use Developer ID signing and notarization; Windows and Linux builds are unsigned.
