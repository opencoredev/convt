//! The tray icon: the menu bar item on macOS, the notification area icon on
//! Windows, and a StatusNotifierItem on Linux. While "Keep running in the
//! background" is on (`settings.menu_bar_icon`) and the icon is up, closing
//! the last window leaves convt running, so file manager requests start at
//! once. Its menu opens convt, opens Settings, or quits; on Windows and Linux
//! a left click opens convt. While conversions run, the tooltip counts them.
//!
//! The platform icons talk to the app through [`Event`]s on a channel that a
//! GPUI task drains, like the single-instance requests in `main.rs`. Where no
//! icon can be shown (a Linux desktop with no tray, or an error), convt logs
//! it and quits with its last window, as with the setting off.

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui_kit::{App, Entity, Global};

use crate::model::AppState;
use crate::ui::{self, SettingsTab};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicator {
    /// The plain convt icon.
    Idle,
    /// Conversions are running.
    Busy,
}

/// The icon to show, or `None` when the user turned the background setting
/// off.
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

/// "convt", or "convt: converting 3 files" while jobs run.
pub fn tooltip(state: &AppState) -> String {
    match state.queue.active() {
        0 => "convt".into(),
        1 => "convt: converting 1 file".into(),
        n => format!("convt: converting {n} files"),
    }
}

/// What the icon asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Show the main window.
    Open,
    /// Open Settings.
    Settings,
    /// Quit convt.
    Quit,
    /// The icon went away by itself: the Linux tray host stopped.
    #[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
    Lost,
}

/// A tray icon on screen. Dropping it removes the icon.
pub trait Icon {
    fn set_tooltip(&mut self, tooltip: &str);
}

/// Puts an icon on screen that sends its clicks to the channel, or says why
/// it couldn't. [`platform::spawn`] in the app; tests pass their own.
pub type Spawn = fn(UnboundedSender<Event>) -> Result<Box<dyn Icon>, String>;

struct Tray {
    spawn: Spawn,
    events: UnboundedSender<Event>,
    icon: Option<Box<dyn Icon>>,
    tooltip: String,
    /// The last attempt failed. Turning the setting off and on tries again.
    failed: bool,
}

impl Global for Tray {}

/// Shows the icon while the setting is on, and follows the setting and the
/// running jobs from then on.
pub fn init(state: &Entity<AppState>, spawn: Spawn, cx: &mut App) {
    let (events, mut rx) = unbounded();
    cx.set_global(Tray {
        spawn,
        events,
        icon: None,
        tooltip: String::new(),
        failed: false,
    });
    sync(state, cx);
    cx.observe(state, |state, cx| sync(&state, cx)).detach();
    cx.spawn(async move |cx| {
        while let Some(event) = rx.next().await {
            cx.update(|cx| handle(event, cx));
        }
    })
    .detach();
}

/// Whether the icon is on screen.
pub fn shown(cx: &App) -> bool {
    cx.try_global::<Tray>().is_some_and(|t| t.icon.is_some())
}

/// Whether closing the last window leaves convt running: the setting is on
/// and the icon is there to bring it back or quit it. Takes the setting
/// rather than reading the state, so `AppState` can ask mid-update.
pub fn keeps_running(menu_bar_icon: bool, cx: &App) -> bool {
    menu_bar_icon && shown(cx)
}

fn sync(state: &Entity<AppState>, cx: &mut App) {
    let (wanted, tooltip) = {
        let state = state.read(cx);
        (indicator(state).is_some(), tooltip(state))
    };
    let Some(tray) = cx.try_global::<Tray>() else {
        return;
    };
    let unchanged = match &tray.icon {
        Some(_) => wanted && tray.tooltip == tooltip,
        // Nothing to do while off, or after a failed try until it's turned
        // off and on again.
        None => wanted == tray.failed,
    };
    if unchanged {
        return;
    }
    let tray = cx.global_mut::<Tray>();
    if !wanted {
        tray.icon = None;
        tray.failed = false;
        return;
    }
    match &mut tray.icon {
        Some(icon) => icon.set_tooltip(&tooltip),
        None => match (tray.spawn)(tray.events.clone()) {
            Ok(mut icon) => {
                icon.set_tooltip(&tooltip);
                tray.icon = Some(icon);
            }
            Err(e) => {
                tracing::warn!(error = %e, "no tray icon; convt quits with its last window");
                tray.failed = true;
            }
        },
    }
    tray.tooltip = tooltip;
}

