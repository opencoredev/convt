# @convt/desktop

## 0.5.0

### Minor Changes

- [#116](https://github.com/opencoredev/convt/pull/116) [`b1b1e31`](https://github.com/opencoredev/convt/commit/b1b1e31c8a0b29b1f4fb20b8fabaa814876331b7) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Turning on the Finder menu now opens a small guide beside System Settings on macOS 15 and later. It shows where the switch is (scroll down to Extensions, click ⓘ next to convt, turn it on), follows the System Settings window, and closes once convt is on.

- [#120](https://github.com/opencoredev/convt/pull/120) [`e218e71`](https://github.com/opencoredev/convt/commit/e218e7115fc0e6d9a11c4ae1bff7630341733f14) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Choose which formats the "Convert with convt" right-click menu offers for each kind of file: images, SVG, video, audio, PDFs, documents, spreadsheets and presentations. Pick them in the new Settings → Menu tab, in the order you want them, or with `convt menu set audio flac mp3`. Finder, Explorer, Nautilus and `convt targets --menu` follow the choice; for Dolphin, Nemo and Thunar, run `integrations/linux/install.py` again.

### Patch Changes

- [#117](https://github.com/opencoredev/convt/pull/117) [`3ff6a7f`](https://github.com/opencoredev/convt/commit/3ff6a7f5e5f3eb820dfb1d566261fa1e00f2ee54) Thanks [@leoisadev1](https://github.com/leoisadev1)! - FLAC files converted from MP3 (or from a video with MP3 audio) now open in QuickTime and Music instead of failing with "An unknown error occurred (1718449215)". BMP files with transparency now open in Preview and Finder, and keep their transparency. Converting a video that has no sound to an audio format now says the file has no audio track instead of showing an FFmpeg error.

- [#119](https://github.com/opencoredev/convt/pull/119) [`61f1ca3`](https://github.com/opencoredev/convt/commit/61f1ca30501e231ec944564b2eceb5ccd7fe53e7) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Settings moved from the sidebar list to a gear beside the convt logo. It opens its own window, so it no longer sits with Activity and Automations, which switch the page in the main window.

## 0.4.0

### Minor Changes

- [#110](https://github.com/opencoredev/convt/pull/110) [`4a3df83`](https://github.com/opencoredev/convt/commit/4a3df835a9e6ab09cdfc7574cec7d5a72347d491) Thanks [@leoisadev1](https://github.com/leoisadev1)! - The desktop app now installs updates itself. While update checks are on, a new version your license covers downloads in the background, is checked against the signed list of releases, and waits for you to click Restart to update. This works for the Mac app, the Windows installer and the Linux AppImage; the deb, rpm and tarball still link to the download page.

- [#110](https://github.com/opencoredev/convt/pull/110) [`4a3df83`](https://github.com/opencoredev/convt/commit/4a3df835a9e6ab09cdfc7574cec7d5a72347d491) Thanks [@leoisadev1](https://github.com/leoisadev1)! - Closing the convt window no longer quits the app. It keeps running with an icon in the menu bar (the system tray on Windows and Linux) so right-click conversions start at once, and leaves the Dock on macOS. Quit from that icon, or with ⌘Q on a Mac and Ctrl+Q elsewhere. "Keep running in the background" in Settings turns this off on every platform. The menu bar icon is a template glyph that follows the menu bar's color. Its menu shows what convt is doing and offers Convert Files…, your last five converted files (click one to show it in Finder or your file manager), Open convt, Settings…, and Check for Updates… or Restart to Update. The Finder menu shows the colored convt logo.

- [#111](https://github.com/opencoredev/convt/pull/111) [`fa09406`](https://github.com/opencoredev/convt/commit/fa09406faafb6a36e3a327738c25eb4fefb10b40) Thanks [@leoisadev1](https://github.com/leoisadev1)! - The CLI prints a SKILL.md for AI coding agents. Run `convt --skill` or `convt skill`.

- [#68](https://github.com/opencoredev/convt/pull/68) [`18c6a05`](https://github.com/opencoredev/convt/commit/18c6a05b4c5651e854c7a4de387f55babd09202d) Thanks [@leoisadev1](https://github.com/leoisadev1)! - A redesigned desktop app, set in Inter with Hugeicons icons: Activity, Quick convert, Settings and first run share one look in light and dark, with clearer empty, error and trial states.

  First run now starts with your convt.app account. Sign in through your browser with Google or an email code, or enter a license key instead. A new account can then start the 7-day Pro trial through checkout, which charges nothing until the trial ends. If you skip it, Start free trial stays in the sidebar and in Settings > License.

### Patch Changes

- [#68](https://github.com/opencoredev/convt/pull/68) [`18c6a05`](https://github.com/opencoredev/convt/commit/18c6a05b4c5651e854c7a4de387f55babd09202d) Thanks [@leoisadev1](https://github.com/leoisadev1)! - On macOS, the menu bar has convt, File, Edit, Window and Help menus, with About convt, Check for Updates…, Settings… (⌘,) and the usual shortcuts. Quit and Close Window work from the keyboard on every platform: ⌘Q and ⌘W on macOS, Ctrl+Q and Ctrl+W on Linux and Windows. convt checks for updates at every launch and every 5 hours while automatic checks are on, and the Updates card in Settings shows this version, when it last checked, a Check now button and any newer build with its release notes and Download.

- [#68](https://github.com/opencoredev/convt/pull/68) [`18c6a05`](https://github.com/opencoredev/convt/commit/18c6a05b4c5651e854c7a4de387f55babd09202d) Thanks [@leoisadev1](https://github.com/leoisadev1)! - When convt has no tray icon, because "Keep running in the background" is off or the Linux desktop has no system tray, it quits once its last window is closed and its conversions finish, including conversions started from the file manager.

- [#68](https://github.com/opencoredev/convt/pull/68) [`18c6a05`](https://github.com/opencoredev/convt/commit/18c6a05b4c5651e854c7a4de387f55babd09202d) Thanks [@leoisadev1](https://github.com/leoisadev1)! - A conversion started from the file manager's right-click menu now shows a notification when it finishes, like any other conversion, while "Show a notification" is on in Settings.

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
