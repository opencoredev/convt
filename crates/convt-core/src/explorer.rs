//! Static Explorer cascade verbs. The Windows MSI registers these under
//! `SystemFileAssociations\<ext>\shell` so "Show more options" works without
//! a signed sparse package. Targets are the same preferred list the other
//! file-manager menus use; the source format itself stays in Quick convert.

use crate::formats::{Category, FORMATS, Format, format_by_id};

/// Registry key for the cascade. Stable so upgrades replace the same verb.
pub const EXPLORER_VERB_ID: &str = "ConvertWithConvt";
/// Label shown on the cascade in Explorer.
pub const EXPLORER_VERB_LABEL: &str = "Convert with Convt";

/// One input extension and the submenu targets the installer registers for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerAssociation {
    pub extension: &'static str,
    pub source: &'static Format,
    pub targets: Vec<&'static Format>,
}

/// Preferred right-click targets for `from`, most wanted first, not yet
/// filtered by what this machine can reach. The source format is omitted so
/// a PNG menu offers JPEG and WebP, matching [`Registry::menu_targets`].
pub fn menu_preferred(from: &Format) -> &'static [&'static str] {
    match (from.id, from.category) {
        ("gif", _) => &["mp4", "webp", "png"],
        (_, Category::Image) => &["jpeg", "png", "webp"],
        (_, Category::Vector) => &["png", "jpeg", "pdf"],
        (_, Category::Video) => &["mp4", "mov", "gif", "mp3"],
        (_, Category::Audio) => &["mp3", "m4a", "wav"],
        (_, Category::Pdf) => &["png", "jpeg", "docx"],
        (_, Category::Document) => &["pdf", "docx", "txt"],
        (_, Category::Spreadsheet) => &["pdf", "xlsx", "csv"],
        (_, Category::Presentation) => &["pdf", "pptx"],
    }
}

/// Common submenu targets for a static Explorer verb: preferred formats
/// except the input's own id.
pub fn explorer_targets(from: &Format) -> Vec<&'static Format> {
    menu_preferred(from)
        .iter()
        .filter(|id| **id != from.id)
        .filter_map(|id| format_by_id(id))
        .collect()
}

/// Every supported input extension and the cascade entries to register.
pub fn explorer_associations() -> Vec<ExplorerAssociation> {
    let mut out = Vec::new();
    for source in FORMATS {
        let targets = explorer_targets(source);
        for extension in source.extensions {
            out.push(ExplorerAssociation {
                extension,
                source,
                targets: targets.clone(),
            });
        }
    }
    out
}

/// Registry path under `Software\Classes` for one cascade (no hive).
pub fn explorer_verb_key(extension: &str) -> String {
    format!("Software\\Classes\\SystemFileAssociations\\.{extension}\\shell\\{EXPLORER_VERB_ID}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format_by_id;

    #[test]
    fn every_format_extension_has_a_cascade() {
        let associations = explorer_associations();
        for format in FORMATS {
            for extension in format.extensions {
                assert!(
                    associations
                        .iter()
                        .any(|a| a.extension == *extension && a.source.id == format.id),
                    "missing .{extension}"
                );
            }
        }
        assert_eq!(
            associations.len(),
            FORMATS.iter().map(|f| f.extensions.len()).sum::<usize>()
        );
    }

    #[test]
    fn png_offers_jpeg_and_webp_not_itself() {
        let png = format_by_id("png").unwrap();
        let ids: Vec<_> = explorer_targets(png).iter().map(|f| f.id).collect();
        assert_eq!(ids, ["jpeg", "webp"]);
    }

    #[test]
    fn gif_keeps_its_video_and_still_targets() {
        let gif = format_by_id("gif").unwrap();
        let ids: Vec<_> = explorer_targets(gif).iter().map(|f| f.id).collect();
        assert_eq!(ids, ["mp4", "webp", "png"]);
    }

    #[test]
    fn verb_key_uses_system_file_associations() {
        assert_eq!(
            explorer_verb_key("png"),
            "Software\\Classes\\SystemFileAssociations\\.png\\shell\\ConvertWithConvt"
        );
    }

    #[test]
    fn windows_wxs_registers_every_association() {
        let wxs = include_str!("../../../packaging/windows/explorer-verbs.wxs");
        assert!(wxs.contains("Root=\"HKMU\""));
        assert!(wxs.contains("convt-app.exe"));
        assert!(!wxs.replace("convt-app.exe", "").contains("convt.exe"));
        assert!(wxs.contains("Id=\"SendToShortcut\""));
        assert!(wxs.contains("More options…"));
        for association in explorer_associations() {
            let key = explorer_verb_key(association.extension);
            assert!(wxs.contains(&key), "{key}");
            for (index, target) in association.targets.iter().enumerate() {
                let sub = format!("{key}\\shell\\{:02}_{}", (index + 1) * 10, target.id);
                assert!(wxs.contains(&sub), "{sub}");
                assert!(
                    wxs.contains(&format!("open --to {} --", target.id)),
                    "{}",
                    target.id
                );
            }
        }
    }
}
