//! Folder watching for automation rules.
//!
//! Each enabled rule looks at **one** directory, never recursively: the
//! macOS screenshot location, the screen-recording folder, or a path the
//! rule names. Home, the filesystem root and other catch-all folders only
//! match files that look like screenshots or recordings, so a Desktop watch
//! does not convert every image that lands there.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use convt_core::{Category, Format, format_by_extension, format_by_id};
use gpui_kit::{AppContext, Context, Task};

use crate::model::AppState;
use crate::settings::{Automation, WatchKind, looks_like_recording, looks_like_screenshot};

/// How often the poller lists each watched folder.
pub const POLL: Duration = Duration::from_secs(1);

/// What a rule is waiting on, for the Automations page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchStatus {
    /// The folder exists and will be listed on each poll.
    Folder(PathBuf),
    /// macOS screenshots are set to the clipboard, so nothing lands on disk.
    Clipboard,
    /// The folder is missing, or this build has no folder to watch (tests
    /// without an override).
    Waiting(String),
    /// The path would watch too much (home or the filesystem root).
    Skipped(&'static str),
}

/// Snapshot of each watched directory so existing files are not converted
/// when a rule is turned on, and in-progress recordings wait until they
/// stop growing.
#[derive(Debug, Default)]
pub struct WatchState {
    primed: HashSet<PathBuf>,
    seen: HashMap<PathBuf, u64>,
    submitted: HashSet<PathBuf>,
}

impl WatchState {
    /// Files that are new, stable and match an enabled rule, as
    /// `(rule index, path)` in rule order.
    pub fn drain_ready(&mut self, rules: &[Automation]) -> Vec<(usize, PathBuf)> {
        let mut ready = Vec::new();
        for (index, rule) in rules.iter().enumerate() {
            if !rule.is_watched() {
                continue;
            }
            let WatchStatus::Folder(dir) = watch_status(rule) else {
                continue;
            };
            let Ok(entries) = list_dir(&dir) else {
                continue;
            };
            if self.primed.insert(dir.clone()) {
                for (path, len) in &entries {
                    self.seen.insert(path.clone(), *len);
                    self.submitted.insert(path.clone());
                }
                continue;
            }
            for (path, len) in entries {
                if self.submitted.contains(&path) {
                    self.seen.insert(path, len);
                    continue;
                }
                match self.seen.get(&path) {
                    None => {
                        self.seen.insert(path, len);
                    }
                    Some(&prev) if prev != len => {
                        self.seen.insert(path, len);
                    }
                    Some(_) => {
                        if matches_rule(rule, &path, &dir) {
                            self.submitted.insert(path.clone());
                            if let Some(to) = format_by_id(&rule.to) {
                                self.submitted.insert(path.with_extension(to.extension()));
                            }
                            ready.push((index, path));
                        } else {
                            self.submitted.insert(path);
                        }
                    }
                }
            }
        }
        ready
    }
}

/// Polls enabled rules so a new screenshot or recording is converted. Tests
/// call [`AppState::poll_automations`] themselves; a live poll would race
/// them and could see the real Desktop.
pub fn watch(cx: &mut Context<AppState>) -> Task<()> {
    cx.spawn(async move |this, cx| {
        loop {
            if this
                .update(cx, |state, cx| state.poll_automations(cx))
                .is_err()
            {
                break;
            }
            cx.background_executor().timer(POLL).await;
        }
    })
}

/// Where a rule looks, and why it might not.
pub fn watch_status(rule: &Automation) -> WatchStatus {
    if let Some(folder) = rule.folder.as_ref().filter(|p| !p.as_os_str().is_empty()) {
        let dir = expand_tilde(folder);
        if is_too_broad(&dir) {
            return WatchStatus::Skipped("This folder is too broad to watch.");
        }
        if dir.is_dir() {
            return WatchStatus::Folder(dir);
        }
        return WatchStatus::Waiting(format!("{} isn't there yet.", display_path(&dir)));
    }
    match rule.watch_kind() {
        WatchKind::Screenshot => match screenshot_location() {
            ScreenshotLocation::Clipboard => WatchStatus::Clipboard,
            ScreenshotLocation::Folder(dir) if is_too_broad(&dir) => {
                WatchStatus::Skipped("This folder is too broad to watch.")
            }
            ScreenshotLocation::Folder(dir) if dir.is_dir() => WatchStatus::Folder(dir),
            ScreenshotLocation::Folder(dir) => {
                WatchStatus::Waiting(format!("{} isn't there yet.", display_path(&dir)))
            }
            ScreenshotLocation::Unknown => {
                WatchStatus::Waiting("No screenshot folder is set.".into())
            }
        },
        WatchKind::Recording => match recording_dir() {
            Some(dir) if is_too_broad(&dir) => {
                WatchStatus::Skipped("This folder is too broad to watch.")
            }
            Some(dir) if dir.is_dir() => WatchStatus::Folder(dir),
            Some(dir) => WatchStatus::Waiting(format!("{} isn't there yet.", display_path(&dir))),
            None => WatchStatus::Waiting("No screen-recording folder is set.".into()),
        },
        WatchKind::Folder => match source_dir(&rule.source) {
            Some(dir) if is_too_broad(&dir) => {
                WatchStatus::Skipped("This folder is too broad to watch.")
            }
            Some(dir) if dir.is_dir() => WatchStatus::Folder(dir),
            Some(dir) => WatchStatus::Waiting(format!("{} isn't there yet.", display_path(&dir))),
            None => WatchStatus::Waiting(format!("No folder for {}.", rule.source)),
        },
    }
}

