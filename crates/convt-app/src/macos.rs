//! macOS glue: the Finder extension's target list and request inbox, files
//! opened with convt, the check that the extension is on, the Services
//! menu entry, and hiding from the Dock.
//!
//! The Finder Sync extension is sandboxed and runs from its own executable, so
//! probing tools there would give different targets than the app. Instead the
//! app writes what each format can become to the App Group container shared
//! with the extension, at launch and whenever the engines change.
//!
//! A sandboxed process can't pass launch arguments (AppKit drops
//! `NSWorkspace.OpenConfiguration.arguments` for sandboxed callers), so the
//! extension leaves each request as a JSON file in the container's `requests`
//! folder and opens `convt://finder` with this app. The link only wakes the
//! app: a web page can open it, but only processes in the App Group can write
//! requests, so a link alone never converts anything. See
//! `integrations/macos/README.md`.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use convt_core::{FORMATS, Registry};
use futures::channel::mpsc::UnboundedSender;
use gpui_kit::{App, Entity};
use serde::{Deserialize, Serialize};

use crate::model::AppState;
use crate::request::{self, Command, Request, Source};

/// The Finder extension's bundle identifier, as `packaging/macos` builds it.
pub const FINDER_EXTENSION: &str = "app.convt.desktop.FinderSync";

/// The file the extension reads, inside the App Group container.
const TARGETS_FILE: &str = "targets.json";
/// Where the extension leaves requests, inside the App Group container.
const REQUESTS_DIR: &str = "requests";
/// The link the extension opens after leaving a request.
const FINDER_WAKE: &str = "convt://finder";
/// Older requests are stale (the app wasn't there to take them) and dropped.
const REQUEST_MAX_AGE: Duration = Duration::from_secs(120);
/// Larger request files are not the extension's and are dropped unread.
/// `FinderSync.swift` opens the files instead of writing a larger request.
const REQUEST_MAX_BYTES: u64 = 1 << 20;
/// A Finder request has been taken since launch.
static TOOK_REQUEST: AtomicBool = AtomicBool::new(false);

/// Publishes the target list now and after every registry rebuild, and
/// registers the Services menu entry.
pub fn init(state: &Entity<AppState>, requests: UnboundedSender<Request>, cx: &mut App) {
    services::register(requests);
    publish(&state.read(cx).registry);
    let mut generation = state.read(cx).registry_generation;
    cx.observe(state, move |state, cx| {
        let state = state.read(cx);
        if state.registry_generation != generation {
            generation = state.registry_generation;
            publish(&state.registry);
        }
    })
    .detach();
}

/// What the launch should do. A plain launch (no arguments) that the Finder
/// extension caused runs the extension's requests instead of opening the main
/// window, so converting from Finder shows no window.
pub fn launch_requests(first: Request) -> Vec<Request> {
    if first != Request::default() {
        return vec![first];
    }
    let pending = take_finder_requests();
    // The wake-up link may have arrived first and taken them already.
    if pending.is_empty() && !TOOK_REQUEST.load(Ordering::SeqCst) {
        vec![first]
    } else {
        pending
    }
}

/// Splits links macOS delivered into requests this module handles (the
/// extension's wake-up link, and files opened with convt from Finder's "Open
/// With") and the rest, which `request::parse_url` handles.
pub fn open_urls(links: Vec<String>) -> (Vec<Request>, Vec<String>) {
    let mut requests = Vec::new();
    let mut files = Vec::new();
    let mut rest = Vec::new();
    for link in links {
        if link == FINDER_WAKE || link.starts_with("convt://finder?") {
            requests.extend(take_finder_requests());
        } else if link.starts_with("file://") {
            files.push(OsString::from(link));
        } else {
            rest.push(link);
        }
    }
    if !files.is_empty() {
        // The command line already reads `file://` URIs. No `--to`, so this
        // opens Quick convert.
        files.insert(0, "open".into());
        match request::parse_args(files, Path::new("/")) {
            Ok(Command::Run(req)) => requests.push(req),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "ignored files opened with convt"),
        }
    }
    (requests, rest)
}

#[derive(Serialize)]
struct TargetList<'a> {
    version: u32,
    /// Lowercase extension to format id.
    extensions: BTreeMap<&'a str, &'a str>,
    /// Format id to the formats it converts to, in menu order.
    targets: BTreeMap<&'a str, Vec<Target<'a>>>,
}

#[derive(Serialize)]
struct Target<'a> {
    id: &'a str,
    name: &'a str,
    category: String,
}

fn target_list(registry: &Registry) -> TargetList<'static> {
    let mut list = TargetList {
        version: 1,
        extensions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    for format in FORMATS {
        let targets = registry.menu_targets(format);
        if targets.is_empty() {
            continue;
        }
        for ext in format.extensions {
            list.extensions.insert(ext, format.id);
        }
        list.targets.insert(
            format.id,
            targets
                .into_iter()
                .map(|t| Target {
                    id: t.id,
                    name: t.name,
                    category: format!("{:?}", t.category),
                })
                .collect(),
        );
    }
    list
}

