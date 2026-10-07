//! The first-run window: turn on the Finder menu (macOS only), start the
//! trial or enter a license, and a last word on how to convert. It shows
//! until the last step is finished, and only in builds that check licenses.
//! Closing mid-setup shows it again. Skipping the Finder step still leaves
//! a recover card on Activity until the extension is on.
//!
//! The plan step starts the free trial, which needs a convt.app account:
//! "Start 7-day trial" opens convt.app in the browser to sign in (or, when
//! this computer is signed in already, goes straight on), then asks
//! convt.app for the trial (see `crate::account`). The step follows that
//! flow: waiting for the browser, the trial on, a computer whose trial
//! another account used, or a sign-in that was cancelled or failed. Only a
//! trial key that came back and was stored lets it move on; nothing starts
//! a trial offline. "I have a license" takes a pasted key and needs no
//! account. Someone who bought on convt.app can sign in from the line under
//! the cards and get their key without pasting it.

use convt_license::client::{BUY_URL, CONTACT_URL, State, TRIAL_DAYS};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Icon, IconName};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::LICENSE_PRICE;
use super::theme::{self, Palette, mono, primary_button, text, text_button};
use crate::account::{SignIn, Trial};
use crate::finder::EXTENSION_SETTINGS;
use crate::model::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Finder,
    Plan,
    Done,
}

impl Step {
    /// This step's place among the steps this platform shows.
    fn number(self) -> usize {
        let skipped = usize::from(first_step() != Step::Finder);
        let n = match self {
            Step::Finder => 1,
            Step::Plan => 2,
            Step::Done => 3,
        };
        n - skipped
    }

    /// How many steps this platform shows: the Finder step is macOS only.
    fn count() -> usize {
        if first_step() == Step::Finder { 3 } else { 2 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    Trial,
    Key,
}

/// The trial as the plan step shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TrialView {
    /// No trial yet: the cards.
    Pick,
    /// The browser is open on convt.app.
    Waiting,
    /// Finishing the sign-in or asking convt.app for the trial.
    Starting,
    /// The trial is on with this many days left, or 0 when a license
    /// converts already.
    On(i64),
    /// Another account used the trial on this computer.
    DeviceUsed,
    /// This account used its trial before.
    Ended,
    Failed(String),
}

/// "Your 7-day trial is on. 7 days left.", or for 0 (a license converts
/// already) that the computer is licensed.
fn trial_on(days_left: i64) -> String {
    if days_left == 0 {
        return "This computer is licensed.".to_string();
    }
    let left = match days_left {
        1 => "This is its last day.".to_string(),
        n => format!("{n} days left."),
    };
    if days_left <= TRIAL_DAYS {
        format!("Your {TRIAL_DAYS}-day trial is on. {left}")
    } else {
        format!("Your trial is on. {left}")
    }
}

/// Where first run starts: the Finder step exists only on macOS.
pub fn first_step() -> Step {
    if cfg!(target_os = "macos") {
        Step::Finder
    } else {
        Step::Plan
    }
}

pub struct FirstRunView {
    app: Entity<AppState>,
    pub(super) step: Step,
    pub(super) plan: Plan,
    /// System Settings was opened from the Finder step.
    opened_settings: bool,
    pub(super) key: Entity<InputState>,
    pub(super) error: Option<String>,
    _observe: Subscription,
    _appearance: Subscription,
}

impl FirstRunView {
    pub fn new(
        app: Entity<AppState>,
        step: Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            _observe: cx.observe(&app, |_, _, cx| cx.notify()),
            _appearance: theme::observe_appearance(window, cx),
            app,
            step,
            plan: Plan::Trial,
            opened_settings: false,
            key: cx.new(|cx| InputState::new(window, cx).placeholder("License key")),
            error: None,
        }
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.step {
            // "Skip for now".
            Step::Finder => self.step = Step::Plan,
            Step::Plan if cfg!(target_os = "macos") => self.step = Step::Finder,
            Step::Plan => {}
            Step::Done => self.step = Step::Plan,
        }
        self.error = None;
        let _ = window;
        cx.notify();
    }