fn handle(event: Event, cx: &mut App) {
    match event {
        Event::Open => ui::show_main(cx),
        Event::Settings => ui::show_settings(SettingsTab::General, cx),
        Event::Quit => crate::menu::quit(cx),
        Event::Lost => {
            tracing::warn!("the tray icon went away; convt quits with its last window");
            let tray = cx.global_mut::<Tray>();
            tray.icon = None;
            tray.failed = true;
            if cx.windows().is_empty() {
                crate::nothing_open(cx);
            }
        }
    }
}

/// The convt mark in color, for Windows and Linux trays.
#[cfg(not(target_os = "macos"))]
const COLOR_32: &[u8] = include_bytes!("../assets/tray/tray32.png");
#[cfg(target_os = "linux")]
const COLOR_64: &[u8] = include_bytes!("../assets/tray/tray64.png");
/// The convt mark in color for the macOS menu bar, 18 pt at 2x. Like the
/// Finder menu icon (`integrations/macos/FinderSync/MenuIcon.svg`), it keeps
/// the brand colors rather than being a template image.
#[cfg(target_os = "macos")]
const MENU_BAR: &[u8] = include_bytes!("../assets/tray/menubar.png");

/// A PNG's pixels as RGBA, with its width and height.
fn rgba(png: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let image = image::load_from_memory(png)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Ok((image.into_raw(), width, height))
}

#[cfg(any(target_os = "macos", windows))]
pub mod platform {
    //! tray-icon draws the icon and its menu on the main thread, which GPUI's
    //! run loop (AppKit's, or the Win32 message loop) keeps serving. It
    //! reports clicks through process-wide handlers.

    use futures::channel::mpsc::UnboundedSender;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    use super::{Event, Icon};

    impl Icon for TrayIcon {
        fn set_tooltip(&mut self, tooltip: &str) {
            if let Err(e) = TrayIcon::set_tooltip(self, Some(tooltip)) {
                tracing::debug!(error = %e, "could not set the tray tooltip");
            }
        }
    }

    pub fn spawn(events: UnboundedSender<Event>) -> Result<Box<dyn Icon>, String> {
        let menu = Menu::new();
        let open = MenuItem::with_id("open", "Open convt", true, None);
        let settings = MenuItem::with_id("settings", "Settings…", true, None);
        let quit = MenuItem::with_id("quit", "Quit convt", true, None);
        menu.append_items(&[&open, &settings, &PredefinedMenuItem::separator(), &quit])
            .map_err(|e| e.to_string())?;

        // The handlers can be set once per process; turning the setting off
        // and on again keeps the first ones, which send to the same channel.
        let menu_events = events.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let event = match event.id.0.as_str() {
                "open" => Event::Open,
                "settings" => Event::Settings,
                "quit" => Event::Quit,
                _ => return,
            };
            drop(menu_events.unbounded_send(event));
        }));
        // On Windows a left click opens convt and a right click shows the
        // menu. On macOS any click shows the menu, as menu bar items do.
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
                && cfg!(windows)
            {
                drop(events.unbounded_send(Event::Open));
            }
        }));

        let builder = TrayIconBuilder::new()
            .with_id("convt")
            .with_menu(Box::new(menu))
            .with_tooltip("convt");
        #[cfg(target_os = "macos")]
        let builder = {
            let (rgba, width, height) = super::rgba(super::MENU_BAR)?;
            let icon =
                tray_icon::Icon::from_rgba(rgba, width, height).map_err(|e| e.to_string())?;
            builder.with_icon(icon).with_autosave_name("convt")
        };
        #[cfg(windows)]
        let builder = {
            let (rgba, width, height) = super::rgba(super::COLOR_32)?;
            let icon =
                tray_icon::Icon::from_rgba(rgba, width, height).map_err(|e| e.to_string())?;
            builder.with_icon(icon).with_menu_on_left_click(false)
        };
        let icon = builder.build().map_err(|e| e.to_string())?;
        Ok(Box::new(icon))
    }
}

