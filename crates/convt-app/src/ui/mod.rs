//! Windows and the pieces they share. The main window lists activity, takes
//! dropped and added files in its composer, and holds Settings as a page;
//! Quick convert opens for files sent without a target; conversions started
//! without a window show the progress HUD; Settings also opens as a window
//! from the tray and links; the menu bar popover belongs to the tray.

mod about;
mod account;
mod chrome;
mod feedback;
mod finder_guide;
mod first_run;
mod hud;
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
pub use finder_guide::FinderGuideView;
pub use first_run::FirstRunView;
pub use hud::HudView;
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
    "menu",
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

/// Onboarding's dithered glow and the setup step's spinner
/// (`assets/onboarding/generate.py`).
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
        "onboarding/spinner-track.svg",
        include_bytes!("../../assets/onboarding/spinner-track.svg"),
    ),
    (
        "onboarding/spinner-arc.svg",
        include_bytes!("../../assets/onboarding/spinner-arc.svg"),
    ),
];

/// The Finder guide's drawn pointer (`finder_guide`): its edge and its fill,
/// drawn in two colors.
const GUIDE: &[(&str, &[u8])] = &[
    (
        "guide/pointer-edge.svg",
        include_bytes!("../../assets/guide/pointer-edge.svg"),
    ),
    (
        "guide/pointer.svg",
        include_bytes!("../../assets/guide/pointer.svg"),
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
            .chain(GUIDE)
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
                .chain(GUIDE)
                .filter(|(p, _)| p.starts_with(path))
                .map(|(p, _)| SharedString::from(*p)),
        );
        Ok(all)
    }
}

/// The license price quoted in the trial card and the first-run window.
pub const LICENSE_PRICE: &str = "$39";

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
    show_on(None, size, None, title, cx, build)
}