/// The path shown under a rule, or the reason it is waiting.
pub fn source_line(rule: &Automation) -> String {
    match watch_status(rule) {
        WatchStatus::Folder(dir) => display_path(&dir),
        WatchStatus::Clipboard => {
            "Screenshots go to the clipboard, so there is no folder to watch.".into()
        }
        WatchStatus::Waiting(msg) => msg,
        WatchStatus::Skipped(msg) => msg.to_string(),
    }
}

fn matches_rule(rule: &Automation, path: &Path, dir: &Path) -> bool {
    if path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.') || is_temp_name(n))
    {
        return false;
    }
    let Some(from) = format_by_extension(path) else {
        return false;
    };
    let Some(to) = format_by_id(&rule.to) else {
        return false;
    };
    if from.id == to.id {
        return false;
    }
    match rule.watch_kind() {
        WatchKind::Screenshot => {
            is_still(from) && (looks_like_screenshot(path) || !is_catch_all_folder(dir))
        }
        WatchKind::Recording => {
            from.category == Category::Video
                && (looks_like_recording(path) || !is_catch_all_folder(dir))
        }
        WatchKind::Folder => match format_by_id(&rule.name) {
            Some(only) => from.id == only.id,
            None => true,
        },
    }
}

fn is_still(format: &Format) -> bool {
    matches!(format.category, Category::Image | Category::Vector)
}

fn is_temp_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".tmp")
        || lower.ends_with(".part")
        || lower.ends_with(".download")
        || lower.contains(".convt-")
}

fn is_catch_all_folder(dir: &Path) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return true;
    };
    matches!(
        name.to_ascii_lowercase().as_str(),
        "desktop" | "downloads" | "documents" | "movies" | "pictures" | "videos" | "music"
    ) || home_dir().is_some_and(|home| same_path(dir, &home))
}

fn is_too_broad(dir: &Path) -> bool {
    dir.parent().is_none() || home_dir().is_some_and(|home| same_path(dir, &home))
}

fn same_path(a: &Path, b: &Path) -> bool {
    fn norm(path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }
    norm(a) == norm(b)
}

fn list_dir(dir: &Path) -> std::io::Result<Vec<(PathBuf, u64)>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() || meta.file_type().is_symlink() {
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        out.push((path, meta.len()));
    }
    Ok(out)
}

/// Where macOS (or this machine) puts still screenshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenshotLocation {
    Folder(PathBuf),
    Clipboard,
    Unknown,
}

pub fn screenshot_location() -> ScreenshotLocation {
    if let Some(dir) = env_dir("CONVT_SCREENSHOT_DIR") {
        return ScreenshotLocation::Folder(dir);
    }
    if cfg!(test) {
        return ScreenshotLocation::Unknown;
    }
    #[cfg(target_os = "macos")]
    {
        if macos_screencapture("target").is_some_and(|t| t.eq_ignore_ascii_case("clipboard")) {
            return ScreenshotLocation::Clipboard;
        }
        if let Some(dir) = macos_screencapture("location").and_then(|s| existing_or_path(&s)) {
            return ScreenshotLocation::Folder(dir);
        }
    }
    if let Some(dir) = picture_dir()
        .map(|p| p.join("Screenshots"))
        .filter(|p| p.is_dir())
    {
        return ScreenshotLocation::Folder(dir);
    }
    match desktop_dir() {
        Some(dir) => ScreenshotLocation::Folder(dir),
        None => ScreenshotLocation::Unknown,
    }
}

