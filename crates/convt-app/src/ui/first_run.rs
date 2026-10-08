//! The onboarding window: sign in (or enter a key), see what the account
//! allows, answer one question per screen, then a short "Setting convt up"
//! moment before the main window opens. It shows until that last moment ends,
//! and only in builds that check licenses; closing it earlier shows it again.
//!
//! The look is calm on purpose: lots of room, the mark and one line, pill
//! buttons, and a dithered green glow rising from the bottom edge
//! (`assets/onboarding`, drawn by its `generate.py`) that breathes slowly
//! unless the system asks for reduced motion.
//!
//! Signing in goes through convt.app in the browser (`crate::account`). What
//! comes back decides the next screen: Pro and a running trial continue; an
//! account that can still start its trial starts it through checkout; a
//! lapsed one can buy or use a key. Nothing here starts a local trial.

use std::time::Duration;

use convt_license::client::{BUY_URL, State};
use gpui_kit::component::Sizable;
use gpui_kit::component::input::InputState;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{self, IconName, Look, Palette, icon, styled, text};
use crate::account::{Access, Provider, Refresh, SignIn};
use crate::finder::EXTENSION_SETTINGS;
use crate::model::{AppState, PackPhase};

/// How long the "Setting convt up" moment lasts before the main window
/// opens, its fade included. It never waits for anything: the document
/// download, the one slow thing, carries on in the background, and Activity
/// shows it.
pub const CALIBRATE: Duration = Duration::from_millis(2300);
/// The last part of [`CALIBRATE`], while the screen fades out.
const FADE: Duration = Duration::from_millis(300);

/// The site's terms and privacy pages, linked under the sign-in buttons.
const TERMS_URL: &str = "https://convt.app/terms";
const PRIVACY_URL: &str = "https://convt.app/privacy";

/// Where onboarding is. The account screens are one place: what they show
/// follows the sign-in and the account ([`Stage`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Account,
    /// "I have a license key".
    Key,
    Question(Question),
    /// The last moment, then the main window.
    Calibrating,
}

/// The questions, one per screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Question {
    /// Install the document pack.
    Documents,
    /// Turn on the Finder menu (macOS only).
    Finder,
}

impl Question {
    fn title(self) -> &'static str {
        match self {
            Question::Documents => "Convert PDFs and documents too?",
            Question::Finder => "Add convt to Finder?",
        }
    }
}

/// What the account screen shows, from the sign-in flow, the license on
/// this computer and what the account allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// Nothing yet: the sign-in buttons.
    SignIn,
    /// The browser is open on convt.app.
    Waiting,
    /// The link came back and the app is finishing.
    Finishing,
    Failed(String),
    /// Signed in, and the account's answer hasn't come yet.
    Checking,
    /// Signed in, and asking the account failed or didn't say what it
    /// allows: Retry, or use a key. Never a dead end.
    CheckFailed(String),
    Pro,
    /// A desktop license key, without Pro.
    Licensed,
    /// A saved key whose updates ended before this build.
    NotCovered {
        until: String,
    },
    Trial {
        ends_on: String,
    },
    CanStartTrial,
    /// The trial checkout is open in the browser.
    AwaitingTrial,
    Lapsed,
}

pub struct FirstRunView {
    app: Entity<AppState>,
    pub(super) screen: Screen,
    /// The button pressed last, for Try again.
    pub(super) provider: Provider,
    /// The highlighted tile of a question: true for Yes.
    pub(super) yes: bool,
    pub(super) key: Entity<InputState>,
    pub(super) error: Option<String>,
    /// Onboarding asked the account itself, for a launch that didn't (the
    /// launch check runs once a UTC day).
    asked: bool,
    focus: FocusHandle,
    _finish: Option<Task<()>>,
    _observe: Subscription,
    _appearance: Subscription,
    _activation: Subscription,
}

