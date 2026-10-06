//! The update check as the windows show it: a card in the main window's
//! sidebar when a newer build is out, and the Update checks row in Settings.
//! Failures appear only in Settings.

use convt_license::client::{DOWNLOAD_URL, State};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{self, Clickable, Palette, mono, text, text_button};
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

/// The sidebar card: only when a newer build is out.
pub fn sidebar_card(app: &Entity<AppState>, p: &Palette, cx: &App) -> Option<Clickable> {
    let (title, detail, button, url) = match &app.read(cx).update {
        Update::Available { version, .. } => (
            "Update available",
            format!("convt {version}"),
            text_button("update-download", "Download", p.green, 12.),
            DOWNLOAD_URL.to_string(),
        ),
        Update::NotCovered {
            version,
            purchase_url,
            ..
        } => (
            "New version",
            format!("convt {version} needs a renewed license"),
            text_button("update-renew", "Renew to update", p.green, 12.),
            purchase_url.clone(),
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
            .child(
                div()
                    .flex()
                    .child(button.font_weight(FontWeight::MEDIUM).on_click(open(url))),
            ),
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
    let (status, action): (Clickable, Option<Clickable>) = if !on {
        (
            line(
                "update-status",
                "Off. convt won't look for new versions.",
                p.secondary,
            ),
            None,
        )
    } else {
        match &state.update {
            Update::Idle => (line("update-status", last, p.secondary), None),
            Update::Checking => (line("update-status", "Checking…", p.secondary), None),
            Update::UpToDate => (
                line("update-status", "convt is up to date.", p.secondary),
                None,
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
                    Some(
                        text_button("update-download", "Download", p.green, 12.)
                            .on_click(open(DOWNLOAD_URL.to_string())),
                    ),
                )
            }
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
                Some(
                    text_button("update-renew", "Renew", p.green, 12.)
                        .on_click(open(purchase_url.clone())),
                ),
            ),
            // Quiet: a note, never an alert.
            Update::Failed(why) => (
                line(
                    "update-status",
                    format!("Couldn't check for updates. {why} {last}"),
                    p.tertiary,
                ),
                None,
            ),
        }
    };
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
                .gap(px(14.))
                .children(action)
                .when(on && state.update != Update::Checking, |d| {
                    d.child(check_now)
                }),
        )
}
