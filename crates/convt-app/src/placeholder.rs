//! Placeholder values. Nothing here is real yet except the example
//! automation rules, which a fresh install stores and the automation
//! engine now runs. The update manifest URL still waits for the release
//! host. Keep every such value in this module so it is easy to find.

use crate::settings::{Automation, WatchKind};

/// Where the signed update manifest lives. Placeholder: nothing is published
/// there yet (P11 sets up the release host). Builds from source can point at a
/// local server with `CONVT_UPDATE_URL`.
pub const UPDATE_MANIFEST_URL: &str = "https://convt.app/updates/manifest.json";

/// Example rules a fresh install lists under Automations. Existing
/// `settings.toml` files keep the rules they already saved.
pub fn example_automations() -> Vec<Automation> {
    vec![
        Automation {
            name: "Screenshots".into(),
            to: "png".into(),
            source: "Screenshots".into(),
            detail: "copy to clipboard".into(),
            enabled: true,
            watch: Some(WatchKind::Screenshot),
            folder: None,
            copy_to_clipboard: Some(true),
        },
        Automation {
            name: "Screen recordings".into(),
            to: "mp4".into(),
            source: "Screen recordings".into(),
            detail: "save beside original".into(),
            enabled: true,
            watch: Some(WatchKind::Recording),
            folder: None,
            copy_to_clipboard: Some(false),
        },
        Automation {
            name: "HEIC".into(),
            to: "jpeg".into(),
            source: "Downloads".into(),
            detail: "keep original".into(),
            enabled: false,
            watch: Some(WatchKind::Folder),
            folder: None,
            copy_to_clipboard: Some(false),
        },
    ]
}