impl FirstRunView {
    pub fn new(
        app: Entity<AppState>,
        screen: Screen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut view = Self {
            _observe: cx.observe(&app, |this, _, cx| {
                this.ask_account(cx);
                cx.notify()
            }),
            _appearance: theme::observe_appearance(window, cx),
            // The glow only breathes while the window is in front.
            _activation: cx.observe_window_activation(window, |_, _, cx| cx.notify()),
            app,
            screen,
            provider: Provider::Google,
            yes: true,
            key: cx.new(|cx| InputState::new(window, cx).placeholder("Paste your license key")),
            error: None,
            asked: false,
            focus,
            _finish: None,
        };
        if screen == Screen::Calibrating {
            view.calibrate(window, cx);
        }
        view.ask_account(cx);
        view
    }

    /// What the account screen shows now.
    pub(super) fn stage(&self, cx: &App) -> Stage {
        let state = self.app.read(cx);
        let account = &state.account;
        let licensed = match &state.license {
            State::Licensed(license) => Some(license.plan),
            _ => None,
        };
        if let State::NotCovered(license) = &state.license
            && account.session.is_none()
        {
            return Stage::NotCovered {
                until: license.updates_until.clone(),
            };
        }
        if account.session.is_none() {
            return match (&account.sign_in, licensed) {
                (SignIn::Waiting, _) => Stage::Waiting,
                (SignIn::Finishing, _) => Stage::Finishing,
                (_, Some(convt_license::Plan::Pro)) => Stage::Pro,
                (_, Some(convt_license::Plan::Desktop)) => Stage::Licensed,
                (SignIn::Failed(e), None) => Stage::Failed(e.clone()),
                (SignIn::Idle, None) => Stage::SignIn,
            };
        }
        match &account.access {
            Some(Access::Pro) => Stage::Pro,
            Some(Access::Trial { ends_on }) => Stage::Trial {
                ends_on: ends_on.clone(),
            },
            Some(Access::CanStartTrial { .. }) if account.awaiting_trial => Stage::AwaitingTrial,
            Some(Access::CanStartTrial { .. }) => Stage::CanStartTrial,
            Some(Access::Lapsed) => Stage::Lapsed,
            None => match (licensed, &account.refresh) {
                (Some(convt_license::Plan::Pro), _) => Stage::Pro,
                (Some(convt_license::Plan::Desktop), _) => Stage::Licensed,
                (None, Refresh::Running) => Stage::Checking,
                // [`Self::ask_account`] is about to ask.
                (None, Refresh::Idle) if !self.asked => Stage::Checking,
                (None, Refresh::Failed(e)) => Stage::CheckFailed(e.clone()),
                (None, _) => Stage::CheckFailed(
                    "convt.app didn't say what this account includes. Try again in a moment."
                        .into(),
                ),
            },
        }
    }

    /// Asks convt.app what a signed-in account allows when nothing has
    /// asked yet, once: the launch check skips a day it already asked, and
    /// onboarding can't wait on an answer that isn't coming.
    fn ask_account(&mut self, cx: &mut Context<Self>) {
        let account = &self.app.read(cx).account;
        if self.asked
            || self.screen != Screen::Account
            || account.session.is_none()
            || account.access.is_some()
            || account.refresh != Refresh::Idle
        {
            return;
        }
        self.asked = true;
        self.app.update(cx, |s, cx| s.refresh_license(cx));
    }

    /// The questions this computer gets, in order.
    fn questions(&self, cx: &App) -> Vec<Question> {
        let state = self.app.read(cx);
        let mut out = Vec::new();
        let pack_busy = matches!(state.pack.phase, PackPhase::Working(_));
        if state.pack.offer.configured && !state.documents_supported() && !pack_busy {
            out.push(Question::Documents);
        }
        if cfg!(target_os = "macos") && state.finder_on != Some(true) {
            out.push(Question::Finder);
        }
        out
    }

