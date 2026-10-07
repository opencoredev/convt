//! The update check as the windows show it: a card in the main window's
//! sidebar when a newer build is out, and the Update checks row in Settings.
//! Failures appear only in Settings.

use convt_license::client::{DOWNLOAD_URL, State};
use gpui_kit::component::Sizable;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::*;

use gpui_kit::component::IconName;

use super::theme::{self, Button, Clickable, Palette, Tone, radius, size, space, styled};
use crate::account::VERSION;
use crate::clock::Local;
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

/// "Last checked today at 4:08 PM", or "Not checked yet".
pub(super) fn last_checked(at: Option<u64>) -> String {
    let Some(at) = at else {
        return "Not checked yet".into();
    };
    let when = Local::at(at as i64);
    let day = when.day_label(&Local::now());
    let day = if day == "Today" || day == "Yesterday" {
        day.to_lowercase()
    } else {
        format!("on {day}")
    };
    format!("Last checked {day} at {}", when.time())
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

/// The release notes for `version` on convt.app's changelog.
pub(super) fn release_notes_url(version: &str) -> String {
    format!("https://convt.app/changelog#v{version}")
}

/// "Oct 3, 2026" for "2026-10-03"; anything else as it is.
pub(super) fn long_date(day: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = day.splitn(3, '-').map(str::parse::<u32>);
    match (parts.next(), parts.next(), parts.next()) {
        (Some(Ok(y)), Some(Ok(m @ 1..=12)), Some(Ok(d @ 1..=31))) => {
            format!("{} {d}, {y}", MONTHS[m as usize - 1])
        }
        _ => day.to_string(),
    }
}

/// The Updates card in Settings, General: this version and when convt last
/// checked, with Check now; what the check found and what to do about it;
/// then the switch for automatic checks.
pub fn settings_rows(app: &Entity<AppState>, p: &Palette, cx: &App) -> Vec<AnyElement> {
    let state = app.read(cx);
    let on = state.settings.update_checks;
    let checking = state.update == Update::Checking;

    let check_now = Button::secondary(
        "check-updates",
        if checking { "Checking…" } else { "Check now" },
    )
    .icon(IconName::RefreshCw)
    .small()
    .loading(checking)
    .build(p)
    .on_click({
        let app = app.clone();
        move |_, _, cx| app.update(cx, |s, cx| s.check_updates(cx))
    });
    let version = div()
        .flex()
        .items_center()
        .gap(px(space::MD))
        .min_h(px(60.))
        .px(px(space::LG))
        .py(px(space::MD))
        .child(theme::mark(28., p))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(2.))
                .child(
                    div()
                        .id("update-version")
                        .test_support()
                        .aria_label(SharedString::from(format!("convt {VERSION}")))
                        .child(
                            styled(size::BODY, p.text)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("convt {VERSION}")),
                        ),
                )
                .child(line(
                    "update-last-checked",
                    format!(
                        "Built {} · {}",
                        long_date(state.licensing.build_date()),
                        last_checked(state.settings.update_checked_at)
                    ),
                    p.secondary,
                )),
        )
        .child(check_now)
        .into_any_element();

    let status = div()
        .flex()
        .flex_col()
        .justify_center()
        .min_h(px(44.))
        .px(px(space::LG))
        .py(px(space::MD))
        .child(status(state, on, p))
        .into_any_element();

    let switch = theme::switch("update-checks", on, false, p).on_click({
        let app = app.clone();
        move |_, _, cx| app.update(cx, |s, cx| s.set_update_checks(!on, cx))
    });
    let toggle = theme::row(
        "Check automatically",
        Some(theme::detail("At launch and every 5 hours", p)),
        switch,
        p,
    )
    .into_any_element();

    vec![version, status, toggle]
}

/// A one-line status with an icon (or a spinner) in front.
fn note(lead: AnyElement, message: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(space::SM))
        .child(div().flex_shrink_0().child(lead))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(line("update-status", message, color)),
        )
}

/// What the last check found, as the middle row of the Updates card.
fn status(state: &AppState, on: bool, p: &Palette) -> AnyElement {
    let lead = |name: IconName, color: Hsla| theme::icon(name, 14., color).into_any_element();
    let offer = |title: String, body: String, version: &str, tone: Tone, action: Clickable| {
        let words = div()
            .id("update-status")
            .test_support()
            .aria_label(SharedString::from(format!("{title} {body}")))
            .child(theme::callout_words(title, body, p));
        let notes = Button::ghost("update-notes", "Release notes")
            .icon(IconName::ExternalLink)
            .small()
            .build(p)
            .on_click(open(release_notes_url(version)));
        theme::callout(
            IconName::ArrowDown,
            tone,
            div()
                .flex()
                .flex_col()
                .gap(px(space::MD))
                .child(words)
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(space::SM))
                        .child(action)
                        .child(notes),
                ),
            p,
        )
        .into_any_element()
    };
    match &state.update {
        Update::Idle if on => note(
            lead(IconName::Calendar, p.tertiary),
            "convt looks for a new version at launch and every 5 hours.",
            p.secondary,
        )
        .into_any_element(),
        Update::Idle => note(
            lead(IconName::Ban, p.tertiary),
            "Automatic checks are off. convt looks only when you click Check now.",
            p.secondary,
        )
        .into_any_element(),
        Update::Checking => note(
            Spinner::new()
                .with_size(px(14.))
                .color(p.secondary)
                .into_any_element(),
            "Looking for a new version on convt.app…",
            p.secondary,
        )
        .into_any_element(),
        Update::UpToDate => note(
            lead(IconName::CircleCheck, p.green_text),
            format!("You're up to date. convt {VERSION} is the newest version."),
            p.text,
        )
        .into_any_element(),
        Update::Available {
            version,
            date,
            uncovered,
        } => {
            let mut body = format!("Built {}.", long_date(date));
            if let Some(newer) = uncovered {
                body.push_str(&format!(
                    " convt {newer} is out too and needs a renewed license."
                ));
            }
            let download = Button::primary("update-download", "Download")
                .icon(IconName::ArrowDown)
                .small()
                .build(p)
                .on_click(open(DOWNLOAD_URL.to_string()));
            offer(
                format!("convt {version} is available"),
                body,
                version,
                Tone::Green,
                download,
            )
        }
        Update::NotCovered {
            version,
            date,
            purchase_url,
        } => {
            let body = if matches!(state.license, State::NotCovered(_)) {
                // This build is past the license too, so it can't convert.
                format!(
                    "Built {}. Your license's updates ended before this build too: renew to \
                     convert again, or use a build your license covers.",
                    long_date(date)
                )
            } else {
                format!(
                    "Built {}, after your license's updates ended. Renew to get it; this \
                     version keeps working.",
                    long_date(date)
                )
            };
            let renew = Button::secondary("update-renew", "Renew")
                .small()
                .build(p)
                .on_click(open(purchase_url.clone()));
            offer(
                format!("convt {version} is out"),
                body,
                version,
                Tone::Neutral,
                renew,
            )
        }
        // Quiet: a note, never an alert.
        Update::Failed(why) => note(
            lead(IconName::CircleAlert, p.tertiary),
            format!("Couldn't check for updates. {why}"),
            p.secondary,
        )
        .into_any_element(),
    }
}
