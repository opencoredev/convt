//! The app menus. On macOS they fill the menu bar: convt, File, Edit, Window
//! and Help, with the usual shortcuts. Other platforms have no app menu bar,
//! so there the menus aren't installed; the actions behind them are
//! registered everywhere, the windows offer the same things, and Quit and
//! Close Window keep their shortcuts (Ctrl+Q and Ctrl+W).

use gpui_kit::component::input;
use gpui_kit::*;

use super::{SettingsTab, SettingsView};
use crate::model;

actions!(
    convt,
    [
        About,
        CheckForUpdates,
        OpenSettings,
        Hide,
        HideOthers,
        ShowAll,
        Quit,
        AddFiles,
        CloseWindow,
        Minimize,
        Zoom,
        ShowActivity,
        OpenHelp,
        OpenReleaseNotes,
        ContactSupport,
    ]
);

pub const DOCS_URL: &str = "https://convt.app/docs";
pub const SUPPORT_URL: &str = "https://convt.app/contact";
pub const CHANGELOG_URL: &str = "https://convt.app/changelog";
pub const SOURCE_URL: &str = "https://github.com/opencoredev/convt";

/// The menu bar, left to right.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn menus() -> Vec<Menu> {
    vec![
        Menu::new("convt").items([
            MenuItem::action("About convt", About),
            MenuItem::separator(),
            MenuItem::action("Check for Updates…", CheckForUpdates),
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Hide convt", Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action("Quit convt", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("Add Files…", AddFiles),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        // The text fields handle these; with no field focused they're dimmed.
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", input::Undo, OsAction::Undo),
            MenuItem::os_action("Redo", input::Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", input::Cut, OsAction::Cut),
            MenuItem::os_action("Copy", input::Copy, OsAction::Copy),
            MenuItem::os_action("Paste", input::Paste, OsAction::Paste),
            MenuItem::os_action("Select All", input::SelectAll, OsAction::SelectAll),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", Zoom),
            MenuItem::separator(),
            MenuItem::action("Activity", ShowActivity),
        ]),
        Menu::new("Help").items([
            MenuItem::action("convt Help", OpenHelp),
            MenuItem::action("Release Notes", OpenReleaseNotes),
            MenuItem::separator(),
            MenuItem::action("Contact Support", ContactSupport),
        ]),
    ]
}

/// Quit and Close Window: ⌘Q and ⌘W on macOS, Ctrl+Q and Ctrl+W elsewhere.
/// The windows bind neither, so nothing else ever answers them.
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-w", CloseWindow, None),
    ]
}

/// The other shortcuts macOS users expect. Copy, Paste and the rest are
/// bound by the text fields themselves.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn mac_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-o", AddFiles, None),
        KeyBinding::new("cmd-m", Minimize, None),
    ]
}

/// Registers what the menu items do and binds Quit and Close Window; on
/// macOS, also the other shortcuts and the menu bar.
pub fn init(cx: &mut App) {
    on::<About>(super::show_about, cx);
    on::<CheckForUpdates>(check_for_updates, cx);
    on::<OpenSettings>(|cx| super::show_settings(SettingsTab::General, cx), cx);
    on::<Hide>(|cx| cx.hide(), cx);
    on::<HideOthers>(|cx| cx.hide_other_apps(), cx);
    on::<ShowAll>(|cx| cx.unhide_other_apps(), cx);
    on::<Quit>(crate::menu::quit, cx);
    on::<AddFiles>(super::add_files, cx);
    on::<CloseWindow>(|cx| on_active_window(cx, |w| w.remove_window()), cx);
    on::<Minimize>(|cx| on_active_window(cx, |w| w.minimize_window()), cx);
    on::<Zoom>(|cx| on_active_window(cx, |w| w.zoom_window()), cx);
    on::<ShowActivity>(super::show_main, cx);
    on::<OpenHelp>(|cx| cx.open_url(DOCS_URL), cx);
    on::<OpenReleaseNotes>(|cx| cx.open_url(CHANGELOG_URL), cx);
    on::<ContactSupport>(|cx| cx.open_url(SUPPORT_URL), cx);
    cx.bind_keys(key_bindings());
    #[cfg(target_os = "macos")]
    {
        cx.bind_keys(mac_key_bindings());
        cx.set_menus(menus());
    }
}

/// Runs `f` for action `A` once the dispatch is over. Menu items and
/// shortcuts are dispatched through the key window, which can't be brought
/// forward, closed or zoomed until then.
fn on<A: Action>(f: fn(&mut App), cx: &mut App) {
    cx.on_action(move |_: &A, cx| cx.defer(f));
}

fn on_active_window(cx: &mut App, f: impl FnOnce(&mut Window)) {
    if let Some(window) = cx.active_window() {
        let _ = window.update(cx, |_, window, _| f(window));
    }
}

/// Check for Updates…: checks now, whether or not automatic checks are on,
/// and shows Settings at the Updates card, where the result appears.
pub fn check_for_updates(cx: &mut App) {
    super::show_settings(SettingsTab::General, cx);
    if let Some((handle, view)) = super::Open::<SettingsView>::get(cx) {
        let _ = handle.update(cx, |_, _, cx| view.update(cx, |v, cx| v.reveal_updates(cx)));
    }
    model::shared(cx).update(cx, |s, cx| s.check_updates(cx));
}
