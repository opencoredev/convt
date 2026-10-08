//! Quitting, and the app menu. Quit (⌘Q on macOS, Ctrl+Q elsewhere) works
//! from every window and from the tray menu, and asks nothing first. While
//! an update installs, every quit waits for the install to finish, so convt
//! is never left half replaced. macOS also gets the menu bar menus: convt,
//! Edit and Window.

use gpui_kit::{App, Global, KeyBinding, Menu, MenuItem, OsAction, SystemMenuType, actions};

use crate::ui::{self, SettingsTab};

actions!(convt, [Quit, OpenSettings, Hide, CloseWindow]);

/// Binds the keys and, on macOS, sets the menu bar menus.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| quit(cx));
    cx.on_action(|_: &OpenSettings, cx| ui::show_settings(SettingsTab::General, cx));
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &CloseWindow, cx| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
    });
    if cfg!(target_os = "macos") {
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-,", OpenSettings, None),
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("cmd-w", CloseWindow, None),
        ]);
    } else {
        cx.bind_keys([KeyBinding::new("ctrl-q", Quit, None)]);
    }
    #[cfg(target_os = "macos")]
    cx.set_menus(menus());
}

/// Quits now, whatever is open or running, unless an update is installing:
/// then the quit happens when the install ends. The quit observers
/// (`thumbs`) clean up. Every way of quitting comes through here.
pub fn quit(cx: &mut App) {
    if let Some(install) = cx.try_global::<Install>()
        && install.running
    {
        cx.global_mut::<Install>().quit = true;
        return;
    }
    #[cfg(test)]
    {
        cx.default_global::<Quits>().0 += 1;
    }
    cx.quit();
}

/// Whether an update is installing, and whether someone asked to quit
/// meanwhile.
#[derive(Default)]
struct Install {
    running: bool,
    quit: bool,
}

impl Global for Install {}

/// An update install starts: quits wait until [`install_ended`].
pub fn install_started(cx: &mut App) {
    *cx.default_global::<Install>() = Install {
        running: true,
        quit: false,
    };
}

/// The install ended. Returns whether a quit was asked for while it ran.
pub fn install_ended(cx: &mut App) -> bool {
    std::mem::take(cx.default_global::<Install>()).quit
}

/// How often the app asked to quit. The test platform's quit does nothing,
/// so tests read this instead.
#[cfg(test)]
#[derive(Default)]
pub struct Quits(pub usize);

#[cfg(test)]
impl gpui_kit::Global for Quits {}

/// The menu bar menus. Built everywhere so tests check them; only macOS
/// shows them.
#[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
fn menus() -> Vec<Menu> {
    use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
    vec![
        Menu::new("convt").items([
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Hide convt", Hide),
            MenuItem::separator(),
            MenuItem::action("Quit convt", Quit),
        ]),
        // The text fields (license key, file names) handle these.
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", Undo, OsAction::Undo),
            MenuItem::os_action("Redo", Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", Cut, OsAction::Cut),
            MenuItem::os_action("Copy", Copy, OsAction::Copy),
            MenuItem::os_action("Paste", Paste, OsAction::Paste),
            MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
        ]),
        Menu::new("Window").items([MenuItem::action("Close", CloseWindow)]),
    ]
}

#[cfg(test)]
mod tests {
    use gpui_kit::MenuItem;

    #[test]
    fn the_app_menu_quits_and_opens_settings() {
        let menus = super::menus();
        let names: Vec<_> = menus.iter().map(|m| m.name.to_string()).collect();
        assert_eq!(names, ["convt", "Edit", "Window"]);
        let actions: Vec<_> = menus[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { name, action, .. } => Some((name.to_string(), action.name())),
                _ => None,
            })
            .collect();
        assert!(actions.contains(&("Quit convt".into(), "convt::Quit")));
        assert!(actions.contains(&("Settings…".into(), "convt::OpenSettings")));
    }
}