    pub(super) fn next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let finder_on = self.app.read(cx).finder_on;
        match self.step {
            Step::Finder if !self.opened_settings && finder_on != Some(true) => {
                cx.open_url(EXTENSION_SETTINGS);
                self.opened_settings = true;
            }
            Step::Finder => self.step = Step::Plan,
            Step::Plan => match self.plan {
                Plan::Trial => match self.trial_view(cx) {
                    TrialView::On(_) => self.step = Step::Done,
                    TrialView::Waiting | TrialView::Starting => {}
                    TrialView::DeviceUsed | TrialView::Ended => cx.open_url(BUY_URL),
                    TrialView::Pick | TrialView::Failed(_) => {
                        self.app.update(cx, |s, cx| s.start_trial(cx))
                    }
                },
                Plan::Key => {
                    let key = self.key.read(cx).value().trim().to_string();
                    if key.is_empty() {
                        self.error = Some("Paste your license key first.".into());
                    } else {
                        match self.app.update(cx, |s, cx| s.activate(&key, cx)) {
                            Ok(_) => {
                                self.error = None;
                                self.step = Step::Done;
                            }
                            Err(e) => self.error = Some(e),
                        }
                    }
                }
            },
            Step::Done => {
                // Finish first run only here, so closing earlier still shows
                // it on the next launch. Open the main window first, so the
                // app never sees its last window close and quits.
                self.app.update(cx, |s, cx| {
                    s.update_settings(|s| s.first_run_done = true, cx)
                });
                super::show_main(cx);
                window.remove_window();
                return;
            }
        }
        cx.notify();
    }

    /// What the plan step shows for the trial.
    fn trial_view(&self, cx: &App) -> TrialView {
        let state = self.app.read(cx);
        match (&state.license, &state.account.trial, &state.account.sign_in) {
            (State::Trial { days_left, .. }, _, _) => TrialView::On(*days_left),
            // A license already here converts; Continue moves on.
            (license, _, _) if license.allows_conversion() => TrialView::On(0),
            (_, Trial::SigningIn, SignIn::Waiting) => TrialView::Waiting,
            (_, Trial::SigningIn | Trial::Starting, _) => TrialView::Starting,
            (_, Trial::DeviceUsed, _) => TrialView::DeviceUsed,
            (_, Trial::Ended, _) => TrialView::Ended,
            (_, Trial::Failed(e), _) => TrialView::Failed(e.clone()),
            _ => TrialView::Pick,
        }
    }

    fn pick_plan(&mut self, plan: Plan, cx: &mut Context<Self>) {
        self.plan = plan;
        self.error = None;
        cx.notify();
    }

    fn finder_art(&self, on: Option<bool>, p: &Palette) -> impl IntoElement {
        let caption = if on == Some(true) {
            "It's on. Come back here and continue."
        } else {
            "This picture isn't a switch."
        };
        div()
            .id("finder-preview")
            .test_support()
            .aria_label("Preview of System Settings. This picture is not a switch.")
            .occlude()
            .cursor_default()
            .flex()
            .flex_col()
            .flex_1()
            .gap(px(8.))
            .p(px(14.))
            .rounded(px(10.))
            .bg(p.recessed)
            .border_1()
            .border_color(p.recessed_border)
            .opacity(0.88)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        text(11., 14., p.tertiary)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("System Settings"),
                    )
                    .child(
                        div()
                            .id("finder-preview-badge")
                            .test_support()
                            .aria_label("Preview")
                            .px(px(6.))
                            .py(px(1.))
                            .rounded(px(4.))
                            .bg(p.chip)
                            .border_1()
                            .border_color(p.chip_border)
                            .child(
                                text(10., 13., p.tertiary)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Preview"),
                            ),
                    ),
            )
            .child(text(11., 14., p.secondary).child("General › Login Items & Extensions"))
            .child(text(11., 14., p.tertiary).child("↓  Scroll to Extensions"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(8.))
                    .bg(p.mock_item)
                    .border_1()
                    .border_color(if p.dark {
                        p.chrome_border
                    } else {
                        p.recessed_border
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text(13., 16., p.text).child("convt"))
                            .child(text(11., 14., p.secondary).child("Finder extension")),
                    )
                    .child(
                        // Illustrated only: not theme::switch, no pointer, no click.
                        div()
                            .flex()
                            .items_center()
                            .w(px(28.))
                            .h(px(16.))
                            .p(px(2.))
                            .rounded(px(8.))
                            .opacity(0.7)
                            .when(on == Some(true), |d| d.justify_end())
                            .bg(if on == Some(true) {
                                p.control_on
                            } else {
                                p.toggle_off
                            })
                            .child(div().size(px(12.)).rounded(px(6.)).bg(rgb(0xFFFFFF))),
                    ),
            )
            .child(
                div()
                    .id("finder-preview-caption")
                    .test_support()
                    .aria_label(caption)
                    .child(text(11., 14., p.tertiary).child(caption)),
            )
    }

    /// The plan step's picture while the trial is on its way, on, or
    /// refused: one panel in place of the cards, so the window keeps its size.
    fn trial_art(&self, view: &TrialView, p: &Palette, cx: &mut Context<Self>) -> Div {
        let app = self.app.clone();
        let status = |message: String, color: Hsla| {
            div()
                .id("trial-status")
                .test_support()
                .aria_label(SharedString::from(message.clone()))
                .child(text(13., 19., color).child(message))
        };
        let email = self.app.read(cx).account.email().map(str::to_string);
        let buy = || {
            text_button(
                "trial-buy-license",
                format!("Buy a license · {LICENSE_PRICE}"),
                p.green,
                12.,
            )
            .font_weight(FontWeight::MEDIUM)
            .on_click(|_, _, cx| cx.open_url(BUY_URL))
        };
        let enter_key = || {
            text_button("trial-enter-key", "I have a license", p.secondary, 12.)
                .on_click(cx.listener(|this, _, _, cx| this.pick_plan(Plan::Key, cx)))
        };
        let row = || div().flex().items_center().gap(px(16.));
        let (tint, ring, body) = match view {
            TrialView::Waiting => (
                p.recessed,
                p.recessed_border,
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(status("Waiting for your browser…".into(), p.text))
                    .child(
                        text(12., 16., p.secondary)
                            .child("Sign in on convt.app and approve this computer."),
                    )
                    .child(
                        row()
                            .child(
                                text_button("sign-in-reopen", "Open the page again", p.green, 12.)
                                    .font_weight(FontWeight::MEDIUM)
                                    .on_click({
                                        let app = app.clone();
                                        move |_, _, cx| app.update(cx, AppState::reopen_sign_in)
                                    }),
                            )
                            .child(
                                text_button("sign-in-cancel", "Cancel", p.secondary, 12.).on_click(
                                    {
                                        let app = app.clone();
                                        move |_, _, cx| app.update(cx, AppState::cancel_sign_in)
                                    },
                                ),
                            ),
                    ),
            ),
            TrialView::Starting => {
                (
                    p.recessed,
                    p.recessed_border,
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(status("Starting your trial…".into(), p.text))
                        .children(email.map(|e| {
                            text(12., 16., p.secondary).child(format!("Signed in as {e}"))
                        })),
                )
            }
            TrialView::On(days_left) => {
                (
                    p.green_tint,
                    p.green,
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .child(
                                    div()
                                        .size(px(20.))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(10.))
                                        .bg(p.green)
                                        .child(
                                            Icon::new(IconName::Check)
                                                .size(px(12.))
                                                .text_color(rgb(0xFFFFFF)),
                                        ),
                                )
                                .child(status(trial_on(*days_left), p.text)),
                        )
                        .children(email.map(|e| {
                            text(12., 16., p.secondary).child(format!("Signed in as {e}"))
                        })),
                )
            }
            TrialView::DeviceUsed => (
                p.recessed,
                p.recessed_border,
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(status(
                        super::DEVICE_USED.to_string(),
                        p.text,
                    ))
                    .child(
                        row()
                            .child(buy())
                            .child(
                                text_button("trial-contact", "Contact us", p.secondary, 12.)
                                    .on_click(|_, _, cx| {
                                        cx.open_url(CONTACT_URL)
                                    }),
                            )
                            .child(enter_key()),
                    ),
            ),
            TrialView::Ended => (
                p.recessed,
                p.recessed_border,
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(status(
                        "The free trial on this account has ended. Buy a license to keep converting."
                            .to_string(),
                        p.text,
                    ))
                    .child(row().child(buy()).child(enter_key())),
            ),
            TrialView::Failed(e) => (
                p.recessed,
                p.recessed_border,
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(status(e.clone(), p.error))
                    .child(row().child(buy()).child(enter_key())),
            ),
            TrialView::Pick => unreachable!("the cards show instead"),
        };
        // min_w(0) lets long lines wrap instead of widening the window.
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .p(px(14.))
            .rounded(px(10.))
            .bg(tint)
            .shadow(vec![theme::inset_ring(ring, 1.)])
            .child(body)
    }

    fn plan_art(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        if self.plan == Plan::Trial {
            let view = self.trial_view(cx);
            if view != TrialView::Pick {
                return self.trial_art(&view, p, cx);
            }
        }
        let card = |id: &'static str, plan: Plan, title: &'static str, about: AnyElement| {
            let on = self.plan == plan;
            theme::clickable(id, title)
                .aria_selected(on)
                .flex()
                .items_center()
                .gap(px(12.))
                .p(px(14.))
                .rounded(px(10.))
                .map(|d| {
                    if on {
                        d.bg(p.green_tint)
                            .shadow(vec![theme::inset_ring(p.green, 1.5)])
                    } else {
                        d.shadow(vec![theme::inset_ring(p.card_border, 1.)])
                    }
                })
                .on_click(cx.listener(move |this, _, _, cx| this.pick_plan(plan, cx)))
                .child(if on {
                    div()
                        .size(px(16.))
                        .flex_shrink_0()
                        .rounded(px(8.))
                        .bg(rgb(0xFFFFFF))
                        .shadow(vec![theme::inset_ring(p.control_on, 5.)])
                } else {
                    div()
                        .size(px(16.))
                        .flex_shrink_0()
                        .rounded(px(8.))
                        .shadow(vec![theme::inset_ring(p.radio_off, 1.5)])
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .gap(px(2.))
                        .child(
                            text(13., 16., p.text)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(about),
                )
        };
        let extra = super::account::compact(&self.app, p, cx);
        let about = |line: &'static str| text(12., 16., p.secondary).child(line).into_any_element();
        // The key field takes the place of the description, so the window
        // keeps its size.
        let key_about = if self.plan == Plan::Key {
            div()
                .pt(px(4.))
                .child(theme::field(&self.key, "first-run-key"))
                .into_any_element()
        } else {
            about("Paste the key from your receipt.")
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .gap(px(10.))
            .child(card(
                "plan-trial",
                Plan::Trial,
                "Start 7-day trial",
                about("Every feature, no card, free account."),
            ))
            .child(card("plan-key", Plan::Key, "I have a license", key_about))
            .child(extra)
    }

    fn done_art(&self, p: &Palette) -> Div {
        let item = |label: &'static str| {
            div()
                .px(px(8.))
                .py(px(3.))
                .child(text(12., 16., p.tertiary).child(label))
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .items_start()
            .gap(px(8.))
            .p(px(14.))
            .rounded(px(10.))
            .bg(p.recessed)
            .border_1()
            .border_color(p.recessed_border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .w(px(22.))
                            .h(px(16.))
                            .rounded(px(3.))
                            .bg(linear_gradient(
                                135.,
                                linear_color_stop(rgb(0xDDE6F2), 0.),
                                linear_color_stop(rgb(0xB9C8DE), 1.),
                            ))
                            .shadow(vec![theme::inset_ring(p.thumb_border, 1.)]),
                    )
                    .child(mono(11., 14., p.text).child("IMG_2041.heic")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(220.))
                    .p(px(5.))
                    .rounded(px(8.))
                    .bg(p.mock_menu)
                    .border_1()
                    .border_color(p.mock_separator)
                    .child(item("Open"))
                    // Finder's wording on macOS, the file managers' elsewhere.
                    .child(item(if cfg!(target_os = "macos") {
                        "Get Info"
                    } else {
                        "Properties"
                    }))
                    .child(div().h(px(1.)).bg(p.mock_separator))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(8.))
                            .py(px(3.))
                            .rounded(px(4.))
                            .bg(rgb(0x2F6FE4))
                            .child(text(12., 16., rgb(0xFFFFFF).into()).child("Convert with convt"))
                            .child(text(12., 16., rgb(0xFFFFFF).into()).child("›")),
                    ),
            )
    }
}

