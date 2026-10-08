//! Windows and the pieces they share. The main window lists activity; Quick
//! convert opens for files sent without a target; Settings and the first-run
//! window are their own windows; the menu bar popover belongs to the tray.

mod about;
mod account;
mod chrome;
mod first_run;
mod main_window;
pub mod menus;
mod pack;
mod popover;
mod quick;
mod settings_window;
#[cfg(test)]
mod tests;
pub mod theme;
mod update;

use std::path::{Path, PathBuf};

use convt_core::Preset;
use convt_license::client::{BUY_URL, DOWNLOAD_URL, State};
use gpui_kit::*;
use theme::IconName;

pub use about::AboutView;
pub use first_run::FirstRunView;
pub use main_window::MainView;
pub use popover::PopoverView;
pub use quick::QuickView;
pub use settings_window::{SettingsTab, SettingsView};

use crate::model;
use crate::request::Request;
use theme::Palette;

/// The icons the windows draw, from Hugeicons (see [`theme::IconName`]).
/// Register it with `Application::with_assets`; without it every icon draws
/// empty.
pub fn assets() -> Assets {
    Assets
}

/// The bundled Hugeicons and Google's G. The component library's own
/// controls (the spinner, a text field's clear button) load gpui-kit's
/// Lucide paths, so those few paths answer with the matching Hugeicon too;
/// everything else falls through to gpui-kit's set.
pub struct Assets;

macro_rules! hugeicons {
    ($($file:literal),* $(,)?) => {
        &[$((
            concat!("icons/hugeicons/", $file, ".svg"),
            include_bytes!(concat!("../../assets/icons/hugeicons/", $file, ".svg")),
        )),*]
    };
}

/// Every file `assets/icons/generate.mjs` writes, at the path
/// [`theme::IconName`] gives it, and the Google G.
const ICONS: &[(&str, &[u8])] = hugeicons![
    "add",
    "alert-circle",
    "alert-triangle",
    "arrow-down",
    "arrow-right",
    "calendar",
    "cancel",
    "cancel-circle",
    "check",
    "check-circle",
    "chevron-down",
    "chevron-right",
    "chevrons-up-down",
    "cloud",
    "computer",
    "document",
    "download",
    "edit",
    "external-link",
    "folder",
    "folder-open",
    "google",
    "hard-drive",
    "inbox",
    "info",
    "key",
    "loading",
    "magic-wand",
    "mail",
    "minus",
    "refresh",
    "restore",
    "rotate",
    "settings",
    "sparkles",
    "square",
    "star",
    "user-circle",
];

const GOOGLE_G: (&str, &[u8]) = (
    "icons/google-g.svg",
    include_bytes!("../../assets/icons/google-g.svg"),
);

/// Onboarding's dithered glow and bloom (`assets/onboarding/generate.py`).
const ONBOARDING: &[(&str, &[u8])] = &[
    (
        "onboarding/glow-light.png",
        include_bytes!("../../assets/onboarding/glow-light.png"),
    ),
    (
        "onboarding/glow-dark.png",
        include_bytes!("../../assets/onboarding/glow-dark.png"),
    ),
    (
        "onboarding/bloom-light.png",
        include_bytes!("../../assets/onboarding/bloom-light.png"),
    ),
    (
        "onboarding/bloom-dark.png",
        include_bytes!("../../assets/onboarding/bloom-dark.png"),
    ),
];

/// The Lucide paths gpui-kit's components load, and the Hugeicon each gets.
const COMPONENT_ICONS: &[(&str, &str)] = &[
    ("icons/loader.svg", "loading"),
    ("icons/loader-circle.svg", "loading"),
    ("icons/close.svg", "cancel"),
    ("icons/check.svg", "check"),
    ("icons/chevron-down.svg", "chevron-down"),
    ("icons/chevron-right.svg", "chevron-right"),
    ("icons/minus.svg", "minus"),
    ("icons/plus.svg", "add"),
];

impl Assets {
    fn bundled(path: &str) -> Option<&'static [u8]> {
        let lucide = COMPONENT_ICONS
            .iter()
            .find(|(lucide, _)| *lucide == path)
            .map(|(_, file)| format!("icons/hugeicons/{file}.svg"));
        let path = lucide.as_deref().unwrap_or(path);
        ICONS
            .iter()
            .chain(std::iter::once(&GOOGLE_G))
            .chain(ONBOARDING)
            .find(|(p, _)| *p == path)
            .map(|(_, bytes)| *bytes)
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        match Self::bundled(path) {
            Some(bytes) => Ok(Some(std::borrow::Cow::Borrowed(bytes))),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut all = gpui_kit::assets::Assets.list(path)?;
        all.extend(
            ICONS
                .iter()
                .chain(std::iter::once(&GOOGLE_G))
                .chain(ONBOARDING)
                .filter(|(p, _)| p.starts_with(path))
                .map(|(p, _)| SharedString::from(*p)),
        );
        Ok(all)
    }
}

