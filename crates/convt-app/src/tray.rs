//! The tray icon: the menu bar item on macOS, the notification area icon on
//! Windows, and a StatusNotifierItem on Linux. While "Keep running in the
//! background" is on (`settings.menu_bar_icon`) and the icon is up, closing
//! the last window leaves convt running, so file manager requests start at
//! once. On Windows and Linux a left click opens convt. While conversions
//! run, the icon shows it and the tooltip counts them.
//!
//! The menu is [`menu`], a pure function of what the app is doing
//! ([`MenuState`]): a status line, Convert Files…, the last few converted
//! files (a click shows one in the file manager), Open convt, Settings…,
//! Check for Updates… (or Restart to Update once one is ready), and Quit.
//! It is rebuilt whenever that changes.
//!
//! The platform icons talk to the app through [`Event`]s on a channel that a
//! GPUI task drains, like the single-instance requests in `main.rs`. Where no
//! icon can be shown (a Linux desktop with no tray, or an error), convt logs
//! it and quits with its last window, as with the setting off.

use std::path::{Path, PathBuf};

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui_kit::{App, Entity, Global};

use crate::history::Outcome;
use crate::model::AppState;
use crate::ui::{self, SettingsTab};
use crate::update::Update;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Show the main window.
    Open,
    /// Pick files, then Quick convert them.
    ConvertFiles,
    /// Show a converted file in the file manager.
    Reveal(PathBuf),
    /// Open Settings.
    Settings,
    /// Check for updates now, and show the result in Settings.
    CheckForUpdates,
    /// Install the downloaded update and restart.
    RestartToUpdate,
    /// Quit convt.
    Quit,
    /// The icon went away by itself: the Linux tray host stopped.
    #[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
    Lost,
}

impl Event {
    /// The menu item id that stands for this event. Ids carry the whole
    /// event, so a click on a menu that was rebuilt meanwhile still does
    /// what its label said.
    #[cfg_attr(not(any(target_os = "macos", windows, test)), allow(dead_code))]
    pub fn id(&self) -> String {
        match self {
            Event::Open => "open".into(),
            Event::ConvertFiles => "convert".into(),
            Event::Reveal(path) => format!("reveal:{}", path.display()),
            Event::Settings => "settings".into(),
            Event::CheckForUpdates => "updates".into(),
            Event::RestartToUpdate => "restart".into(),
            Event::Quit => "quit".into(),
            Event::Lost => "lost".into(),
        }
    }

    /// The event a menu item id stands for.
    #[cfg_attr(not(any(target_os = "macos", windows, test)), allow(dead_code))]
    pub fn from_id(id: &str) -> Option<Event> {
        Some(match id {
            "open" => Event::Open,
            "convert" => Event::ConvertFiles,
            "settings" => Event::Settings,
            "updates" => Event::CheckForUpdates,
            "restart" => Event::RestartToUpdate,
            "quit" => Event::Quit,
            _ => Event::Reveal(PathBuf::from(id.strip_prefix("reveal:")?)),
        })
    }
}

/// Whose words the menu uses for the file manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
    Linux,
}

impl Platform {
    pub const CURRENT: Platform = if cfg!(target_os = "macos") {
        Platform::Mac
    } else if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    };

    /// The heading over the recent files, which a click shows there.
    fn show_in(self) -> &'static str {
        match self {
            Platform::Mac => "Show in Finder",
            Platform::Windows => "Show in File Explorer",
            Platform::Linux => "Show in File Manager",
        }
    }
}

/// One line of the tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// Says something; can't be clicked.
    Note(String),
    Action(String, Event),
    Separator,
}

/// How many converted files the menu lists.
pub const RECENT_FILES: usize = 5;

/// What the menu shows, read off the app's state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuState {
    /// Conversions running or waiting.
    pub active: usize,
    /// The version of an update that is downloaded and ready to install.
    pub ready: Option<String>,
    /// The version of an update that is installing.
    pub installing: Option<String>,
    /// The newest converted files that are still there, newest first.
    pub recent: Vec<PathBuf>,
}

