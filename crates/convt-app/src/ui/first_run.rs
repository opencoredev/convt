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
use crate::pack;

/// How long the "Setting convt up" moment lasts before the main window
/// opens, its fade included. Nothing it lists takes longer: the document
/// download, the one slow thing, carries on in the background.
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
    /// Yes was the answer to the documents question, so the setup step
    /// follows the download.
    pub(super) documents_chosen: bool,
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
            documents_chosen: false,
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
                Question::Documents => {
                    self.documents_chosen = true;
                    super::pack::start_install(&self.app, cx)
                }
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

    /// What the setup step lists, in order, each with its state now.
    pub(super) fn setup_steps(&self, cx: &App) -> Vec<SetupStep> {
        let state = self.app.read(cx);
        let mut steps = Vec::new();
        if state.account.session.is_some() {
            steps.push(SetupStep::ready("account", "Signed in"));
        } else if matches!(state.license, State::Licensed(_) | State::NotCovered(_)) {
            steps.push(SetupStep::ready("account", "License key added"));
        }
        let plan = match self.stage(cx) {
            Stage::Trial { ends_on } => {
                Some(format!("Free trial on until {}", friendly_date(&ends_on)))
            }
            Stage::Pro => Some("convt Pro".to_string()),
            Stage::Licensed => Some("convt license".to_string()),
            _ => None,
        };
        steps.extend(plan.map(|label| SetupStep::ready("plan", label)));
        if self.documents_chosen {
            let pack = &state.pack;
            let documents = match &pack.phase {
                _ if state.documents_supported() => {
                    SetupStep::ready("documents", "Document support is ready")
                }
                PackPhase::Working(step) => {
                    let fraction = match step {
                        pack::Progress::Download { bytes, total } => total
                            .filter(|t| *t >= *bytes && *t > 0)
                            .or(pack.offer.download)
                            .map(|total| *bytes as f32 / total as f32),
                        _ => Some(1.),
                    };
                    let label = match step {
                        pack::Progress::Download { .. } => "Adding document support…",
                        pack::Progress::Verifying => "Checking document support…",
                        pack::Progress::Installing => "Installing document support…",
                    };
                    SetupStep {
                        key: "documents",
                        label: label.into(),
                        status: StepStatus::Working(fraction),
                    }
                }
                PackPhase::Failed(f) if f.kind == pack::FailureKind::Cancelled => SetupStep {
                    key: "documents",
                    label: "Document support stopped".into(),
                    status: StepStatus::Failed("Download it from Settings any time.".into()),
                },
                _ => SetupStep {
                    key: "documents",
                    label: "Couldn't add document support".into(),
                    status: StepStatus::Failed("Try again in Settings.".into()),
                },
            };
            steps.push(documents);
        }
        if cfg!(target_os = "macos") {
            steps.push(SetupStep::ready("finder", "Right-click menu in Finder"));
        }
        steps
    }

    fn calibrating(&self, p: &Palette, cx: &App) -> AnyElement {
        let steps = self.setup_steps(cx);
        let still = cx.reduce_motion();
        let p = *p;
        let content = div()
            .id("setup")
            .flex()
            .flex_col()
            .items_center()
            .w(px(400.));
        if still {
            return setup_content(content, &steps, 1., &p, true).into_any_element();
        }
        let total = CALIBRATE.as_secs_f32();
        content
            .with_animation("setup", Animation::new(CALIBRATE), move |d, t| {
                setup_content(d, &steps, t * total, &p, false)
            })
            .into_any_element()
    }

    /// The dithered glow behind every screen, and the bloom that fills the
    /// window while convt sets up.
    fn backdrop(&self, p: &Palette, window: &Window, cx: &App) -> Div {
        let theme = if p.dark { "dark" } else { "light" };
        let still = cx.reduce_motion();
        // Behind other windows nothing needs to move, so nothing asks for
        // frames.
        let breathing = !still && window.is_window_active();
        let calibrating = self.screen == Screen::Calibrating;
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
        let bloom = calibrating.then(|| {
            // Its brightest point (62% down the image) sits at the
            // window's middle.
            let top = -BLOOM.1 * 0.62;
            let bloom = img(SharedString::from(format!("onboarding/bloom-{theme}.png")))
                .absolute()
                .top(relative(0.5))
                .mt(px(top))
                .left(relative(0.5))
                .ml(px(-BLOOM.0 / 2.))
                .w(px(BLOOM.0))
                .h(px(BLOOM.1));
            if still {
                bloom.opacity(BLOOM_STRENGTH).into_any_element()
            } else {
                // Rises and brightens over the first half, holds, and
                // fades with the rest of the screen.
                let total = CALIBRATE.as_secs_f32();
                bloom
                    .with_animation("bloom", Animation::new(CALIBRATE), move |img, t| {
                        let rise = ease_out_quint()((t * total / 1.2).min(1.));
                        img.opacity(BLOOM_STRENGTH * rise * fade_out(t * total))
                            .mt(px(top + 160. * (1. - rise)))
                    })
                    .into_any_element()
            }
        });
        div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .child(glow)
            .children(bloom)
    }
}

