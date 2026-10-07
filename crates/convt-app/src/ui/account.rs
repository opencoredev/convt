//! The convt.app sign-in as Settings and first run show it: signed out,
//! waiting for the browser, finishing, signed in, failed, the last license
//! refresh, and where starting the free trial stands.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{Clickable, Palette, secondary_button, text, text_button};
use convt_license::Plan;
use convt_license::client::{self, BUY_URL, CONTACT_URL};

use crate::account::{Refresh, SignIn, Trial};
use crate::model::AppState;

/// What the automatic license refresh sends, for Settings and first run.
pub const REFRESH_NOTE: &str = "While you're signed in, convt asks convt.app for your current \
     license key once a day at launch and when you click Refresh license. It sends this \
     computer's sign-in token and the app version, never your files. Starting the trial also \
     sends a one-way hash of this computer's ID, so each computer gets one trial.";

/// A line of text tests can read by `id`.
fn line(id: &'static str, message: impl Into<SharedString>, color: Hsla) -> Clickable {
    let message = message.into();
    div()
        .id(id)
        .test_support()
        .aria_label(message.clone())
        .child(text(12., 16., color).child(message))
}

/// Calls `f` on the app state from a click.
fn on_app(
    app: &Entity<AppState>,
    f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let app = app.clone();
    move |_, _, cx| app.update(cx, |s, cx| f(s, cx))
}

/// The sign-in line under the plan cards in first run: one row, so the
/// window keeps its size.
pub fn compact(app: &Entity<AppState>, p: &Palette, cx: &App) -> Div {
    let state = app.read(cx);
    let account = &state.account;
    let row = div().flex().items_center().gap(px(12.));
    let row = match (&account.sign_in, account.email()) {
        (SignIn::Waiting, _) => row
            .child(line(
                "account-status",
                "Waiting for your browser…",
                p.secondary,
            ))
            .child(
                text_button("sign-in-reopen", "Open again", p.green, 12.)
                    .on_click(on_app(app, AppState::reopen_sign_in)),
            )
            .child(
                text_button("sign-in-cancel", "Cancel", p.secondary, 12.)
                    .on_click(on_app(app, AppState::cancel_sign_in)),
            ),
        (SignIn::Finishing, _) => {
            row.child(line("account-status", "Finishing sign-in…", p.secondary))
        }
        (SignIn::Failed(e), _) => row
            .child(line("account-status", e.clone(), p.error).flex_1())
            .child(
                text_button("sign-in", "Try again", p.green, 12.)
                    .on_click(on_app(app, AppState::start_sign_in)),
            ),
        (SignIn::Idle, Some(email)) => {
            let pro = match (&state.license, &account.refresh) {
                (client::State::Licensed(l), _) if l.plan == Plan::Pro => {
                    format!(" · Pro until {}", l.updates_until)
                }
                (client::State::Licensed(_), _) => " · Desktop license".into(),
                (client::State::Trial { days_left, .. }, _) => match days_left {
                    1 => " · trial, last day".into(),
                    n => format!(" · trial, {n} days left"),
                },
                (_, Refresh::Running) => " · fetching your key…".into(),
                _ => String::new(),
            };
            row.child(line(
                "account-status",
                format!("Signed in as {email}{pro}"),
                p.secondary,
            ))
        }
        (SignIn::Idle, None) => row
            .child(text(12., 16., p.secondary).child("Bought on convt.app?"))
            .child(
                text_button("sign-in", "Sign in with convt.app", p.green, 12.)
                    .font_weight(FontWeight::MEDIUM)
                    .on_click(on_app(app, AppState::start_sign_in)),
            ),
    };
    let notice = account
        .notice
        .clone()
        .map(|n| line("account-notice", n, p.secondary));
    div()
        .flex()
        .flex_col()
        .min_w(px(0.))
        .gap(px(4.))
        .child(row)
        .children(notice)
}