impl MenuState {
    /// Reads the state. `exists` says whether a converted file is still
    /// there; the app passes `Path::exists`.
    pub fn of(state: &AppState, exists: impl Fn(&Path) -> bool) -> Self {
        let mut recent: Vec<PathBuf> = Vec::new();
        for record in &state.recent {
            if recent.len() == RECENT_FILES {
                break;
            }
            if let Outcome::Done(outputs) = &record.outcome
                && let Some(first) = outputs.first()
                && !recent.contains(first)
                && exists(first)
            {
                recent.push(first.clone());
            }
        }
        let (ready, installing) = match &state.update {
            Update::Ready { version, .. } => (Some(version.clone()), None),
            Update::Installing { version } => (None, Some(version.clone())),
            _ => (None, None),
        };
        Self {
            active: state.queue.active(),
            ready,
            installing,
            recent,
        }
    }
}

/// The tray menu for this state, top to bottom.
pub fn menu(s: &MenuState, platform: Platform) -> Vec<Item> {
    let status = match (&s.installing, s.active) {
        (Some(version), _) => format!("Installing convt {version}…"),
        (None, 0) => "Ready".into(),
        (None, 1) => "Converting 1 file…".into(),
        (None, n) => format!("Converting {n} files…"),
    };
    let mut items = vec![Item::Note(status), Item::Separator];
    // Conversions wait while an update installs, so offer none.
    if s.installing.is_none() {
        items.push(Item::Action("Convert Files…".into(), Event::ConvertFiles));
        items.push(Item::Separator);
    }
    if !s.recent.is_empty() {
        items.push(Item::Note(platform.show_in().into()));
        for path in &s.recent {
            items.push(Item::Action(
                file_label(path, &s.recent),
                Event::Reveal(path.clone()),
            ));
        }
        items.push(Item::Separator);
    }
    items.push(Item::Action("Open convt".into(), Event::Open));
    items.push(Item::Action("Settings…".into(), Event::Settings));
    match (&s.ready, &s.installing) {
        (_, Some(_)) => {}
        (Some(_), None) => items.push(Item::Action(
            "Restart to Update".into(),
            Event::RestartToUpdate,
        )),
        (None, None) => items.push(Item::Action(
            "Check for Updates…".into(),
            Event::CheckForUpdates,
        )),
    }
    items.push(Item::Separator);
    items.push(Item::Action("Quit convt".into(), Event::Quit));
    items
}

/// A converted file's name, with its folder when another listed file has
/// the same name.
fn file_label(path: &Path, all: &[PathBuf]) -> String {
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    let own = name(path).unwrap_or_else(|| path.display().to_string());
    let twins = all
        .iter()
        .filter(|p| name(p).as_deref() == Some(own.as_str()))
        .count();
    match path.parent().and_then(|d| d.file_name()) {
        Some(folder) if twins > 1 => format!("{own} ({})", folder.to_string_lossy()),
        _ => own,
    }
}

/// A label as the platform's menus take it: Windows reads `&` as a
/// mnemonic and Linux's DBusMenu reads `_` as one, so file names double them.
#[cfg_attr(not(any(windows, target_os = "linux", test)), allow(dead_code))]
pub fn escape(label: &str, platform: Platform) -> String {
    match platform {
        Platform::Mac => label.to_string(),
        Platform::Windows => label.replace('&', "&&"),
        Platform::Linux => label.replace('_', "__"),
    }
}

/// What the icon should show: its look, its tooltip and its menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub busy: bool,
    pub tooltip: String,
    pub menu: Vec<Item>,
}

impl View {
    fn of(state: &AppState) -> Self {
        Self {
            busy: state.queue.active() > 0,
            tooltip: tooltip(state),
            menu: menu(&MenuState::of(state, Path::exists), Platform::CURRENT),
        }
    }
}

