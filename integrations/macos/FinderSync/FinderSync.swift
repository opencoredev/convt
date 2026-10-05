import Cocoa
import FinderSync

/// Adds "Convert with convt" to Finder's context menu. The extension only
/// builds the menu. It asks the Rust core which targets fit the selection,
/// then hands the job to the main app, because sandboxed extensions shouldn't
/// run long conversions themselves.
///
/// Picking a format converts in place with no window: the extension launches
/// the app with `open --to <format> -- <files>`. Only a local process can pass
/// those arguments. A `convt://` link would not do, because any web page can
/// open one, so the app never converts from a link without a click.
/// "More options…" launches `open -- <files>`, which opens Quick convert.
final class FinderSync: FIFinderSync {
    override init() {
        super.init()
        // Watch the whole disk so the menu shows up everywhere.
        FIFinderSyncController.default().directoryURLs = [URL(fileURLWithPath: "/")]
    }

    override func menu(for menuKind: FIMenuKind) -> NSMenu? {
        guard menuKind == .contextualMenuForItems,
              let items = FIFinderSyncController.default().selectedItemURLs(),
              !items.isEmpty
        else { return nil }

        // Offer only targets every selected file supports.
        let perFile = items.map { Set(targetsFor(path: $0.path).map(\.id)) }
        let common = perFile.dropFirst().reduce(perFile[0]) { $0.intersection($1) }
        let targets = targetsFor(path: items[0].path).filter { common.contains($0.id) }
        guard !targets.isEmpty else { return nil }
        // Still images can't become video, so a video target means the
        // selection is video, and its audio targets drop the picture.
        let isVideo = targets.contains { $0.category == "Video" }

        let submenu = NSMenu(title: "Convert with convt")
        var lastCategory: String?
        for target in targets {
            if target.category != lastCategory {
                if lastCategory != nil {
                    submenu.addItem(.separator())
                }
                submenu.addItem(header(for: target.category, isVideo: isVideo))
            }
            lastCategory = target.category
            let item = NSMenuItem(title: target.name, action: #selector(convert(_:)), keyEquivalent: "")
            item.representedObject = target.id
            item.target = self
            submenu.addItem(item)
        }
        submenu.addItem(.separator())
        let more = NSMenuItem(title: "More options…", action: #selector(moreOptions(_:)), keyEquivalent: "")
        more.target = self
        submenu.addItem(more)

        let menu = NSMenu(title: "")
        let root = NSMenuItem(title: "Convert with convt", action: nil, keyEquivalent: "")
        root.image = NSImage(named: "MenuIcon")
        root.submenu = submenu
        menu.addItem(root)
        return menu
    }

    /// A disabled item naming the group of formats below it.
    private func header(for category: String, isVideo: Bool) -> NSMenuItem {
        let title: String
        switch category {
        case "Audio": title = isVideo ? "Audio only" : "Audio"
        case "Pdf": title = "PDF"
        case "Vector": title = "Vector"
        default: title = category
        }
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.isEnabled = false
        return item
    }

    /// Converts the selection in place, with no window.
    @objc private func convert(_ sender: NSMenuItem) {
        guard let to = sender.representedObject as? String,
              let items = FIFinderSyncController.default().selectedItemURLs()
        else { return }
        launchApp(arguments: ["open", "--to", to, "--"] + items.map(\.path), activate: false)
    }

    /// Opens Quick convert for the selection.
    @objc private func moreOptions(_ sender: NSMenuItem) {
        guard let items = FIFinderSyncController.default().selectedItemURLs() else { return }
        launchApp(arguments: ["open", "--"] + items.map(\.path), activate: true)
    }

    /// Starts a new convt process with `arguments`. A running convt receives
    /// them over its single-instance socket and the new process exits, so the
    /// arguments arrive even when the app is already open.
    private func launchApp(arguments: [String], activate: Bool) {
        // The extension lives in convt.app/Contents/PlugIns/<name>.appex.
        let appURL = Bundle.main.bundleURL
            .deletingLastPathComponent() // PlugIns
            .deletingLastPathComponent() // Contents
            .deletingLastPathComponent() // convt.app
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.arguments = arguments
        configuration.createsNewApplicationInstance = true
        configuration.activates = activate
        configuration.addsToRecentItems = false
        NSWorkspace.shared.openApplication(at: appURL, configuration: configuration) { _, error in
            if let error {
                NSLog("convt: could not start the app: \(error.localizedDescription)")
            }
        }
    }
}
