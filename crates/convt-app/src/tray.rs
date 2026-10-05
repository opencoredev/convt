//! What the menu bar (tray) icon shows. While conversions run, the icon shows
//! a small passive spinner: no percentage and no highlight. Clicking it, or
//! dropping a file on it, opens the popover (`ui::open_popover`).
//!
//! GPUI has no tray or status-item API on any platform yet, so nothing draws
//! the icon: this module decides what it would show, and tests check that.
//! Wire it up here once GPUI can put an icon in the menu bar.

use crate::model::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicator {
    /// The plain convt icon.
    Idle,
    /// The icon with a passive spinner.
    Busy,
}

/// The icon to show, or `None` when the user turned the menu bar icon off.
#[cfg_attr(not(test), allow(dead_code))]
pub fn indicator(state: &AppState) -> Option<Indicator> {
    if !state.settings.menu_bar_icon {
        return None;
    }
    Some(if state.queue.active() > 0 {
        Indicator::Busy
    } else {
        Indicator::Idle
    })
}