/// A tray icon on screen. Dropping it removes the icon.
pub trait Icon {
    /// Shows `view`: called only when it changed.
    fn show(&mut self, view: &View);
}

/// Puts an icon on screen showing `view` that sends its clicks to the
/// channel, or says why it couldn't. [`platform::spawn`] in the app; tests
/// pass their own.
pub type Spawn = fn(UnboundedSender<Event>, &View) -> Result<Box<dyn Icon>, String>;

struct Tray {
    spawn: Spawn,
    events: UnboundedSender<Event>,
    icon: Option<Box<dyn Icon>>,
    /// What the icon shows now.
    view: Option<View>,
    /// The last attempt failed. Turning the setting off and on tries again.
    failed: bool,
}

impl Global for Tray {}

/// Shows the icon while the setting is on, and follows the setting, the
/// running jobs and the recent files from then on.
pub fn init(state: &Entity<AppState>, spawn: Spawn, cx: &mut App) {
    let (events, mut rx) = unbounded();
    cx.set_global(Tray {
        spawn,
        events,
        icon: None,
        view: None,
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
    let Some(tray) = cx.try_global::<Tray>() else {
        return;
    };
    let wanted = indicator(state.read(cx)).is_some();
    match (&tray.icon, wanted) {
        (None, false) => {
            if tray.failed {
                cx.global_mut::<Tray>().failed = false;
            }
            return;
        }
        // After a failed try, wait until it's turned off and on again.
        (None, true) if tray.failed => return,
        (Some(_), false) => {
            let tray = cx.global_mut::<Tray>();
            tray.icon = None;
            tray.view = None;
            return;
        }
        _ => {}
    }
    let view = View::of(state.read(cx));
    let tray = cx.global_mut::<Tray>();
    if tray.view.as_ref() == Some(&view) {
        return;
    }
    match &mut tray.icon {
        Some(icon) => icon.show(&view),
        None => match (tray.spawn)(tray.events.clone(), &view) {
            Ok(icon) => tray.icon = Some(icon),
            Err(e) => {
                tracing::warn!(error = %e, "no tray icon; convt quits with its last window");
                tray.failed = true;
                return;
            }
        },
    }
    tray.view = Some(view);
}

fn handle(event: Event, cx: &mut App) {
    match event {
        Event::Open => ui::show_main(cx),
        Event::ConvertFiles => ui::convert_files(cx),
        Event::Reveal(path) => {
            // GPUI's test platform can't reveal files.
            #[cfg(not(test))]
            cx.reveal_path(&path);
            #[cfg(test)]
            crate::model::shared(cx).update(cx, |s, _| s.revealed.push(path));
        }
        Event::Settings => ui::show_settings(SettingsTab::General, cx),
        Event::CheckForUpdates => ui::menus::check_for_updates(cx),
        Event::RestartToUpdate => {
            crate::model::shared(cx).update(cx, |s, cx| s.restart_to_update(cx));
        }
        Event::Quit => crate::menu::quit(cx),
        Event::Lost => {
            tracing::warn!("the tray icon went away; convt quits with its last window");
            let tray = cx.global_mut::<Tray>();
            tray.icon = None;
            tray.view = None;
            tray.failed = true;
            if cx.windows().is_empty() {
                crate::nothing_open(cx);
            }
        }
    }
}

/// The menu bar glyphs: 18 pt template images at 2x, which macOS tints for
/// a light or dark menu bar and the highlighted item. Drawn by
/// `assets/tray/generate.py`.
#[cfg(target_os = "macos")]
const MENU_BAR: &[u8] = include_bytes!("../assets/tray/menubar.png");
#[cfg(target_os = "macos")]
const MENU_BAR_BUSY: &[u8] = include_bytes!("../assets/tray/menubar-busy.png");
/// The colored tile for the Windows notification area and the Linux tray's
/// fallback pixmap: it reads on a light and a dark taskbar.
#[cfg(not(target_os = "macos"))]
const COLOR_32: &[u8] = include_bytes!("../assets/tray/tray32.png");
#[cfg(not(target_os = "macos"))]
const COLOR_32_BUSY: &[u8] = include_bytes!("../assets/tray/tray32-busy.png");
#[cfg(target_os = "linux")]
const COLOR_64: &[u8] = include_bytes!("../assets/tray/tray64.png");
#[cfg(target_os = "linux")]
const COLOR_64_BUSY: &[u8] = include_bytes!("../assets/tray/tray64-busy.png");
/// The symbolic icons a Linux tray host recolors for its panel.
#[cfg(target_os = "linux")]
const SYMBOLIC: &str = include_str!("../assets/tray/convt-symbolic.svg");
#[cfg(target_os = "linux")]
const SYMBOLIC_BUSY: &str = include_str!("../assets/tray/convt-busy-symbolic.svg");

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

    use super::{Event, Icon, Item, Platform, View, escape};

    struct Native {
        icon: TrayIcon,
        busy: bool,
    }

    impl Icon for Native {
        fn show(&mut self, view: &View) {
            if let Err(e) = self.icon.set_tooltip(Some(&view.tooltip)) {
                tracing::debug!(error = %e, "could not set the tray tooltip");
            }
            if view.busy != self.busy {
                match image(view.busy) {
                    Ok(image) => {
                        // A template image: macOS draws it in the menu bar's
                        // own color.
                        #[cfg(target_os = "macos")]
                        let set = self.icon.set_icon_templated(Some(image));
                        #[cfg(windows)]
                        let set = self.icon.set_icon(Some(image));
                        if let Err(e) = set {
                            tracing::debug!(error = %e, "could not change the tray icon");
                        }
                    }
                    Err(e) => tracing::debug!(error = %e, "could not load the tray icon"),
                }
                self.busy = view.busy;
            }
            match menu(&view.menu) {
                Ok(menu) => self.icon.set_menu(Some(Box::new(menu))),
                Err(e) => tracing::debug!(error = %e, "could not build the tray menu"),
            }
        }
    }

    fn image(busy: bool) -> Result<tray_icon::Icon, String> {
        #[cfg(target_os = "macos")]
        let png = if busy {
            super::MENU_BAR_BUSY
        } else {
            super::MENU_BAR
        };
        #[cfg(windows)]
        let png = if busy {
            super::COLOR_32_BUSY
        } else {
            super::COLOR_32
        };
        let (rgba, width, height) = super::rgba(png)?;
        tray_icon::Icon::from_rgba(rgba, width, height).map_err(|e| e.to_string())
    }

    fn menu(items: &[Item]) -> Result<Menu, String> {
        let menu = Menu::new();
        for item in items {
            let result = match item {
                Item::Note(label) => menu.append(&MenuItem::new(
                    escape(label, Platform::CURRENT),
                    false,
                    None,
                )),
                Item::Action(label, event) => menu.append(&MenuItem::with_id(
                    event.id(),
                    escape(label, Platform::CURRENT),
                    true,
                    None,
                )),
                Item::Separator => menu.append(&PredefinedMenuItem::separator()),
            };
            result.map_err(|e| e.to_string())?;
        }
        Ok(menu)
    }

    pub fn spawn(events: UnboundedSender<Event>, view: &View) -> Result<Box<dyn Icon>, String> {
        // The handlers can be set once per process; turning the setting off
        // and on again keeps the first ones, which send to the same channel.
        let menu_events = events.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some(event) = Event::from_id(&event.id.0) {
                drop(menu_events.unbounded_send(event));
            }
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
            .with_menu(Box::new(menu(&view.menu)?))
            .with_tooltip(&view.tooltip);
        // A template image: macOS draws it in the menu bar's own color.
        #[cfg(target_os = "macos")]
        let builder = builder
            .with_icon_templated(image(view.busy)?)
            .with_autosave_name("convt");
        #[cfg(windows)]
        let builder = builder
            .with_icon(image(view.busy)?)
            .with_menu_on_left_click(false);
        let icon = builder.build().map_err(|e| e.to_string())?;
        Ok(Box::new(Native {
            icon,
            busy: view.busy,
        }))
    }
}

#[cfg(target_os = "linux")]
pub mod platform {
    //! ksni serves the StatusNotifierItem from its own thread over D-Bus, so
    //! nothing here needs GTK or GPUI's main thread. Spawning fails when no
    //! tray host is running (GNOME without the AppIndicator extension).
    //!
    //! The icon is a symbolic one, named `convt-symbolic` (or
    //! `convt-busy-symbolic`), from a theme folder convt writes to its
    //! runtime directory, so the host recolors it for its panel. Hosts that
    //! can't find it draw the colored pixmap instead.

    use std::path::PathBuf;

    use futures::channel::mpsc::UnboundedSender;
    use ksni::blocking::{Handle, TrayMethods};
    use ksni::menu::StandardItem;

    use super::{Event, Icon, Item, Platform, View, escape};

    struct Sni {
        events: UnboundedSender<Event>,
        view: View,
        /// The theme folder with the symbolic icons, if it could be written.
        theme: Option<PathBuf>,
    }

    impl Sni {
        fn send(&self, event: Event) {
            drop(self.events.unbounded_send(event));
        }
    }

    impl ksni::Tray for Sni {
        fn id(&self) -> String {
            "convt".into()
        }

        fn title(&self) -> String {
            "convt".into()
        }

        fn status(&self) -> ksni::Status {
            ksni::Status::Active
        }

        fn icon_theme_path(&self) -> String {
            self.theme
                .as_ref()
                .map(|t| t.display().to_string())
                .unwrap_or_default()
        }

        fn icon_name(&self) -> String {
            match (&self.theme, self.view.busy) {
                (None, _) => String::new(),
                (Some(_), false) => "convt-symbolic".into(),
                (Some(_), true) => "convt-busy-symbolic".into(),
            }
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            let pngs = if self.view.busy {
                [super::COLOR_32_BUSY, super::COLOR_64_BUSY]
            } else {
                [super::COLOR_32, super::COLOR_64]
            };
            pngs.into_iter().filter_map(|png| argb(png).ok()).collect()
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: self.view.tooltip.clone(),
                ..Default::default()
            }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            self.send(Event::Open);
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            self.view
                .menu
                .iter()
                .map(|item| match item {
                    Item::Note(label) => StandardItem {
                        label: escape(label, Platform::Linux),
                        enabled: false,
                        ..Default::default()
                    }
                    .into(),
                    Item::Action(label, event) => {
                        let event = event.clone();
                        StandardItem {
                            label: escape(label, Platform::Linux),
                            activate: Box::new(move |sni: &mut Self| sni.send(event.clone())),
                            ..Default::default()
                        }
                        .into()
                    }
                    Item::Separator => ksni::MenuItem::Separator,
                })
                .collect()
        }

        fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
            tracing::info!(?reason, "the tray host went away");
            self.send(Event::Lost);
            false
        }
    }

    struct Running(Handle<Sni>);

    impl Icon for Running {
        fn show(&mut self, view: &View) {
            let view = view.clone();
            self.0.update(move |sni| sni.view = view);
        }
    }

    impl Drop for Running {
        fn drop(&mut self) {
            // Removes the icon. Doesn't wait: the service thread may be
            // stuck on a slow bus, and the app shouldn't be.
            drop(self.0.shutdown());
        }
    }

    pub fn spawn(events: UnboundedSender<Event>, view: &View) -> Result<Box<dyn Icon>, String> {
        let sni = Sni {
            events,
            view: view.clone(),
            theme: write_theme(),
        };
        let handle = sni.spawn().map_err(|e| e.to_string())?;
        Ok(Box::new(Running(handle)))
    }

    /// Writes the symbolic icons to a folder in convt's runtime directory,
    /// which only this user can read. Hosts look a name up in that folder as
    /// an icon theme search path, which finds icons at its top level (an
    /// `index.theme` would be needed for subfolders).
    fn write_theme() -> Option<PathBuf> {
        let root = std::env::var_os("CONVT_RUNTIME_DIR")
            .or_else(|| std::env::var_os("XDG_RUNTIME_DIR"))
            .map(PathBuf::from)?
            .join("convt-icons");
        let written = std::fs::create_dir_all(&root)
            .and_then(|()| std::fs::write(root.join("convt-symbolic.svg"), super::SYMBOLIC))
            .and_then(|()| {
                std::fs::write(root.join("convt-busy-symbolic.svg"), super::SYMBOLIC_BUSY)
            });
        match written {
            Ok(()) => Some(root),
            Err(e) => {
                tracing::debug!(error = %e, "could not write the tray icons; using the pixmap");
                None
            }
        }
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
    use std::path::PathBuf;

    use super::{Event, Item, MenuState, Platform, escape, menu};

    fn labels(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                Item::Note(l) => format!("({l})"),
                Item::Action(l, _) => l.clone(),
                Item::Separator => "-".into(),
            })
            .collect()
    }

    #[test]
    fn an_idle_menu_offers_converting_and_the_app() {
        let items = menu(&MenuState::default(), Platform::Mac);
        assert_eq!(
            labels(&items),
            [
                "(Ready)",
                "-",
                "Convert Files…",
                "-",
                "Open convt",
                "Settings…",
                "Check for Updates…",
                "-",
                "Quit convt",
            ]
        );
        let events: Vec<_> = items
            .iter()
            .filter_map(|i| match i {
                Item::Action(_, e) => Some(e.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            events,
            [
                Event::ConvertFiles,
                Event::Open,
                Event::Settings,
                Event::CheckForUpdates,
                Event::Quit
            ]
        );
    }

    #[test]
    fn the_status_counts_running_conversions() {
        let one = MenuState {
            active: 1,
            ..Default::default()
        };
        assert_eq!(
            menu(&one, Platform::Mac)[0],
            Item::Note("Converting 1 file…".into())
        );
        let three = MenuState {
            active: 3,
            ..Default::default()
        };
        assert_eq!(
            menu(&three, Platform::Linux)[0],
            Item::Note("Converting 3 files…".into())
        );
    }

    #[test]
    fn recent_files_show_in_the_platforms_file_manager() {
        let state = MenuState {
            recent: vec![
                PathBuf::from("/home/a/Pictures/photo.webp"),
                PathBuf::from("/home/a/Downloads/report.pdf"),
            ],
            ..Default::default()
        };
        for (platform, heading) in [
            (Platform::Mac, "Show in Finder"),
            (Platform::Windows, "Show in File Explorer"),
            (Platform::Linux, "Show in File Manager"),
        ] {
            let items = menu(&state, platform);
            let at = items
                .iter()
                .position(|i| *i == Item::Note(heading.into()))
                .unwrap_or_else(|| panic!("{heading}"));
            assert_eq!(
                items[at + 1],
                Item::Action(
                    "photo.webp".into(),
                    Event::Reveal("/home/a/Pictures/photo.webp".into())
                )
            );
            assert_eq!(labels(&items[at + 2..at + 4]), ["report.pdf", "-"]);
        }
    }

    #[test]
    fn files_with_the_same_name_say_their_folder() {
        let state = MenuState {
            recent: vec![
                PathBuf::from("/a/Desktop/photo.png"),
                PathBuf::from("/a/Downloads/photo.png"),
                PathBuf::from("/a/Downloads/other.png"),
            ],
            ..Default::default()
        };
        let items = menu(&state, Platform::Mac);
        let names: Vec<_> = labels(&items)
            .into_iter()
            .filter(|l| l.ends_with(".png") || l.ends_with(')') && !l.starts_with('('))
            .collect();
        assert_eq!(
            names,
            ["photo.png (Desktop)", "photo.png (Downloads)", "other.png"]
        );
    }

    #[test]
    fn a_ready_update_offers_the_restart_and_an_install_hides_converting() {
        let ready = MenuState {
            ready: Some("0.4.0".into()),
            ..Default::default()
        };
        let items = menu(&ready, Platform::Windows);
        assert!(items.contains(&Item::Action(
            "Restart to Update".into(),
            Event::RestartToUpdate
        )));
        assert!(!labels(&items).contains(&"Check for Updates…".to_string()));

        let installing = MenuState {
            installing: Some("0.4.0".into()),
            active: 2,
            ..Default::default()
        };
        let items = menu(&installing, Platform::Mac);
        assert_eq!(
            labels(&items),
            [
                "(Installing convt 0.4.0…)",
                "-",
                "Open convt",
                "Settings…",
                "-",
                "Quit convt"
            ]
        );
    }

    #[test]
    fn menu_ids_carry_the_whole_event() {
        for event in [
            Event::Open,
            Event::ConvertFiles,
            Event::Reveal("/tmp/a b/c:d.png".into()),
            Event::Settings,
            Event::CheckForUpdates,
            Event::RestartToUpdate,
            Event::Quit,
        ] {
            assert_eq!(Event::from_id(&event.id()), Some(event));
        }
        assert_eq!(Event::from_id("something else"), None);
    }

    #[test]
    fn labels_keep_their_underscores_and_ampersands() {
        assert_eq!(
            escape("my_photo & co.png", Platform::Mac),
            "my_photo & co.png"
        );
        assert_eq!(
            escape("my_photo & co.png", Platform::Windows),
            "my_photo && co.png"
        );
        assert_eq!(
            escape("my_photo & co.png", Platform::Linux),
            "my__photo & co.png"
        );
    }

    #[test]
    fn the_menu_bar_icons_are_template_glyphs() {
        // Black with alpha, as macOS template images are: only the shape
        // counts, and macOS tints it for the menu bar.
        for png in [
            &include_bytes!("../assets/tray/menubar.png")[..],
            include_bytes!("../assets/tray/menubar-busy.png"),
        ] {
            let (pixels, w, h) = super::rgba(png).unwrap();
            assert_eq!((w, h), (36, 36), "18 pt at 2x");
            assert!(pixels.chunks_exact(4).all(|p| p[..3] == [0, 0, 0]));
            assert!(pixels.chunks_exact(4).any(|p| p[3] == 255));
            // Clear at the edges: a glyph, not a tile.
            assert_eq!(pixels[3], 0);
        }
        assert_ne!(
            include_bytes!("../assets/tray/menubar.png")[..],
            include_bytes!("../assets/tray/menubar-busy.png")[..]
        );
    }

    #[test]
    fn the_color_icons_and_symbolic_icons_are_there() {
        for (png, size) in [
            (&include_bytes!("../assets/tray/tray32.png")[..], 32),
            (include_bytes!("../assets/tray/tray32-busy.png"), 32),
            (include_bytes!("../assets/tray/tray64.png"), 64),
            (include_bytes!("../assets/tray/tray64-busy.png"), 64),
        ] {
            assert_eq!(super::rgba(png).unwrap().1, size);
        }
        for svg in [
            include_str!("../assets/tray/convt-symbolic.svg"),
            include_str!("../assets/tray/convt-busy-symbolic.svg"),
        ] {
            assert!(svg.contains("currentColor") && svg.contains("ColorScheme-Text"));
        }
    }
}
