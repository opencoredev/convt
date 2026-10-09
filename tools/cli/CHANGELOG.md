# @convt/cli

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

- [#41](https://github.com/opencoredev/convt/pull/41) [`29831b6`](https://github.com/opencoredev/convt/commit/29831b635d243d9a49fc7f5b4d464ef426e81fd6) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Linux downloads: an AppImage, `.deb`, `.rpm` and a tarball for x86_64, built from pinned sources with the release's corresponding-source archive.

- [#93](https://github.com/opencoredev/convt/pull/93) [`57f13c0`](https://github.com/opencoredev/convt/commit/57f13c0ef1a3b08bb67d958551fde3114dad91de) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Photos keep the rotation and mirroring you see in Preview when converted to WebP, PNG, JPEG and the other image formats. HEIC irot/imir and JPEG EXIF orientation are baked into the pixels so a later hop cannot flip the picture again.

- [#96](https://github.com/opencoredev/convt/pull/96) [`0833b0a`](https://github.com/opencoredev/convt/commit/0833b0a65d7fe4200d191614081baa8945dc0927) Thanks [@leoisadev1](https://github.com/leoisadev1)! - The Windows desktop app no longer opens a black console window when launched from the Start menu, and conversion tools no longer flash one. Linux .desktop entries set Terminal=false so a launching terminal does not need to stay open.

- [#92](https://github.com/opencoredev/convt/pull/92) [`9214ba0`](https://github.com/opencoredev/convt/commit/9214ba0e898ebc3fa8acb1733861a70c03914823) Thanks [@leoisadev1](https://github.com/leoisadev1)! - The Windows installer no longer embeds the LibreOffice document pack or codec source trees, so the MSI is much closer to the Mac and Linux download size. Document support downloads once from the GitHub release, same as the existing Install flow on Mac and Linux.

## 0.2.0

### Minor Changes

- [#8](https://github.com/opencoredev/convt/pull/8) [`2d21073`](https://github.com/opencoredev/convt/commit/2d21073be78c737210a014b12f84a397587453eb) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Prepare the desktop launch with local file conversion, a shared desktop, CLI and website version, and GitHub release artifacts. macOS builds use Developer ID signing and notarization; Windows and Linux builds are unsigned.
