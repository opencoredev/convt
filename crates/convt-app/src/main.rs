//! The convt desktop app. One process runs per user: a second launch hands
//! its files to the running app and exits.

mod clock;
mod history;
mod instance;
mod jobs;
mod model;
mod pack;
mod placeholder;
mod request;
mod settings;
mod thumbs;
mod tray;
mod ui;

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
        cx.set_global(Shared(state));
        cx.on_window_closed(last_window_closed).detach();

        primary.listen(move |req| drop(tx.unbounded_send(req)));
        cx.spawn(async move |cx| {
            while let Some(req) = rx.next().await {
                cx.update(|cx| ui::route(req, cx));
            }
        })
        .detach();
        ui::route(first, cx);
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
    if state.read(cx).queue.active() == 0 {
        cx.quit();
    } else {
        state.update(cx, |s, _| s.quit_when_idle = true);
    }
}
