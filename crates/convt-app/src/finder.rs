//! Finder extension setup: the System Settings URL and whether the
//! extension is on. The first-run window, Activity and Settings share this
//! so a skipped or closed setup can still be finished.

use std::collections::BTreeMap;
use std::time::Duration;

use convt_core::{FORMATS, Registry};
use gpui_kit::{AppContext, Context, Task};
use serde::Serialize;

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

/// What the Finder extension reads: each format's right-click targets.
#[derive(Serialize)]
pub struct TargetList<'a> {
    pub version: u32,
    /// Lowercase extension to format id.
    pub extensions: BTreeMap<&'a str, &'a str>,
    /// Format id to the formats it converts to, in menu order.
    pub targets: BTreeMap<&'a str, Vec<Target<'a>>>,
}

#[derive(Serialize)]
pub struct Target<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub category: String,
}

/// The menu for every format `registry` gives targets. Pass
/// `AppState::menu_registry`, so documents keep their menu while the
/// document pack isn't installed: a click on one opens Quick convert with
/// Install & Convert.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
pub fn target_list(registry: &Registry) -> TargetList<'static> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_planned_registry_gives_documents_their_menu() {
        let list = target_list(&convt_engines::registry_with_planned_documents());
        for (ext, menu) in [
            ("docx", &["pdf", "txt"][..]),
            ("xlsx", &["pdf", "csv"]),
            ("pptx", &["pdf"]),
            ("odt", &["pdf", "docx"]),
        ] {
            let id = list.extensions[ext];
            let targets: Vec<_> = list.targets[id].iter().map(|t| t.id).collect();
            for want in menu {
                assert!(targets.contains(want), "{ext}: {targets:?}");
            }
        }
    }
}