/// [`show`] for a window that can't be resized smaller than `min`, so its
/// columns and grids keep their room.
fn show_min<V: Render>(
    size: Size<Pixels>,
    min: Size<Pixels>,
    title: &str,
    cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> Option<(AnyWindowHandle, Entity<V>)> {
    show_on(None, size, Some(min), title, cx, build)
}

/// [`show`], centered on `display` (the primary display for `None`), and
/// no smaller than `min` when given.
fn show_on<V: Render>(
    display: Option<DisplayId>,
    size: Size<Pixels>,
    min: Option<Size<Pixels>>,
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
    let options = WindowOptions {
        window_min_size: min,
        ..window_options_on(display, size, title, cx)
    };
    let opened = open_window(options, cx, build);
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
    // Back in the Dock if the app hid there with no window open.
    #[cfg(target_os = "macos")]
    crate::macos::show_in_dock(true);
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
/// - While an update installs, files convert nothing; a notification says
///   to try again once convt restarts.
pub fn route(request: Request, cx: &mut App) {
    let app = model::shared(cx);
    // Every right-click lands here, so the next menu follows a `convt menu`
    // change made since.
    app.update(cx, |s, cx| s.reload_menu(cx));
    if !request.files.is_empty() && request.auth.is_none() && app.read(cx).installing() {
        cx.show_system_notification(SystemNotification {
            tag: "convt-update".into(),
            title: "convt is updating".into(),
            body: model::INSTALLING.into(),
            actions: Vec::new(),
        });
    } else if let Some(reply) = request.auth {
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
        match silent {
            // With no window to show it, the HUD follows the conversion.
            Ok(jobs) if !request.show_progress => show_hud(jobs, cx),
            Ok(_) => {}
            Err(e) => {
                tracing::info!(reason = %e, "opening Quick convert instead of converting in place");
                open_quick(request, cx);
            }
        }
    } else {
        open_quick(request, cx);
    }
}

/// The end of onboarding: [`show_main`], or, while the Finder guide is up
/// beside System Settings, once the user is done there (the extension came
/// on, they closed the guide or System Settings, or they left System
/// Settings), so the main window doesn't pull them out of it in the middle
/// of the steps.
pub fn show_main_after_finder_guide(cx: &mut App) {
    match Open::<FinderGuideView>::get(cx) {
        Some((_, guide)) => {
            guide.update(cx, |guide, _| guide.then_main = true);
            cx.observe_release(&guide, |guide, cx| {
                if guide.then_main {
                    cx.defer(show_main);
                }
            })
            .detach();
        }
        None => show_main(cx),
    }
}

/// Opens the main window, or the first-run window until a licensed build
/// finishes it. Closing mid-setup leaves first run unfinished, so the next
/// launch shows it again. A build from source that doesn't check licenses
/// never shows the first-run window.
pub fn show_main(cx: &mut App) {
    #[cfg(all(debug_assertions, not(test)))]
    if let Ok(screen) = std::env::var("CONVT_DEV_SCREEN") {
        let app = model::shared(cx);
        let (display, size) = first_run_bounds(cx);
        match screen.as_str() {
            "onboarding-signin" => {
                show_on(display, size, None, "Welcome to convt", cx, |window, cx| {
                    cx.new(|cx| FirstRunView::new(app, first_run::Screen::Account, window, cx))
                });
                return;
            }
            "onboarding-question" => {
                show_on(display, size, None, "Welcome to convt", cx, |window, cx| {
                    cx.new(|cx| {
                        FirstRunView::new(
                            app,
                            first_run::Screen::Question(first_run::Question::Documents),
                            window,
                            cx,
                        )
                    })
                });
                return;
            }
            "finder-guide" => {
                show_finder_guide(cx);
                return;
            }
            _ => {}
        }
    }
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
    let opened = show_min(
        size(px(1040.), px(640.)),
        size(px(MAIN_MIN_SIZE.0), px(MAIN_MIN_SIZE.1)),
        "convt",
        cx,
        |window, cx| cx.new(|cx| MainView::new(app, window, cx)),
    );
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

/// The tray's Convert Files…: the system file picker, then Quick convert
/// for what was picked, without the main window. Until first run is done,
/// the first-run window comes forward instead.
pub fn convert_files(cx: &mut App) {
    if first_run_pending(cx) {
        open_first_run(cx);
        return;
    }
    // With no window open, convt may not be the active app; the picker
    // should come up in front.
    cx.activate(true);
    let picked = cx.prompt_for_paths(main_window::add_files_prompt(
        cx.can_select_mixed_files_and_dirs(),
    ));
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = picked.await
            && !paths.is_empty()
        {
            cx.update(|cx| {
                open_quick(
                    Request {
                        files: paths,
                        ..Request::default()
                    },
                    cx,
                )
            });
        }
    })
    .detach();
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

/// Quick convert's opening size: room for a typical image's popular formats
/// (3x2 grid), More formats, the Options row and the compact destination/engine
/// strip, with a little air above the footer. The window grows when more shows
/// (`QuickView::sync_window_height`); long lists scroll.
pub(super) const QUICK_SIZE: (f32, f32) = (580., 396.);

/// The smallest each window can be resized to: room for Activity's status
/// column, Quick convert's format grid, and the forms of Settings and Feedback.
pub(super) const MAIN_MIN_SIZE: (f32, f32) = (820., 520.);
pub(super) const QUICK_MIN_SIZE: (f32, f32) = (520., 370.);
pub(super) const SETTINGS_MIN_SIZE: (f32, f32) = (560., 480.);
pub(super) const FEEDBACK_MIN_SIZE: (f32, f32) = (420., 440.);

/// Quick convert's height for `content`, plus the title bar macOS draws
/// inside it.
pub(super) fn quick_height_for(content: f32) -> Pixels {
    px(content
        + if theme::transparent_titlebar() {
            24.
        } else {
            0.
        })
}

/// Quick convert's opening height, plus the title bar macOS draws inside it.
pub(super) fn quick_height() -> Pixels {
    quick_height_for(QUICK_SIZE.1)
}

/// Onboarding's focused window size, centered on the display.
pub(super) const FIRST_RUN_SIZE: (f32, f32) = (540., 496.);

/// The display onboarding opens on: the one with the window in front (the
/// user is looking there), else the primary one. GPUI can't tell where the
/// pointer is before a window exists.
fn first_run_display(cx: &mut App) -> Option<std::rc::Rc<dyn PlatformDisplay>> {
    cx.active_window()
        .and_then(|w| w.update(cx, |_, window, cx| window.display(cx)).ok())
        .flatten()
        .or_else(|| cx.primary_display())
}

/// Onboarding's size on a display whose usable area is `visible`: compact
/// [`FIRST_RUN_SIZE`], clamped to 95% on small displays so it never crosses
/// the edges.
pub(super) fn first_run_fit(visible: Size<Pixels>) -> Size<Pixels> {
    size(
        px(FIRST_RUN_SIZE.0).min(visible.width * 0.95).round(),
        px(FIRST_RUN_SIZE.1).min(visible.height * 0.95).round(),
    )
}

/// Where onboarding opens and how big.
pub(super) fn first_run_bounds(cx: &mut App) -> (Option<DisplayId>, Size<Pixels>) {
    match first_run_display(cx) {
        Some(display) => (
            Some(display.id()),
            first_run_fit(display.visible_bounds().size),
        ),
        None => (None, size(px(FIRST_RUN_SIZE.0), px(FIRST_RUN_SIZE.1))),
    }
}

fn open_first_run(cx: &mut App) {
    let app = model::shared(cx);
    let (display, size) = first_run_bounds(cx);
    show_on(display, size, None, "Welcome to convt", cx, |window, cx| {
        cx.new(|cx| FirstRunView::new(app, first_run::Screen::Account, window, cx))
    });
}

/// Opens the guide beside System Settings for turning on the Finder
/// extension, or plays it again from the start if it's open. Only
/// `finder::open_settings` calls it.
pub fn show_finder_guide(cx: &mut App) {
    if let Some((handle, _)) = Open::<FinderGuideView>::get(cx) {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
    let app = model::shared(cx);
    let (w, h) = finder_guide::SIZE;
    let options = WindowOptions {
        titlebar: None,
        kind: WindowKind::PopUp,
        focus: false,
        // macOS shows it once System Settings' window is up, beside it.
        show: !cfg!(all(target_os = "macos", not(test))),
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        // It is never the active window, and its loop should stay smooth.
        inactive_frame_interval: None,
        ..window_options(size(px(w), px(h)), "Turn on convt in Finder", cx)
    };
    match open_window(options, cx, |window, cx| {
        cx.new(|cx| FinderGuideView::new(app, window, cx))
    }) {
        Ok((handle, view)) => cx.set_global(Open(handle, view.downgrade())),
        Err(e) => tracing::error!(error = %e, "could not open the Finder guide"),
    }
}

/// Opens Settings on `tab`: the main window's Settings page while the main
/// window is open, so there is one place for it, else the Settings window.
pub fn show_settings(tab: SettingsTab, cx: &mut App) {
    open_settings(tab, cx, |_, _, _| {});
}

/// Settings > General scrolled to Updates.
pub fn show_updates(cx: &mut App) {
    open_settings(SettingsTab::General, cx, |view, _, cx| {
        view.reveal_updates(cx)
    });
}

/// [`show_settings`], then `then` on the Settings view that shows it.
fn open_settings(
    tab: SettingsTab,
    cx: &mut App,
    then: impl FnOnce(&mut SettingsView, &mut Window, &mut Context<SettingsView>),
) {
    let app = model::shared(cx);
    app.update(cx, |s, cx| s.refresh_pack(cx));
    let mut then = Some(then);
    if let Some((handle, main)) = Open::<MainView>::get(cx) {
        let shown = handle.update(cx, |_, window, cx| {
            window.activate_window();
            let settings = main.update(cx, |main, cx| {
                let page = match tab {
                    SettingsTab::Presets => main_window::Page::Presets,
                    SettingsTab::Menu => main_window::Page::Menu,
                    SettingsTab::General | SettingsTab::License => main_window::Page::Settings,
                };
                main.set_page(page, cx);
                main.settings.clone()
            });
            settings.update(cx, |view, cx| {
                view.set_tab(tab, cx);
                if let Some(then) = then.take() {
                    then(view, window, cx);
                }
            });
        });
        if shown.is_ok() {
            cx.activate(true);
            return;
        }
    }
    if let Some((handle, view)) = show_min(
        size(px(620.), px(600.)),
        size(px(SETTINGS_MIN_SIZE.0), px(SETTINGS_MIN_SIZE.1)),
        "Settings",
        cx,
        |window, cx| cx.new(|cx| SettingsView::new(app, window, cx)),
    ) {
        let _ = handle.update(cx, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.set_tab(tab, cx);
                if let Some(then) = then.take() {
                    then(view, window, cx);
                }
            })
        });
    }
}

