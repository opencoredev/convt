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

use super::theme::{self, IconName, Palette, icon, radius, styled, text};
use crate::account::{Access, Provider, Refresh, SignIn};
use crate::finder::EXTENSION_SETTINGS;
use crate::model::{AppState, PackPhase};

/// How long the "Setting convt up" moment lasts before the main window opens.
pub const CALIBRATE: Duration = Duration::from_millis(1800);

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
    /// Signed in, and asking the account failed.
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
    focus: FocusHandle,
    _finish: Option<Task<()>>,
    _observe: Subscription,
    _appearance: Subscription,
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
            _observe: cx.observe(&app, |_, _, cx| cx.notify()),
            _appearance: theme::observe_appearance(window, cx),
            app,
            screen,
            provider: Provider::Google,
            yes: true,
            key: cx.new(|cx| InputState::new(window, cx).placeholder("Paste your license key")),
            error: None,
            focus,
            _finish: None,
        };
        if screen == Screen::Calibrating {
            view.calibrate(window, cx);
        }
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
                (None, Refresh::Failed(e)) => Stage::CheckFailed(e.clone()),
                (None, _) => Stage::Checking,
            },
        }
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
        let email = self.app.read(cx).account.email().map(str::to_string);
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
                            pill("onboarding-reopen", "Open again", Pill::Soft, p)
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
                .child(super::error_text(e, p))
                .child(
                    actions()
                        .child(
                            pill("onboarding-retry", "Try again", Pill::Strong, p)
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
                .child(heading("Couldn't reach your account", p))
                .child(super::error_text(e, p))
                .child(
                    actions().child(
                        pill("onboarding-retry", "Try again", Pill::Strong, p)
                                .w_auto()
                                .min_w(px(160.))
                                .px(px(24.)).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.app.update(cx, |s, cx| s.refresh_license(cx))
                            }),
                        ),
                    ),
                ),
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
                    pill("onboarding-primary", "Renew", Pill::Brand, p)
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
                    pill("onboarding-primary", "Start free trial", Pill::Brand, p)
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
                            pill("onboarding-reopen", "Open checkout again", Pill::Soft, p)
                                .w_auto()
                                .min_w(px(160.))
                                .px(px(24.))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.app.update(cx, |s, cx| s.start_trial(cx))
                                })),
                        )
                        .child(self.not_now_link(p, cx)),
                ),
            Stage::Lapsed => column()
                .child(heading("Your Pro plan has ended", p))
                .child(line("Get convt Pro to keep converting, or use a license key.", p))
                .child(
                    pill("onboarding-primary", "Get convt Pro", Pill::Brand, p)
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
                pill("onboarding-primary", "Continue", Pill::Strong, p).on_click(
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
                pill("onboarding-google", "Continue with Google", Pill::Strong, p)
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
                pill("onboarding-email", "Continue with Email", Pill::Soft, p)
                    .on_click(cx.listener(|this, _, _, cx| this.sign_in(Provider::Email, cx))),
            )
            .children(notice.map(|n| super::error_text(n, p)))
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
            .children(self.error.clone().map(|e| super::error_text(e, p)))
            .child(
                pill("onboarding-activate", "Activate", Pill::Strong, p)
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
            let on = self.yes == yes;
            theme::clickable(id, label)
                .aria_selected(on)
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(12.))
                .w(px(176.))
                .h(px(136.))
                .rounded(px(radius::PANEL + 2.))
                .bg(p.surface)
                .shadow(if on {
                    vec![
                        theme::inset_ring(p.green, 2.),
                        theme::shadow(p.shadow_soft, 8., 24.),
                    ]
                } else {
                    vec![
                        theme::inset_ring(p.border, 1.),
                        theme::shadow(p.shadow_soft, 2., 6.),
                    ]
                })
                .hover(|s| s.bg(p.recessed))
                .on_click(cx.listener(move |this, _, window, cx| this.answer(yes, window, cx)))
                .child(icon(glyph, 30., if on { p.green } else { p.secondary }))
                .child(
                    styled(theme::size::BODY, p.text)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(label),
                )
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
        let words = "Setting convt up…";
        // Over the bloom, so darker than the page's secondary text.
        let (base, bright) = (p.text.opacity(0.5), p.text);
        let still = cx.reduce_motion();
        let label = div()
            .id("onboarding-title")
            .test_support()
            .aria_label(words);
        if still {
            return label
                .child(
                    text(17., 24., base)
                        .font_weight(FontWeight::MEDIUM)
                        .child(words),
                )
                .into_any_element();
        }
        // A band of light sweeps across the words, letter by letter.
        label
            .with_animation(
                "shimmer",
                Animation::new(Duration::from_millis(1400)).repeat(),
                move |d, t| {
                    let n = words.chars().count() as f32;
                    let centre = -3. + t * (n + 6.);
                    let mut runs = Vec::new();
                    for (i, (at, ch)) in words.char_indices().enumerate() {
                        let k = (-((i as f32 - centre) / 2.2).powi(2)).exp();
                        let color = mix(base, bright, k);
                        runs.push((
                            at..at + ch.len_utf8(),
                            HighlightStyle {
                                color: Some(color),
                                ..Default::default()
                            },
                        ));
                    }
                    d.child(
                        text(17., 24., base)
                            .font_weight(FontWeight::MEDIUM)
                            .child(StyledText::new(words).with_highlights(runs)),
                    )
                },
            )
            .into_any_element()
    }

    /// The dithered glow behind every screen, and the bloom that fills the
    /// window while convt sets up.
    fn backdrop(&self, p: &Palette, cx: &App) -> Div {
        let theme = if p.dark { "dark" } else { "light" };
        let still = cx.reduce_motion();
        let calibrating = self.screen == Screen::Calibrating;
        let glow = img(SharedString::from(format!("onboarding/glow-{theme}.png")))
            .absolute()
            .bottom_0()
            .left(relative(0.5))
            .ml(px(-GLOW.0 / 2.))
            .w(px(GLOW.0))
            .h(px(GLOW.1));
        let glow = if still {
            glow.into_any_element()
        } else {
            // A slow drift and breath, about one cycle every seven seconds.
            glow.with_animation(
                "glow",
                Animation::new(Duration::from_secs(7)).repeat(),
                |img, t| {
                    let wave = (1. - (t * std::f32::consts::TAU).cos()) / 2.;
                    let sway = (t * std::f32::consts::TAU).sin();
                    img.opacity(0.78 + 0.22 * wave)
                        .mb(px(-14. + 14. * wave))
                        .ml(px(-GLOW.0 / 2. + 24. * sway))
                },
            )
            .into_any_element()
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
                bloom.into_any_element()
            } else {
                // Rises and brightens over the first half, then holds.
                bloom
                    .with_animation(
                        "bloom",
                        Animation::new(CALIBRATE.mul_f32(0.55)).with_easing(ease_out_quint()),
                        move |img, t| img.opacity(t).mt(px(top + 160. * (1. - t))),
                    )
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

/// The glow's and the bloom's size: the PNGs' pixel size, drawn unscaled.
const GLOW: (f32, f32) = (2400., 540.);
const BLOOM: (f32, f32) = (2400., 1500.);

const PILL_WIDTH: f32 = 340.;

#[derive(Clone, Copy, PartialEq)]
enum Pill {
    /// Ink on the page: the main sign-in button.
    Strong,
    /// A soft gray fill, for the second way in.
    Soft,
    /// The brand green, for a purchase or trial.
    Brand,
}

/// A full-width rounded button, as on the sign-in screen.
fn pill(id: &'static str, label: &'static str, look: Pill, p: &Palette) -> theme::Clickable {
    let (bg, fg) = match look {
        Pill::Strong if p.dark => (p.text, p.window),
        Pill::Strong => (rgb(0x111312).into(), rgb(0xFFFFFF).into()),
        Pill::Soft => (
            if p.dark {
                p.hover
            } else {
                rgb(0xF0F2F1).into()
            },
            p.text,
        ),
        Pill::Brand => (rgb(0x127A47).into(), rgb(0xFFFFFF).into()),
    };
    let lead = match id {
        "onboarding-google" => Some(theme::google_mark(16.).into_any_element()),
        "onboarding-email" => Some(icon(IconName::Mail, 16., fg).into_any_element()),
        _ => None,
    };
    theme::clickable(id, label)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(10.))
        .w(px(PILL_WIDTH))
        .h(px(42.))
        .rounded_full()
        .bg(bg)
        .when(look == Pill::Soft, |d| {
            d.shadow(vec![theme::inset_ring(p.border, 1.)])
        })
        .when(look != Pill::Soft, |d| {
            d.shadow(vec![theme::shadow(p.shadow_soft, 2., 8.)])
        })
        .hover(|s| s.opacity(0.9))
        .children(lead)
        .child(
            text(14., 18., fg)
                .font_weight(FontWeight::MEDIUM)
                .child(label),
        )
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(self.backdrop(&p, cx))
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
