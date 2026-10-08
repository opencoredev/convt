//! The first-run window: turn on the Finder menu (macOS only), start the
//! trial or enter a license, and a last word on how to convert. It shows
//! until the last step is finished, and only in builds that check licenses.
//! Closing mid-setup shows it again. Skipping the Finder step still leaves
//! a recover card on Activity until the extension is on.
//!
//! The plan step also offers an optional convt.app sign-in for Pro
//! subscribers, so their key renews itself (see `crate::account`). The trial
//! and a Desktop key never need it: starting the trial opens no browser.
//! A machine that already has a license (a key, or a sign-in that fetched
//! one) sees it on the plan step instead of the choice.

use convt_license::License;
use convt_license::client::{BUY_URL, State};
use gpui_kit::component::IconName;
use gpui_kit::component::input::InputState;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::LICENSE_PRICE;
use super::theme::{
    self, Button, Palette, Tone, icon, mono, radius, size, space, styled, text, text_button,
};
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
            Step::Plan if self.licensed(cx).is_some() => self.step = Step::Done,
            Step::Plan => match self.plan {
                Plan::Trial => self.step = Step::Done,
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

    /// The license this machine already has, which makes the trial moot.
    fn licensed(&self, cx: &App) -> Option<License> {
        match &self.app.read(cx).license {
            State::Licensed(license) => Some(license.clone()),
            _ => None,
        }
    }

    fn pick_plan(&mut self, plan: Plan, cx: &mut Context<Self>) {
        self.plan = plan;
        self.error = None;
        cx.notify();
    }

    /// A picture of the System Settings pane, so the user knows what to look
    /// for. It is drawn, not a control: nothing in it reacts to the pointer.
    fn finder_art(&self, on: Option<bool>, p: &Palette) -> impl IntoElement {
        let on = on == Some(true);
        let caption = if on {
            "It's on. Come back here and continue."
        } else {
            "This picture isn't a switch."
        };
        let dot = |color: u32| div().size(px(8.)).rounded(px(4.)).bg(rgb(color));
        div()
            .id("finder-preview")
            .test_support()
            .aria_label("Preview of System Settings. This picture is not a switch.")
            .occlude()
            .cursor_default()
            .flex()
            .flex_col()
            .flex_1()
            .rounded(px(radius::CARD))
            .overflow_hidden()
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(30.))
                    .px(px(space::MD))
                    .bg(p.recessed)
                    .border_b_1()
                    .border_color(p.hairline)
                    .child(dot(0xFF5F57))
                    .child(dot(0xFEBC2E))
                    .child(dot(0x28C840))
                    .child(
                        styled(size::CAPTION, p.tertiary)
                            .flex_1()
                            .pl(px(6.))
                            .font_weight(FontWeight::MEDIUM)
                            .child("System Settings"),
                    )
                    .child(
                        div()
                            .id("finder-preview-badge")
                            .test_support()
                            .aria_label("Preview")
                            .child(theme::badge("PREVIEW", Tone::Neutral, p)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .p(px(14.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(styled(size::SMALL, p.secondary).child("General"))
                            .child(icon(IconName::ChevronRight, 11., p.tertiary))
                            .child(
                                styled(size::SMALL, p.text)
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Login Items & Extensions"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(icon(IconName::ArrowDown, 11., p.tertiary))
                            .child(styled(size::CAPTION, p.tertiary).child("Scroll to Extensions")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .px(px(space::MD))
                            .py(px(10.))
                            .rounded(px(8.))
                            .bg(p.recessed)
                            .border_1()
                            .border_color(if on { p.green_border } else { p.border })
                            .child(theme::mark(22., p))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .child(
                                        styled(size::BODY, p.text)
                                            .font_weight(FontWeight::MEDIUM)
                                            .child("convt"),
                                    )
                                    .child(
                                        styled(size::CAPTION, p.secondary)
                                            .child("Finder extension"),
                                    ),
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
                                    .opacity(0.75)
                                    .when(on, |d| d.justify_end())
                                    .bg(if on { p.green } else { p.toggle_off })
                                    .child(div().size(px(12.)).rounded(px(6.)).bg(rgb(0xFFFFFF))),
                            ),
                    )
                    .child(
                        div()
                            .id("finder-preview-caption")
                            .test_support()
                            .aria_label(caption)
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .children(on.then(|| icon(IconName::CircleCheck, 12., p.green)))
                            .child(
                                styled(size::CAPTION, if on { p.green_text } else { p.tertiary })
                                    .child(caption),
                            ),
                    ),
            )
    }

    /// The plan step for a machine that has a license: it, selected, in
    /// place of the trial and the key field.
    fn licensed_art(&self, license: &License, p: &Palette, cx: &App) -> Div {
        let (title, about) = match license.plan {
            convt_license::Plan::Pro => (
                "convt Pro",
                format!("Paid through {}", license.updates_until),
            ),
            convt_license::Plan::Desktop => (
                "convt license",
                format!("Updates through {}", license.updates_until),
            ),
        };
        let email = self.app.read(cx).account.email().map(str::to_string);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .gap(px(space::SM))
            .child(
                div()
                    .id("plan-licensed")
                    .test_support()
                    .aria_label(title)
                    .aria_selected(true)
                    .flex()
                    .items_start()
                    .gap(px(space::MD))
                    .p(px(14.))
                    .rounded(px(radius::CARD))
                    .bg(p.green_tint)
                    .shadow(vec![theme::inset_ring(p.green, 1.5)])
                    .child(
                        div()
                            .pt(px(1.))
                            .child(icon(IconName::CircleCheck, 16., p.green)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .gap(px(3.))
                            .child(
                                styled(size::BODY, p.text)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(styled(size::SMALL, p.secondary).child(about)),
                    ),
            )
            .children(email.map(|email| {
                let signed_in = SharedString::from(format!("Signed in as {email}"));
                div()
                    .id("account-status")
                    .test_support()
                    .aria_label(signed_in.clone())
                    .px(px(2.))
                    .pt(px(6.))
                    .child(styled(size::SMALL, p.secondary).child(signed_in))
            }))
    }

    fn plan_art(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let card = |id: &'static str, plan: Plan, title: &'static str, about: AnyElement| {
            let on = self.plan == plan;
            theme::clickable(id, title)
                .aria_selected(on)
                .flex()
                .items_start()
                .gap(px(space::MD))
                .p(px(14.))
                .rounded(px(radius::CARD))
                .map(|d| {
                    if on {
                        d.bg(p.green_tint)
                            .shadow(vec![theme::inset_ring(p.green, 1.5)])
                    } else {
                        d.bg(p.surface)
                            .shadow(vec![theme::inset_ring(p.border, 1.)])
                            .hover(|s| s.bg(p.recessed))
                    }
                })
                .on_click(cx.listener(move |this, _, _, cx| this.pick_plan(plan, cx)))
                .child(div().pt(px(1.)).child(theme::radio(on, p)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .gap(px(3.))
                        .child(
                            styled(size::BODY, p.text)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(about),
                )
        };
        let extra = super::account::compact(&self.app, p, cx);
        let about = |line: &'static str| {
            styled(size::SMALL, p.secondary)
                .child(line)
                .into_any_element()
        };
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
            .gap(px(space::SM))
            .child(card(
                "plan-trial",
                Plan::Trial,
                "Start 7-day trial",
                about("Every feature, no card, no account."),
            ))
            .child(card("plan-key", Plan::Key, "I have a license", key_about))
            .child(div().px(px(2.)).pt(px(6.)).child(extra))
    }

    /// A file's right-click menu with convt's submenu, as the file manager
    /// draws it.
    fn done_art(&self, p: &Palette) -> Div {
        let item = |label: &'static str| {
            div()
                .px(px(8.))
                .py(px(3.))
                .child(styled(size::SMALL, p.secondary).child(label))
        };
        let menu = |w: f32| {
            div()
                .flex()
                .flex_col()
                .w(px(w))
                .p(px(5.))
                .rounded(px(8.))
                .bg(p.overlay)
                .shadow(vec![
                    theme::inset_ring(p.border, 1.),
                    theme::shadow(p.shadow_soft, 6., 16.),
                ])
        };
        let separator = || div().h(px(1.)).my(px(4.)).mx(px(4.)).bg(p.hairline);
        // The light --green in both appearances: white on it is 5.4:1.
        let highlight = rgb(0x127A47);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .gap(px(10.))
            .p(px(space::LG))
            .rounded(px(radius::CARD))
            .bg(p.recessed)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .w(px(26.))
                            .h(px(20.))
                            .rounded(px(4.))
                            .bg(linear_gradient(
                                135.,
                                linear_color_stop(rgb(0xF4A261), 0.),
                                linear_color_stop(rgb(0x3A7BD5), 1.),
                            ))
                            .shadow(vec![theme::inset_ring(p.thumb_border, 1.)]),
                    )
                    .child(mono(11., 14., p.text).child("IMG_2041.heic")),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .child(
                        menu(196.)
                            .child(item("Open"))
                            // Finder's wording on macOS, the file managers' elsewhere.
                            .child(item(if cfg!(target_os = "macos") {
                                "Get Info"
                            } else {
                                "Properties"
                            }))
                            .child(separator())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px(px(8.))
                                    .py(px(3.))
                                    .rounded(px(4.))
                                    .bg(highlight)
                                    .child(
                                        text(12., 17., rgb(0xFFFFFF).into())
                                            .font_weight(FontWeight::MEDIUM)
                                            .child("Convert with convt"),
                                    )
                                    .child(icon(IconName::ChevronRight, 11., rgb(0xFFFFFF).into())),
                            ),
                    )
                    .child(
                        menu(132.)
                            .ml(px(-4.))
                            .mt(px(38.))
                            .child(item("JPEG"))
                            .child(item("PNG"))
                            .child(
                                div()
                                    .px(px(8.))
                                    .py(px(3.))
                                    .rounded(px(4.))
                                    .bg(p.hover)
                                    .child(styled(size::SMALL, p.text).child("WebP")),
                            )
                            .child(separator())
                            .child(item("More options…")),
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
            .items_center()
            .gap(px(5.))
            .children((1..=Step::count()).map(|i| {
                div()
                    .h(px(6.))
                    .w(px(if i == n { 18. } else { 6. }))
                    .rounded(px(3.))
                    .bg(if i <= n { p.green } else { p.track })
            }));
        // A saved key whose updates ended before this build can't convert.
        let not_covered = match &self.app.read(cx).license {
            State::NotCovered(license) if self.step == Step::Done => {
                Some(license.updates_until.clone())
            }
            _ => None,
        };
        let finder_on = self.app.read(cx).finder_on;
        let licensed = self.licensed(cx).filter(|_| self.step == Step::Plan);
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
            Step::Plan if let Some(license) = &licensed => match license.plan {
                convt_license::Plan::Pro => (
                    "You have Pro",
                    "convt and Cloud conversion are unlocked on this computer.".to_string(),
                    cfg!(target_os = "macos").then_some("Back"),
                    "Continue",
                ),
                convt_license::Plan::Desktop => (
                    "You have a license",
                    "convt is unlocked on this computer.".to_string(),
                    cfg!(target_os = "macos").then_some("Back"),
                    "Continue",
                ),
            },
            Step::Plan => (
                "Try it or unlock it",
                format!(
                    "The trial runs for 7 days with every feature. A {LICENSE_PRICE} license keeps convt working for good, with a year of updates."
                ),
                cfg!(target_os = "macos").then_some("Back"),
                "Continue",
            ),
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
                    "Right-click a file in Finder and pick a format, or drop files on the convt window.".to_string()
                } else {
                    "Right-click a file in your file manager and pick a format, or drop files on the convt window.".to_string()
                },
                Some("Back"),
                "Start converting",
            ),
        };
        let art = match self.step {
            Step::Finder => self.finder_art(finder_on, &p).into_any_element(),
            Step::Plan => match &licensed {
                Some(license) => self.licensed_art(license, &p, cx).into_any_element(),
                None => self.plan_art(&p, cx).into_any_element(),
            },
            Step::Done => self.done_art(&p).into_any_element(),
        };
        let step_label = format!("STEP {n} OF {}", Step::count());
        let next_button = match self.step {
            Step::Finder if !self.opened_settings && finder_on != Some(true) => {
                Button::primary("first-run-next", next).icon(IconName::ExternalLink)
            }
            _ => Button::primary("first-run-next", next),
        };
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
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .h(px(52.))
                    // The traffic lights sit at the left of a transparent title bar.
                    .pl(px(if theme::transparent_titlebar() {
                        84.
                    } else {
                        28.
                    }))
                    .pr(px(28.))
                    .child(theme::lockup(13., &p))
                    .child(steps),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .px(px(28.))
                    .pt(px(14.))
                    .child(
                        div()
                            .id("step")
                            .test_support()
                            .aria_label(SharedString::from(step_label))
                            .child(
                                mono(11., 14., p.tertiary)
                                    .child(format!("Step {n} of {}", Step::count())),
                            ),
                    )
                    .child(
                        div()
                            .id("first-run-title")
                            .test_support()
                            .aria_label(SharedString::from(title))
                            .child(
                                styled(size::DISPLAY, p.text)
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
                        div().flex().pt(px(2.)).child(
                            text_button("first-run-renew", "Renew", p.green_text, 12.)
                                .on_click(|_, _, cx| cx.open_url(BUY_URL)),
                        )
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .px(px(28.))
                    .pt(px(space::XL))
                    .pb(px(space::LG))
                    .child(art),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .px(px(28.))
                    .py(px(14.))
                    .bg(p.recessed)
                    .border_t_1()
                    .border_color(p.hairline)
                    .child(match back {
                        Some(label) => Button::ghost("first-run-back", label)
                            .build(&p)
                            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)))
                            .into_any_element(),
                        None => div().into_any_element(),
                    })
                    .child(
                        next_button
                            .build(&p)
                            .on_click(cx.listener(|this, _, window, cx| this.next(window, cx))),
                    ),
            )
    }
}
