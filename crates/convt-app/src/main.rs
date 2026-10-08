//! The convt desktop app. One process runs per user: a second launch hands
//! its files to the running app and exits.

// Release Windows builds must not allocate a console. A console subsystem
// binary opens a black "convt" terminal with the Start-menu shortcut, and
// closing that window kills the app. Debug builds keep a console for logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod account;
mod automation;
mod clipboard;
mod clock;
// Quick convert's Cloud choice uses this; remove the allow once it does.
#[allow(dead_code)]
mod cloud;
mod finder;
mod history;
mod instance;
mod jobs;
#[cfg(target_os = "macos")]
mod macos;
mod model;
mod pack;
mod placeholder;
mod request;
mod settings;
mod thumbs;
mod tray;
mod ui;
mod update;

use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

use futures::StreamExt;
use futures::channel::mpsc::unbounded;
use gpui_kit::{App, AppContext};

use crate::instance::Role;
use crate::model::{AppState, Paths, Shared};
use crate::request::{Command, Request, USAGE};

/// Writes `msg` plus a newline. Ignores failure so a GUI-subsystem process
/// without a console does not panic on stdout/stderr.
fn emit(mut w: impl Write, msg: &str) {
    let _ = writeln!(w, "{msg}");
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let cwd = std::env::current_dir().unwrap_or_default();
    let request = match request::parse_args(std::env::args_os().skip(1).collect(), &cwd) {
        Ok(Command::Run(request)) => request,
        Ok(Command::Help) => {
            emit(std::io::stdout(), USAGE);
            return ExitCode::SUCCESS;
        }
        Ok(Command::Version) => {
            emit(
                std::io::stdout(),
                &format!("convt-app {}", env!("CARGO_PKG_VERSION")),
            );
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            emit(std::io::stderr(), &format!("convt-app: {e}\n\n{USAGE}"));
            return ExitCode::from(2);
        }
    };
    let primary = match instance::claim(&instance::runtime_dir(), &request) {
        Ok(Role::Primary(primary)) => primary,
        Ok(Role::Forwarded) => return ExitCode::SUCCESS,
        Err(e) => {
            emit(std::io::stderr(), &format!("convt-app: {e}"));
            return ExitCode::FAILURE;
        }
    };
    run(primary, request);
    ExitCode::SUCCESS
}

/// Ignore hangup so closing a launching terminal (or AppImage wrapper) does
/// not kill the GUI. `--help` / `--version` exit before this runs.
#[cfg(unix)]
fn ignore_hangup() {
    // SAFETY: SIG_IGN is a valid, process-wide handler; no memory is touched.
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }
}

fn run(primary: instance::Primary, first: Request) {
    #[cfg(unix)]
    ignore_hangup();
    let (tx, mut rx) = unbounded::<Request>();
    let app = gpui_kit::application().with_assets(ui::assets());
    let urls = tx.clone();
    app.on_open_urls(move |links| {
        // The Finder extension's requests, and files opened with convt.
        #[cfg(target_os = "macos")]
        let links = {
            let (requests, links) = macos::open_urls(links);
            for req in requests {
                drop(urls.unbounded_send(req));
            }
            links
        };
        for link in links {
            match request::parse_url(&link) {
                Ok(req) => drop(urls.unbounded_send(req)),
                Err(e) => tracing::warn!(error = %e, "ignored a convt link"),
            }
        }
    });
    app.on_reopen(ui::show_main);
    app.run(move |cx| {
        // Notifications need the app's identity, set before any is posted.
        cx.set_app_identity("app.convt.desktop", "convt");
        gpui_kit::init(cx);
        ui::theme::init(cx);
        ui::menus::init(cx);
        let state = cx.new(|cx| AppState::new(Arc::new(pack::Engines), Paths::from_env(), cx));
        #[cfg(target_os = "macos")]
        macos::init(&state, tx.clone(), cx);
        cx.set_global(Shared(state.clone()));
        cx.on_window_closed(last_window_closed).detach();
        // One of the two network calls the app makes by itself: while signed in, at
        // most once a day, ask convt.app for the current Pro key.
        state.update(cx, |s, cx| s.renew_on_launch(cx));
        // The other: while automatic update checks are on, at launch and then
        // every few hours, fetch the signed list of releases.
        state.update(cx, |s, cx| s.start_update_checks(cx));

        primary.listen(move |req| drop(tx.unbounded_send(req)));
        cx.spawn(async move |cx| {
            while let Some(req) = rx.next().await {
                cx.update(|cx| ui::route(req, cx));
            }
        })
        .detach();
        #[cfg(target_os = "macos")]
        let first = macos::launch_requests(first);
        #[cfg(not(target_os = "macos"))]
        let first = vec![first];
        for req in first {
            ui::route(req, cx);
        }
    });
    // In case the platform returns without running the quit observers.
    thumbs::shutdown();
}

/// Quits with the last window, unless conversions are still running; then
/// quits when they finish.
fn last_window_closed(cx: &mut App, _: gpui_kit::WindowId) {
    if !cx.windows().is_empty() {
        return;
    }
    let state = model::shared(cx);
    let keep_running = state.read(cx).settings.stays_in_menu_bar();
    if keep_running {
        // macOS keeps the process alive when the menu bar item is enabled.
        // Calling cx.quit() from the window-closed observer starts GPUI's
        // teardown while AppKit is still unwinding the last NSWindow; a
        // deferred update callback can then cross that teardown boundary.
        return;
    }
    if should_quit_after_last_window(keep_running, state.read(cx).queue.active()) {
        cx.quit();
    } else {
        state.update(cx, |s, _| s.quit_when_idle = true);
    }
}

fn should_quit_after_last_window(menu_bar_icon: bool, active_jobs: usize) -> bool {
    !menu_bar_icon && active_jobs == 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn menu_bar_mode_keeps_the_process_alive_after_last_window() {
        assert!(!super::should_quit_after_last_window(true, 0));
        assert!(!super::should_quit_after_last_window(true, 2));
        assert!(super::should_quit_after_last_window(false, 0));
        assert!(!super::should_quit_after_last_window(false, 1));
    }

    #[test]
    fn release_windows_builds_use_the_windows_subsystem() {
        let src = include_str!("main.rs");
        assert!(
            src.contains("windows_subsystem = \"windows\""),
            "convt-app must set windows_subsystem on Windows release builds"
        );
        assert!(
            src.contains("cfg_attr(all(windows, not(debug_assertions))"),
            "debug and test binaries should keep a console"
        );
    }

    #[test]
    fn help_and_errors_do_not_panic_without_a_console() {
        super::emit(std::io::sink(), "usage");
        super::emit(std::io::stderr(), "");
    }

    #[cfg(unix)]
    #[test]
    fn ignoring_hangup_keeps_the_process_alive() {
        super::ignore_hangup();
        // SAFETY: raise(2) delivers SIGHUP to this test process only.
        unsafe {
            assert_eq!(libc::raise(libc::SIGHUP), 0);
        }
    }
}