    fn go(&mut self, screen: Screen, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = screen;
        self.error = None;
        self.yes = true;
        if screen == Screen::Calibrating {
            self.calibrate(window, cx);
        }
        self.ask_account(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// From the account screen to the first question, or straight on.
    pub(super) fn continue_from_account(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = match self.questions(cx).first() {
            Some(q) => Screen::Question(*q),
            None => Screen::Calibrating,
        };
        self.go(next, window, cx);
    }

    /// Opens the same trial checkout again, for a closed tab. Only the first
    /// press starts the trial flow.
    fn reopen_checkout(&mut self, cx: &mut Context<Self>) {
        let account = &self.app.read(cx).account;
        if account.awaiting_trial
            && let Some(Access::CanStartTrial { checkout_url }) = &account.access
        {
            cx.open_url(checkout_url);
        }
    }

    fn not_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.continue_from_account(window, cx);
    }

    /// Answers the question on screen and moves to the next one.
    pub(super) fn answer(&mut self, yes: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Screen::Question(question) = self.screen else {
            return;
        };
        if yes {
            match question {
                // A click on Yes is the user asking for the download, as the
                // Download button is.
                Question::Documents => super::pack::start_install(&self.app, cx),
                Question::Finder => cx.open_url(EXTENSION_SETTINGS),
            }
        }
        let questions = self.questions(cx);
        let next = questions
            .iter()
            .skip_while(|q| **q != question)
            .nth(1)
            .or_else(|| {
                // The pack question drops out of the list once its download
                // starts; what follows it is still next.
                (question == Question::Documents)
                    .then(|| questions.iter().find(|q| **q != Question::Documents))
                    .flatten()
            })
            .copied();
        let next = match next {
            Some(q) if q != question => Screen::Question(q),
            _ => Screen::Calibrating,
        };
        self.go(next, window, cx);
    }

    fn calibrate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._finish = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(CALIBRATE).await;
            let _ = this.update_in(cx, |this, window, cx| this.finish(window, cx));
        }));
    }

    /// Ends onboarding: only here, so closing earlier shows it again. The
    /// main window opens first, so the app never sees its last window close.
    pub(super) fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.app.update(cx, |s, cx| {
            s.update_settings(|s| s.first_run_done = true, cx)
        });
        super::show_main(cx);
        window.remove_window();
    }

    fn sign_in(&mut self, provider: Provider, cx: &mut Context<Self>) {
        self.provider = provider;
        self.error = None;
        self.app
            .update(cx, |s, cx| s.start_sign_in_with(provider, cx));
    }

    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.key.read(cx).value().trim().to_string();
        if key.is_empty() {
            self.error = Some("Paste your license key first.".into());
            cx.notify();
            return;
        }
        match self.app.update(cx, |s, cx| s.activate(&key, cx)) {
            Ok(_) => self.go(Screen::Account, window, cx),
            Err(e) => {
                self.error = Some(e);
                cx.notify();
            }
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen == Screen::Account && event.keystroke.key == "enter" {
            // Return continues where Continue is the main button.
            if matches!(
                self.stage(cx),
                Stage::Pro | Stage::Licensed | Stage::Trial { .. }
            ) {
                self.continue_from_account(window, cx);
            }
            return;
        }
        let Screen::Question(_) = self.screen else {
            return;
        };
        match event.keystroke.key.as_str() {
            "left" | "up" => self.yes = true,
            "right" | "down" => self.yes = false,
            "tab" => self.yes = !self.yes,
            "enter" | "space" => {
                let yes = self.yes;
                self.answer(yes, window, cx);
                return;
            }
            _ => return,
        }
        cx.notify();
    }

    fn account_screen(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let stage = self.stage(cx);
        let email = self.app.read(cx).account.masked_email();
        let signed_in = email.map(|email| {
            let line = SharedString::from(format!("Signed in as {email}"));
            div()
                .id("account-status")
                .test_support()
                .aria_label(line.clone())
                .child(styled(theme::size::SMALL, p.tertiary).child(line))
        });
        match stage {
            Stage::SignIn => self.sign_in_buttons(p, cx),
            Stage::Waiting => column()
                .child(spinner(p))
                .child(heading("Continue in your browser", p))
                .child(line("Finish signing in on convt.app, then come back here.", p))
                .child(
                    actions()
                        .child(
                            pill("onboarding-reopen", "Open again", Look::Secondary, p)
                                .w_auto()
                                .min_w(px(160.))
                                .px(px(24.)).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.app.update(cx, |s, cx| s.reopen_sign_in(cx))
                                }),
                            ),
                        )
                        .child(link("onboarding-cancel", "Cancel", p).on_click(cx.listener(
                            |this, _, _, cx| this.app.update(cx, |s, cx| s.cancel_sign_in(cx)),
                        ))),
                ),
            Stage::Finishing => column()
                .child(spinner(p))
                .child(heading("Signing you in…", p)),
            Stage::Failed(e) => column()
                .child(heading("Sign-in didn't finish", p))
                .child(problem(e, p))
                .child(
                    actions()
                        .child(
                            pill("onboarding-retry", "Try again", Look::Primary, p)
                                .w_auto()
                                .min_w(px(160.))
                                .px(px(24.)).on_click(
                                cx.listener(|this, _, _, cx| {
                                    let provider = this.provider;
                                    this.sign_in(provider, cx)
                                }),
                            ),
                        )
                        .child(self.key_link(p, cx)),
                ),
            Stage::Checking => column()
                .child(spinner(p))
                .child(heading("Checking your account…", p))
                .children(signed_in),
            Stage::CheckFailed(e) => column()
                .child(heading("Couldn't reach convt.app", p))
                .child(problem(e, p))
                .child(
                    pill("onboarding-retry", "Retry", Look::Primary, p).on_click(cx.listener(
                        |this, _, _, cx| this.app.update(cx, |s, cx| s.refresh_license(cx)),
                    )),
                )
                .child(self.key_link(p, cx))
                .children(signed_in),
            Stage::Pro => self
                .welcome(
                    "You have convt Pro",
                    "Every format, on this computer and in the Cloud.",
                    p,
                    cx,
                )
                .children(signed_in),
            Stage::Licensed => self
                .welcome(
                    "You have a convt license",
                    "convt converts on this computer, for good.",
                    p,
                    cx,
                )
                .children(signed_in),
            Stage::NotCovered { until } => column()
                .child(heading("Your license is saved", p))
                .child(line(
                    &format!(
                        "It covers versions released up to {until}. This one is newer, so renew to convert with it."
                    ),
                    p,
                ))
                .child(
                    pill("onboarding-primary", "Renew", Look::Brand, p)
                        .on_click(|_, _, cx| cx.open_url(BUY_URL)),
                )
                .child(self.secondary_links(p, cx)),
            Stage::Trial { ends_on } => self
                .welcome(
                    "Your free trial is on",
                    &format!("Every Pro feature until {}.", friendly_date(&ends_on)),
                    p,
                    cx,
                )
                .children(signed_in),
            Stage::CanStartTrial => column()
                .child(heading("Start your 7-day free trial", p))
                .child(line(
                    "Checkout opens in your browser. You won't be charged until the trial ends, and you can cancel before then.",
                    p,
                ))
                .child(
                    pill("onboarding-primary", "Start free trial", Look::Brand, p)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.app.update(cx, |s, cx| s.start_trial(cx))
                        })),
                )
                .child(self.secondary_links(p, cx))
                .children(signed_in),
            Stage::AwaitingTrial => column()
                .child(spinner(p))
                .child(heading("Finish checkout in your browser", p))
                .child(line("This updates on its own once your trial starts.", p))
                .child(
                    actions()
                        .child(
                            pill("onboarding-reopen", "Open checkout again", Look::Secondary, p)
                                .w_auto()
                                .min_w(px(160.))
                                .px(px(24.))
                                .on_click(cx.listener(|this, _, _, cx| this.reopen_checkout(cx))),
                        )
                        .child(self.not_now_link(p, cx)),
                ),
            Stage::Lapsed => column()
                .child(heading("Your Pro plan has ended", p))
                .child(line("Get convt Pro to keep converting, or use a license key.", p))
                .child(
                    pill("onboarding-primary", "Get convt Pro", Look::Brand, p)
                        .on_click(|_, _, cx| cx.open_url(BUY_URL)),
                )
                .child(self.secondary_links(p, cx))
                .children(signed_in),
        }
    }

    /// The title, one line and Continue.
    fn welcome(&self, title: &str, body: &str, p: &Palette, cx: &mut Context<Self>) -> Div {
        column()
            .child(
                div()
                    .size(px(44.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(p.green_tint)
                    .shadow(vec![theme::inset_ring(p.green_border, 1.)])
                    .child(icon(IconName::Check, 22., p.green)),
            )
            .child(heading(title, p))
            .child(line(body, p))
            .child(
                pill("onboarding-primary", "Continue", Look::Primary, p).on_click(
                    cx.listener(|this, _, window, cx| this.continue_from_account(window, cx)),
                ),
            )
    }

    fn sign_in_buttons(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let notice = self.app.read(cx).account.notice.clone();
        column()
            .child(
                text(17., 24., p.secondary)
                    .font_weight(FontWeight::MEDIUM)
                    .child("Sign in or create your account"),
            )
            .child(div().h(px(14.)))
            .child(
                pill(
                    "onboarding-google",
                    "Continue with Google",
                    Look::Primary,
                    p,
                )
                .on_click(cx.listener(|this, _, _, cx| this.sign_in(Provider::Google, cx))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .w(px(PILL_WIDTH))
                    .child(div().flex_1().h(px(1.)).bg(p.control_border))
                    .child(styled(theme::size::SMALL, p.tertiary).child("or"))
                    .child(div().flex_1().h(px(1.)).bg(p.control_border)),
            )
            .child(
                pill(
                    "onboarding-email",
                    "Continue with Email",
                    Look::Secondary,
                    p,
                )
                .on_click(cx.listener(|this, _, _, cx| this.sign_in(Provider::Email, cx))),
            )
            .children(notice.map(|n| problem(n, p)))
            .child(div().h(px(6.)))
            .child(self.key_link(p, cx))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        styled(theme::size::CAPTION, p.tertiary)
                            .child("By continuing, you agree to the"),
                    )
                    .child(underlined("terms", "Terms", TERMS_URL, p))
                    .child(styled(theme::size::CAPTION, p.tertiary).child("and"))
                    .child(underlined("privacy", "Privacy Policy", PRIVACY_URL, p)),
            )
    }

    fn key_link(&self, p: &Palette, cx: &mut Context<Self>) -> theme::Clickable {
        link("onboarding-key-link", "I have a license key", p)
            .on_click(cx.listener(|this, _, window, cx| this.go(Screen::Key, window, cx)))
    }

    fn not_now_link(&self, p: &Palette, cx: &mut Context<Self>) -> theme::Clickable {
        link("onboarding-not-now", "Not now", p)
            .on_click(cx.listener(|this, _, window, cx| this.not_now(window, cx)))
    }

    fn secondary_links(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        actions()
            .child(self.key_link(p, cx))
            .child(div().size(px(3.)).rounded_full().bg(p.tertiary))
            .child(self.not_now_link(p, cx))
    }

    fn key_screen(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        column()
            .child(heading("Enter your license key", p))
            .child(line("It's in the email you got with your purchase.", p))
            .child(
                div()
                    .w(px(PILL_WIDTH))
                    .child(theme::field(&self.key, "onboarding-key")),
            )
            .children(self.error.clone().map(|e| problem(e, p)))
            .child(
                pill("onboarding-activate", "Activate", Look::Primary, p)
                    .on_click(cx.listener(|this, _, window, cx| this.activate(window, cx))),
            )
            .child(
                link("onboarding-back", "Back", p).on_click(
                    cx.listener(|this, _, window, cx| this.go(Screen::Account, window, cx)),
                ),
            )
    }

    fn question_screen(&self, question: Question, p: &Palette, cx: &mut Context<Self>) -> Div {
        let state = self.app.read(cx);
        let detail = match question {
            Question::Documents => match state.pack.offer.download {
                Some(bytes) => format!(
                    "Word, Excel, PowerPoint and more. One {} download.",
                    super::human_size(bytes)
                ),
                None => "Word, Excel, PowerPoint and more. One download.".to_string(),
            },
            Question::Finder => {
                "Right-click any file in Finder to convert it. System Settings opens to turn it on."
                    .to_string()
            }
        };
        let (yes_icon, yes_label, no_label) = match question {
            Question::Documents => (IconName::Document, "Yes, add them", "Not now"),
            Question::Finder => (IconName::FolderOpen, "Yes, add it", "Not now"),
        };
        let tile = |id: &'static str, yes: bool, glyph: IconName, label: &'static str| {
            theme::choice_tile(id, label, glyph, self.yes == yes, p)
                .on_click(cx.listener(move |this, _, window, cx| this.answer(yes, window, cx)))
        };
        column()
            .child(heading(question.title(), p))
            .child(line(&detail, p))
            .child(div().h(px(8.)))
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .child(tile("question-yes", true, yes_icon, yes_label))
                    .child(tile("question-no", false, IconName::Close, no_label)),
            )
            .child(
                styled(theme::size::CAPTION, p.tertiary)
                    .pt(px(6.))
                    .child("Use the arrow keys and Return"),
            )
    }

    fn calibrating(&self, p: &Palette, cx: &App) -> AnyElement {
        let still = cx.reduce_motion();
        let p = *p;
        let content = div().id("setup").flex().flex_col().items_center();
        if still {
            return setup_content(content, 1., &p, true).into_any_element();
        }
        let total = CALIBRATE.as_secs_f32();
        content
            .with_animation("setup", Animation::new(CALIBRATE), move |d, t| {
                setup_content(d, t * total, &p, false)
            })
            .into_any_element()
    }

    /// The dithered glow behind every screen.
    fn backdrop(&self, p: &Palette, window: &Window, cx: &App) -> Div {
        let theme = if p.dark { "dark" } else { "light" };
        let still = cx.reduce_motion();
        // Behind other windows nothing needs to move, so nothing asks for
        // frames.
        let breathing = !still && window.is_window_active();
        let glow = img(SharedString::from(format!("onboarding/glow-{theme}.png")))
            .absolute()
            .bottom_0()
            .left(relative(0.5))
            .ml(px(-GLOW.0 / 2.))
            .w(px(GLOW.0))
            .h(px(GLOW.1));
        let glow = if breathing {
            // A slow breath, about once every seven seconds. Only the
            // opacity changes: nothing moves, so nothing around it does.
            glow.with_animation(
                "glow",
                Animation::new(Duration::from_secs(7)).repeat(),
                |img, t| img.opacity(breath(t)),
            )
            .into_any_element()
        } else {
            glow.opacity(breath(0.)).into_any_element()
        };
        div().absolute().inset_0().overflow_hidden().child(glow)
    }
}