/// The account section of the License tab.
pub fn section(app: &Entity<AppState>, p: &Palette, cx: &App) -> Div {
    let state = app.read(cx);
    let account = &state.account;
    let heading = text(13., 16., p.text)
        .font_weight(FontWeight::SEMIBOLD)
        .child("convt.app account");
    let body = div().flex().flex_col().gap(px(10.));
    let body = match (&account.sign_in, account.email()) {
        (SignIn::Waiting, _) => body
            .child(line(
                "account-status",
                "Waiting for your browser. Approve this computer on convt.app, then come back here.",
                p.text,
            ))
            .child(
                div()
                    .flex()
                    .gap(px(14.))
                    .child(
                        text_button("sign-in-reopen", "Open the page again", p.green, 12.)
                            .on_click(on_app(app, AppState::reopen_sign_in)),
                    )
                    .child(
                        text_button("sign-in-cancel", "Cancel", p.secondary, 12.)
                            .on_click(on_app(app, AppState::cancel_sign_in)),
                    ),
            ),
        (SignIn::Finishing, _) => body.child(line("account-status", "Finishing sign-in…", p.text)),
        (SignIn::Idle, Some(email)) => {
            let running = account.refresh == Refresh::Running;
            let refresh = match &account.refresh {
                Refresh::Idle => None,
                Refresh::Running => Some(line("refresh-status", "Checking convt.app…", p.secondary)),
                // Body text, not green: green text is below 4.5:1 on white.
                Refresh::Done(m) => Some(line("refresh-status", m.clone(), p.text)),
                Refresh::Failed(m) => Some(line("refresh-status", m.clone(), p.error)),
            };
            body.child(line(
                "account-status",
                format!("Signed in to convt.app as {email}."),
                p.text,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .child(
                        secondary_button("refresh-license", "Refresh license", p)
                            .when(running, |d| d.opacity(0.5).cursor_default())
                            .when(!running, |d| {
                                d.on_click(on_app(app, AppState::refresh_license))
                            }),
                    )
                    .child(
                        text_button("sign-out", "Sign out", p.secondary, 12.)
                            .on_click(on_app(app, AppState::sign_out)),
                    ),
            )
            .children(refresh)
        }
        (failed, None) | (failed @ SignIn::Failed(_), Some(_)) => {
            let error = match failed {
                SignIn::Failed(e) => Some(line("account-status", e.clone(), p.error)),
                _ => None,
            };
            // A refresh that signed this computer out says why.
            let refresh = match &account.refresh {
                Refresh::Failed(m) if error.is_none() => {
                    Some(line("refresh-status", m.clone(), p.error))
                }
                _ => None,
            };
            body.children(error)
                .children(refresh)
                .when(!matches!(failed, SignIn::Failed(_)), |d| {
                    d.child(text(12., 17., p.secondary).child(
                        "Sign in with your convt.app account to get the key you bought \
                         without pasting it; Start 7-day trial signs you in too. Pro keys last one billing \
                         period, and convt fetches each new one for you. A pasted license key \
                         never needs an account.",
                    ))
                })
                .child(
                    div().flex().child(
                        secondary_button(
                            "sign-in",
                            if matches!(failed, SignIn::Failed(_)) {
                                "Try again"
                            } else {
                                "Sign in with convt.app"
                            },
                            p,
                        )
                        .on_click(on_app(app, AppState::start_sign_in)),
                    ),
                )
        }
    };
    let notice = account
        .notice
        .clone()
        .map(|n| line("account-notice", n, p.secondary));
    let trial = trial_line(app, p, cx);
    div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .px(px(40.))
        .pt(px(22.))
        .pb(px(28.))
        .border_t_1()
        .border_color(p.hairline)
        .child(heading)
        .children(trial)
        .child(body)
        .children(notice)
        .child(
            div()
                .id("refresh-note")
                .test_support()
                .aria_label(REFRESH_NOTE)
                .child(text(12., 17., p.secondary).child(REFRESH_NOTE)),
        )
}

/// Where starting the trial stands, for the License tab, with what the user
/// can do next. Nothing while no trial was asked for and none is needed.
fn trial_line(app: &Entity<AppState>, p: &Palette, cx: &App) -> Option<Div> {
    let state = app.read(cx);
    let buy = || {
        text_button("trial-buy-license", "Buy a license", p.green, 12.)
            .on_click(|_, _, cx| cx.open_url(BUY_URL))
    };
    let column = || div().flex().flex_col().gap(px(8.));
    let row = || div().flex().items_center().gap(px(14.));
    Some(match (&state.account.trial, &state.license) {
        (Trial::SigningIn, _) => column().child(line(
            "trial-status",
            "Your free trial starts once you're signed in.",
            p.text,
        )),
        (Trial::Starting, _) => {
            column().child(line("trial-status", "Starting your trial…", p.text))
        }
        (Trial::Started, client::State::Trial { .. }) => {
            column().child(line("trial-status", state.license.summary(), p.text))
        }
        (Trial::DeviceUsed, _) => column()
            .child(line("trial-status", super::DEVICE_USED, p.error))
            .child(
                row().child(buy()).child(
                    text_button("trial-contact", "Contact us", p.secondary, 12.)
                        .on_click(|_, _, cx| cx.open_url(CONTACT_URL)),
                ),
            ),
        (Trial::Ended, _) => column()
            .child(line(
                "trial-status",
                "The free trial on this account has ended. Buy a license to keep converting.",
                p.error,
            ))
            .child(row().child(buy())),
        (Trial::Failed(e), _) => column()
            .child(line("trial-status", e.clone(), p.error))
            .child(
                row()
                    .child(
                        text_button("trial-retry", "Try again", p.green, 12.)
                            .on_click(on_app(app, AppState::start_trial)),
                    )
                    .child(buy()),
            ),
        // No trial yet, or a bought key this build has outgrown: one click
        // starts it, signing in first if needed. A bought key stays stored.
        (_, client::State::NoTrial | client::State::NotCovered(_)) => column().child(
            div().flex().child(
                secondary_button("start-trial", "Start 7-day trial", p)
                    .on_click(on_app(app, AppState::start_trial)),
            ),
        ),
        _ => return None,
    })
}
