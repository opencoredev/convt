//! App settings, stored as `settings.toml` in the convt config directory.
//! Unknown keys are kept out of the way rather than rejected, so an older app
//! can read a newer file.

use std::io;
use std::path::{Path, PathBuf};

use convt_core::{Category, Format, Output, format_by_id};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Where converted files go. Unset means next to each input.
    pub output_dir: Option<PathBuf>,
    /// How many conversions run at once. Unset means Auto: one per CPU core.
    pub concurrency: Option<usize>,
    /// Show a system notification when a conversion finishes while the
    /// window is in the background.
    pub notifications: bool,
    /// Show each finished file in the file manager.
    pub reveal_when_done: bool,
    /// Show the menu bar (tray) icon where the platform has one.
    pub menu_bar_icon: bool,
    /// The first-run window was finished (Start converting / Open convt).
    /// Closing or quitting mid-setup leaves this false so the next launch
    /// shows first run again.
    pub first_run_done: bool,
    /// The UTC day (`YYYY-MM-DD`) the app last asked convt.app for the
    /// current Pro key, so launches renew at most once a day.
    pub license_checked: Option<String>,
    /// Cached online Pro trial end day, used while offline.
    pub trial_ends_on: Option<String>,
    /// Check convt.app for a newer build at launch and every few hours. On
    /// by default. Check now works either way.
    pub update_checks: bool,
    /// When (Unix seconds) an update check last got an answer that checked
    /// out. Older files kept the day in `update_checked`, which is ignored.
    pub update_checked_at: Option<u64>,
    /// The highest update manifest `sequence` accepted, so an older signed
    /// manifest can't be replayed to hide a newer release.
    pub update_sequence: u64,
    /// What files dropped on the menu bar popover convert to, by kind.
    pub defaults: Defaults,
    /// Automation rules. Each enabled rule watches one folder.
    pub automations: Vec<Automation>,
    /// The user agreed that Cloud conversions upload the file to convt's
    /// servers. Asked once, the first time they pick Cloud.
    pub cloud_consent: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_dir: None,
            concurrency: None,
            notifications: true,
            reveal_when_done: false,
            menu_bar_icon: true,
            first_run_done: false,
            license_checked: None,
            trial_ends_on: None,
            update_checks: true,
            update_checked_at: None,
            update_sequence: 0,
            defaults: Defaults::default(),
            automations: crate::placeholder::example_automations(),
            cloud_consent: false,
        }
    }
}

/// The format the popover's drop bar picks for each kind of file, by format id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Defaults {
    /// Camera and web photos (HEIC, AVIF, WebP, JPEG).
    pub photos: String,
    /// Screenshots, transparent images, vectors and other stills.
    pub images: String,
    pub video: String,
    pub audio: String,
    pub documents: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            photos: "jpeg".into(),
            images: "png".into(),
            video: "mp4".into(),
            audio: "mp3".into(),
            documents: "pdf".into(),
        }
    }
}

impl<'de> Deserialize<'de> for Defaults {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            images: Option<String>,
            photos: Option<String>,
            video: Option<String>,
            audio: Option<String>,
            documents: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let fresh = Defaults::default();
        // Files written before the photo / image split stored one `images`
        // default for every still. Keep that value for both kinds so a
        // saved WebP (or whatever they picked) is not rewritten.
        let (photos, images) = match (raw.photos, raw.images) {
            (Some(photos), images) => (photos, images.unwrap_or(fresh.images)),
            (None, Some(legacy)) => (legacy.clone(), legacy),
            (None, None) => (fresh.photos, fresh.images),
        };
        Ok(Self {
            photos,
            images,
            video: raw.video.unwrap_or(fresh.video),
            audio: raw.audio.unwrap_or(fresh.audio),
            documents: raw.documents.unwrap_or(fresh.documents),
        })
    }
}

/// A kind of file with its own default target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Photos,
    Images,
    Video,
    Audio,
    Documents,
}

