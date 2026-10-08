//! Keeping convt running in the background: the tray icon follows the
//! setting, closing the last window keeps the app only while the icon is up,
//! and Quit works from the keyboard and the tray. The icon is a fake that
//! records what the app asked of it; the test platform's quit does nothing,
//! so `menu::Quits` counts the quits.

use std::cell::RefCell;

use futures::channel::mpsc::UnboundedSender;

use super::*;
use crate::menu::{self, Quits};
use crate::tray::{Event, Icon};

#[derive(Default)]
struct FakeTray {
    /// Make the next spawns fail, like a desktop with no tray.
    fail: bool,
    spawns: usize,
    live: usize,
    tooltip: String,
    events: Option<UnboundedSender<Event>>,
}

thread_local! {
    static FAKE: RefCell<FakeTray> = RefCell::default();
}

struct FakeIcon;

impl Icon for FakeIcon {
    fn set_tooltip(&mut self, tooltip: &str) {
        FAKE.with(|t| t.borrow_mut().tooltip = tooltip.into());
    }
}

impl Drop for FakeIcon {
    fn drop(&mut self) {
        FAKE.with(|t| t.borrow_mut().live -= 1);
    }
}

fn spawn(events: UnboundedSender<Event>) -> Result<Box<dyn Icon>, String> {
    FAKE.with(|t| {
        let mut t = t.borrow_mut();
        if t.fail {
            return Err("no StatusNotifierHost exists".into());
        }
        t.spawns += 1;
        t.live += 1;
        t.events = Some(events);
        Ok(Box::new(FakeIcon) as Box<dyn Icon>)
    })
}

fn fake<R>(f: impl FnOnce(&FakeTray) -> R) -> R {
    FAKE.with(|t| f(&t.borrow()))
}

/// What `main.rs` sets up around the windows: keys, menus, the tray and the
/// last-window rule.
fn background(f: &Fixture, cx: &mut TestAppContext, tray_works: bool) {
    FAKE.with(|t| t.borrow_mut().fail = !tray_works);
    let app = f.app.clone();
    cx.update(|cx| {
        menu::init(cx);
        tray::init(&app, spawn, cx);
        cx.on_window_closed(crate::last_window_closed).detach();
    });
    cx.run_until_parked();
}

fn quits(cx: &mut TestAppContext) -> usize {
    cx.run_until_parked();
    cx.read(|cx| cx.try_global::<Quits>().map_or(0, |q| q.0))
}

fn send(cx: &mut TestAppContext, event: Event) {
    let events = fake(|t| t.events.clone()).expect("the icon is up");
    events.unbounded_send(event).unwrap();
    cx.run_until_parked();
}

fn close(cx: &mut TestAppContext, window: AnyWindowHandle) {
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
}

fn keeps_running(f: &Fixture, cx: &mut TestAppContext) -> bool {
    cx.read(|cx| tray::keeps_running(f.app.read(cx).settings.menu_bar_icon, cx))
}

#[gpui_kit::test]
fn the_tray_icon_follows_the_background_setting(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    background(&f, cx, true);
    assert_eq!(fake(|t| (t.spawns, t.live)), (1, 1));
    assert_eq!(fake(|t| t.tooltip.clone()), "convt");
    assert!(keeps_running(&f, cx));

    let (window, _) = f.settings(SettingsTab::General, cx);
    let note = label(cx, window, "menu-bar-icon-note").unwrap();
    let quit_key = if cfg!(target_os = "macos") {
        "⌘Q"
    } else {
        "Ctrl+Q"
    };
    assert!(note.contains(quit_key), "{note}");

    click(cx, window, "menu-bar-icon");
    assert_eq!(fake(|t| t.live), 0, "turning it off removes the icon");
    assert!(!keeps_running(&f, cx));
    assert!(f.settings_file().contains("menu_bar_icon = false"));

    click(cx, window, "menu-bar-icon");
    assert_eq!(
        fake(|t| (t.spawns, t.live)),
        (2, 1),
        "and on brings it back"
    );
    assert!(keeps_running(&f, cx));
}

#[gpui_kit::test]
fn the_tooltip_counts_running_conversions(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    background(&f, cx, true);
    let files: Vec<PathBuf> = (0..3).map(|i| f.png(&format!("{i}.png"))).collect();
    let app = f.app.clone();
    cx.update(|cx| {
        app.update(cx, |s, cx| {
            s.update_settings(|s| s.concurrency = Some(1), cx);
            let to = convt_core::format_by_id("jpeg").unwrap();
            s.convert(&files, to, &Options::default(), cx).unwrap();
        })
    });
    assert_eq!(fake(|t| t.tooltip.clone()), "convt: converting 3 files");
    wait_until(cx, "the batch to finish", |cx| {
        app.read(cx).queue.active() == 0
    });
    assert_eq!(fake(|t| t.tooltip.clone()), "convt");
}