/// Opens the Feedback window, or brings it forward.
pub fn show_feedback(cx: &mut App) {
    let app = model::shared(cx);
    show_min(
        size(px(460.), px(500.)),
        size(px(FEEDBACK_MIN_SIZE.0), px(FEEDBACK_MIN_SIZE.1)),
        "Send feedback",
        cx,
        |window, cx| cx.new(|cx| feedback::FeedbackView::new(app, window, cx)),
    );
}

/// Opens the License tab of Settings, with `key` filled in. Keys from links
/// are never activated without a click: any web page can open one.
pub fn show_license(key: Option<String>, cx: &mut App) {
    open_settings(SettingsTab::License, cx, move |view, window, cx| {
        view.fill_license(key, window, cx)
    });
}

/// Shows the progress HUD for jobs started without a window, or adds them
/// to the one showing. It opens near the top right of the primary display,
/// never takes focus and stays out of the Dock.
pub fn show_hud(jobs: Vec<crate::jobs::JobId>, cx: &mut App) {
    if jobs.is_empty() {
        return;
    }
    if let Some((handle, view)) = Open::<HudView>::get(cx) {
        let more = jobs.clone();
        if handle
            .update(cx, |_, window, cx| {
                view.update(cx, |v, cx| v.add(more, window, cx))
            })
            .is_ok()
        {
            return;
        }
    }
    let app = model::shared(cx);
    let hud = size(px(hud::SIZE.0), px(hud::SIZE.1));
    let display = cx.primary_display();
    let origin = display.as_ref().map_or(point(px(0.), px(0.)), |d| {
        let area = d.visible_bounds();
        point(
            area.right() - hud.width - px(hud::MARGIN),
            area.top() + px(hud::MARGIN),
        )
    });
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds { origin, size: hud })),
        display_id: display.map(|d| d.id()),
        titlebar: None,
        kind: WindowKind::PopUp,
        focus: false,
        show: true,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: if hud::transparent() {
            WindowBackgroundAppearance::Transparent
        } else {
            WindowBackgroundAppearance::Opaque
        },
        app_id: Some("convt".into()),
        ..Default::default()
    };
    // Not `open_window`: the HUD brings back no Dock icon and needs no title
    // bar drawn for it.
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| HudView::new(app, jobs, window, cx))
    }) {
        Ok((handle, view)) => cx.set_global(Open(handle, view.downgrade())),
        Err(e) => tracing::warn!(error = %e, "could not show the progress HUD"),
    }
}

