//! The document pack: LibreOffice as a one-click add-on. Images, video,
//! audio, PDF and SVG work out of the box; Word, Excel, PowerPoint and
//! OpenDocument files need the pack. Status checks never touch the network.
//! The only download starts from the user's click on Download (see
//! `AppState::download_pack`), and runs through [`Backend::install`].

use std::path::{Path, PathBuf};

use convt_core::{Category, Format, Registry};
use convt_engines::packs;

/// What is installed, checked offline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Installed(PathBuf),
    NotInstalled,
    /// A pack is on disk but this build won't run it. The reason is the
    /// engines' own, for details; [`plain_reason`] words it for people.
    Rejected(String),
}

/// What this build can download: whether it has a pinned pack at all, and
/// the pinned sizes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Offer {
    pub configured: bool,
    pub download: Option<u64>,
    pub installed: Option<u64>,
    pub destination: Option<PathBuf>,
}

/// How far an install has got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    Download { bytes: u64, total: Option<u64> },
    Verifying,
    Installing,
}

/// What kind of failure an install was, from the engines: network, HTTP
/// status, permission, disk full, checksum, rejected folder, busy,
/// cancelled or other.
pub use packs::FailureKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    /// The engines' message, for details.
    pub message: String,
}

/// The pack operations the app uses. [`Engines`] is the real one; tests use
/// their own so they never download, install or read the user's data folder.
pub trait Backend: Send + Sync {
    fn offer(&self) -> Offer;
    fn status(&self) -> Status;
    /// Downloads, verifies and installs the pinned pack. Runs on a worker
    /// thread, and only after the user clicked Download.
    fn install(
        &self,
        progress: &dyn Fn(Progress),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<PathBuf, Failure>;
    fn remove(&self) -> Result<(), String>;
    /// The registry every conversion uses, rebuilt after an install or a
    /// removal so new targets show up.
    fn registry(&self) -> Registry;
    /// The registry while the pack is being removed: everything but
    /// documents, so nothing starts LibreOffice from a folder being deleted.
    fn registry_without_documents(&self) -> Registry;
}

/// The pack API in `convt-engines`, with the source pinned into this build.
pub struct Engines;

impl Backend for Engines {
    fn offer(&self) -> Offer {
        let sizes = packs::documents_sizes();
        Offer {
            configured: packs::documents_configured(),
            download: sizes.download,
            installed: sizes.installed,
            destination: packs::documents_dir(),
        }
    }

    fn status(&self) -> Status {
        match packs::documents_status() {
            Ok(exe) => Status::Installed(exe),
            Err(error) => {
                let pointer = packs::documents_dir().map(|d| d.join("current"));
                match pointer.map(|p| std::fs::symlink_metadata(p).is_ok()) {
                    Some(true) => Status::Rejected(format!("{error:#}")),
                    _ => Status::NotInstalled,
                }
            }
        }
    }