impl Render for FirstRunView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _ = window;
        let p = theme::palette(cx);
        let n = self.step.number();
        let steps = div()
            .flex()
            .gap(px(4.))
            .children((1..=Step::count()).map(|i| {
                div().w(px(20.)).h(px(4.)).rounded(px(2.)).bg(if i <= n {
                    p.green
                } else {
                    p.step_off
                })
            }));
        // A saved key whose updates ended before this build can't convert.
        let not_covered = match &self.app.read(cx).license {
            State::NotCovered(license) if self.step == Step::Done => {
                Some(license.updates_until.clone())
            }
            _ => None,
        };
        let finder_on = self.app.read(cx).finder_on;
        let (title, body, back, next) = match self.step {
            Step::Finder if finder_on == Some(true) => (
                "The Finder menu is on",
                "Right-click a file in Finder to see Convert with convt. You can turn it off in System Settings.".to_string(),
                None,
                "Continue",
            ),
            Step::Finder => (
                "Turn on the Finder menu",
                "Open System Settings, scroll down to Extensions, and turn on convt. Then come back here.".to_string(),
                Some("Skip for now"),
                if self.opened_settings {
                    "Continue"
                } else {
                    "Open System Settings"
                },
            ),
            Step::Plan if self.plan == Plan::Key => (
                "Try it or unlock it",
                format!(
                    "Paste the key from your receipt. A {LICENSE_PRICE} license keeps convt working for good, with a year of updates."
                ),
                cfg!(target_os = "macos").then_some("Back"),
                "Continue",
            ),
            Step::Plan => match self.trial_view(cx) {
                TrialView::Pick => (
                    "Try it or unlock it",
                    format!(
                        "Sign in to convt.app to get {TRIAL_DAYS} days with every feature. A {LICENSE_PRICE} license keeps convt working for good."
                    ),
                    cfg!(target_os = "macos").then_some("Back"),
                    "Start 7-day trial",
                ),
                TrialView::Waiting => (
                    "Sign in to start your trial",
                    "Your trial starts as soon as you approve this computer on convt.app.".to_string(),
                    None,
                    "Waiting…",
                ),
                TrialView::Starting => (
                    "Starting your trial",
                    "convt.app is setting up your trial for this computer.".to_string(),
                    None,
                    "Starting…",
                ),
                TrialView::On(0) => (
                    "You're licensed",
                    "This computer already has a license, so there's no need for a trial.".to_string(),
                    None,
                    "Continue",
                ),
                TrialView::On(_) => (
                    "Your trial is on",
                    format!(
                        "Every feature works until the trial ends. A {LICENSE_PRICE} license keeps convt working after that."
                    ),
                    None,
                    "Continue",
                ),
                TrialView::DeviceUsed => (
                    "Trial already used",
                    format!(
                        "A {LICENSE_PRICE} license keeps convt working for good, with a year of updates."
                    ),
                    cfg!(target_os = "macos").then_some("Back"),
                    "Buy a license",
                ),
                TrialView::Ended => (
                    "Your trial has ended",
                    format!(
                        "A {LICENSE_PRICE} license keeps convt working for good, with a year of updates."
                    ),
                    cfg!(target_os = "macos").then_some("Back"),
                    "Buy a license",
                ),
                TrialView::Failed(_) => (
                    "The trial didn't start",
                    "Nothing changed on this computer. Try again, or unlock convt with a license.".to_string(),
                    cfg!(target_os = "macos").then_some("Back"),
                    "Try again",
                ),
            },
            Step::Done if let Some(until) = &not_covered => (
                "License saved",
                format!(
                    "It covers builds released up to {until}. This build is newer, so it can't convert until you renew."
                ),
                Some("Back"),
                "Open convt",
            ),
            Step::Done => (
                "You're set",
                if cfg!(target_os = "macos") {
                    "Right-click a file in Finder and pick a format, or drop files on convt: photos become JPEG, screenshots and other images become PNG.".to_string()
                } else {
                    "Right-click a file in your file manager and pick a format, or drop files on convt: photos become JPEG, screenshots and other images become PNG.".to_string()
                },
                Some("Back"),
                "Start converting",
            ),
        };
        let art = match self.step {
            Step::Finder => self.finder_art(finder_on, &p).into_any_element(),
            Step::Plan => self.plan_art(&p, cx).into_any_element(),
            Step::Done => self.done_art(&p).into_any_element(),
        };
        let busy = self.step == Step::Plan
            && self.plan == Plan::Trial
            && matches!(
                self.trial_view(cx),
                TrialView::Waiting | TrialView::Starting
            );
        let step_label = format!("STEP {n} OF {}", Step::count());
        div()
            .id("first-run")
            .flex()
            .flex_col()
            .size_full()
            .bg(p.window)
            .font_family(theme::SANS)
            .text_color(p.text)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .px(px(16.))
                    .pt(px(16.))
                    .h(px(28.))
                    .child(steps),
            )
            .child(div().flex().px(px(28.)).pt(px(24.)).child(art))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap(px(8.))
                    .px(px(28.))
                    .pt(px(24.))
                    .child(
                        div()
                            .id("step")
                            .test_support()
                            .aria_label(SharedString::from(step_label.clone()))
                            .child(mono(11., 14., p.tertiary).child(step_label)),
                    )
                    .child(
                        div()
                            .id("first-run-title")
                            .test_support()
                            .aria_label(SharedString::from(title))
                            .child(
                                text(22., 28., p.text)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            ),
                    )
                    // An error takes the place of the description, so the
                    // window keeps its size.
                    .child(match self.error.clone() {
                        Some(e) => super::error_text(e, &p).into_any_element(),
                        None => div()
                            .id("first-run-body")
                            .test_support()
                            .aria_label(SharedString::from(body.clone()))
                            .child(text(13., 19., p.secondary).child(body))
                            .into_any_element(),
                    })
                    .children(not_covered.is_some().then(|| {
                        div().flex().child(
                            text_button("first-run-renew", "Renew", p.green, 12.)
                                .on_click(|_, _, cx| cx.open_url(BUY_URL)),
                        )
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .px(px(28.))
                    .py(px(16.))
                    .bg(p.recessed)
                    .border_t_1()
                    .border_color(p.hairline)
                    .child(match back {
                        Some(label) => text_button("first-run-back", label, p.secondary, 12.)
                            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)))
                            .into_any_element(),
                        None => div().into_any_element(),
                    })
                    .child(
                        primary_button("first-run-next", next, 13., busy)
                            .on_click(cx.listener(|this, _, window, cx| this.next(window, cx))),
                    ),
            )
    }
}
