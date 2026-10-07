//! The update check as the windows show it: a card in the main window's
//! sidebar when a newer build is out, and the Update checks row in Settings.
//! Failures appear only in Settings.

use convt_license::client::{DOWNLOAD_URL, State};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use gpui_kit::component::IconName;

use super::theme::{
    self, Button, Clickable, Palette, Tone, radius, size, space, styled, text_button,
};
use crate::model::AppState;
use crate::update::Update;

fn line(id: &'static str, message: impl Into<SharedString>, color: Hsla) -> Clickable {
    let message = message.into();
    div()
        .id(id)
        .test_support()
        .aria_label(message.clone())
        .child(styled(size::SMALL, color).child(message))
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
            Button::primary("update-download", "Download")
                .icon(IconName::ArrowDown)
                .small(),
            DOWNLOAD_URL.to_string(),
        ),
        Update::NotCovered {
            version,
            purchase_url,
            ..
        } => (
            "New version",
            format!("convt {version} needs a renewed license"),
            Button::secondary("update-renew", "Renew to update").small(),
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
            .gap(px(space::SM))
            .p(px(space::MD))
            .rounded(px(radius::CARD))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::SM))
                    .child(theme::icon_tile(IconName::ArrowDown, Tone::Green, 22., p))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                styled(size::SMALL, p.text)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(styled(size::CAPTION, p.secondary).child(detail)),
                    ),
            )
            .child(button.build(p).w_full().on_click(open(url))),
    )
}

/// The Update checks rows in Settings, General: the switch, then what the
/// last check found and what to do about it.
pub fn settings_rows(app: &Entity<AppState>, p: &Palette, cx: &App) -> Vec<AnyElement> {
    let state = app.read(cx);
    let on = state.settings.update_checks;
    let switch = theme::switch("update-checks", on, false, p).on_click({
        let app = app.clone();
        move |_, _, cx| app.update(cx, |s, cx| s.set_update_checks(!on, cx))
    });
    let toggle = theme::row(
        "Check for updates",
        Some(theme::detail("Once a day and when you click Check now", p)),
        switch,
        p,
    )
    .into_any_element();
    let last = state
        .settings
        .update_checked
        .as_deref()
        .map_or("Not checked yet.".to_string(), |d| {
            format!("Last checked {d}.")
        });
    let check_now = Button::secondary("check-updates", "Check now")
        .icon(IconName::RefreshCw)
        .small()
        .build(p)
        .on_click({
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
                    line("update-status", message, p.green_text),
                    Some(
                        Button::primary("update-download", "Download")
                            .icon(IconName::ArrowDown)
                            .small()
                            .build(p)
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
                    text_button("update-renew", "Renew", p.green_text, 12.)
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
    let status_row = div()
        .flex()
        .items_center()
        .gap(px(space::LG))
        .min_h(px(44.))
        .px(px(space::LG))
        .py(px(10.))
        .child(div().flex_1().min_w_0().child(status))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(space::SM))
                .children(action)
                .when(on && state.update != Update::Checking, |d| {
                    d.child(check_now)
                }),
        )
        .into_any_element();
    vec![toggle, status_row]
}
