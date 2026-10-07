//! App settings, stored as `settings.toml` in the convt config directory.
//! Unknown keys are kept out of the way rather than rejected, so an older app
//! can read a newer file.

use std::io;
use std::path::{Path, PathBuf};

use convt_core::{Category, Format, Output, format_by_id};
use serde::{Deserialize, Serialize};

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
    /// Check convt.app for a newer build at launch and every few hours. On
    /// by default. Check now works either way.
    pub update_checks: bool,
    /// When (Unix seconds) an update check last got an answer that checked
    /// out. Older files kept the day in `update_checked`, which is ignored.
    pub update_checked_at: Option<u64>,
    /// The highest update manifest `sequence` accepted, so an older signed
    /// manifest can't be replayed to hide a newer release.
    pub update_sequence: u64,
    /// What Add files converts each kind of file to.
    pub defaults: Defaults,
    /// Automation rules. Only stored for now: nothing runs them yet.
    pub automations: Vec<Automation>,
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
            update_checks: true,
            update_checked_at: None,
            update_sequence: 0,
            defaults: Defaults::default(),
            automations: crate::placeholder::example_automations(),
        }
    }
}

/// The format Add files picks for each kind of file, by format id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Defaults {
    pub images: String,
    pub video: String,
    pub audio: String,
    pub documents: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            images: "webp".into(),
            video: "mp4".into(),
            audio: "mp3".into(),
            documents: "pdf".into(),
        }
    }
}

/// A kind of file with its own default target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Images,
    Video,
    Audio,
    Documents,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Images, Kind::Video, Kind::Audio, Kind::Documents];

    /// The kind a file of `format` belongs to. PDFs have none: they are
    /// already what documents become.
    pub fn of(format: &Format) -> Option<Kind> {
        match format.category {
            Category::Image | Category::Vector => Some(Kind::Images),
            Category::Video => Some(Kind::Video),
            Category::Audio => Some(Kind::Audio),
            Category::Document | Category::Presentation | Category::Spreadsheet => {
                Some(Kind::Documents)
            }
            Category::Pdf => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Images => "Images",
            Kind::Video => "Video",
            Kind::Audio => "Audio",
            Kind::Documents => "Documents",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Kind::Images => "images",
            Kind::Video => "video",
            Kind::Audio => "audio",
            Kind::Documents => "documents",
        }
    }
}

impl Defaults {
    pub fn get(&self, kind: Kind) -> Option<&'static Format> {
        format_by_id(match kind {
            Kind::Images => &self.images,
            Kind::Video => &self.video,
            Kind::Audio => &self.audio,
            Kind::Documents => &self.documents,
        })
    }

    pub fn set(&mut self, kind: Kind, to: &Format) {
        let slot = match kind {
            Kind::Images => &mut self.images,
            Kind::Video => &mut self.video,
            Kind::Audio => &mut self.audio,
            Kind::Documents => &mut self.documents,
        };
        *slot = to.id.to_string();
    }
}

/// A rule such as "Screenshots on the Desktop become WebP".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Automation {
    /// What the rule watches, e.g. "Screenshots".
    pub name: String,
    /// Target format id.
    pub to: String,
    /// Where it looks, e.g. "Desktop".
    pub source: String,
    /// What else it does, e.g. "copy to clipboard".
    pub detail: String,
    pub enabled: bool,
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
        s.defaults.set(Kind::Images, format_by_id("png").unwrap());
        s.automations[0].enabled = false;
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), s);
        assert_eq!(s.concurrency(), 2);
        assert!(matches!(s.output(), Output::Dir(_)));

        // Unknown keys, such as `account` from before desktop sign-in and the
        // daily `update_checked`, are ignored.
        std::fs::write(
            &path,
            "notifications = false\nfuture_key = 1\naccount = \"a@b.c\"\n\
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
}
