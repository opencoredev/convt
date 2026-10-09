//! The update check as the windows show it: a card in the main window's
//! sidebar when a newer build is out, downloading or ready to install, and
//! the Update checks row in Settings. Failed checks appear only in Settings;
//! a failed download or install shows in both, with the download page as the
//! way out.

use convt_license::client::{DOWNLOAD_URL, State};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{self, Clickable, Palette, mono, primary_button, text, text_button};
use crate::model::AppState;
use crate::update::Update;

fn line(id: &'static str, message: impl Into<SharedString>, color: Hsla) -> Clickable {
    let message = message.into();
    div()
        .id(id)
        .test_support()
        .aria_label(message.clone())
        .child(text(12., 17., color).child(message))
}

fn open(url: String) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, _, cx| cx.open_url(&url)
}

fn act(
    app: &Entity<AppState>,
    f: fn(&mut AppState, &mut Context<AppState>),
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let app = app.clone();
    move |_, _, cx| app.update(cx, f)
}

fn downloading(version: &str, percent: u8) -> String {
    format!("Downloading convt {version}… {percent}%")
}

/// The buttons a state offers, shared by the card and Settings. Empty for
/// states with nothing to do.
fn actions(app: &Entity<AppState>, state: &AppState, p: &Palette) -> Vec<Clickable> {
    let self_install = state.update_config.install.is_some();
    match &state.update {
        Update::Available { .. } if self_install => vec![
            text_button("update-install", "Update", p.green, 12.)
                .on_click(act(app, AppState::download_update)),
        ],
        Update::Available { .. } => vec![
            text_button("update-download", "Download", p.green, 12.)
                .on_click(open(DOWNLOAD_URL.to_string())),
        ],
        Update::Ready { .. } => vec![
            primary_button("update-restart", "Restart to update", 12., false)
                .on_click(act(app, AppState::restart_to_update)),
        ],
        Update::InstallFailed { .. } => vec![
            text_button("update-retry", "Try again", p.green, 12.)
                .on_click(act(app, AppState::download_update)),
            text_button("update-download", "Download instead", p.secondary, 12.)
                .on_click(open(DOWNLOAD_URL.to_string())),
        ],
        _ => Vec::new(),
    }
}

/// The sidebar card: only when a newer build is out.
pub fn sidebar_card(app: &Entity<AppState>, p: &Palette, cx: &App) -> Option<Clickable> {
    let state = app.read(cx);
    let (title, detail, buttons) = match &state.update {
        Update::Available { version, .. } => (
            "Update available".to_string(),
            format!("convt {version}"),
            actions(app, state, p),
        ),
        Update::Downloading { version, percent } => (
            "Update available".to_string(),
            downloading(version, *percent),
            Vec::new(),
        ),
        Update::Ready { version, .. } => (
            format!("convt {version} is ready"),
            state
                .updater
                .notice
                .clone()
                .unwrap_or_else(|| "Restarting takes a few seconds.".into()),
            actions(app, state, p),
        ),
        Update::Installing { version } => (
            format!("Installing convt {version}…"),
            "convt restarts when it's done.".to_string(),
            Vec::new(),
        ),
        Update::InstallFailed { version, why } => (
            format!("convt {version} didn't install"),
            why.clone(),
            actions(app, state, p),
        ),
        Update::NotCovered {
            version,
            purchase_url,
            ..
        } => (
            "New version".to_string(),
            format!("convt {version} needs a renewed license"),
            vec![
                text_button("update-renew", "Renew to update", p.green, 12.)
                    .on_click(open(purchase_url.clone())),
            ],
        ),
        _ => return None,
    };
    Some(
        div()
            .id("update-card")
            .test_support()
            .aria_label(SharedString::from(format!("{title}: {detail}")))
            .flex()
            .flex_col()
            .gap(px(6.))
            .mb(px(8.))
            .p(px(12.))
            .rounded(px(8.))
            .bg(p.trial_card)
            .border_1()
            .border_color(p.chrome_border)
            .child(
                text(12., 16., p.text)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(text(12., 16., p.secondary).child(detail))
            .when(!buttons.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(12.))
                        .children(
                            buttons
                                .into_iter()
                                .map(|b| b.font_weight(FontWeight::MEDIUM)),
                        ),
                )
            }),
    )
}

