//! The convt desktop app. One process runs per user: a second launch hands
//! its files to the running app and exits.

mod account;
mod clock;
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

use std::process::ExitCode;
use std::sync::Arc;

use futures::StreamExt;
use futures::channel::mpsc::unbounded;
use gpui_kit::{App, AppContext};

use crate::instance::Role;
use crate::model::{AppState, Paths, Shared};
use crate::request::{Command, Request, USAGE};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let cwd = std::env::current_dir().unwrap_or_default();
    let request = match request::parse_args(std::env::args_os().skip(1).collect(), &cwd) {
        Ok(Command::Run(request)) => request,
        Ok(Command::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Command::Version) => {
            println!("convt-app {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("convt-app: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let primary = match instance::claim(&instance::runtime_dir(), &request) {
        Ok(Role::Primary(primary)) => primary,
        Ok(Role::Forwarded) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("convt-app: {e}");
            return ExitCode::FAILURE;
        }
    };
    run(primary, request);
    ExitCode::SUCCESS
}

fn run(primary: instance::Primary, first: Request) {
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
        let state = cx.new(|cx| AppState::new(Arc::new(pack::Engines), Paths::from_env(), cx));
        #[cfg(target_os = "macos")]
        macos::init(&state, tx.clone(), cx);
        cx.set_global(Shared(state.clone()));
        cx.on_window_closed(last_window_closed).detach();
        // One of the two network calls the app makes by itself: while signed in, at
        // most once a day, ask convt.app for the current Pro key.
        state.update(cx, |s, cx| s.renew_on_launch(cx));
        // The other: when update checks are on, at most once a day, fetch the
        // signed list of releases.
        state.update(cx, |s, cx| s.check_updates_on_launch(cx));

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
    let keep_running = cfg!(target_os = "macos") && state.read(cx).settings.menu_bar_icon;
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
}