#[gpui_kit::test]
fn closing_the_last_window_keeps_convt_running_while_the_tray_is_up(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    background(&f, cx, true);
    cx.update(super::super::show_main);
    let (window, _) = window_of::<MainView>(cx);
    close(cx, window);
    assert!(cx.update(|cx| cx.windows().is_empty()));
    assert_eq!(quits(cx), 0, "the tray keeps it running");

    // The tray brings the windows back and quits.
    send(cx, Event::Open);
    window_of::<MainView>(cx);
    send(cx, Event::Settings);
    window_of::<SettingsView>(cx);
    send(cx, Event::Quit);
    assert_eq!(quits(cx), 1);
}

#[gpui_kit::test]
fn without_a_tray_closing_the_last_window_quits(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    background(&f, cx, false);
    assert_eq!(fake(|t| t.live), 0);
    assert!(
        !keeps_running(&f, cx),
        "the setting alone keeps nothing running"
    );
    let (window, _) = f.main(cx);
    close(cx, window);
    assert_eq!(quits(cx), 1);
}

#[gpui_kit::test]
fn with_the_setting_off_closing_the_last_window_quits(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let app = f.app.clone();
    cx.update(|cx| {
        app.update(cx, |s, cx| {
            s.update_settings(|s| s.menu_bar_icon = false, cx)
        })
    });
    background(&f, cx, true);
    assert_eq!(fake(|t| t.spawns), 0, "no icon while it's off");
    let (main, _) = f.main(cx);
    let (settings, _) = f.settings(SettingsTab::General, cx);
    close(cx, main);
    assert_eq!(quits(cx), 0, "Settings is still open");
    close(cx, settings);
    assert_eq!(quits(cx), 1);
}

/// Converts one file from the command line with no window open, and returns
/// how often the app asked to quit by the time the job finished.
fn quits_after_a_silent_conversion(cx: &mut TestAppContext, tray_works: bool) -> usize {
    let f = Fixture::new(cx);
    background(&f, cx, tray_works);
    let png = f.png("silent.png");
    cx.update(|cx| super::super::route(cli(vec![png], Some("jpeg"), None), cx));
    let job = f.last_job(cx);
    assert_eq!(quits(cx), 0, "not while the job runs");
    wait_until(cx, "the job to finish", |cx| {
        f.app
            .read(cx)
            .entry(job)
            .is_some_and(|e| e.status.is_finished())
    });
    quits(cx)
}

#[gpui_kit::test]
fn a_silent_conversion_keeps_convt_running_while_the_tray_is_up(cx: &mut TestAppContext) {
    assert_eq!(quits_after_a_silent_conversion(cx, true), 0);
}

#[gpui_kit::test]
fn without_a_tray_a_silent_conversion_quits_when_done(cx: &mut TestAppContext) {
    assert_eq!(quits_after_a_silent_conversion(cx, false), 1);
}

#[gpui_kit::test]
fn a_tray_host_that_goes_away_stops_keeping_convt_running(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    background(&f, cx, true);
    let events = fake(|t| t.events.clone()).unwrap();
    // The icon reports it from its own thread.
    std::thread::spawn(move || events.unbounded_send(Event::Lost).unwrap())
        .join()
        .unwrap();
    assert_eq!(quits(cx), 1, "no window is open and nothing else keeps it");
    assert!(!keeps_running(&f, cx));
}

#[gpui_kit::test]
fn the_quit_key_quits_from_any_window(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    background(&f, cx, true);
    let key = if cfg!(target_os = "macos") {
        "cmd-q"
    } else {
        "ctrl-q"
    };
    let (window, _) = f.main(cx);
    cx.simulate_keystrokes(window, key);
    assert_eq!(quits(cx), 1);
    let (window, _) = f.settings(SettingsTab::General, cx);
    cx.simulate_keystrokes(window, key);
    assert_eq!(quits(cx), 2);
    let png = f.png("quick.png");
    let (window, _) = f.quick(cli(vec![png], None, None), cx);
    cx.simulate_keystrokes(window, key);
    assert_eq!(quits(cx), 3);
}