/// The license price quoted in the trial card and the first-run window.
pub const LICENSE_PRICE: &str = "$29";

/// An open window of one kind, if any.
struct Open<V: 'static>(AnyWindowHandle, WeakEntity<V>);

impl<V: 'static> Global for Open<V> {}

impl<V: 'static> Open<V> {
    /// The window and its view, if it is still open.
    fn get(cx: &App) -> Option<(AnyWindowHandle, Entity<V>)> {
        let open = cx.try_global::<Self>()?;
        Some((open.0, open.1.upgrade()?))
    }
}

/// Brings an open window of kind `V` forward, or opens one with `build`.
fn show<V: Render>(
    size: Size<Pixels>,
    title: &str,
    cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> Option<(AnyWindowHandle, Entity<V>)> {
    if let Some((handle, view)) = Open::<V>::get(cx)
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return Some((handle, view));
    }
    let opened = open_window(window_options(size, title, cx), cx, build);
    cx.activate(true);
    match opened {
        Ok((handle, view)) => {
            cx.set_global(Open(handle, view.downgrade()));
            Some((handle, view))
        }
        Err(e) => {
            tracing::error!(error = %e, title, "could not open a window");
            None
        }
    }
}

/// Opens a window for the view `build` makes, wrapped in [`chrome::Chrome`]
/// so a Linux window the compositor leaves undecorated still gets a title bar.
fn open_window<V: Render>(
    options: WindowOptions,
    cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> gpui_kit::Result<(AnyWindowHandle, Entity<V>)> {
    let title = options.titlebar.as_ref().and_then(|t| t.title.clone());
    let mut built = None;
    let (handle, _) = gpui_kit::open_window(options, cx, |window, cx| {
        let view = build(window, cx);
        built = Some(view.clone());
        cx.new(|_| chrome::Chrome::new(view, title))
    })?;
    Ok((handle, built.expect("open_window ran its build closure")))
}

/// Sends a request where it belongs:
///
/// - A sign-in reply finishes the sign-in the app started, if it did, and
///   brings back the window that started it: first run, or the License tab.
/// - A license key opens the License tab of Settings.
/// - Files with a target, from the command line or the Finder menu, convert
///   in place with no window. Links never do: any web page can open one.
/// - Other files open Quick convert, and no files open the main window.
pub fn route(request: Request, cx: &mut App) {
    let app = model::shared(cx);
    if let Some(reply) = request.auth {
        // A link the app didn't ask for is dropped inside; the window that
        // comes forward says so.
        let _ = app.update(cx, |s, cx| s.finish_sign_in(reply, cx));
        match Open::<FirstRunView>::get(cx) {
            Some((handle, _)) if handle.update(cx, |_, w, _| w.activate_window()).is_ok() => {}
            _ => show_settings(SettingsTab::License, cx),
        }
    } else if request.license.is_some() {
        show_license(request.license, cx);
    } else if request.files.is_empty() {
        show_main(cx);
    } else if request.auto_start() {
        // Silent conversions never download: a document that needs the pack
        // fails here and opens Quick convert, which offers it.
        app.update(cx, |s, cx| s.refresh_pack(cx));
        if request.show_progress {
            open_main(cx);
        }
        let silent = app.update(cx, |s, cx| s.convert_silently(&request, cx));
        if let Err(e) = silent {
            tracing::info!(reason = %e, "opening Quick convert instead of converting in place");
            open_quick(request, cx);
        }
    } else {
        open_quick(request, cx);
    }
}

/// Opens the main window, or the first-run window until a licensed build
/// finishes it. Closing mid-setup leaves first run unfinished, so the next
/// launch shows it again. A build from source that doesn't check licenses
/// never shows the first-run window.
pub fn show_main(cx: &mut App) {
    if first_run_pending(cx) {
        open_first_run(cx);
    } else {
        open_main(cx);
    }
}

fn first_run_pending(cx: &App) -> bool {
    let state = model::shared(cx).read(cx);
    state.license_enforced() && !state.settings.first_run_done
}

fn open_main(cx: &mut App) -> Option<(AnyWindowHandle, Entity<MainView>)> {
    let app = model::shared(cx);
    let opened = show(size(px(1040.), px(640.)), "convt", cx, |window, cx| {
        cx.new(|cx| MainView::new(app, window, cx))
    });
    if let Some((handle, view)) = &opened {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        view.update(cx, |view, cx| {
            view.set_page(main_window::Page::Activity, cx)
        });
    }
    opened
}

/// File > Add Files…: the main window's file picker. Until first run is
/// done, the first-run window comes forward instead.
pub fn add_files(cx: &mut App) {
    if first_run_pending(cx) {
        open_first_run(cx);
    } else if let Some((_, view)) = open_main(cx) {
        view.update(cx, |view, cx| view.pick_files(cx));
    }
}

/// Opens About convt.
pub fn show_about(cx: &mut App) {
    let app = model::shared(cx);
    show(
        size(px(about::ABOUT_SIZE.0), px(about::ABOUT_SIZE.1)),
        "About convt",
        cx,
        |window, cx| cx.new(|cx| AboutView::new(app, window, cx)),
    );
}

/// Quick convert's size: room for a typical image's format cards, its three
/// options and Save without scrolling. Video options and long lists scroll.
pub(super) const QUICK_SIZE: (f32, f32) = (600., 680.);

/// Quick convert's height, plus the title bar macOS draws inside it.
pub(super) fn quick_height() -> Pixels {
    px(QUICK_SIZE.1
        + if theme::transparent_titlebar() {
            24.
        } else {
            0.
        })
}

/// The smallest onboarding window; it opens at three quarters of the
/// primary display, centered.
pub(super) const FIRST_RUN_SIZE: (f32, f32) = (900., 640.);

/// Three quarters of the primary display, no smaller than [`FIRST_RUN_SIZE`].
pub(super) fn first_run_size(cx: &App) -> Size<Pixels> {
    let display = cx
        .primary_display()
        .map(|d| d.bounds().size)
        .unwrap_or(size(px(FIRST_RUN_SIZE.0), px(FIRST_RUN_SIZE.1)));
    size(
        (display.width * 0.75).max(px(FIRST_RUN_SIZE.0)).round(),
        (display.height * 0.75).max(px(FIRST_RUN_SIZE.1)).round(),
    )
}

fn open_first_run(cx: &mut App) {
    let app = model::shared(cx);
    show(first_run_size(cx), "Welcome to convt", cx, |window, cx| {
        cx.new(|cx| FirstRunView::new(app, first_run::Screen::Account, window, cx))
    });
}

/// Opens Settings on `tab`.
pub fn show_settings(tab: SettingsTab, cx: &mut App) {
    let app = model::shared(cx);
    app.update(cx, |s, cx| s.refresh_pack(cx));
    if let Some((handle, view)) = show(size(px(620.), px(600.)), "Settings", cx, |window, cx| {
        cx.new(|cx| SettingsView::new(app, window, cx))
    }) {
        let _ = handle.update(cx, |_, _, cx| view.update(cx, |v, cx| v.set_tab(tab, cx)));
    }
}

/// Opens the License tab of Settings, with `key` filled in. Keys from links
/// are never activated without a click: any web page can open one.
pub fn show_license(key: Option<String>, cx: &mut App) {
    show_settings(SettingsTab::License, cx);
    if let Some((handle, view)) = Open::<SettingsView>::get(cx) {
        let _ = handle.update(cx, |_, window, cx| {
            view.update(cx, |view, cx| view.fill_license(key, window, cx))
        });
    }
}

pub fn open_quick(request: Request, cx: &mut App) {
    let app = model::shared(cx);
    // Status is read offline; it may have changed through the CLI.
    app.update(cx, |s, cx| s.refresh_pack(cx));
    let options = window_options(size(px(QUICK_SIZE.0), quick_height()), "Convert", cx);
    match open_window(options, cx, |window, cx| {
        cx.new(|cx| QuickView::new(app, request, window, cx))
    }) {
        // Each request gets its own window; the global tracks the newest.
        Ok((handle, view)) => cx.set_global(Open(handle, view.downgrade())),
        Err(e) => tracing::error!(error = %e, "could not open Quick convert"),
    }
    cx.activate(true);
}

/// Opens the menu bar popover. Only the tray icon should call this, when it
/// is clicked or a file is dropped on it. GPUI has no tray yet, so nothing
/// calls it outside tests; see `tray.rs`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn open_popover(cx: &mut App) -> Option<(AnyWindowHandle, Entity<PopoverView>)> {
    let app = model::shared(cx);
    show(size(px(340.), px(520.)), "convt", cx, |window, cx| {
        cx.new(|cx| PopoverView::new(app, window, cx))
    })
}