/// The glow's opacity `t` of the way through a breath: dimmest at the start
/// and end, so a still glow and a breathing one meet without a jump.
pub(super) fn breath(t: f32) -> f32 {
    let wave = (1. - (t * std::f32::consts::TAU).cos()) / 2.;
    0.78 + 0.22 * wave
}

use theme::GLOW;

const PILL_WIDTH: f32 = 340.;

/// A wide pill, as on the sign-in screen: the app's [`theme::Button`] at its
/// large size.
fn pill(id: &'static str, label: &'static str, look: Look, p: &Palette) -> theme::Clickable {
    let button = theme::Button::new(id, label, look).large();
    let button = match id {
        "onboarding-google" => button.google(),
        "onboarding-email" => button.icon(IconName::Mail),
        _ => button,
    };
    button.build(p).w(px(PILL_WIDTH))
}

fn link(id: &'static str, label: &'static str, p: &Palette) -> theme::Clickable {
    theme::text_button(id, label, p.secondary, 13.)
}

fn underlined(
    id: &'static str,
    label: &'static str,
    url: &'static str,
    p: &Palette,
) -> theme::Clickable {
    theme::clickable(id, label)
        .child(
            styled(theme::size::CAPTION, p.secondary)
                .underline()
                .hover(|s| s.text_color(p.text))
                .child(label),
        )
        .on_click(move |_, _, cx| cx.open_url(url))
}