/// Starts "Restart to update" with the installer held mid-install.
fn installing(f: &Fixture, cx: &mut TestAppContext) -> Arc<TestInstaller> {
    let (_, installer) = f.self_installing(cx);
    installer.hold.store(true, Ordering::SeqCst);
    ready(f, cx);
    cx.update(|cx| f.app.update(cx, |s, cx| s.restart_to_update(cx)));
    wait_until(cx, "the install started", |_| {
        installer.started.load(Ordering::SeqCst) == 1
    });
    assert!(cx.read(|cx| f.app.read(cx).installing()));
    installer
}

#[gpui_kit::test]
fn quitting_waits_for_an_install_to_finish(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, Some(&license_key("a@b.c", "2027-10-01")));
    background(&f, cx, true);
    let installer = installing(&f, cx);
    // Every way of quitting: the key, the tray, and the last window closing
    // with no tray.
    let key = if cfg!(target_os = "macos") {
        "cmd-q"
    } else {
        "ctrl-q"
    };
    let (window, _) = f.main(cx);
    cx.simulate_keystrokes(window, key);
    send(cx, Event::Quit);
    assert_eq!(quits(cx), 0, "not while the install runs");
    // Turning update checks off doesn't stop it either.
    cx.update(|cx| f.app.update(cx, |s, cx| s.set_update_checks(false, cx)));
    assert!(cx.read(|cx| f.app.read(cx).installing()));
    installer.hold.store(false, Ordering::SeqCst);
    wait_until(cx, "installed", |_| {
        !installer.installed.lock().unwrap().is_empty()
    });
    assert_eq!(quits(cx), 1, "one quit, once it's done");
}

#[gpui_kit::test]
fn a_quit_asked_for_during_a_failed_install_still_quits(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, Some(&license_key("a@b.c", "2027-10-01")));
    background(&f, cx, true);
    let installer = installing(&f, cx);
    *installer.fail.lock().unwrap() = Some("convt can't write to /opt/apps.".into());
    send(cx, Event::Quit);
    assert_eq!(quits(cx), 0);
    installer.hold.store(false, Ordering::SeqCst);
    wait_until(cx, "the install failed", |cx| {
        matches!(f.app.read(cx).update, Update::InstallFailed { .. })
    });
    assert_eq!(quits(cx), 1);

    // Without a quit asked for, a failed install keeps convt running.
    let g = Fixture::licensed(cx, None, Some(&license_key("a@b.c", "2027-10-01")));
    let installer = installing(&g, cx);
    *installer.fail.lock().unwrap() = Some("no".into());
    installer.hold.store(false, Ordering::SeqCst);
    wait_until(cx, "the second install failed", |cx| {
        matches!(g.app.read(cx).update, Update::InstallFailed { .. })
    });
    assert_eq!(quits(cx), 1);
    // And quitting works normally again.
    send(cx, Event::Quit);
    assert_eq!(quits(cx), 2);
}

#[gpui_kit::test]
fn conversions_wait_while_an_update_installs(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, Some(&license_key("a@b.c", "2027-10-01")));
    background(&f, cx, true);
    let installer = installing(&f, cx);
    let jobs = f.jobs(cx);
    // A Finder or command-line conversion: refused with a notification,
    // and no window opens.
    let png = f.png("late.png");
    let windows = cx.update(|cx| cx.windows().len());
    cx.update(|cx| super::super::route(cli(vec![png.clone()], Some("jpeg"), None), cx));
    cx.update(|cx| super::super::route(cli(vec![png.clone()], None, None), cx));
    cx.run_until_parked();
    assert_eq!(f.jobs(cx), jobs);
    assert_eq!(cx.update(|cx| cx.windows().len()), windows);
    let shown = cx.shown_system_notifications();
    assert!(
        shown
            .iter()
            .any(|n| n.body.contains("installing an update")),
        "{:?}",
        shown.iter().map(|n| n.body.to_string()).collect::<Vec<_>>()
    );
    // Add files, Quick convert, automations and Retry all queue through here.
    let refused = cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            let to = convt_core::format_by_id("jpeg").unwrap();
            s.convert(std::slice::from_ref(&png), to, &Options::default(), cx)
        })
    });
    assert_eq!(refused, Err(crate::model::INSTALLING.to_string()));
    installer.hold.store(false, Ordering::SeqCst);
    wait_until(cx, "installed", |_| {
        !installer.installed.lock().unwrap().is_empty()
    });
    // Nothing was running, so it quits for the relaunch.
    assert_eq!(quits(cx), 1);
}
