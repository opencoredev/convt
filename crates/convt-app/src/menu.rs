//! Quitting. Quit (⌘Q on macOS, Ctrl+Q elsewhere, bound in `ui::menus`)
//! works from every window and from the tray menu, and asks nothing first. While
//! an update installs, these quits wait for the install to finish. A quit
//! from the Dock, a logout or a shutdown goes through AppKit, which GPUI
//! gives no way to delay, so the install itself never leaves convt half
//! replaced (`update/install.rs` swaps the bundle in one step).

use gpui_kit::{App, Global};

/// Quits now, whatever is open or running, unless an update is installing:
/// then the quit happens when the install ends. The quit observers
/// (`thumbs`) clean up. Every quit convt starts itself comes through here.
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