#[cfg(target_os = "linux")]
pub mod platform {
    //! ksni serves the StatusNotifierItem from its own thread over D-Bus, so
    //! nothing here needs GTK or GPUI's main thread. Spawning fails when no
    //! tray host is running (GNOME without the AppIndicator extension).

    use futures::channel::mpsc::UnboundedSender;
    use ksni::blocking::{Handle, TrayMethods};
    use ksni::menu::StandardItem;

    use super::{Event, Icon};

    struct Item {
        events: UnboundedSender<Event>,
        icons: Vec<ksni::Icon>,
        tooltip: String,
    }

    impl Item {
        fn send(&self, event: Event) {
            drop(self.events.unbounded_send(event));
        }
    }

    impl ksni::Tray for Item {
        fn id(&self) -> String {
            "convt".into()
        }

        fn title(&self) -> String {
            "convt".into()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icons.clone()
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: self.tooltip.clone(),
                ..Default::default()
            }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            self.send(Event::Open);
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            vec![
                StandardItem {
                    label: "Open convt".into(),
                    activate: Box::new(|item: &mut Self| item.send(Event::Open)),
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: "Settings…".into(),
                    activate: Box::new(|item: &mut Self| item.send(Event::Settings)),
                    ..Default::default()
                }
                .into(),
                ksni::MenuItem::Separator,
                StandardItem {
                    label: "Quit convt".into(),
                    activate: Box::new(|item: &mut Self| item.send(Event::Quit)),
                    ..Default::default()
                }
                .into(),
            ]
        }

        fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
            tracing::info!(?reason, "the tray host went away");
            self.send(Event::Lost);
            false
        }
    }

    struct Sni(Handle<Item>);

    impl Icon for Sni {
        fn set_tooltip(&mut self, tooltip: &str) {
            let tooltip = tooltip.to_string();
            self.0.update(move |item| item.tooltip = tooltip);
        }
    }

    impl Drop for Sni {
        fn drop(&mut self) {
            // Removes the icon. Doesn't wait: the service thread may be
            // stuck on a slow bus, and the app shouldn't be.
            drop(self.0.shutdown());
        }
    }

    pub fn spawn(events: UnboundedSender<Event>) -> Result<Box<dyn Icon>, String> {
        let icons = [super::COLOR_32, super::COLOR_64]
            .into_iter()
            .map(argb)
            .collect::<Result<_, _>>()?;
        let item = Item {
            events,
            icons,
            tooltip: "convt".into(),
        };
        let handle = item.spawn().map_err(|e| e.to_string())?;
        Ok(Box::new(Sni(handle)))
    }

    /// StatusNotifierItem pixmaps are ARGB32 in network byte order.
    fn argb(png: &[u8]) -> Result<ksni::Icon, String> {
        let (mut data, width, height) = super::rgba(png)?;
        for pixel in data.chunks_exact_mut(4) {
            pixel.rotate_right(1);
        }
        Ok(ksni::Icon {
            width: width as i32,
            height: height as i32,
            data,
        })
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn pixmaps_are_argb() {
            let icon = super::argb(super::super::COLOR_32).unwrap();
            assert_eq!((icon.width, icon.height), (32, 32));
            assert_eq!(icon.data.len(), 32 * 32 * 4);
            // The tile's corner is transparent; its middle is opaque.
            assert_eq!(icon.data[0], 0, "alpha first");
            let middle = (16 * 32 + 16) * 4;
            assert_eq!(icon.data[middle], 255);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_bundled_icons_decode() {
        #[cfg(not(target_os = "macos"))]
        assert_eq!(super::rgba(super::COLOR_32).unwrap().1, 32);
        #[cfg(target_os = "linux")]
        assert_eq!(super::rgba(super::COLOR_64).unwrap().1, 64);
        #[cfg(target_os = "macos")]
        assert_eq!(super::rgba(super::MENU_BAR).unwrap().1, 36);
        // The macOS menu bar icon is the colored mark, not a black template.
        let (menu_bar, w, h) = super::rgba(include_bytes!("../assets/tray/menubar.png")).unwrap();
        assert_eq!((w, h), (36, 36));
        assert!(
            menu_bar
                .chunks_exact(4)
                .any(|p| p[3] > 0 && u16::from(p[1]) > u16::from(p[0]) + 40)
        );
    }
}