fn recording_dir() -> Option<PathBuf> {
    if let Some(dir) = env_dir("CONVT_RECORDING_DIR") {
        return Some(dir);
    }
    if cfg!(test) {
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        for key in ["videolocation", "videoLocation", "recordingLocation"] {
            if let Some(dir) = macos_screencapture(key).and_then(|s| existing_or_path(&s)) {
                return Some(dir);
            }
        }
    }
    if let Some(dir) = video_dir()
        .map(|p| p.join("Captures"))
        .filter(|p| p.is_dir())
    {
        return Some(dir);
    }
    desktop_dir()
}

fn source_dir(source: &str) -> Option<PathBuf> {
    let source = source.trim();
    if source.is_empty() {
        return None;
    }
    match source.to_ascii_lowercase().as_str() {
        "desktop" => desktop_dir(),
        "downloads" => download_dir(),
        "documents" => dirs::document_dir(),
        "pictures" => picture_dir(),
        "movies" | "videos" => video_dir(),
        _ if source.starts_with('~') || source.starts_with('/') || source.contains(':') => {
            Some(expand_tilde(Path::new(source)))
        }
        _ => None,
    }
}

fn existing_or_path(raw: &str) -> Option<PathBuf> {
    let path = expand_tilde(Path::new(raw.trim().trim_matches('"')));
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

fn expand_tilde(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if raw == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = home_dir()
    {
        return home.join(rest);
    }
    path.to_path_buf()
}

pub fn display_path(path: &Path) -> String {
    if let Some(home) = home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        if rest.as_os_str().is_empty() {
            return "~".into();
        }
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

fn env_dir(name: &str) -> Option<PathBuf> {
    let value = std::env::var_os(name)?;
    if value.is_empty() {
        return None;
    }
    Some(expand_tilde(Path::new(&value)))
}

fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

fn desktop_dir() -> Option<PathBuf> {
    dirs::desktop_dir().or_else(|| home_dir().map(|h| h.join("Desktop")))
}

fn download_dir() -> Option<PathBuf> {
    dirs::download_dir().or_else(|| home_dir().map(|h| h.join("Downloads")))
}

fn picture_dir() -> Option<PathBuf> {
    dirs::picture_dir()
}

fn video_dir() -> Option<PathBuf> {
    dirs::video_dir()
}

#[cfg(target_os = "macos")]
fn macos_screencapture(key: &str) -> Option<String> {
    let out = std::process::Command::new("defaults")
        .args(["read", "com.apple.screencapture", key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8(out.stdout).ok()?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::placeholder::example_automations;
    use crate::settings::{Automation, is_photo_format};

    fn rule(name: &str, to: &str, kind: WatchKind, folder: &Path) -> Automation {
        Automation {
            name: name.into(),
            to: to.into(),
            source: "test".into(),
            detail: String::new(),
            enabled: true,
            watch: Some(kind),
            folder: Some(folder.to_path_buf()),
            copy_to_clipboard: Some(false),
        }
    }

    #[test]
    fn screenshot_names_and_photo_formats() {
        assert!(looks_like_screenshot(Path::new(
            "/Desktop/Screenshot 2026-10-07 at 10.38.00 AM.heic"
        )));
        assert!(looks_like_screenshot(Path::new("Screen Shot 1.png")));
        assert!(looks_like_recording(Path::new(
            "Screen Recording 2026-10-07 at 10.38.00 AM.mov"
        )));
        assert!(!looks_like_screenshot(Path::new("IMG_2041.heic")));
        assert!(is_photo_format(format_by_id("heic").unwrap()));
        assert!(is_photo_format(format_by_id("webp").unwrap()));
        assert!(!is_photo_format(format_by_id("png").unwrap()));
        assert!(!is_photo_format(format_by_id("svg").unwrap()));
    }

    #[test]
    fn desktop_screenshot_rule_ignores_ordinary_photos() {
        let dir = Path::new("/Users/leo/Desktop");
        let shots = Automation {
            name: "Screenshots".into(),
            to: "png".into(),
            source: "Desktop".into(),
            detail: "copy to clipboard".into(),
            enabled: true,
            watch: Some(WatchKind::Screenshot),
            folder: None,
            copy_to_clipboard: Some(true),
        };
        assert!(matches_rule(&shots, &dir.join("Screenshot 1.heic"), dir));
        assert!(!matches_rule(&shots, &dir.join("IMG_2041.heic"), dir));
        assert!(!matches_rule(&shots, &dir.join("Screenshot 1.png"), dir));
    }

    #[test]
    fn dedicated_screenshot_folder_takes_any_still() {
        let dir = Path::new("/Users/leo/Pictures/Screenshots");
        let shots = Automation {
            name: "Screenshots".into(),
            to: "png".into(),
            source: "Screenshots".into(),
            detail: String::new(),
            enabled: true,
            watch: Some(WatchKind::Screenshot),
            folder: None,
            copy_to_clipboard: None,
        };
        assert!(matches_rule(&shots, &dir.join("capture.heic"), dir));
        assert!(!matches_rule(&shots, &dir.join("notes.mp4"), dir));
    }

    #[test]
    fn recording_rule_matches_screen_recordings() {
        let dir = Path::new("/Users/leo/Desktop");
        let rec = Automation {
            name: "Screen recordings".into(),
            to: "mp4".into(),
            source: "Desktop".into(),
            detail: String::new(),
            enabled: true,
            watch: Some(WatchKind::Recording),
            folder: None,
            copy_to_clipboard: None,
        };
        assert!(matches_rule(&rec, &dir.join("Screen Recording 1.mov"), dir));
        assert!(!matches_rule(&rec, &dir.join("holiday.mov"), dir));
        assert!(!matches_rule(
            &rec,
            &dir.join("Screen Recording 1.mp4"),
            dir
        ));
    }

    #[test]
    fn heic_folder_rule_only_matches_heic() {
        let dir = Path::new("/Users/leo/Downloads");
        let heic = Automation {
            name: "HEIC".into(),
            to: "jpeg".into(),
            source: "Downloads".into(),
            detail: "keep original".into(),
            enabled: true,
            watch: Some(WatchKind::Folder),
            folder: None,
            copy_to_clipboard: None,
        };
        assert!(matches_rule(&heic, &dir.join("IMG_2041.heic"), dir));
        assert!(!matches_rule(&heic, &dir.join("photo.webp"), dir));
    }

    #[test]
    fn watch_state_primes_then_emits_stable_new_files() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path();
        let shots = rule("Screenshots", "png", WatchKind::Screenshot, dir);
        std::fs::write(dir.join("Screenshot old.bmp"), b"old").unwrap();
        let mut state = WatchState::default();
        assert!(state.drain_ready(&[shots.clone()]).is_empty());

        std::fs::write(dir.join("notes.bmp"), b"skip").unwrap();
        std::fs::write(dir.join("Screenshot new.bmp"), b"new").unwrap();
        assert!(state.drain_ready(&[shots.clone()]).is_empty());
        let ready = state.drain_ready(&[shots]);
        assert_eq!(ready.len(), 1);
        assert!(ready[0].1.ends_with("Screenshot new.bmp"));
    }

    #[test]
    fn growing_files_wait_until_the_size_settles() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path();
        let rec = rule("Screen recordings", "mp4", WatchKind::Recording, dir);
        let mut state = WatchState::default();
        assert!(state.drain_ready(&[rec.clone()]).is_empty());

        let file = dir.join("Screen Recording 1.mov");
        std::fs::write(&file, b"one").unwrap();
        assert!(state.drain_ready(&[rec.clone()]).is_empty());
        std::fs::write(&file, b"longer").unwrap();
        assert!(state.drain_ready(&[rec.clone()]).is_empty());
        let ready = state.drain_ready(&[rec]);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].1, file);
    }

    #[test]
    fn example_rules_are_screenshot_and_recording() {
        let rules = example_automations();
        assert_eq!(rules[0].name, "Screenshots");
        assert_eq!(rules[0].to, "png");
        assert_eq!(rules[0].watch_kind(), WatchKind::Screenshot);
        assert!(rules[0].copies_to_clipboard());
        assert_eq!(rules[1].name, "Screen recordings");
        assert_eq!(rules[1].to, "mp4");
        assert_eq!(rules[1].watch_kind(), WatchKind::Recording);
        assert!(!rules[1].copies_to_clipboard());
    }

    #[test]
    fn a_legacy_exports_placeholder_is_not_watched() {
        let exports = Automation {
            name: "Exports".into(),
            to: "mp4".into(),
            source: "~/Movies/Exports".into(),
            detail: "H.264 1080p".into(),
            enabled: true,
            watch: None,
            folder: None,
            copy_to_clipboard: None,
        };
        assert!(!exports.is_watched());
        assert_eq!(exports.watch_kind(), WatchKind::Folder);
    }

    #[test]
    fn tests_do_not_resolve_the_real_desktop() {
        let shots = &example_automations()[0];
        assert!(matches!(watch_status(shots), WatchStatus::Waiting(_)));
    }

    #[test]
    fn home_and_root_are_too_broad() {
        let root = Path::new("/");
        assert!(is_too_broad(root));
        if let Some(home) = home_dir() {
            assert!(is_too_broad(&home));
            assert!(!is_too_broad(&home.join("Desktop")));
        }
    }
}