/// Whether the progress HUD is up.
pub(crate) fn hud_open(cx: &App) -> bool {
    Open::<HudView>::get(cx).is_some_and(|(handle, _)| cx.windows().contains(&handle))
}

/// Whether the progress HUD is up and shows how `job` ends.
pub(crate) fn hud_follows(job: crate::jobs::JobId, cx: &App) -> bool {
    Open::<HudView>::get(cx).is_some_and(|(handle, view)| {
        cx.windows().contains(&handle) && view.read(cx).jobs.contains(&job)
    })
}

/// Whether a window other than the HUD is open. The HUD alone never brings
/// convt into the Dock.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn has_regular_windows(cx: &App) -> bool {
    !only_hud_open(cx)
}

/// The HUD's or Quick convert's Details for a failure: the main window on
/// Activity, with each failed record opened to its full message (matching the
/// exact recorded id when known, or falling back to the newest failed record
/// for that input).
pub(crate) fn show_failures(failures: Vec<(Option<i64>, PathBuf)>, cx: &mut App) {
    if first_run_pending(cx) {
        open_first_run(cx);
        return;
    }
    if let Some((_, view)) = open_main(cx) {
        view.update(cx, |view, cx| view.expand_failures(&failures, cx));
    }
}

/// Whether no window is open but the HUD. The HUD alone doesn't keep convt
/// open: with no tray, it quits once silent conversions are done.
pub(crate) fn only_hud_open(cx: &App) -> bool {
    let hud = Open::<HudView>::get(cx).map(|(handle, _)| handle);
    cx.windows().iter().all(|w| Some(*w) == hud)
}