fn column() -> Div {
    div().flex().flex_col().items_center().gap(px(14.))
}

fn actions() -> Div {
    div().flex().items_center().justify_center().gap(px(16.))
}

fn heading(title: &str, p: &Palette) -> impl IntoElement + use<> {
    let title = SharedString::from(title.to_string());
    div()
        .id("onboarding-title")
        .test_support()
        .aria_label(title.clone())
        .child(
            text(24., 30., p.text)
                .font_weight(FontWeight::SEMIBOLD)
                .text_center()
                .child(title),
        )
}

fn line(body: &str, p: &Palette) -> Div {
    text(14., 21., p.secondary)
        .max_w(px(380.))
        .text_center()
        .child(SharedString::from(body.to_string()))
}

/// What went wrong, centered and wrapped like [`line`], in the error color.
fn problem(message: impl Into<SharedString>, p: &Palette) -> impl IntoElement {
    let message = message.into();
    div()
        .id("error")
        .test_support()
        .aria_label(message.clone())
        .max_w(px(420.))
        .child(text(13., 20., p.error).text_center().child(message))
}

fn spinner(p: &Palette) -> impl IntoElement {
    Spinner::new().with_size(px(22.)).color(p.green)
}

/// One turn of the setup spinner. Steady, not eased: an easing that slows
/// every turn reads as a stutter.
const TURN: Duration = Duration::from_millis(1100);
/// The spinner's size: its SVGs' pixel size, drawn unscaled.
const SPINNER: f32 = 64.;

