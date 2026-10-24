//! Finder extension setup: the System Settings URL, whether the extension
//! is on, and where the guide that walks through turning it on sits. The
//! first-run window, Activity and Settings share this so a skipped or closed
//! setup can still be finished.

use std::collections::BTreeMap;
use std::time::Duration;

use convt_core::{FORMATS, MenuChoices, Registry};
use gpui_kit::{App, AppContext, Context, Task};
use serde::Serialize;

use crate::model::AppState;

/// Opens Login Items & Extensions in macOS System Settings. Extensions sit
/// below Login Items; the user has to scroll.
pub const EXTENSION_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.LoginItems-Settings.extension";

/// Opens System Settings at Login Items & Extensions and, while the
/// extension is off, the guide that sits beside it and shows where the
/// switch is (`ui::finder_guide`). The guide draws the macOS 15 pane, so
/// earlier versions get only System Settings.
pub fn open_settings(cx: &mut App) {
    cx.open_url(EXTENSION_SETTINGS);
    #[cfg(all(target_os = "macos", not(test)))]
    if !crate::system_settings::lists_extensions_in_login_items() {
        return;
    }
    if crate::model::shared(cx).read(cx).finder_on != Some(true) {
        crate::ui::show_finder_guide(cx);
    }
}

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

/// The menu for every format `registry` gives targets, following the lists
/// picked in `menu`. Pass `AppState::menu_registry`, so documents keep their
/// menu while the document pack isn't installed: a click on one opens Quick
/// convert with Install & Convert.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
pub fn target_list(registry: &Registry, menu: &MenuChoices) -> TargetList<'static> {
    let mut list = TargetList {
        version: 1,
        extensions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    for format in FORMATS {
        let targets = registry.menu_targets_with(format, menu);
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

#[cfg(any(target_os = "macos", test))]
/// A rectangle in screen points, measured from the top left of the primary
/// display, as the window server reports windows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[cfg(any(target_os = "macos", test))]
impl Frame {
    fn right(&self) -> f64 {
        self.x + self.w
    }

    fn bottom(&self) -> f64 {
        self.y + self.h
    }

    fn contains_center_of(&self, other: &Frame) -> bool {
        let (cx, cy) = (other.x + other.w / 2., other.y + other.h / 2.);
        cx >= self.x && cx < self.right() && cy >= self.y && cy < self.bottom()
    }
}

#[cfg(any(target_os = "macos", test))]
/// The space between System Settings and the guide.
const DOCK_GAP: f64 = 12.;

#[cfg(any(target_os = "macos", test))]
/// Where the guide's top left corner goes for a System Settings window at
/// `settings`: beside its right edge, top edges level; beside the left edge
/// when the right has no room on that screen; tucked inside its bottom right
/// corner when neither side has room. `screens` are the usable areas (no
/// menu bar or Dock) of every display.
pub fn dock(settings: Frame, guide: (f64, f64), screens: &[Frame]) -> (f64, f64) {
    let (w, h) = guide;
    let screen = screens
        .iter()
        .find(|s| s.contains_center_of(&settings))
        .or(screens.first())
        .copied()
        .unwrap_or(settings);
    let y = settings.y.min(screen.bottom() - h).max(screen.y);
    let right = settings.right() + DOCK_GAP;
    if right + w <= screen.right() {
        return (right, y);
    }
    let left = settings.x - DOCK_GAP - w;
    if left >= screen.x {
        return (left, y);
    }
    let x = (settings.right() - DOCK_GAP - w)
        .min(screen.right() - w)
        .max(screen.x);
    let y = (settings.bottom() - DOCK_GAP - h)
        .min(screen.bottom() - h)
        .max(screen.y);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use convt_core::{Category, format_by_id};

    #[test]
    fn target_list_covers_every_convertible_extension() {
        let list = target_list(&convt_engines::default_registry(), &MenuChoices::default());
        assert_eq!(list.extensions.get("jpg"), Some(&"jpeg"));
        let png = &list.targets["png"];
        assert!(png.iter().any(|t| t.id == "webp" && t.category == "Image"));
        assert!(png.iter().all(|t| t.id != "png"));
        for id in list.extensions.values() {
            assert!(list.targets.contains_key(id));
        }
    }

    #[test]
    fn target_list_follows_the_menu_choices() {
        let mut menu = MenuChoices::default();
        menu.set(
            Category::Image,
            vec![format_by_id("tiff").unwrap(), format_by_id("png").unwrap()],
        );
        let list = target_list(&convt_engines::default_registry(), &menu);
        let ids: Vec<_> = list.targets["jpeg"].iter().map(|t| t.id).collect();
        assert_eq!(ids, ["tiff", "png"]);
    }

    #[test]
    fn the_planned_registry_gives_documents_their_menu() {
        let planned = convt_engines::registry_with_planned_documents();
        let list = target_list(&planned, &MenuChoices::default());
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

        // A list someone picked for documents applies to the pack's targets
        // too, in their order.
        let mut menu = MenuChoices::default();
        menu.set(
            Category::Document,
            vec![format_by_id("rtf").unwrap(), format_by_id("odt").unwrap()],
        );
        let list = target_list(&planned, &menu);
        let ids: Vec<_> = list.targets["docx"].iter().map(|t| t.id).collect();
        assert_eq!(ids, ["rtf", "odt"]);
    }

    const SCREEN: Frame = Frame {
        x: 0.,
        y: 25.,
        w: 1512.,
        h: 900.,
    };

    fn settings(x: f64, y: f64) -> Frame {
        Frame {
            x,
            y,
            w: 740.,
            h: 625.,
        }
    }

    #[test]
    fn the_guide_docks_beside_system_settings() {
        // Room on the right: beside it, tops level.
        assert_eq!(
            dock(settings(400., 200.), (320., 360.), &[SCREEN]),
            (1152., 200.)
        );
        // No room on the right: on the left.
        assert_eq!(
            dock(settings(700., 200.), (320., 360.), &[SCREEN]),
            (368., 200.)
        );
        // Room on neither side: inside the bottom right corner.
        let wide = Frame {
            x: 100.,
            y: 100.,
            w: 1300.,
            h: 700.,
        };
        assert_eq!(dock(wide, (320., 360.), &[SCREEN]), (1068., 428.));
        // Low on the screen: lifted so the guide stays whole.
        assert_eq!(
            dock(settings(400., 700.), (320., 360.), &[SCREEN]),
            (1152., 565.)
        );
    }

    #[test]
    fn the_guide_stays_on_the_display_system_settings_is_on() {
        let second = Frame {
            x: 1512.,
            y: 0.,
            w: 1920.,
            h: 1080.,
        };
        let (x, _) = dock(settings(2300., 200.), (320., 360.), &[SCREEN, second]);
        assert_eq!(x, 3052.);
        // Pushed to the left on the second display, not onto the first.
        let (x, _) = dock(settings(2800., 200.), (320., 360.), &[SCREEN, second]);
        assert_eq!(x, 2468.);
    }
}