pub fn open_quick(request: Request, cx: &mut App) {
    let app = model::shared(cx);
    // Status is read offline; it may have changed through the CLI.
    app.update(cx, |s, cx| s.refresh_pack(cx));
    let mut options = WindowOptions {
        window_min_size: Some(size(px(QUICK_MIN_SIZE.0), px(QUICK_MIN_SIZE.1))),
        ..window_options(size(px(QUICK_SIZE.0), quick_height()), "Convert", cx)
    };
    // Position slightly above geometric center so expanding Options or the
    // PNG warning grows downward cleanly without crossing the bottom edge.
    if let Some(display) = cx.primary_display() {
        let visible = display.visible_bounds();
        let w = px(QUICK_SIZE.0);
        let h = quick_height();
        let x = visible.origin.x + ((visible.size.width - w).max(px(0.)) / 2.).floor();
        let room_y = ((visible.size.height - px(780.)).max(px(32.)) / 2.).floor();
        let y = visible.origin.y + room_y;
        options.window_bounds = Some(WindowBounds::Windowed(Bounds::new(point(x, y), size(w, h))));
    }
    match open_window(options, cx, |window, cx| {
        cx.new(|cx| {
            let view = QuickView::new(app, request, window, cx);
            // Enter converts and Escape closes from the start.
            window.focus(&view.focus, cx);
            view
        })
    }) {
        Ok((handle, view)) => {
            // Taller than the opening size when there is more to show: the
            // PNG warning, a license banner or the pack card.
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| view.sync_window_height(window, cx))
            });
            // Each request gets its own window; the global tracks the newest.
            cx.set_global(Open(handle, view.downgrade()));
        }
        Err(e) => tracing::error!(error = %e, "could not open Quick convert"),
    }
    cx.activate(true);
}

/// Opens the menu bar popover. Nothing calls it outside tests: the tray icon
/// (`tray.rs`) shows a native menu instead, because tray-icon can't anchor a
/// GPUI window to the icon or take a file dropped on it.
#[cfg_attr(not(test), allow(dead_code))]
pub fn open_popover(cx: &mut App) -> Option<(AnyWindowHandle, Entity<PopoverView>)> {
    let app = model::shared(cx);
    show(size(px(340.), px(520.)), "convt", cx, |window, cx| {
        cx.new(|cx| PopoverView::new(app, window, cx))
    })
}

fn window_options(size: Size<Pixels>, title: &str, cx: &App) -> WindowOptions {
    window_options_on(None, size, title, cx)
}

fn window_options_on(
    display: Option<DisplayId>,
    size: Size<Pixels>,
    title: &str,
    cx: &App,
) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(display, size, cx))),
        display_id: display,
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
    if let Some(bytes) = o.target_size {
        parts.push(format!("under {}", convt_core::format_size(bytes)));
    }
    if o.strip_metadata {
        parts.push("no metadata".into());
    }
    match (o.trim_start, o.trim_end) {
        (Some(start), Some(end)) => parts.push(format!("{start}s to {end}s")),
        (Some(start), None) => parts.push(format!("from {start}s")),
        (None, Some(end)) => parts.push(format!("first {end}s")),
        (None, None) => {}
    }
    if let Some(fps) = o.fps {
        parts.push(format!("{fps} fps"));
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
    // The License tab has the sign-in button and follows the flow.
    let sign_in = matches!(state, State::SignInNeeded).then(|| {
        theme::Button::primary("sign-in-banner", "Sign in")
            .small()
            .build(p)
            .on_click(|_, _, cx| show_license(None, cx))
    });
    let download = matches!(state, State::NotCovered(_)).then(|| {
        theme::Button::secondary("download", "Download a covered build")
            .small()
            .build(p)
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
                    .flex_wrap()
                    .gap(px(8.))
                    .children(sign_in)
                    .child(
                        theme::Button::brand("buy", buy)
                            .small()
                            .build(p)
                            .on_click(|_, _, cx| cx.open_url(BUY_URL)),
                    )
                    .children(download)
                    .child(
                        theme::Button::secondary("enter-license", "Enter license")
                            .small()
                            .build(p)
                            .on_click(|_, _, cx| show_license(None, cx)),
                    ),
            ),
        p,
    ))
}

