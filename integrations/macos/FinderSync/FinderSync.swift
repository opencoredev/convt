import Cocoa
import FinderSync

/// The formats each input can become, as the app publishes them to the App
/// Group container (`crates/convt-app/src/macos.rs`). The extension is
/// sandboxed and runs from its own executable, so it doesn't probe tools
/// itself: that could offer different targets than the app can convert.
private struct TargetList: Decodable {
    let version: Int
    /// Lowercase extension to format id.
    let extensions: [String: String]
    /// Format id to its targets, in menu order.
    let targets: [String: [Target]]
}

private struct Target: Decodable {
    let id: String
    let name: String
    let category: String
}

/// Adds "Convert with convt" to Finder's context menu. The extension only
/// builds the menu and hands the job to the main app, because sandboxed
/// extensions shouldn't run long conversions themselves.
///
/// Picking a format converts in place with no window; "More options…" opens
/// Quick convert. Either way the request goes through the App Group
/// container (see `send`), which only processes in the group can write, so a
/// `convt://` link from a web page can never start a conversion.
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

        let submenu = NSMenu(title: "Convert with convt")
        if let list = loadTargets() {
            // Offer only targets every selected file supports.
            let perFile = items.map { url -> [Target] in
                list.extensions[url.pathExtension.lowercased()].flatMap { list.targets[$0] } ?? []
            }
            let common = perFile.dropFirst().reduce(Set(perFile[0].map(\.id))) {
                $0.intersection($1.map(\.id))
            }
            let targets = perFile[0].filter { common.contains($0.id) }
            guard !targets.isEmpty else { return nil }
            // The app publishes only the few popular targets, so no section
            // headers. Still images can't become video, so a video target
            // means the selection is video, and an audio target drops the
            // picture.
            let isVideo = targets.contains { $0.category == "Video" }
            for target in targets {
                let title = isVideo && target.category == "Audio" ? "\(target.name) (audio only)" : target.name
                // Finder copies this menu into its own process and keeps only
                // each item's title and action, so `convert` finds the
                // target again by title.
                let item = NSMenuItem(title: title, action: #selector(convert(_:)), keyEquivalent: "")
                item.target = self
                submenu.addItem(item)
            }
            submenu.addItem(.separator())
            submenu.addItem(moreItem(title: "More options…"))
        } else {
            // The app hasn't published its list yet (it has never run, or
            // the list is unreadable): let the app work out the targets
            // rather than guess from the format table.
            submenu.addItem(moreItem(title: "Open in convt…"))
        }

        let menu = NSMenu(title: "")
        let root = NSMenuItem(title: "Convert with convt", action: nil, keyEquivalent: "")
        // The colored mark, not a template image: Finder copies this menu into
        // its own process, loses the template flag and draws a template black
        // on a dark menu.
        root.image = NSImage(named: "MenuIcon")
        root.submenu = submenu
        menu.addItem(root)
        return menu
    }

    /// The list the app wrote to the App Group container named by
    /// `ConvtAppGroup` in this extension's Info.plist.
    private func loadTargets() -> TargetList? {
        guard let group = Bundle.main.object(forInfoDictionaryKey: "ConvtAppGroup") as? String,
              let dir = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group),
              let data = try? Data(contentsOf: dir.appendingPathComponent("targets.json")),
              let list = try? JSONDecoder().decode(TargetList.self, from: data),
              list.version == 1
        else { return nil }
        return list
    }

    private func moreItem(title: String) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: #selector(moreOptions(_:)), keyEquivalent: "")
        item.target = self
        return item
    }

    /// Converts the selection in place, with no window.
    @objc private func convert(_ sender: NSMenuItem) {
        guard let items = FIFinderSyncController.default().selectedItemURLs(), !items.isEmpty else {
            return
        }
        let name = sender.title.replacingOccurrences(of: " (audio only)", with: "")
        let target = loadTargets().flatMap { list in
            list.targets.values.lazy.flatMap { $0 }.first { $0.name == name }
        }
        guard let target else {
            // The list changed since the menu was built: let the app decide.
            NSLog("convt: no target named \(name); opening Quick convert")
            send(to: nil, items: items, activate: true)
            return
        }
        send(to: target.id, items: items, activate: false)
    }

    /// Opens Quick convert for the selection.
    @objc private func moreOptions(_ sender: NSMenuItem) {
        guard let items = FIFinderSyncController.default().selectedItemURLs() else { return }
        send(to: nil, items: items, activate: true)
    }

    private struct Request: Encodable {
        var version = 1
        let to: String?
        let files: [String]
        /// Seconds since 1970; the app drops requests older than two minutes.
        let created: Double
    }

    /// Hands the selection to the app. AppKit drops launch arguments from
    /// sandboxed callers, so the request goes into the App Group container
    /// and `convt://finder` only wakes the app, which takes it from there. The
    /// link is opened with this bundle's app by path, because another app may
    /// also claim the `convt` scheme.
    private func send(to: String?, items: [URL], activate: Bool) {
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.activates = activate
        configuration.addsToRecentItems = false
        let done: (NSRunningApplication?, Error?) -> Void = { _, error in
            if let error {
                NSLog("convt: could not start the app: \(error.localizedDescription)")
            }
        }
        do {
            guard let group = Bundle.main.object(forInfoDictionaryKey: "ConvtAppGroup") as? String,
                  let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)
            else { throw CocoaError(.fileNoSuchFile) }
            let dir = container.appendingPathComponent("requests", isDirectory: true)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            let request = Request(to: to, files: items.map(\.path), created: Date().timeIntervalSince1970)
            let data = try JSONEncoder().encode(request)
            // The app drops request files over 1 MiB unread.
            guard data.count <= 1 << 20 else { throw CocoaError(.fileWriteOutOfSpace) }
            try data.write(to: dir.appendingPathComponent(UUID().uuidString + ".json"), options: .atomic)
        } catch {
            // No usable shared container (an ad-hoc build, which macOS
            // doesn't grant the group), or a selection too large for one
            // request: open the files with convt instead, which shows Quick
            // convert.
            NSLog("convt: could not leave a request (\(error.localizedDescription)); opening the files instead")
            NSWorkspace.shared.open(items, withApplicationAt: appURL, configuration: configuration, completionHandler: done)
            return
        }
        NSWorkspace.shared.open([URL(string: "convt://finder")!], withApplicationAt: appURL, configuration: configuration, completionHandler: done)
    }

    /// convt.app, which contains this extension at Contents/PlugIns/<name>.appex.
    private var appURL: URL {
        Bundle.main.bundleURL
            .deletingLastPathComponent() // PlugIns
            .deletingLastPathComponent() // Contents
            .deletingLastPathComponent() // convt.app
    }
}
