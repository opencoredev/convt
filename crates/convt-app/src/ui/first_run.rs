//! The first-run window: turn on the Finder menu (macOS only), start the
//! trial or enter a license, and a last word on how to convert. It shows
//! until the last step is finished, and only in builds that check licenses.
//! Closing mid-setup shows it again. Skipping the Finder step still leaves
//! a recover card on Activity until the extension is on.
//!
//! The plan step also offers an optional convt.app sign-in for Pro
//! subscribers, so their key renews itself (see `crate::account`). The trial
//! and a Desktop key never need it: starting the trial opens no browser.

use convt_license::client::{BUY_URL, State};
use gpui_kit::component::input::InputState;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::LICENSE_PRICE;
use super::theme::{self, Palette, mono, primary_button, text, text_button};
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

    fn plan_art(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
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
                about("Every feature, no card, no account."),
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
            Step::Plan => (
                "Try it or unlock it",
                format!(
                    "The trial runs for 7 days with every feature. A {LICENSE_PRICE} license keeps convt working for good, with a year of updates. Anonymous usage statistics are on by default; turn them off in Settings."
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
                    "Right-click any file in Finder and pick a format. The menu bar icon shows progress and lets you drop files in.".to_string()
                } else {
                    "Right-click any file in your file manager and pick a format, or drop files on the convt window.".to_string()
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
                        primary_button("first-run-next", next, 13., false)
                            .on_click(cx.listener(|this, _, window, cx| this.next(window, cx))),
                    ),
            )
    }
}