impl Kind {
    /// The kind a file of `format` belongs to. PDFs have none: they are
    /// already what documents become.
    pub fn of(format: &Format) -> Option<Kind> {
        match format.category {
            Category::Image | Category::Vector if is_photo_format(format) => Some(Kind::Photos),
            Category::Image | Category::Vector => Some(Kind::Images),
            Category::Video => Some(Kind::Video),
            Category::Audio => Some(Kind::Audio),
            Category::Document | Category::Presentation | Category::Spreadsheet => {
                Some(Kind::Documents)
            }
            Category::Pdf => None,
        }
    }

    /// [`Self::of`], except a screenshot-named photo (HEIC from Cmd+Shift+3)
    /// uses the Images default so it becomes PNG, not JPEG.
    pub fn of_file(path: &Path, format: &Format) -> Option<Kind> {
        match Kind::of(format) {
            Some(Kind::Photos) if looks_like_screenshot(path) => Some(Kind::Images),
            other => other,
        }
    }
}

impl Defaults {
    pub fn get(&self, kind: Kind) -> Option<&'static Format> {
        format_by_id(match kind {
            Kind::Photos => &self.photos,
            Kind::Images => &self.images,
            Kind::Video => &self.video,
            Kind::Audio => &self.audio,
            Kind::Documents => &self.documents,
        })
    }

    #[cfg(test)]
    pub fn set(&mut self, kind: Kind, to: &Format) {
        let slot = match kind {
            Kind::Photos => &mut self.photos,
            Kind::Images => &mut self.images,
            Kind::Video => &mut self.video,
            Kind::Audio => &mut self.audio,
            Kind::Documents => &mut self.documents,
        };
        *slot = to.id.to_string();
    }
}

/// How a rule decides which new files to convert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchKind {
    Screenshot,
    Recording,
    Folder,
}

/// A rule such as "Screenshots become PNG".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Automation {
    /// What the rule watches, e.g. "Screenshots".
    pub name: String,
    /// Target format id.
    pub to: String,
    /// Where it looks, e.g. "Desktop" or "Screenshots". Display and, for
    /// older files, the folder to resolve.
    pub source: String,
    /// What else it does, e.g. "copy to clipboard".
    pub detail: String,
    pub enabled: bool,
    /// When set, overrides the kind inferred from [`Self::name`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch: Option<WatchKind>,
    /// An explicit folder. When set, the system screenshot location is ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    /// When set, overrides the "copy to clipboard" phrase in [`Self::detail`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy_to_clipboard: Option<bool>,
}

impl Automation {
    pub fn watch_kind(&self) -> WatchKind {
        if let Some(kind) = self.watch {
            return kind;
        }
        let name = self.name.to_ascii_lowercase();
        if name.contains("screenshot") {
            WatchKind::Screenshot
        } else if name.contains("recording") {
            WatchKind::Recording
        } else {
            WatchKind::Folder
        }
    }

    pub fn copies_to_clipboard(&self) -> bool {
        self.copy_to_clipboard.unwrap_or_else(|| {
            self.detail
                .to_ascii_lowercase()
                .contains("copy to clipboard")
        })
    }

    /// Whether the engine should look at this rule. A legacy "Exports"
    /// placeholder without an explicit watch kind or folder is left idle.
    pub fn is_watched(&self) -> bool {
        self.enabled
            && (self.watch.is_some()
                || self.folder.is_some()
                || matches!(
                    self.watch_kind(),
                    WatchKind::Screenshot | WatchKind::Recording
                ))
    }
}

/// Camera and web photo formats: Add files turns these into JPEG.
pub fn is_photo_format(format: &Format) -> bool {
    matches!(format.id, "jpeg" | "heic" | "avif" | "webp")
}

/// A file name that macOS, Windows or common tools use for a still capture.
pub fn looks_like_screenshot(path: &Path) -> bool {
    let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    name.starts_with("screenshot")
        || name.starts_with("screen shot")
        || name.starts_with("cleanshot")
        || name.starts_with("simulator screen")
}

