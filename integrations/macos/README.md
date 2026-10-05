# macOS integration

A Finder Sync extension that adds **Convert with convt** to Finder's right-click menu.

None of this builds or runs on Linux, and it has not been compiled or tried in Finder yet. It needs Xcode, a signed host app and a Mac.

## The menu

- `FinderSync/FinderSync.swift` builds the menu. It calls `targetsFor(path:)` from the Rust core (`crates/convt-ffi`) to list the formats a selection can become, grouped under a header per category ("Video", "Audio only" for a video's audio, and so on), with **More options…** at the bottom.
- Picking a format converts in place with no window. The extension launches the app with `NSWorkspace.openApplication(at:configuration:)` and the arguments `open --to <format> -- <files>`, with `createsNewApplicationInstance` set so the arguments arrive even when convt is running: the new process hands them to the running app over its single-instance socket and exits. A web page can't start a process with arguments, so this path can't be triggered from a browser. `convt://` links still never convert without a click.
- **More options…** launches `open -- <files>`, which opens Quick convert.
- The extension is sandboxed and ships inside `convt.app/Contents/PlugIns/`. The GPUI app registers the `convt://` scheme for license and sign-in links.

## Building the bindings

```sh
cargo build -p convt-ffi
cargo run -p convt-ffi --bin uniffi-bindgen -- generate \
  --library target/debug/libconvt_ffi.dylib \
  --language swift --out-dir integrations/macos/FinderSync/Generated
cargo build -p convt-ffi --release --target aarch64-apple-darwin
```

Bindings come from the debug library because release builds strip the metadata uniffi reads; the API is identical. Link the release `libconvt_ffi.a` into the extension target and add the generated Swift file and module map.

## Finder progress while converting (not built)

The design shows the new file appearing next to the original as soon as the conversion starts, with Finder's progress bar under its icon. Finder draws that bar for any file whose progress a process publishes with `NSProgress`. The plan, all in `crates/convt-app` behind `cfg(target_os = "macos")` using the `objc2-foundation` bindings:

1. Know the output path before the job runs. Today `convt-core` picks the final name when it publishes the result (`publish.rs` adds `-1`, `-2` on a collision), so the app can't know it in advance. Add a function to `convt-core` that resolves and reserves the output path up front, by creating an empty placeholder file there. That is a core change, which is why it isn't done yet.
2. When the job starts, create `NSProgress(totalUnitCount: 100)`, set `kind = .file`, `fileOperationKind = .copying` and `fileURL` to the placeholder's URL, then call `publish()`.
3. On every progress update from the runner, set `completedUnitCount`. Jobs without progress (`Status::Running(None)`) leave it indeterminate (`totalUnitCount = -1`).
4. When the job finishes, call `unpublish()`. The engine writes the real file over the placeholder in one rename. On failure or cancel, unpublish and delete the placeholder.

The extension needs no change for this: Finder picks up published progress from any process.

The menu bar icon's passive spinner (no percentage, no highlight) needs an `NSStatusItem`, which GPUI doesn't offer yet; `crates/convt-app/src/tray.rs` decides what the icon shows and is the place to wire it.

## Still to do

- Xcode project (or `xcodegen` spec) that wraps the GPUI binary as the host app.
- Signing with a Developer ID. Finder Sync extensions only load when the host app is signed and notarized.
- Users enable the extension once in System Settings → General → Login Items & Extensions. The first-run window and Settings link there (`x-apple.systempreferences:com.apple.LoginItems-Settings.extension`), but the app can't yet tell whether the extension is on, so the first-run step doesn't move on by itself. Detecting it needs `FIFinderSyncController.isExtensionEnabled` from Swift, passed to the app.