/// 1 until the fade begins, then down to 0 at the end of [`CALIBRATE`].
fn fade_out(at: f32) -> f32 {
    let (end, fade) = (CALIBRATE.as_secs_f32(), FADE.as_secs_f32());
    ((end - at) / fade).clamp(0., 1.)
}

/// The setup step `at` seconds in: a ring with a turning arc around the mark,
/// and one line under it. It eases in, then fades out. `still`
/// draws it with nothing moving.
fn setup_content(d: Stateful<Div>, at: f32, p: &Palette, still: bool) -> Stateful<Div> {
    let appear = if still {
        1.
    } else {
        ease_out_quint()((at / 0.5).clamp(0., 1.))
    };
    let ring = |path: &'static str| svg().absolute().inset_0().size(px(SPINNER)).path(path);
    let arc = ring("onboarding/spinner-arc.svg").text_color(p.green);
    let arc = if still {
        arc.with_transformation(Transformation::rotate(percentage(0.1)))
            .into_any_element()
    } else {
        arc.with_animation("turn", Animation::new(TURN).repeat(), |arc, t| {
            arc.with_transformation(Transformation::rotate(percentage(t)))
        })
        .into_any_element()
    };
    let spinner = div()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .size(px(SPINNER))
        .child(ring("onboarding/spinner-track.svg").text_color(p.text.opacity(0.07)))
        .child(arc)
        .child(theme::mark(24., p));
    let title = "Setting up convt";
    d.opacity(if still { 1. } else { appear * fade_out(at) })
        .child(spinner)
        .child(
            div()
                .id("onboarding-title")
                .test_support()
                .aria_label(title)
                .mt(px(22. + 4. * (1. - appear)))
                .child(
                    text(15., 22., p.text)
                        .font_weight(FontWeight::MEDIUM)
                        .text_center()
                        .child(title),
                ),
        )
}

