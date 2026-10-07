//! Finder extension setup: the System Settings URL and whether the
//! extension is on. The first-run window, Activity and Settings share this
//! so a skipped or closed setup can still be finished.

use std::time::Duration;

use gpui_kit::{AppContext, Context, Task};

use crate::model::AppState;

/// Opens Login Items & Extensions in macOS System Settings. Extensions sit
/// below Login Items; the user has to scroll.
pub const EXTENSION_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.LoginItems-Settings.extension";

/// Whether the Finder extension is switched on. `None` when this platform
/// has no Finder extension, or macOS does not know it (the app is not in a
/// bundle, or pluginkit failed).
pub fn enabled() -> Option<bool> {
    #[cfg(target_os = "macos")]
    return crate::macos::finder_extension_enabled();
    #[cfg(not(target_os = "macos"))]
    None
}

/// Polls [`enabled`] so Activity and Settings update when the user comes
/// back from System Settings.
pub fn watch(cx: &mut Context<AppState>) -> Task<()> {
    cx.spawn(async move |this, cx| {
        loop {
            let on = cx.background_spawn(async { enabled() }).await;
            if this
                .update(cx, |s, cx| {
                    if s.finder_on != on {
                        s.finder_on = on;
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
            cx.background_executor().timer(Duration::from_secs(1)).await;
        }
    })
}