/// A file name that macOS or Windows use for a screen recording.
pub fn looks_like_recording(path: &Path) -> bool {
    let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    name.starts_with("screen recording")
        || name.starts_with("screenrecording")
        || name.starts_with("recording ")
        || name.starts_with("recording_")
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        convt_engines::paths::config_dir().map(|d| d.join("settings.toml"))
    }

    /// Reads settings from `path`. A missing file gives the defaults; a
    /// broken one is an error so the caller can say so instead of silently
    /// overwriting it.
    pub fn load(path: &Path) -> io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}: {}", path.display(), e.message()),
                )
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Writes atomically, so a crash never leaves a half-written file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let text = toml::to_string(self).map_err(io::Error::other)?;
        write_atomic(path, text.as_bytes())
    }

    pub fn concurrency(&self) -> usize {
        self.concurrency
            .filter(|n| *n > 0)
            .unwrap_or_else(auto_concurrency)
    }

    pub fn output(&self) -> Output {
        match &self.output_dir {
            Some(dir) => Output::Dir(dir.clone()),
            None => Output::Beside,
        }
    }

    /// Whether the app keeps running with no window open. Only macOS keeps a
    /// menu bar app alive; elsewhere nothing would be left to reopen it from.
    pub fn stays_in_menu_bar(&self) -> bool {
        cfg!(target_os = "macos") && self.menu_bar_icon
    }
}

/// What Auto means for "Jobs at once": one job per CPU core.
pub fn auto_concurrency() -> usize {
    std::thread::available_parallelism().map_or(4, std::num::NonZero::get)
}

/// Writes `bytes` to a sibling temp file, then renames it over `path`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/settings.toml");
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());
        let mut s = Settings {
            output_dir: Some(dir.path().join("out dir")),
            concurrency: Some(2),
            notifications: false,
            first_run_done: true,
            license_checked: Some("2026-10-05".into()),
            ..Settings::default()
        };
        s.defaults.set(Kind::Images, format_by_id("webp").unwrap());
        s.defaults.set(Kind::Photos, format_by_id("png").unwrap());
        s.automations[0].enabled = false;
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), s);
        assert_eq!(s.concurrency(), 2);
        assert!(matches!(s.output(), Output::Dir(_)));

        // Unknown keys, such as `account` from before desktop sign-in and the
        // daily `update_checked`, are ignored.
        std::fs::write(
            &path,
            "notifications = false\nfuture_key = 1\naccount = \"fixture-account\"\n\
             update_checked = \"2026-10-05\"\n",
        )
        .unwrap();
        let s = Settings::load(&path).unwrap();
        assert!(!s.notifications && s.output_dir.is_none());
        assert_eq!(s.update_checked_at, None);
        assert!(matches!(s.output(), Output::Beside));
        assert_eq!(s.defaults, Defaults::default());
        assert!(!s.first_run_done && s.menu_bar_icon);
        assert_eq!(Settings::default().concurrency(), auto_concurrency());

        std::fs::write(&path, "concurrency = \"lots\"").unwrap();
        assert!(Settings::load(&path).is_err());
    }

    #[test]
    fn only_macos_stays_running_for_the_menu_bar() {
        let on = Settings::default();
        assert_eq!(on.stays_in_menu_bar(), cfg!(target_os = "macos"));
        let off = Settings {
            menu_bar_icon: false,
            ..Settings::default()
        };
        assert!(!off.stays_in_menu_bar());
    }

    #[test]
    fn a_saved_images_default_is_not_replaced_by_the_split() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "[defaults]\nimages = \"webp\"\nvideo = \"mp4\"\n").unwrap();
        let s = Settings::load(&path).unwrap();
        assert_eq!(s.defaults.images, "webp");
        assert_eq!(s.defaults.photos, "webp");
        assert_eq!(s.defaults.video, "mp4");

        std::fs::write(&path, "[defaults]\nphotos = \"jpeg\"\nimages = \"png\"\n").unwrap();
        let s = Settings::load(&path).unwrap();
        assert_eq!(s.defaults.photos, "jpeg");
        assert_eq!(s.defaults.images, "png");
    }

    #[test]
    fn screenshot_named_photos_use_the_images_default() {
        let heic = format_by_id("heic").unwrap();
        let png = format_by_id("png").unwrap();
        assert_eq!(Kind::of(heic), Some(Kind::Photos));
        assert_eq!(Kind::of(png), Some(Kind::Images));
        assert_eq!(
            Kind::of_file(Path::new("IMG_2041.heic"), heic),
            Some(Kind::Photos)
        );
        assert_eq!(
            Kind::of_file(Path::new("Screenshot 1.heic"), heic),
            Some(Kind::Images)
        );
    }
}