    fn install(
        &self,
        progress: &dyn Fn(Progress),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<PathBuf, Failure> {
        // The engines make convt's data folder private first, after checking
        // the folders above it (`packs::secure_data_dir`).
        packs::install_documents_cancellable(
            &packs::documents_source(),
            &|event| match event {
                packs::Progress::Download { bytes, total } => {
                    progress(Progress::Download { bytes, total })
                }
                packs::Progress::Verifying => progress(Progress::Verifying),
                packs::Progress::Extracting => progress(Progress::Installing),
                packs::Progress::Installed(_) => {}
            },
            cancelled,
        )
        .map_err(|error| Failure {
            kind: packs::failure_kind(&error),
            message: format!("{error:#}"),
        })
    }

    fn remove(&self) -> Result<(), String> {
        packs::remove_documents().map_err(|e| format!("{e:#}"))
    }

    fn registry(&self) -> Registry {
        convt_engines::default_registry()
    }

    fn registry_without_documents(&self) -> Registry {
        convt_engines::registry_without_documents()
    }
}

/// Whether files of `format` are what the document pack converts: Word,
/// Excel, PowerPoint, OpenDocument, RTF, text, HTML and CSV.
pub fn is_document(format: &Format) -> bool {
    matches!(
        format.category,
        Category::Document | Category::Presentation | Category::Spreadsheet
    )
}

/// Whether `file` is a document this registry can't convert yet, so the
/// document pack would help.
pub fn needs_pack(registry: &Registry, file: &Path) -> bool {
    convt_core::format_by_extension(file)
        .is_some_and(|f| is_document(f) && registry.targets(f).is_empty())
}

/// Why an installed pack was rejected, for people. The engines' message
/// goes underneath as the detail.
pub fn plain_reason(reason: &str) -> &'static str {
    if reason.contains("writable") || reason.contains("owned by") {
        "Other users on this computer could change its files, so convt won't run it."
    } else if reason.contains("symlink") {
        "It contains a link to files outside the pack, so convt won't run it."
    } else if reason.contains("pin") || reason.contains("digest") || reason.contains("receipt") {
        "It's a different version from the one this convt expects."
    } else if reason.contains("not executable")
        || reason.contains("regular file")
        || reason.contains("unavailable")
    {
        "It's incomplete or damaged."
    } else {
        "It was changed after it was installed, so convt won't run it."
    }
}

/// What a failed install means, for people: a title and what to do. The
/// engines' message goes underneath as the detail.
pub fn plain_failure(failure: &Failure, offer: &Offer) -> (String, String) {
    let resumes = "The download picks up where it stopped.";
    let (title, body) = match failure.kind {
        FailureKind::Cancelled => (
            "Download stopped".to_string(),
            "Downloading again picks up where it stopped.".to_string(),
        ),
        FailureKind::Network => (
            "Couldn't download document support".to_string(),
            format!("convt couldn't reach the download. Check your internet connection and try again. {resumes}"),
        ),
        FailureKind::HttpStatus(status) => (
            "The download server had a problem".to_string(),
            format!("It answered with an error (HTTP {status}). Try again later. {resumes}"),
        ),
        FailureKind::Permission => (
            "convt can't write where document support goes".to_string(),
            "It isn't allowed to write to the folder in the details below. Nothing was installed.".to_string(),
        ),
        FailureKind::DiskFull => (
            "Not enough disk space".to_string(),
            match offer.download.zip(offer.installed) {
                Some((download, installed)) => format!(
                    "Document support needs about {} free while it installs. Free up some space and try again. {resumes}",
                    crate::ui::human_size(download + installed)
                ),
                None => format!("Free up some space and try again. {resumes}"),
            },
        ),
        FailureKind::Checksum => (
            "The download didn't check out".to_string(),
            "It didn't match the checksum this version of convt expects, so it was deleted. Nothing was installed or run.".to_string(),
        ),
        FailureKind::Rejected => (
            "Couldn't install document support".to_string(),
            "Other users on this computer could change a folder it would go in, so convt won't install it there. The details say which folder.".to_string(),
        ),
        FailureKind::Busy => (
            "Document support is busy".to_string(),
            "Another convt is installing or removing it. Try again in a moment.".to_string(),
        ),
        FailureKind::Other => (
            "Couldn't install document support".to_string(),
            "Something went wrong; the details below say what. Try again.".to_string(),
        ),
    };
    (title, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_failure_kind_has_its_own_words() {
        let offer = Offer {
            configured: true,
            download: Some(150_000_000),
            installed: Some(410_000_000),
            destination: None,
        };
        let kinds = [
            FailureKind::Network,
            FailureKind::HttpStatus(503),
            FailureKind::Permission,
            FailureKind::DiskFull,
            FailureKind::Checksum,
            FailureKind::Rejected,
            FailureKind::Busy,
            FailureKind::Cancelled,
            FailureKind::Other,
        ];
        let words: Vec<_> = kinds
            .iter()
            .map(|&kind| {
                plain_failure(
                    &Failure {
                        kind,
                        message: String::new(),
                    },
                    &offer,
                )
            })
            .collect();
        for (i, a) in words.iter().enumerate() {
            for b in &words[i + 1..] {
                assert_ne!(a.1, b.1);
            }
        }
        let say = |kind| words[kinds.iter().position(|k| *k == kind).unwrap()].clone();
        // Only the network gets network advice; local failures don't.
        assert!(say(FailureKind::Network).1.contains("internet connection"));
        for local in [
            FailureKind::Permission,
            FailureKind::DiskFull,
            FailureKind::Other,
        ] {
            assert!(!say(local).1.contains("internet"), "{local:?}");
            assert!(!say(local).1.contains("Nothing was changed"), "{local:?}");
        }
        assert!(say(FailureKind::HttpStatus(503)).1.contains("HTTP 503"));
        assert!(say(FailureKind::DiskFull).1.contains("560 MB"));
        // The CLI's wording stays in the CLI.
        assert!(
            words
                .iter()
                .all(|(t, b)| !t.contains("convt pack") && !b.contains("convt pack"))
        );
    }

    #[test]
    fn rejection_reasons_read_as_plain_words() {
        for (reason, plain) in [
            (
                "document-pack path must be owned by the current user and not group/world writable: /x",
                "Other users",
            ),
            ("document-pack symlink forbidden: /x/soffice", "link"),
            (
                "current document-pack digest does not match this build's pin",
                "different version",
            ),
            (
                "document-pack receipt does not match this build's pin",
                "different version",
            ),
            ("document-pack launcher is not executable", "damaged"),
        ] {
            assert!(plain_reason(reason).contains(plain), "{reason}");
        }
    }

    #[test]
    fn only_office_formats_need_the_pack() {
        let empty = Registry::new();
        for (name, needs) in [
            ("a.docx", true),
            ("a.xlsx", true),
            ("a.pptx", true),
            ("a.csv", true),
            ("a.pdf", false),
            ("a.png", false),
            ("a.unknown", false),
        ] {
            assert_eq!(needs_pack(&empty, Path::new(name)), needs, "{name}");
        }
    }
}