/// One plain line for an engine's error: what went wrong in words a person
/// reads, without the tool's name, FFmpeg's `[demuxer @ 0x…]` tags or the
/// rest of its log. The raw message stays in Activity's Details.
pub(super) fn friendly_error(raw: &str) -> String {
    let lower = raw.to_lowercase();
    let known = [
        (
            &[
                "moov atom not found",
                "invalid data found when processing input",
            ][..],
            "File is damaged or unreadable.",
        ),
        (
            &["no such file", "cannot find the file"][..],
            "The original file could not be found.",
        ),
        (
            &["permission denied"][..],
            "Permission denied reading or writing the file.",
        ),
        (
            &["no space left"][..],
            "Not enough disk space to save the file.",
        ),
    ];
    if let Some((_, words)) = known
        .iter()
        .find(|(needles, _)| needles.iter().any(|n| lower.contains(n)))
    {
        return (*words).to_string();
    }
    let mut text = raw.trim();
    for tool in ["ffmpeg failed:", "sips failed:", "libreoffice failed:"] {
        if text.len() >= tool.len()
            && text.is_char_boundary(tool.len())
            && text[..tool.len()].eq_ignore_ascii_case(tool)
        {
            text = text[tool.len()..].trim_start();
        }
    }
    let line = text
        .lines()
        .map(|line| collapse_whitespace(&strip_address_tags(line)))
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if line.is_empty() {
        return raw
            .trim()
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
    }
    // The first sentence, when a line runs on into more.
    match line.find(". ") {
        Some(end) => line[..=end].to_string(),
        None => line,
    }
}

/// Drops FFmpeg's `[mov,mp4,m4a @ 0x57dd70369580]` tags.
fn strip_address_tags(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find('[') {
        let Some(len) = rest[start..].find(']') else {
            break;
        };
        let tag = &rest[start..start + len + 1];
        out.push_str(&rest[..start]);
        if !tag.contains(" @ 0x") {
            out.push_str(tag);
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    out
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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
    use super::{friendly_error, human_size, time_left};

    #[test]
    fn engine_errors_read_as_one_plain_line() {
        let moov = "ffmpeg failed: [mov,mp4,m4a,3gp,3g2,mj2 @ 0x57dd70369580] moov atom not found\n\
                    [in#0 @ 0x57dd70368f40] Error opening input: Invalid data found when processing input\n\
                    Error opening input file /tmp/broken.mp4.\n\
                    Error opening input files: Invalid data found when processing input";
        assert_eq!(friendly_error(moov), "File is damaged or unreadable.");
        assert_eq!(
            friendly_error("sips failed: /Users/a/x.heic: Permission denied"),
            "Permission denied reading or writing the file."
        );
        assert_eq!(
            friendly_error("open /tmp/gone.png: No such file or directory (os error 2)"),
            "The original file could not be found."
        );
        assert_eq!(
            friendly_error("write failed: No space left on device"),
            "Not enough disk space to save the file."
        );
        // Plain messages stay as they are, so Activity offers no Details.
        let plain = "The image is too large to convert.";
        assert_eq!(friendly_error(plain), plain);
        assert_eq!(
            friendly_error("  unsupported format  "),
            "unsupported format"
        );
        // Unknown FFmpeg errors lose the tool name and the address tags,
        // and keep their first line.
        assert_eq!(
            friendly_error(
                "ffmpeg failed: [libx264 @ 0x6000]   height not divisible by 2 (1081x721)\n\
                 Error while opening encoder"
            ),
            "height not divisible by 2 (1081x721)"
        );
        assert_eq!(
            friendly_error("libreoffice failed: Source file could not be loaded. Details follow"),
            "Source file could not be loaded."
        );
        assert_eq!(friendly_error("[keep this] tag"), "[keep this] tag");
    }

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