fn publish(registry: &Registry) {
    let Some(dir) = cocoa::group_container() else {
        tracing::debug!("no App Group container; not publishing Finder targets");
        return;
    };
    let json = serde_json::to_vec(&target_list(registry)).expect("serializable");
    match crate::settings::write_atomic(&dir.join(TARGETS_FILE), &json) {
        Ok(()) => tracing::debug!(dir = %dir.display(), "published Finder targets"),
        Err(e) => tracing::warn!(error = %e, "could not publish Finder targets"),
    }
}

/// One request file, as `FinderSync.swift` writes it.
#[derive(Deserialize)]
struct FinderRequest {
    version: u32,
    to: Option<String>,
    files: Vec<PathBuf>,
    /// Seconds since 1970.
    created: f64,
}

/// Takes every fresh request the Finder extension left, oldest first. Each
/// file is deleted before it is used, so two convt processes never run the
/// same request.
fn take_finder_requests() -> Vec<Request> {
    let Some(dir) = cocoa::group_container().map(|d| d.join(REQUESTS_DIR)) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let mut taken: Vec<FinderRequest> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|path| {
            let meta = std::fs::symlink_metadata(&path).ok()?;
            let bytes = (meta.is_file() && meta.len() <= REQUEST_MAX_BYTES)
                .then(|| std::fs::read(&path).ok())
                .flatten();
            std::fs::remove_file(&path).ok()?;
            serde_json::from_slice(&bytes?).ok()
        })
        .filter(|r: &FinderRequest| {
            r.version == 1
                && !r.files.is_empty()
                && r.files.iter().all(|f| f.is_absolute())
                && (now - r.created).abs() <= REQUEST_MAX_AGE.as_secs_f64()
        })
        .collect();
    taken.sort_by(|a, b| a.created.total_cmp(&b.created));
    if !taken.is_empty() {
        TOOK_REQUEST.store(true, Ordering::SeqCst);
    }
    taken
        .into_iter()
        .map(|r| Request {
            files: r.files,
            to: r.to,
            // As trusted as the command line: only the App Group can write it.
            source: Some(Source::Cli),
            ..Request::default()
        })
        .collect()
}

/// Whether the Finder extension is switched on in System Settings. `None`
/// when macOS doesn't know the extension (the app isn't in a bundle, or
/// pluginkit failed).
pub fn finder_extension_enabled() -> Option<bool> {
    let out = std::process::Command::new("/usr/bin/pluginkit")
        .args(["-m", "-i", FINDER_EXTENSION])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    parse_pluginkit(&String::from_utf8_lossy(&out.stdout))
}

/// pluginkit prints one line per registered copy, marked `+` when the user
/// enabled it and `-` when disabled. Any enabled copy counts.
fn parse_pluginkit(out: &str) -> Option<bool> {
    let marks: Vec<char> = out
        .lines()
        .filter(|l| l.contains(&format!("{FINDER_EXTENSION}(")))
        .filter_map(|l| l.trim_start().chars().next())
        .collect();
    if marks.is_empty() {
        None
    } else {
        Some(marks.contains(&'+'))
    }
}

/// Shows or hides the Dock icon (and the app's menu bar menus). convt hides
/// from the Dock while no window is open and the menu bar item keeps it
/// running, and comes back when a window opens.
pub fn show_in_dock(show: bool) {
    use objc::runtime::{BOOL, Object};
    use objc::{class, msg_send, sel, sel_impl};

    // NSApplicationActivationPolicyRegular and ...Accessory.
    let policy: i64 = if show { 0 } else { 1 };
    // SAFETY: called on the main thread, where GPUI runs every App callback.
    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let current: i64 = msg_send![app, activationPolicy];
        if current != policy {
            let _: BOOL = msg_send![app, setActivationPolicy: policy];
        }
    }
}

/// The few Foundation calls this module needs.
mod cocoa {
    use std::ffi::{CStr, CString, OsStr, c_char};
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;

    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    pub(super) fn ns_string(s: &str) -> *mut Object {
        let c = CString::new(s).expect("no NUL");
        // SAFETY: a valid C string; the result is autoreleased.
        unsafe { msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()] }
    }

    /// The path of an `NSURL` or `NSString`.
    pub(super) unsafe fn path_of(object: *mut Object, url: bool) -> Option<PathBuf> {
        if object.is_null() {
            return None;
        }
        unsafe {
            let path: *mut Object = if url { msg_send![object, path] } else { object };
            if path.is_null() {
                return None;
            }
            let utf8: *const c_char = msg_send![path, UTF8String];
            (!utf8.is_null())
                .then(|| PathBuf::from(OsStr::from_bytes(CStr::from_ptr(utf8).to_bytes())))
        }
    }

    /// The App Group container named by `ConvtAppGroup` in the bundle's
    /// Info.plist, through Foundation, which also creates it. `None` outside
    /// the bundle, or when the signature doesn't grant the group.
    pub(super) fn group_container() -> Option<PathBuf> {
        // SAFETY: plain Foundation calls on valid objects; results are
        // checked for nil.
        unsafe {
            let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
            let group: *mut Object =
                msg_send![bundle, objectForInfoDictionaryKey: ns_string("ConvtAppGroup")];
            let is_string: bool = !group.is_null() && {
                let yes: objc::runtime::BOOL = msg_send![group, isKindOfClass: class!(NSString)];
                yes == objc::runtime::YES
            };
            if !is_string {
                return None;
            }
            let fm: *mut Object = msg_send![class!(NSFileManager), defaultManager];
            let url: *mut Object =
                msg_send![fm, containerURLForSecurityApplicationGroupIdentifier: group];
            path_of(url, true)
        }
    }
}

