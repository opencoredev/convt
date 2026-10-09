# macOS integration

A Finder Sync extension that adds **Convert with convt** to Finder's right-click menu.

None of this builds or runs on Linux. It needs Xcode and a Mac.

## The menu

- `FinderSync/FinderSync.swift` builds the menu from the target list the app publishes. The extension is sandboxed and runs from its own executable, so it never probes tools: `crates/convt-app/src/macos.rs` writes `targets.json` (extension to format, format to targets, with categories) into the App Group container `<TEAMID>.app.convt.desktop` at launch and after every engine change. Both bundles name the group in the `ConvtAppGroup` Info.plist key. The menu shows the targets every selected file shares, grouped under a header per category ("Video", "Audio only" for a video's audio, and so on), with **More options…** at the bottom. With no readable list (the app has never run) it shows only **Open in convt…**.
- Picking a format converts in place with no window; **More options…** opens Quick convert. AppKit drops `NSWorkspace.OpenConfiguration.arguments` for sandboxed callers, so the extension can't pass `open --to <format> -- <files>`. Instead it writes the request (`{version, to, files, created}`) to `requests/<uuid>.json` in the App Group container and opens `convt://finder` with this bundle's app by path (another app may claim the `convt` scheme). The app takes every fresh request (under two minutes old, absolute paths, deleted before use) on that link, or at launch in place of the main window. The link only wakes the app: a web page can open it, but only processes in the App Group can write requests, so links still never convert without a click.
- When it can't write the request (no shared container, as in an ad-hoc build), the extension opens the files with convt instead, which shows Quick convert. Finder's "Open With" → convt arrives the same way.
- The Services menu has **Convert with convt** as a fallback when the extension is off (`NSServices` in `packaging/macos/Info.plist`, handled in `macos.rs`). It opens Quick convert.
- The extension is sandboxed and ships inside `convt.app/Contents/PlugIns/`. The GPUI app registers the `convt://` scheme for license and sign-in links.

## Building

`packaging/macos/bundle.sh` compiles the extension with `swiftc` (no Xcode project) and assembles the whole app; see `packaging/macos/README.md`. The extension doesn't link the Rust core, so it needs no uniffi bindings or XCFramework. `crates/convt-ffi` stays for other integrations.

The first-run window's Finder step, Activity and Settings all read `pluginkit -m -i app.convt.desktop.FinderSync`. The app polls every second on macOS so those surfaces update when the user comes back from System Settings. Once the extension is on, the first-run step reads "The Finder menu is on" and Continue moves on. Skipping or closing first run still leaves a recover card on Activity (and a status row in Settings) until the extension is on. The Finder step's System Settings picture is a preview, not a switch: the real control is Open System Settings, then scroll to Extensions.

## Finder progress while converting (not built)

The design shows the new file appearing next to the original as soon as the conversion starts, with Finder's progress bar under its icon. Finder draws that bar for any file whose progress a process publishes with `NSProgress`. The plan, all in `crates/convt-app` behind `cfg(target_os = "macos")` using the `objc2-foundation` bindings:

1. Know the output path before the job runs. Today `convt-core` picks the final name when it publishes the result (`publish.rs` adds `-1`, `-2` on a collision), so the app can't know it in advance. Add a function to `convt-core` that resolves and reserves the output path up front, by creating an empty placeholder file there. That is a core change, which is why it isn't done yet.
2. When the job starts, create `NSProgress(totalUnitCount: 100)`, set `kind = .file`, `fileOperationKind = .copying` and `fileURL` to the placeholder's URL, then call `publish()`.
3. On every progress update from the runner, set `completedUnitCount`. Jobs without progress (`Status::Running(None)`) leave it indeterminate (`totalUnitCount = -1`).
4. When the job finishes, call `unpublish()`. The engine writes the real file over the placeholder in one rename. On failure or cancel, unpublish and delete the placeholder.

The extension needs no change for this: Finder picks up published progress from any process.

## Menu bar item

While "Keep running in the background" is on (Settings, General; on by default), convt shows a menu bar item (`crates/convt-app/src/tray.rs`, through the `tray-icon` crate's `NSStatusItem`) showing the colored convt mark from `FinderSync/MenuIcon.svg`, pre-rendered to `crates/convt-app/assets/tray/menubar.png` (36 px, drawn at 18 pt). Its menu has Open convt, Settings… and Quit convt; while jobs run, its tooltip counts them. Closing the last window leaves convt running, so a Finder request starts without launching the app, and convt leaves the Dock (accessory activation policy) until a window opens again. ⌘Q and the convt menu's Quit convt quit at once. With the setting off, convt quits with its last window, or when the conversions still running finish.

## Still to do

- The App Group only works for a team-signed app: macOS refuses to create the container for an ad-hoc build (`could not publish Finder targets: Operation not permitted`), and the menu then shows only "Open in convt…". Sign with Apple Development or Developer ID.
- The extension's own hand-off is untested until the extension is enabled in Finder and the app is team-signed.
- Users enable the extension once in System Settings → General → Login Items & Extensions → Finder.