/// The glow's opacity `t` of the way through a breath: dimmest at the start
/// and end, so a still glow and a breathing one meet without a jump.
pub(super) fn breath(t: f32) -> f32 {
    let wave = (1. - (t * std::f32::consts::TAU).cos()) / 2.;
    0.78 + 0.22 * wave
}

use theme::GLOW;
/// How strongly the bloom shows behind the setup step: enough to feel the
/// glow gather, never so much that it muddies the words over it.
const BLOOM_STRENGTH: f32 = 0.42;
/// The bloom's size: the PNG's pixel size, drawn unscaled.
const BLOOM: (f32, f32) = (2400., 1500.);

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

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.,
    }
    .into()
}

/// One line of the setup step.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SetupStep {
    pub key: &'static str,
    pub label: String,
    pub status: StepStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum StepStatus {
    /// Settles to a check as the list reaches it.
    Ready,
    /// Still going, with how far when known: the document download.
    Working(Option<f32>),
    /// Didn't work; the note says where to go next.
    Failed(String),
}

impl SetupStep {
    fn ready(key: &'static str, label: impl Into<String>) -> Self {
        Self {
            key,
            label: label.into(),
            status: StepStatus::Ready,
        }
    }
}

/// When the `i`th line appears and when it settles, in seconds.
fn step_times(i: usize) -> (f32, f32) {
    let shown = 0.25 + 0.12 * i as f32;
    (shown, 0.75 + 0.35 * i as f32)
}

/// 1 until the fade begins, then down to 0 at the end of [`CALIBRATE`].
fn fade_out(at: f32) -> f32 {
    let (end, fade) = (CALIBRATE.as_secs_f32(), FADE.as_secs_f32());
    ((end - at) / fade).clamp(0., 1.)
}

/// The setup step `at` seconds in: the breathing mark, the heading with its
/// sheen, and the list settling line by line. `still` draws the end state
/// with nothing moving.
fn setup_content(
    d: Stateful<Div>,
    steps: &[SetupStep],
    at: f32,
    p: &Palette,
    still: bool,
) -> Stateful<Div> {
    let ease = |x: f32| ease_out_quint()(x.clamp(0., 1.));
    // One slow breath over the whole moment.
    let breath = if still {
        0.
    } else {
        (1. - (at / CALIBRATE.as_secs_f32() * std::f32::consts::TAU).cos()) / 2.
    };
    let halo = div()
        .absolute()
        .size(px(112.))
        .rounded_full()
        .bg(p.green_tint)
        .shadow(vec![
            theme::inset_ring(p.green_border, 1.),
            BoxShadow {
                color: p.green.opacity(0.16 + 0.22 * breath),
                offset: point(px(0.), px(0.)),
                blur_radius: px(28. + 24. * breath),
                spread_radius: px(2. + 8. * breath),
                inset: false,
            },
        ]);
    let mark = div()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .size(px(112.))
        .child(halo)
        .child(theme::mark(48., p));
    let lines = steps.iter().enumerate().map(|(i, step)| {
        let (shown, settles) = step_times(i);
        let appear = if still { 1. } else { ease((at - shown) / 0.35) };
        let settled = still || at >= settles;
        setup_line(step, settled, at, p)
            .opacity(appear)
            .mt(px(4. * (1. - appear)))
    });
    // The card arrives with its first line, never empty.
    let card_in = if still {
        1.
    } else {
        ease((at - step_times(0).0) / 0.35)
    };
    let list = (!steps.is_empty()).then(|| {
        div()
            .opacity(card_in)
            .flex()
            .flex_col()
            .w_full()
            .mt(px(28.))
            .py(px(6.))
            .rounded(px(theme::radius::PANEL))
            .bg(p.surface)
            .shadow({
                let mut s = vec![theme::inset_ring(p.border, 1.)];
                s.extend(theme::soft(p));
                s
            })
            .children(lines)
    });
    d.opacity(if still { 1. } else { fade_out(at) })
        .child(mark)
        .child(div().h(px(22.)))
        .child(shimmer_heading("Setting convt up…", at, p, still))
        .child(
            // Over the glow, a step darker than secondary text.
            text(14., 21., mix(p.text, p.secondary, 0.45))
                .pt(px(6.))
                .text_center()
                .child("This only takes a moment."),
        )
        .children(list)
}

