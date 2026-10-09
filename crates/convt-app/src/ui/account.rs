//! The convt.app sign-in as Settings and first run show it: signed out,
//! waiting for the browser, finishing, signed in, failed, and the last
//! license refresh.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::IconName;

use super::theme::{self, Button, Clickable, Palette, size, space, styled};

use crate::account::{Refresh, SignIn};
use crate::model::AppState;

/// What the automatic license refresh sends, for Settings and first run.
pub const REFRESH_NOTE: &str = "While you're signed in, convt asks convt.app for your current Pro key \
     at most once a day and when you click Refresh license. It sends this computer's sign-in \
     token and the app version, never your files.";

/// A line of text tests can read by `id`.
fn line(id: &'static str, message: impl Into<SharedString>, color: Hsla) -> Clickable {
    let message = message.into();
    div()
        .id(id)
        .test_support()
        .aria_label(message.clone())
        .child(styled(size::SMALL, color).child(message))
}

/// Calls `f` on the app state from a click.
fn on_app(
    app: &Entity<AppState>,
    f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let app = app.clone();
    move |_, _, cx| app.update(cx, |s, cx| f(s, cx))
}

/// The account section of the License tab.
pub fn section(app: &Entity<AppState>, p: &Palette, cx: &App) -> Div {
    let state = app.read(cx);
    let account = &state.account;
    let body = div().flex().flex_col().gap(px(10.));
    let body = match (&account.sign_in, account.masked_email()) {
        (SignIn::Waiting, _) => body
            .child(line(
                "account-status",
                "Waiting for your browser. Approve this computer on convt.app, then come back here.",
                p.text,
            ))
            .child(
                div()
                    .flex()
                    .gap(px(space::SM))
                    .child(
                        Button::secondary("sign-in-reopen", "Open the page again")
                            .icon(IconName::ExternalLink)
                            .small()
                            .build(p)
                            .on_click(on_app(app, AppState::reopen_sign_in)),
                    )
                    .child(
                        Button::ghost("sign-in-cancel", "Cancel")
                            .small()
                            .build(p)
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
                    .gap(px(space::SM))
                    .child(
                        Button::secondary("refresh-license", "Refresh license")
                            .icon(IconName::RefreshCw)
                            .small()
                            .disabled(running)
                            .build(p)
                            .when(!running, |d| {
                                d.on_click(on_app(app, AppState::refresh_license))
                            }),
                    )
                    .child(
                        Button::ghost("sign-out", "Sign out")
                            .small()
                            .build(p)
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
                    d.child(styled(size::SMALL, p.secondary).child(
                        "Pro keys last one billing period. Sign in with your convt.app account \
                         and convt fetches each new key for you. A Desktop license never needs \
                         an account.",
                    ))
                })
                .child(
                    div().flex().child(
                        Button::secondary(
                            "sign-in",
                            if matches!(failed, SignIn::Failed(_)) {
                                "Try again"
                            } else {
                                "Sign in with convt.app"
                            },
                        )
                        .icon(IconName::CircleUser)
                        .small()
                        .build(p)
                        .on_click(on_app(app, AppState::start_sign_in)),
                    ),
                )
        }
    };
    let notice = account
        .notice
        .clone()
        .map(|n| line("account-notice", n, p.secondary));
    div()
        .flex()
        .flex_col()
        .child(theme::section_label("Pro renewal", p))
        .child(
            theme::card(p)
                .gap(px(space::MD))
                .p(px(space::LG))
                .child(body)
                .children(notice),
        )
        .child(
            div()
                .id("refresh-note")
                .test_support()
                .aria_label(REFRESH_NOTE)
                .pt(px(space::SM))
                .px(px(2.))
                .child(styled(size::CAPTION, p.tertiary).child(REFRESH_NOTE)),
        )
}
