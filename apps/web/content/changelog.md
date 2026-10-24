# Changelog

Every convt release, newest first. Each version has its own source archive on the [download page](/download).

## 0.5.0

Released October 14, 2026.

### Added

- Choose which formats the "Convert with convt" menu offers for each kind of file, in the order you want them. Pick them in the new Settings → Menu tab or with `convt menu set audio flac mp3`. Finder, Explorer, Nautilus and `convt targets --menu` follow your choice. For Dolphin, Nemo and Thunar, run `integrations/linux/install.py` again.
- On macOS 15 and later, turning on the Finder menu opens a small guide beside System Settings. It shows where the switch is, follows the System Settings window, and closes once convt is on.

### Changed

- Settings moved to a gear beside the convt logo and opens in its own window.
- Dashboard tabs on convt.app switch faster, and the dashboard header is now one bar with the logo, the tabs and your avatar.

### Fixed

- FLAC files converted from MP3 now open in QuickTime and Music.
- BMP files with transparency open in Preview and Finder and keep their transparency.
- Converting a silent video to an audio format says the file has no audio track instead of showing an FFmpeg error.
- Signing in from the Windows app now finishes, and license links from convt.app open the app on Windows.

## 0.4.0

Released October 9, 2026.

### Added

- The desktop app installs its own updates. A new version your license covers downloads in the background, is checked against the signed list of releases, and waits for you to click Restart to update. This works for the Mac app, the Windows installer and the Linux AppImage.
- convt keeps running in the menu bar (the system tray on Windows and Linux) after you close its window, so right-click conversions start at once. The menu shows what convt is doing, your last five converted files, and Convert Files…. Turn this off with "Keep running in the background" in Settings.
- `convt --skill` prints a SKILL.md for AI coding agents.
- First run starts with your convt.app account: sign in with Google or an email code, or enter a license key. A new account can start the 7-day Pro trial.

### Changed

- A redesigned desktop app, set in Inter, with one look across Activity, Quick convert, Settings and first run in light and dark.
- The macOS menu bar has the usual convt, File, Edit, Window and Help menus. ⌘Q and ⌘W (Ctrl+Q and Ctrl+W elsewhere) work on every platform.
- convt checks for updates at every launch and every 5 hours, and Settings shows the current version, the last check and any newer build's release notes.

### Fixed

- On Linux desktops that leave window decorations to the app, such as GNOME on Wayland, convt's windows have a title bar you can drag, double-click and right-click.
- A conversion started from the file manager shows a notification when it finishes.
- Without a tray icon, convt quits once its last window closes and its conversions finish.

## 0.3.0

Released October 8, 2026.

### Added

- Linux downloads: an AppImage, `.deb`, `.rpm` and a tarball for x86_64, built from pinned sources.
- Automations watch the real screenshot and screen-recording folders, with separate presets and an option to copy the result.

### Changed

- New installs convert photos to JPEG and other images, including screenshots, to PNG.
- The Windows installer is much smaller. Document support downloads when you click Install, as on Mac and Linux.

### Fixed

- Photos keep the rotation and mirroring you see in Preview when converted to WebP, PNG, JPEG and the other image formats.
- The Windows app no longer opens a black console window, and conversion tools no longer flash one.

## 0.2.0

Released October 7, 2026. The first public release.

### Added

- Right-click conversion for images, video, audio and documents: 40 formats, plus PDF to one image per page.
- The convt desktop app for macOS, Windows and Linux, with batch folders and saved presets.
- The `convt` command line tool, with `--json` progress for scripts.
- Context menus in Finder on macOS and in Nautilus, Dolphin, Nemo and Thunar on Linux.
- Document support (Word, Excel, PowerPoint and their open formats) as an optional download, installed only when you click Install.
- A 7-day free trial, the Desktop license and convt Pro.
- Signed and notarized macOS builds.