/// A line of the setup list: what it is, then a spinner that settles into a
/// check, live progress, or what went wrong. Tests read it by
/// `setup-step-{key}`.
fn setup_line(step: &SetupStep, settled: bool, at: f32, p: &Palette) -> theme::Clickable {
    let working = matches!(step.status, StepStatus::Working(_));
    let ending = at >= (CALIBRATE - FADE).as_secs_f32() - 0.4;
    let label: SharedString = if working && ending {
        "Document support finishes in the background".into()
    } else {
        step.label.clone().into()
    };
    let glyph = match (&step.status, settled) {
        (StepStatus::Ready, true) => div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(20.))
            .rounded_full()
            .bg(p.green)
            .child(icon(
                IconName::Check,
                12.,
                if p.dark {
                    p.window
                } else {
                    rgb(0xFFFFFF).into()
                },
            ))
            .into_any_element(),
        (StepStatus::Failed(_), true) => {
            icon(IconName::CircleAlert, 20., p.error).into_any_element()
        }
        _ => div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(20.))
            .child(Spinner::new().with_size(px(15.)).color(p.green))
            .into_any_element(),
    };
    let note = match (&step.status, settled) {
        (StepStatus::Failed(note), true) => Some(
            styled(theme::size::SMALL, p.secondary)
                .child(SharedString::from(note.clone()))
                .into_any_element(),
        ),
        (StepStatus::Working(Some(fraction)), _) => Some(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .flex_1()
                        .child(theme::progress(*fraction, p.track, p.green)),
                )
                .child(
                    theme::mono(11., 14., p.secondary)
                        .child(format!("{:.0}%", fraction.clamp(0., 1.) * 100.)),
                )
                .into_any_element(),
        ),
        _ => None,
    };
    let done = settled && step.status == StepStatus::Ready;
    div()
        .id(SharedString::from(format!("setup-step-{}", step.key)))
        .test_support()
        .aria_label(label.clone())
        .flex()
        .items_start()
        .gap(px(12.))
        .px(px(18.))
        .py(px(10.))
        .child(div().flex_shrink_0().pt(px(0.5)).child(glyph))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(6.))
                .child(
                    text(14., 21., if done || working { p.text } else { p.secondary })
                        .font_weight(FontWeight::MEDIUM)
                        .child(label),
                )
                .children(note),
        )
}

/// The heading in ink with a band of green light sweeping across it. Ink
/// and green both read well on the page, so every letter stays legible.
fn shimmer_heading(words: &'static str, at: f32, p: &Palette, still: bool) -> impl IntoElement {
    let base = p.text;
    let bright = p.green_text;
    let label = div()
        .id("onboarding-title")
        .test_support()
        .aria_label(words);
    let heading = |runs: Vec<(std::ops::Range<usize>, HighlightStyle)>| {
        text(24., 30., base)
            .font_weight(FontWeight::SEMIBOLD)
            .text_center()
            .child(StyledText::new(words).with_highlights(runs))
    };
    if still {
        return label.child(heading(Vec::new()));
    }
    // About one and a half passes over the moment.
    let n = words.chars().count() as f32;
    let centre = -4. + (at / 1.4).fract() * (n + 8.);
    let runs = words
        .char_indices()
        .enumerate()
        .map(|(i, (at, ch))| {
            let k = (-((i as f32 - centre) / 2.4).powi(2)).exp();
            (
                at..at + ch.len_utf8(),
                HighlightStyle {
                    color: Some(mix(base, bright, k)),
                    ..Default::default()
                },
            )
        })
        .collect();
    label.child(heading(runs))
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
