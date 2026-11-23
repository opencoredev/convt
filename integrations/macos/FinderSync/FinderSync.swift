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
    /// The items each recent menu was built for, by the tag its items carry.
    /// By the time an item is clicked, Finder may report a different
    /// selection or none at all (right-clicking an item that wasn't selected
    /// targets it without selecting it), and Finder can build another menu,
    /// in another window, before the first one's click arrives. The tag ties
    /// a click to the menu it came from. Only the newest `maxMenuSelections`
    /// are kept, so the memory stays tiny.
    private var menuSelections: [Int: [URL]] = [:]
    /// `menuSelections`' tags, oldest first.
    private var menuTagOrder: [Int] = []
    private var nextMenuTag = 100
    private let maxMenuSelections = 16
    /// The items the last menu was built for, in case Finder drops the tag
    /// when it copies the menu into its own process.
    private var lastMenuItems: [URL] = []

    override init() {
        super.init()
        // Watch the whole disk so the menu shows up everywhere.
        FIFinderSyncController.default().directoryURLs = [URL(fileURLWithPath: "/")]
    }

    override func menu(for menuKind: FIMenuKind) -> NSMenu? {
        guard menuKind == .contextualMenuForItems else { return nil }
        let controller = FIFinderSyncController.default()
        let items: [URL]
        if let selected = controller.selectedItemURLs(), !selected.isEmpty {
            items = selected
        } else if let targeted = controller.targetedURL() {
            items = [targeted]
        } else {
            return nil
        }
        let tag = nextMenuTag
        nextMenuTag = nextMenuTag >= 10000 ? 100 : nextMenuTag + 1
        remember(items, tag: tag)
        lastMenuItems = items

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
                // Finder copies this menu into its own process, so `convert`
                // finds the target again by title, and the files by the tag
                // (with fallbacks in `clickedItems` if the copy loses it).
                let item = NSMenuItem(title: title, action: #selector(convert(_:)), keyEquivalent: "")
                item.target = self
                item.tag = tag
                submenu.addItem(item)
            }
            submenu.addItem(.separator())
            submenu.addItem(moreItem(title: "More options…", tag: tag))
        } else {
            // The app hasn't published its list yet (it has never run, or
            // the list is unreadable): let the app work out the targets
            // rather than guess from the format table.
            submenu.addItem(moreItem(title: "Open in convt…", tag: tag))
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

    private func moreItem(title: String, tag: Int) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: #selector(moreOptions(_:)), keyEquivalent: "")
        item.target = self
        item.tag = tag
        return item
    }

    /// Records the items the menu with `tag` was built for, forgetting the
    /// oldest menu past `maxMenuSelections`.
    private func remember(_ items: [URL], tag: Int) {
        if menuSelections.updateValue(items, forKey: tag) != nil {
            menuTagOrder.removeAll { $0 == tag }
        }
        menuTagOrder.append(tag)
        while menuTagOrder.count > maxMenuSelections {
            menuSelections.removeValue(forKey: menuTagOrder.removeFirst())
        }
    }

    /// The items a click on `sender` applies to: exactly what its menu was
    /// built for (found by its tag), else what the last menu was built for,
    /// else Finder's selection, else the item that was right-clicked. The
    /// menu's items come first because Finder's selection can change between
    /// opening the menu and clicking it, and converting what is selected by
    /// then would convert files the user never picked. They are used once,
    /// so a later click can't reuse an old menu's files.
    private func clickedItems(for sender: NSMenuItem) -> [URL]? {
        if sender.tag != 0, let items = menuSelections.removeValue(forKey: sender.tag), !items.isEmpty {
            menuTagOrder.removeAll { $0 == sender.tag }
            if lastMenuItems == items {
                lastMenuItems = []
            }
            return items
        }
        if !lastMenuItems.isEmpty {
            let items = lastMenuItems
            lastMenuItems = []
            return items
        }
        let controller = FIFinderSyncController.default()
        if let selected = controller.selectedItemURLs(), !selected.isEmpty {
            return selected
        }
        if let targeted = controller.targetedURL() {
            return [targeted]
        }
        NSLog("convt: the menu was clicked with no selected or targeted items")
        return nil
    }

    /// Converts the selection in place, with no window.
    @objc private func convert(_ sender: NSMenuItem) {
        guard let items = clickedItems(for: sender) else { return }
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
        guard let items = clickedItems(for: sender) else { return }
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
    /// also claim the `convt` scheme. Each link carries the request's id, so
    /// macOS doesn't merge two quick clicks into one identical open; the app
    /// reads every waiting request either way.
    private func send(to: String?, items: [URL], activate: Bool) {
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.activates = activate
        configuration.addsToRecentItems = false
        let done: (NSRunningApplication?, Error?) -> Void = { _, error in
            if let error {
                NSLog("convt: could not start the app: \(error.localizedDescription)")
            }
        }
        let requestID = UUID().uuidString
        let requestURL: URL
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
            requestURL = dir.appendingPathComponent(requestID + ".json")
            try data.write(to: requestURL, options: .atomic)
        } catch {
            // No usable shared container (an ad-hoc build, which macOS
            // doesn't grant the group), or a selection too large for one
            // request: open the files with convt instead, which shows Quick
            // convert.
            NSLog("convt: could not leave a request (\(error.localizedDescription)); opening the files instead")
            NSWorkspace.shared.open(items, withApplicationAt: appURL, configuration: configuration, completionHandler: done)
            return
        }
        // If the app didn't wake, take the request back: left in the
        // container, it would run on some later, unrelated launch.
        let woke: (NSRunningApplication?, Error?) -> Void = { _, error in
            guard let error else { return }
            NSLog("convt: could not start the app: \(error.localizedDescription); dropping request \(requestID)")
            try? FileManager.default.removeItem(at: requestURL)
        }
        NSWorkspace.shared.open([URL(string: "convt://finder?id=\(requestID)")!], withApplicationAt: appURL, configuration: configuration, completionHandler: woke)
    }

    /// convt.app, which contains this extension at Contents/PlugIns/<name>.appex.
    private var appURL: URL {
        Bundle.main.bundleURL
            .deletingLastPathComponent() // PlugIns
            .deletingLastPathComponent() // Contents
            .deletingLastPathComponent() // convt.app
    }
}