/// "Convert with convt" in the Services menu, for when the Finder extension
/// is off. It opens Quick convert for the files, like "More options…".
mod services {
    use std::ffi::c_void;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use futures::channel::mpsc::UnboundedSender;
    use objc::declare::ClassDecl;
    use objc::runtime::{BOOL, Object, Sel, YES};
    use objc::{class, msg_send, sel, sel_impl};

    use super::{Request, Source};

    static REQUESTS: OnceLock<UnboundedSender<Request>> = OnceLock::new();

    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {
        fn NSUpdateDynamicServices();
    }

    /// Becomes the app's services provider. `NSServices` in Info.plist names
    /// the `openFiles` message.
    pub fn register(requests: UnboundedSender<Request>) {
        if REQUESTS.set(requests).is_err() {
            return;
        }
        let Some(mut decl) = ClassDecl::new("ConvtServicesProvider", class!(NSObject)) else {
            return;
        };
        // SAFETY: the signature matches `-openFiles:userData:error:` (object,
        // object, pointer), and the provider lives for the rest of the process.
        unsafe {
            decl.add_method(
                sel!(openFiles:userData:error:),
                open_files as extern "C" fn(&Object, Sel, *mut Object, *mut Object, *mut c_void),
            );
            let provider: *mut Object = msg_send![decl.register(), new];
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, setServicesProvider: provider];
            NSUpdateDynamicServices();
        }
    }

    extern "C" fn open_files(
        _: &Object,
        _: Sel,
        pasteboard: *mut Object,
        _: *mut Object,
        _: *mut c_void,
    ) {
        // SAFETY: AppKit passes a valid NSPasteboard.
        let files = unsafe { file_urls(pasteboard) };
        if files.is_empty() {
            return;
        }
        if let Some(tx) = REQUESTS.get() {
            let _ = tx.unbounded_send(Request {
                files,
                source: Some(Source::Cli),
                ..Request::default()
            });
        }
    }

    unsafe fn file_urls(pasteboard: *mut Object) -> Vec<PathBuf> {
        if pasteboard.is_null() {
            return Vec::new();
        }
        unsafe {
            let classes: *mut Object = msg_send![class!(NSArray), arrayWithObject: class!(NSURL)];
            let urls: *mut Object = msg_send![pasteboard, readObjectsForClasses: classes options: std::ptr::null_mut::<Object>()];
            if urls.is_null() {
                return Vec::new();
            }
            let count: usize = msg_send![urls, count];
            (0..count)
                .filter_map(|i| {
                    let url: *mut Object = msg_send![urls, objectAtIndex: i];
                    let is_file: BOOL = msg_send![url, isFileURL];
                    if is_file != YES {
                        return None;
                    }
                    super::cocoa::path_of(url, true)
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_whether_the_extension_is_on() {
        let on = format!("+    {FINDER_EXTENSION}(0.1.0)\n");
        let off = format!("-    {FINDER_EXTENSION}(0.1.0)\n");
        let unset = format!("     {FINDER_EXTENSION}(0.1.0)\tB13F\n");
        assert_eq!(parse_pluginkit(&on), Some(true));
        assert_eq!(parse_pluginkit(&off), Some(false));
        assert_eq!(parse_pluginkit(&unset), Some(false));
        assert_eq!(parse_pluginkit(&format!("{off}{on}")), Some(true));
        assert_eq!(parse_pluginkit(""), None);
        assert_eq!(
            parse_pluginkit("+    leodev.convt.ConvtFinder(1.0)\n"),
            None
        );
    }

    #[test]
    fn target_list_covers_every_convertible_extension() {
        let list = target_list(&convt_engines::default_registry());
        assert_eq!(list.extensions.get("jpg"), Some(&"jpeg"));
        let png = &list.targets["png"];
        assert!(png.iter().any(|t| t.id == "webp" && t.category == "Image"));
        assert!(png.iter().all(|t| t.id != "png"));
        for id in list.extensions.values() {
            assert!(list.targets.contains_key(id));
        }
    }

    #[test]
    fn files_opened_with_convt_open_quick_convert() {
        let (requests, rest) = open_urls(vec![
            "file:///tmp/a%20b.png".into(),
            "file:///tmp/c.jpg".into(),
            "convt://activate?key=K".into(),
        ]);
        assert_eq!(rest, vec!["convt://activate?key=K".to_string()]);
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].files,
            vec![PathBuf::from("/tmp/a b.png"), PathBuf::from("/tmp/c.jpg")]
        );
        assert_eq!(requests[0].to, None);
    }
}