/// The Update checks row in Settings, General: the switch, what the last
/// check found, and what to do about it.
pub fn settings_row(app: &Entity<AppState>, p: &Palette, cx: &App) -> Div {
    let state = app.read(cx);
    let on = state.settings.update_checks;
    let switch = theme::switch("update-checks", on, false, p).on_click({
        let app = app.clone();
        move |_, _, cx| app.update(cx, |s, cx| s.set_update_checks(!on, cx))
    });
    let last = state
        .settings
        .update_checked
        .as_deref()
        .map_or("Not checked yet.".to_string(), |d| {
            format!("Last checked {d}.")
        });
    let check_now = text_button("check-updates", "Check now", p.green, 12.).on_click({
        let app = app.clone();
        move |_, _, cx| app.update(cx, |s, cx| s.check_updates(cx))
    });
    let (status, action): (Clickable, Vec<Clickable>) = if !on {
        (
            line(
                "update-status",
                "Off. convt won't look for new versions.",
                p.secondary,
            ),
            Vec::new(),
        )
    } else {
        match &state.update {
            Update::Idle => (line("update-status", last, p.secondary), Vec::new()),
            Update::Checking => (line("update-status", "Checking…", p.secondary), Vec::new()),
            Update::UpToDate => (
                line("update-status", "convt is up to date.", p.secondary),
                Vec::new(),
            ),
            Update::Available {
                version,
                date,
                uncovered,
            } => {
                let mut message = format!("convt {version} is out, built {date}.");
                if let Some(newer) = uncovered {
                    message.push_str(&format!(" {newer} needs a renewed license."));
                }
                (
                    line("update-status", message, p.green),
                    actions(app, state, p),
                )
            }
            Update::Downloading { version, percent } => (
                line("update-status", downloading(version, *percent), p.secondary),
                Vec::new(),
            ),
            Update::Ready { version, .. } => (
                line(
                    "update-status",
                    match &state.updater.notice {
                        Some(notice) => format!("convt {version} is ready. {notice}"),
                        None => format!("convt {version} is ready to install."),
                    },
                    p.green,
                ),
                actions(app, state, p),
            ),
            Update::Installing { version } => (
                line(
                    "update-status",
                    format!("Installing convt {version}…"),
                    p.secondary,
                ),
                Vec::new(),
            ),
            Update::InstallFailed { version, why } => (
                line(
                    "update-status",
                    format!("convt {version} didn't install. {why}"),
                    p.text,
                ),
                actions(app, state, p),
            ),
            Update::NotCovered {
                version,
                date,
                purchase_url,
            } => (
                line(
                    "update-status",
                    if matches!(state.license, State::NotCovered(_)) {
                        // This build is past the license too, so it can't convert.
                        format!(
                            "convt {version} is out, built {date}. Your license's updates ended \
                             before this build too: renew to convert again, or use a build your \
                             license covers."
                        )
                    } else {
                        format!(
                            "convt {version} is out, built {date}, after your license's updates \
                             ended. Renew to get it; this version keeps working."
                        )
                    },
                    p.text,
                ),
                vec![
                    text_button("update-renew", "Renew", p.green, 12.)
                        .on_click(open(purchase_url.clone())),
                ],
            ),
            // Quiet: a note, never an alert.
            Update::Failed(why) => (
                line(
                    "update-status",
                    format!("Couldn't check for updates. {why} {last}"),
                    p.tertiary,
                ),
                Vec::new(),
            ),
        }
    };
    // Ready too: Restart to update is the thing to do, and a check would
    // only hide it for a moment.
    let busy = matches!(
        state.update,
        Update::Checking
            | Update::Downloading { .. }
            | Update::Ready { .. }
            | Update::Installing { .. }
    );
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.))
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(switch)
                .child(mono(11., 14., p.tertiary).child("ONCE A DAY")),
        )
        .child(status)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .children(action)
                .when(on && !busy, |d| d.child(check_now)),
        )
}