/// "2026-10-15" as "October 15".
fn friendly_date(day: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let mut parts = day.split('-');
    let (_, month, d) = (parts.next(), parts.next(), parts.next());
    match (
        month.and_then(|m| m.parse::<usize>().ok()),
        d.and_then(|d| d.parse::<u32>().ok()),
    ) {
        (Some(m @ 1..=12), Some(d)) => format!("{} {d}", MONTHS[m - 1]),
        _ => day.to_string(),
    }
}

impl Render for FirstRunView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let content = match self.screen {
            Screen::Account => self.account_screen(&p, cx).into_any_element(),
            Screen::Key => self.key_screen(&p, cx).into_any_element(),
            Screen::Question(q) => self.question_screen(q, &p, cx).into_any_element(),
            Screen::Calibrating => self.calibrating(&p, cx),
        };
        let calibrating = self.screen == Screen::Calibrating;
        div()
            .id("first-run")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e, window, cx| this.key_down(e, window, cx)))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(p.window)
            .font_family(theme::SANS)
            .text_color(p.text)
            .child(self.backdrop(&p, window, cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap(px(36.))
                    // Sit a little above the middle, as the eye expects.
                    .pb(px(if calibrating { 0. } else { 96. }))
                    .px(px(32.))
                    .when(!calibrating, |d| d.child(theme::lockup(30., &p)))
                    .child(content),
            )
    }
}
