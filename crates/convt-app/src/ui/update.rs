//! The update check as the windows show it: a card in the main window's
//! sidebar when a newer build is out, downloading or ready to install, and
//! the Updates card in Settings. Failed checks appear only in Settings; a
//! failed download or install shows in both, with the download page as the
//! way out.

use convt_license::client::{DOWNLOAD_URL, State};
use gpui_kit::component::Sizable;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::IconName;

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

/// "Last checked today at 4:08 PM", or nothing before the first check.
pub(super) fn last_checked(at: Option<u64>) -> Option<String> {
    let at = at?;
    let when = Local::at(at as i64);
    let day = when.day_label(&Local::now());
    let day = if day == "Today" || day == "Yesterday" {
        day.to_lowercase()
    } else {
        format!("on {day}")
    };
    Some(format!("Last checked {day} at {}", when.time()))
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

type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The buttons a state offers, shared by the card and Settings, the thing
/// to do first. Empty for states with nothing to do.
fn actions(app: &Entity<AppState>, state: &AppState) -> Vec<(Button, OnClick)> {
    let self_install = state.update_config.install.is_some() && state.settings.update_checks;
    let download_page = || -> OnClick { Box::new(open(DOWNLOAD_URL.to_string())) };
    let buttons: Vec<(Button, OnClick)> = match &state.update {
        Update::Available { .. } if self_install => vec![(
            Button::primary("update-install", "Update").icon(IconName::Download),
            Box::new(act(app, AppState::download_update)),
        )],
        Update::Available { .. } => vec![(
            Button::primary("update-download", "Download").icon(IconName::Download),
            download_page(),
        )],
        Update::Ready { .. } => vec![(
            Button::primary("update-restart", "Restart to update").icon(IconName::RotateCw),
            Box::new(act(app, AppState::restart_to_update)),
        )],
        Update::InstallFailed { .. } => vec![
            (
                Button::primary("update-retry", "Try again").icon(IconName::RefreshCw),
                Box::new(act(app, AppState::download_update)),
            ),
            (
                Button::secondary("update-download", "Download instead"),
                download_page(),
            ),
        ],
        Update::NotCovered { purchase_url, .. } => vec![(
            Button::secondary("update-renew", "Renew to update"),
            Box::new(open(purchase_url.clone())),
        )],
        _ => Vec::new(),
    };
    buttons.into_iter().map(|(b, f)| (b.small(), f)).collect()
}

fn built(app: &Entity<AppState>, state: &AppState, p: &Palette) -> Vec<Clickable> {
    actions(app, state)
        .into_iter()
        .map(|(b, f)| b.build(p).on_click(f))
        .collect()
}

/// The sidebar card: while a newer build is out, downloads, waits to be
/// installed or didn't install.
pub fn sidebar_card(app: &Entity<AppState>, p: &Palette, cx: &App) -> Option<Clickable> {
    let state = app.read(cx);
    let (title, detail, tone, icon) = match &state.update {
        Update::Available { version, .. } => (
            "Update available".to_string(),
            format!("convt {version}"),
            Tone::Green,
            IconName::ArrowDown,
        ),
        Update::Downloading { version, percent } => (
            "Update available".to_string(),
            downloading(version, *percent),
            Tone::Green,
            IconName::ArrowDown,
        ),
        Update::Ready { version, .. } => (
            format!("convt {version} is ready"),
            state
                .updater
                .notice
                .clone()
                .unwrap_or_else(|| "Restarting takes a few seconds.".into()),
            Tone::Green,
            IconName::CircleCheck,
        ),
        Update::Installing { version } => (
            format!("Installing convt {version}…"),
            "convt restarts when it's done.".to_string(),
            Tone::Green,
            IconName::Loader,
        ),
        Update::InstallFailed { version, why } => (
            format!("convt {version} didn't install"),
            why.clone(),
            Tone::Neutral,
            IconName::CircleAlert,
        ),
        Update::NotCovered { version, .. } => (
            "New version".to_string(),
            format!("convt {version} needs a renewed license"),
            Tone::Neutral,
            IconName::ArrowDown,
        ),
        _ => return None,
    };
    let progress = match &state.update {
        Update::Downloading { percent, .. } => Some(*percent),
        _ => None,
    };
    let buttons = built(app, state, p);
    Some(
        div()
            .id("update-card")
            .test_support()
            .aria_label(SharedString::from(format!("{title}: {detail}")))
            .flex()
            .flex_col()
            .gap(px(space::SM))
            .p(px(space::MD))
            .rounded(px(radius::PANEL))
            .bg(p.surface)
            .shadow({
                let mut s = vec![theme::inset_ring(p.border, 1.)];
                s.extend(theme::soft(p));
                s
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::SM))
                    .child(theme::icon_tile(icon, tone, 22., p))
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
            .when_some(progress, |d, percent| {
                d.child(theme::progress(
                    f32::from(percent) / 100.,
                    p.border,
                    p.green,
                ))
            })
            .when(!buttons.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(buttons.into_iter().map(|b| b.w_full())),
                )
            }),
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
    // Downloading, ready or installing: Restart to update is the thing to
    // do, and a check would only hide it for a moment.
    let busy = matches!(
        state.update,
        Update::Downloading { .. } | Update::Ready { .. } | Update::Installing { .. }
    );
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
                    match last_checked(state.settings.update_checked_at) {
                        Some(checked) => {
                            format!(
                                "Built {} · {checked}",
                                long_date(state.licensing.build_date())
                            )
                        }
                        None => format!("Built {}", long_date(state.licensing.build_date())),
                    },
                    p.secondary,
                )),
        )
        .when(!busy, |d| d.child(check_now))
        .into_any_element();

    let status = div()
        .flex()
        .flex_col()
        .justify_center()
        .min_h(px(44.))
        .px(px(space::LG))
        .py(px(space::MD))
        .child(status(app, state, on, p))
        .into_any_element();

    let switch = theme::switch("update-checks", "Check automatically", on, false, p).on_click({
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
fn status(app: &Entity<AppState>, state: &AppState, on: bool, p: &Palette) -> AnyElement {
    let lead = |name: IconName, color: Hsla| theme::icon(name, 14., color).into_any_element();
    let offer = |icon: IconName,
                 title: String,
                 body: String,
                 version: &str,
                 tone: Tone,
                 actions: Vec<Clickable>| {
        let label = if body.is_empty() {
            title.clone()
        } else {
            format!("{title} {body}")
        };
        let words = div()
            .id("update-status")
            .test_support()
            .aria_label(SharedString::from(label))
            .child(theme::callout_words(title, body, p));
        let notes = Button::ghost("update-notes", "Release notes")
            .icon(IconName::ExternalLink)
            .small()
            .build(p)
            .on_click(open(release_notes_url(version)));
        theme::callout(
            icon,
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
                        .children(actions)
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
            format!("You're up to date. convt {VERSION} is the latest build for this install."),
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
            offer(
                IconName::ArrowDown,
                format!("convt {version} is available"),
                body,
                version,
                Tone::Green,
                built(app, state, p),
            )
        }
        Update::Downloading { version, percent } => div()
            .flex()
            .flex_col()
            .gap(px(space::SM))
            .child(note(
                Spinner::new()
                    .with_size(px(14.))
                    .color(p.secondary)
                    .into_any_element(),
                downloading(version, *percent),
                p.secondary,
            ))
            .child(theme::progress(
                f32::from(*percent) / 100.,
                p.border,
                p.green,
            ))
            .into_any_element(),
        Update::Ready { version, .. } => offer(
            IconName::CircleCheck,
            format!("convt {version} is ready to install."),
            state.updater.notice.clone().unwrap_or_default(),
            version,
            Tone::Green,
            built(app, state, p),
        ),
        Update::Installing { version } => note(
            Spinner::new()
                .with_size(px(14.))
                .color(p.secondary)
                .into_any_element(),
            format!("Installing convt {version}… convt restarts when it's done."),
            p.secondary,
        )
        .into_any_element(),
        Update::InstallFailed { version, why } => offer(
            IconName::CircleAlert,
            format!("convt {version} didn't install."),
            why.clone(),
            version,
            Tone::Neutral,
            built(app, state, p),
        ),
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
                IconName::ArrowDown,
                format!("convt {version} is out"),
                body,
                version,
                Tone::Neutral,
                vec![renew],
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
