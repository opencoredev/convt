//! Placeholder values. Nothing here is real yet: the update manifest URL
//! waits for the release host, and the example automation rules stand in
//! until the automation engine exists. Keep every such value in this
//! module so it is easy to find and replace.

use crate::settings::Automation;

/// Where the signed update manifest lives. Placeholder: nothing is published
/// there yet (P11 sets up the release host). Builds from source can point at a
/// local server with `CONVT_UPDATE_URL`.
pub const UPDATE_MANIFEST_URL: &str = "https://convt.app/updates/manifest.json";

/// Example rules a fresh install lists under Automations, matching the
/// design. Placeholder: the automation engine is not built, so these rules
/// are stored and can be switched on and off, but nothing runs them.
pub fn example_automations() -> Vec<Automation> {
    let rule = |name: &str, to: &str, source: &str, detail: &str, enabled| Automation {
        name: name.into(),
        to: to.into(),
        source: source.into(),
        detail: detail.into(),
        enabled,
    };
    vec![
        rule("Screenshots", "webp", "Desktop", "copy to clipboard", true),
        rule("Exports", "mp4", "~/Movies/Exports", "H.264 1080p", true),
        rule("HEIC", "jpeg", "Downloads", "keep original", false),
    ]
}