fn window_options(size: Size<Pixels>, title: &str, cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size, cx))),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from(title.to_string())),
            appears_transparent: theme::transparent_titlebar(),
            traffic_light_position: theme::transparent_titlebar().then(|| point(px(16.), px(16.))),
        }),
        app_id: Some("convt".into()),
        // Linux only. Where the compositor can't draw them (GNOME on
        // Wayland), GPUI falls back to client-side decorations and
        // `chrome::Chrome` draws the title bar.
        window_decorations: Some(WindowDecorations::Server),
        ..Default::default()
    }
}

/// "to webp, quality 80, max 2048 px".
fn describe(preset: &Preset) -> String {
    let o = &preset.options;
    let mut parts = Vec::new();
    match &preset.to {
        Some(to) => parts.push(format!("to {to}")),
        None => parts.push("any format".into()),
    }
    if let Some(q) = o.quality {
        parts.push(format!("quality {q}"));
    }
    if let Some(m) = o.max_size {
        parts.push(format!("max {m} px"));
    }
    if let Some(h) = o.video_height {
        parts.push(format!("{h}p video"));
    }
    if let Some(b) = o.audio_bitrate {
        parts.push(format!("{b} kbit/s audio"));
    }
    if o.pages.is_some() {
        parts.push("some pages".into());
    }
    if let Some(d) = o.dpi {
        parts.push(format!("{d} dpi"));
    }
    if let Some(codec) = o.video_codec {
        parts.push(codec.name().into());
    }
    if o.strip_audio {
        parts.push("no audio".into());
    }
    if let Some(background) = o.background {
        parts.push(format!("{} background", background.name().to_lowercase()));
    }
    parts.join(", ")
}

