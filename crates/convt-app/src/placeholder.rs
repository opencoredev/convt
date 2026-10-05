//! Placeholder values. Nothing here is real yet: the sign-in pages, the link
//! parameters they send back and the example automation rules stand in until
//! convt.app and the automation engine exist. Keep every such value in this
//! module so it is easy to find and replace.

use crate::settings::Automation;

/// Where the sign-in buttons send the browser. Placeholder: convt.app has no
/// desktop sign-in page yet. The page is expected to finish by opening
/// `convt://signin?email=<address>` (see `request::parse_url`).
pub const SIGN_IN_URL: &str = "https://convt.app/signin?client=desktop";

/// The sign-in methods the first-run window offers, as (button id, label,
/// `method` query value). Placeholder: Google and Apple sign-in are not set up.
pub const SIGN_IN_METHODS: [(&str, &str, &str); 3] = [
    ("sign-in-email", "Email", "email"),
    ("sign-in-google", "Google", "google"),
    ("sign-in-apple", "Apple", "apple"),
];

/// The sign-in page for one method.
pub fn sign_in_url(method: &str) -> String {
    format!("{SIGN_IN_URL}&method={method}")
}

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