/// A path with the home folder shortened to `~`. Windows has no `~`, so
/// there the path stays whole, even when a Unix shell set `HOME`.
pub(super) fn tilde(path: &Path) -> String {
    if !cfg!(windows)
        && let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return if rest.as_os_str().is_empty() {
            "~".into()
        } else {
            format!("~/{}", rest.display())
        };
    }
    path.display().to_string()
}

/// An error line that tests can read by the id `error`.
fn error_text(message: impl Into<SharedString>, p: &Palette) -> impl IntoElement {
    let message = message.into();
    div()
        .id("error")
        .test_support()
        .aria_label(message.clone())
        .flex()
        .items_start()
        .gap(px(6.))
        .child(div().flex_shrink_0().pt(px(2.)).child(theme::icon(
            IconName::CircleAlert,
            13.,
            p.error,
        )))
        .child(
            theme::styled(theme::size::SMALL, p.error)
                .flex_1()
                .min_w_0()
                .child(message),
        )
}

/// Why conversions stopped, with what the user can do about it. Nothing
/// while conversions are allowed.
fn blocked_banner(state: &State, p: &Palette) -> Option<impl IntoElement + use<>> {
    let reason = SharedString::from(state.blocked_reason()?);
    let buy = if matches!(state, State::NotCovered(_)) {
        "Renew"
    } else {
        "Buy a license"
    };
    let download = matches!(state, State::NotCovered(_)).then(|| {
        theme::text_button("download", "Download a covered build", p.green_text, 12.)
            .on_click(|_, _, cx| cx.open_url(DOWNLOAD_URL))
    });
    Some(theme::callout(
        IconName::TriangleAlert,
        theme::Tone::Error,
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .id("license-banner")
                    .test_support()
                    .aria_label(reason.clone())
                    .child(
                        theme::styled(theme::size::BODY, p.text)
                            .font_weight(FontWeight::MEDIUM)
                            .child(reason),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .child(
                        theme::text_button("buy", buy, p.green_text, 12.)
                            .on_click(|_, _, cx| cx.open_url(BUY_URL)),
                    )
                    .children(download)
                    .child(
                        theme::text_button("enter-license", "Enter license", p.text, 12.)
                            .on_click(|_, _, cx| show_license(None, cx)),
                    ),
            ),
        p,
    ))
}

/// "1.9 MB", "214 KB".
pub(crate) fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1000.;
    let mut unit = 0;
    while value >= 1000. && unit < UNITS.len() - 1 {
        value /= 1000.;
        unit += 1;
    }
    if value < 10. && unit > 0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

/// "1 min left", "under a minute left".
fn time_left(left: std::time::Duration) -> String {
    match left.as_secs() {
        0..60 => "under a minute left".into(),
        s => format!("{} min left", s.div_ceil(60)),
    }
}

/// Opens a folder in the file manager, creating it first.
fn open_folder(dir: &Path, cx: &mut App) {
    let _ = std::fs::create_dir_all(dir);
    cx.open_with_system(dir);
}

#[cfg(test)]
mod unit {
    use super::{human_size, time_left};

    #[test]
    fn sizes_and_times() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1_900_000), "1.9 MB");
        assert_eq!(human_size(214_000), "214 KB");
        assert_eq!(human_size(38_400_000), "38 MB");
        assert_eq!(
            time_left(std::time::Duration::from_secs(20)),
            "under a minute left"
        );
        assert_eq!(time_left(std::time::Duration::from_secs(61)), "2 min left");
    }
}
