//! The guide that sits beside System Settings while the user turns on the
//! Finder extension: a small panel that plays a loop of the steps on a
//! drawing of the Login Items & Extensions pane (scroll to Extensions at the
//! bottom, click ⓘ next to convt, turn on the switch), with the steps
//! written under it. The step being shown is the one in ink.
//!
//! On macOS the panel docks to the System Settings window and follows it,
//! shows only while System Settings is the frontmost app, and closes when
//! System Settings quits (`crate::system_settings`). It never takes focus
//! from System Settings. Once the extension is on it says so and closes.
//!
//! The drawing is built from the app's own shapes rather than a screenshot,
//! so it stays sharp, follows light and dark, and doesn't promise a pixel
//! layout macOS changes between versions. Reduced motion shows one still
//! frame with the sheet open and the switch on.

use std::time::Duration;
#[cfg(all(target_os = "macos", not(test)))]
use std::time::Instant;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{self, IconName, Palette, icon, size, styled, text};
use crate::model::AppState;

/// The panel's size in points.
pub const SIZE: (f32, f32) = (320., 360.);
/// The drawing's size: the panel's width less its padding.
const STAGE: (f32, f32) = (288., 172.);
/// One pass of the loop, in seconds.
const LOOP: f32 = 7.2;
/// How long "You're all set" shows before the panel closes.
const DONE_HOLD: Duration = Duration::from_millis(2600);
/// How often the panel looks for System Settings' window. Fast enough to
/// keep up while the window is dragged.
#[cfg(all(target_os = "macos", not(test)))]
const TRACK: Duration = Duration::from_millis(33);
/// How often it looks while the panel is hidden: often enough to come back
/// quickly when System Settings does, without waking 30 times a second for
/// a System Settings left open in the background.
#[cfg(all(target_os = "macos", not(test)))]
const TRACK_HIDDEN: Duration = Duration::from_millis(250);
/// System Settings counts as gone after this long without it running: it
/// can drop out briefly while it switches panes.
#[cfg(all(target_os = "macos", not(test)))]
const GONE: Duration = Duration::from_millis(500);
/// How long to wait for System Settings to start before giving up.
#[cfg(all(target_os = "macos", not(test)))]
const STARTUP: Duration = Duration::from_secs(20);

/// The drawn pointer and what it points at, in drawing coordinates.
const POINTER_START: (f32, f32) = (196., 118.);
const INFO: (f32, f32) = (258., 124.);
const SWITCH: (f32, f32) = (218., 87.);
const DONE_BUTTON: (f32, f32) = (222., 123.);

/// The moments of the loop, in seconds.
mod beat {
    pub const SCROLL: (f32, f32) = (0.6, 2.2);
    pub const RING: (f32, f32) = (2.2, 2.5);
    pub const TO_INFO: (f32, f32) = (2.5, 3.2);
    pub const PRESS_INFO: (f32, f32) = (3.25, 3.4);
    pub const SHEET_IN: (f32, f32) = (3.4, 3.7);
    pub const TO_SWITCH: (f32, f32) = (3.8, 4.4);
    pub const PRESS_SWITCH: (f32, f32) = (4.45, 4.6);
    pub const SWITCH_ON: (f32, f32) = (4.55, 4.8);
    pub const TO_DONE: (f32, f32) = (5.0, 5.5);
    pub const PRESS_DONE: (f32, f32) = (5.55, 5.7);
    pub const SHEET_OUT: (f32, f32) = (5.7, 5.95);
    pub const FADE_IN: (f32, f32) = (0., 0.25);
    pub const FADE_OUT: (f32, f32) = (6.7, 7.1);
    /// The frame shown with reduced motion.
    pub const STILL: f32 = 4.9;
}

/// How far `t` is through `span`, from 0 to 1.
fn through(t: f32, span: (f32, f32)) -> f32 {
    ((t - span.0) / (span.1 - span.0)).clamp(0., 1.)
}

fn during(t: f32, span: (f32, f32)) -> bool {
    t >= span.0 && t < span.1
}

fn mix(a: (f32, f32), b: (f32, f32), k: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k)
}

/// Strong ease-in-out for things moving on screen.
fn glide(k: f32) -> f32 {
    if k < 0.5 {
        4. * k * k * k
    } else {
        1. - (-2. * k + 2.).powi(3) / 2.
    }
}

fn settle(k: f32) -> f32 {
    ease_out_quint()(k)
}

/// The step the loop is showing at `t`: 0, 1 or 2.
fn step_at(t: f32) -> usize {
    if t < beat::RING.0 {
        0
    } else if t < beat::TO_SWITCH.0 {
        1
    } else {
        2
    }
}

pub struct FinderGuideView {
    app: Entity<AppState>,
    /// The extension came on: the panel says so, then closes.
    pub(super) done: bool,
    /// System Settings is frontmost with its window on screen, so the panel
    /// is up. Always true where nothing tracks System Settings.
    visible: bool,
    #[cfg(all(target_os = "macos", not(test)))]
    tracking: Tracking,
    _track: Option<Task<()>>,
    _close: Option<Task<()>>,
    _observe: Subscription,
    _appearance: Subscription,
}

/// What one look at System Settings asks of the panel.
#[cfg(all(target_os = "macos", not(test)))]
enum Step {
    Stay,
    Place { at: Option<(f64, f64)>, show: bool },
    Close,
}

#[cfg(all(target_os = "macos", not(test)))]
struct Tracking {
    opened: Instant,
    /// When System Settings was last running, once it has been.
    last_seen: Option<Instant>,
    at: Option<(f64, f64)>,
}

impl FinderGuideView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe_in(&app, window, |this: &mut Self, app, window, cx| {
            if !this.done && app.read(cx).finder_on == Some(true) {
                this.finish(window, cx);
            }
        });
        let mut view = Self {
            app,
            done: false,
            visible: !cfg!(all(target_os = "macos", not(test))),
            #[cfg(all(target_os = "macos", not(test)))]
            tracking: Tracking {
                opened: Instant::now(),
                last_seen: None,
                at: None,
            },
            _track: None,
            _close: None,
            _observe: observe,
            _appearance: theme::observe_appearance(window, cx),
        };
        #[cfg(all(target_os = "macos", not(test)))]
        {
            let panel = crate::system_settings::panel(window);
            view._track = Some(cx.spawn_in(window, async move |this, cx| {
                let mut prepared = false;
                loop {
                    // A failed update means the window is gone, so the
                    // panel is only touched after a successful one.
                    let Ok(step) = this.update_in(cx, |this, window, cx| this.track(window, cx))
                    else {
                        break;
                    };
                    if let Some(panel) = panel.as_ref().filter(|_| !prepared) {
                        panel.prepare();
                        prepared = true;
                    }
                    match (step, &panel) {
                        (Step::Close, _) => break,
                        (Step::Place { at, show }, Some(panel)) => panel.place(at, show),
                        _ => {}
                    }
                    let shown = this.read_with(cx, |this, _| this.visible).unwrap_or(false);
                    cx.background_executor()
                        .timer(if shown { TRACK } else { TRACK_HIDDEN })
                        .await;
                }
            }));
        }
        if view.app.read(cx).finder_on == Some(true) {
            view.finish(window, cx);
        }
        view
    }

    /// Shows "You're all set", then closes.
    fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.done = true;
        self._close = Some(cx.spawn_in(window, async move |_, cx| {
            cx.background_executor().timer(DONE_HOLD).await;
            let _ = cx.update(|window, _| window.remove_window());
        }));
        cx.notify();
    }

    /// Follows System Settings: beside its window while it's frontmost,
    /// hidden while another app is, closed once it quits. Says where the
    /// panel goes; the caller moves it outside this update.
    #[cfg(all(target_os = "macos", not(test)))]
    fn track(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Step {
        use crate::system_settings::{self as settings, Settings};
        let now = Instant::now();
        let state = settings::window();
        let t = &mut self.tracking;
        if state == Settings::NotRunning {
            let gone = match t.last_seen {
                Some(seen) => now - seen > GONE,
                None => now - t.opened > STARTUP,
            };
            if gone {
                window.remove_window();
                return Step::Close;
            }
        } else {
            t.last_seen = Some(now);
        }
        let frame = match state {
            Settings::Window(frame) => Some(frame),
            _ => None,
        };
        let visible = frame.is_some() && settings::frontmost();
        let at = frame
            .map(|f| crate::finder::dock(f, (SIZE.0 as f64, SIZE.1 as f64), &settings::screens()));
        let moved = at.is_some() && at != t.at;
        if moved {
            t.at = at;
        }
        let step = if moved || visible != self.visible {
            Step::Place {
                at: at.filter(|_| moved),
                show: visible,
            }
        } else {
            Step::Stay
        };
        if visible != self.visible {
            self.visible = visible;
            cx.notify();
        }
        step
    }

    fn close(&mut self, window: &mut Window, _: &mut Context<Self>) {
        window.remove_window();
    }

    fn header(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(theme::mark(22., p))
            .child(
                div()
                    .id("finder-guide-title")
                    .test_support()
                    .aria_label("Turn on convt in Finder")
                    .flex_1()
                    .child(
                        styled(size::BODY, p.text)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Turn on convt in Finder"),
                    ),
            )
            .child(
                theme::clickable("finder-guide-close", "Close")
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(24.))
                    .rounded(px(theme::radius::SM))
                    .hover(|d| d.bg(p.hover))
                    .child(icon(IconName::Close, 14., p.tertiary))
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
    }

    fn guide(&self, p: &Palette, cx: &App) -> AnyElement {
        let p = *p;
        let still = cx.reduce_motion() || !self.visible;
        let body = div()
            .id("finder-guide-steps")
            .test_support()
            .aria_label("Steps")
            .flex()
            .flex_col()
            .gap(px(14.));
        if still {
            return body
                .child(stage(beat::STILL, &p))
                .child(steps(None, &p))
                .into_any_element();
        }
        body.with_animation(
            "finder-guide-loop",
            Animation::new(Duration::from_secs_f32(LOOP)).repeat(),
            move |body, k| {
                let t = k * LOOP;
                body.child(stage(t, &p)).child(steps(Some(step_at(t)), &p))
            },
        )
        .into_any_element()
    }

    fn all_set(&self, p: &Palette) -> impl IntoElement {
        div()
            .id("finder-guide-done")
            .test_support()
            .aria_label("You're all set")
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .child(icon(IconName::CircleCheck, 40., p.green))
            .child(
                styled(size::TITLE, p.text)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("You're all set"),
            )
            .child(
                styled(size::SMALL, p.secondary)
                    .text_center()
                    .max_w(px(240.))
                    .child("Right-click any file in Finder and choose Convert with convt."),
            )
    }
}

impl Render for FinderGuideView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        div()
            .id("finder-guide")
            .size_full()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(16.))
            .bg(p.window)
            .font_family(theme::SANS)
            .text_color(p.text)
            .child(self.header(&p, cx))
            .map(|d| {
                if self.done {
                    d.child(self.all_set(&p))
                } else {
                    d.child(self.guide(&p, cx)).child(
                        styled(size::CAPTION, p.tertiary)
                            .child("This closes by itself once convt is on."),
                    )
                }
            })
    }
}

/// The three steps, with `active` in ink and the rest dimmed. `None` (a
/// still frame) shows all of them in ink.
fn steps(active: Option<usize>, p: &Palette) -> Div {
    let row = |n: usize, words: Div| {
        let lit = active.is_none_or(|a| a == n);
        let past = active.is_some_and(|a| n < a);
        let badge = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(18.))
            .rounded(px(9.))
            .map(|d| {
                if lit && active.is_some() {
                    d.bg(p.green).child(
                        text(10.5, 18., p.on_green)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child((n + 1).to_string()),
                    )
                } else if past {
                    d.bg(p.green_tint)
                        .child(icon(IconName::Check, 11., p.green_text))
                } else {
                    d.bg(p.chip).child(
                        text(10.5, 18., if lit { p.text } else { p.tertiary })
                            .font_weight(FontWeight::SEMIBOLD)
                            .child((n + 1).to_string()),
                    )
                }
            });
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(badge)
            .child(words.text_color(if lit { p.text } else { p.tertiary }))
    };
    let words = || {
        styled(size::SMALL, p.text)
            .flex()
            .items_center()
            .gap(px(4.))
    };
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(row(
            0,
            words().child("Scroll down to Extensions, at the bottom"),
        ))
        .child(row(
            1,
            words()
                .child("Click")
                .child(icon(
                    IconName::Info,
                    13.,
                    if active.is_none_or(|a| a == 1) {
                        p.text
                    } else {
                        p.tertiary
                    },
                ))
                .child("next to convt"),
        ))
        .child(row(2, words().child("Turn on the switch, then click Done")))
}

/// macOS's accent blue, for the drawn switch and Done button: the drawing
/// is of System Settings, so its controls look like System Settings'.
fn system_blue(p: &Palette) -> Hsla {
    if p.dark {
        rgb(0x0A84FF).into()
    } else {
        rgb(0x007AFF).into()
    }
}

/// The drawing at `t` seconds into the loop.
fn stage(t: f32, p: &Palette) -> Div {
    let (w, h) = STAGE;
    let shown = through(t, beat::FADE_IN) * (1. - through(t, beat::FADE_OUT));

    // The list scrolls until Extensions is in view.
    let scroll = glide(through(t, beat::SCROLL)) * list::SCROLL;
    let ring = through(t, beat::RING) * (1. - through(t, beat::SHEET_IN));
    let sheet = settle(through(t, beat::SHEET_IN)) * (1. - through(t, beat::SHEET_OUT));
    let switch = settle(through(t, beat::SWITCH_ON));

    let pointer = if t < beat::TO_INFO.0 {
        POINTER_START
    } else if t < beat::TO_SWITCH.0 {
        mix(POINTER_START, INFO, glide(through(t, beat::TO_INFO)))
    } else if t < beat::TO_DONE.0 {
        mix(INFO, SWITCH, glide(through(t, beat::TO_SWITCH)))
    } else {
        mix(SWITCH, DONE_BUTTON, glide(through(t, beat::TO_DONE)))
    };
    let pressed =
        during(t, beat::PRESS_INFO) || during(t, beat::PRESS_SWITCH) || during(t, beat::PRESS_DONE);

    div()
        .relative()
        .flex_shrink_0()
        .w(px(w))
        .h(px(h))
        .rounded(px(10.))
        .overflow_hidden()
        .bg(p.recessed)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .absolute()
                .inset_0()
                .opacity(shown)
                .child(list::content(scroll, ring, during(t, beat::PRESS_INFO), p))
                .child(list::scrollbar(scroll, p))
                .child(toolbar(p))
                .when(sheet > 0., |d| d.child(sheet_layer(sheet, switch, t, p)))
                .child(pointer_at(pointer, pressed, p)),
        )
}

/// The window's toolbar strip: three dots and the pane's name.
fn toolbar(p: &Palette) -> Div {
    let dot = || div().size(px(7.)).rounded(px(3.5)).bg(p.toggle_off);
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(list::TOP))
        .flex()
        .items_center()
        .gap(px(5.))
        .px(px(10.))
        .bg(p.recessed)
        .border_b_1()
        .border_color(p.border)
        .child(dot())
        .child(dot())
        .child(dot())
        .child(
            text(10., 12., p.secondary)
                .ml(px(8.))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Login Items & Extensions"),
        )
}

/// The scrolling list of the drawing.
mod list {
    use super::*;

    /// The toolbar's height, where the list starts.
    pub const TOP: f32 = 24.;
    const SIDE: f32 = 14.;
    const ROW: f32 = 22.;
    /// How far the list scrolls to bring Extensions into view.
    pub const SCROLL: f32 = 198.;
    const LENGTH: f32 = 346.;

    /// A section label at `y`.
    fn label(y: f32, words: &'static str, strong: bool, p: &Palette) -> Div {
        text(
            if strong { 11. } else { 9.5 },
            12.,
            if strong { p.text } else { p.tertiary },
        )
        .absolute()
        .left(px(SIDE + 2.))
        .top(px(y))
        .font_weight(if strong {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::MEDIUM
        })
        .child(words)
    }

    /// A rounded group of rows at `y`.
    fn group(y: f32, rows: Vec<AnyElement>, p: &Palette) -> Div {
        div()
            .absolute()
            .left(px(SIDE))
            .right(px(SIDE))
            .top(px(y))
            .flex()
            .flex_col()
            .rounded(px(7.))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .children(rows)
    }

    /// A row standing for another app: a tinted square and a gray bar.
    fn row(tint: u32, bar: f32, switch: bool, p: &Palette) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(7.))
            .h(px(ROW))
            .px(px(8.))
            .child(
                div()
                    .size(px(11.))
                    .rounded(px(3.))
                    .bg(Hsla::from(rgb(tint)).opacity(if p.dark { 0.55 } else { 0.7 })),
            )
            .child(
                div()
                    .w(px(bar))
                    .h(px(5.))
                    .rounded(px(2.5))
                    .bg(p.text.opacity(0.13)),
            )
            .child(div().flex_1())
            .when(switch, |d| {
                d.child(
                    div()
                        .flex()
                        .justify_end()
                        .items_center()
                        .w(px(18.))
                        .h(px(10.))
                        .p(px(1.5))
                        .rounded(px(5.))
                        .bg(p.text.opacity(0.22))
                        .child(div().size(px(7.)).rounded(px(3.5)).bg(p.window)),
                )
            })
            .into_any_element()
    }

    /// convt's row under Extensions, with its ⓘ button. `ring` (0 to 1)
    /// rings it in green; `pressed` darkens the button.
    fn convt_row(ring: f32, pressed: bool, p: &Palette) -> AnyElement {
        // The ring goes first so the row draws over its tint.
        div()
            .relative()
            .flex()
            .items_center()
            .gap(px(7.))
            .h(px(ROW + 2.))
            .px(px(8.))
            .when(ring > 0., |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(6.))
                        .bg(p.green_tint.opacity(ring))
                        .shadow(vec![theme::inset_ring(p.green.opacity(ring), 1.5)]),
                )
            })
            .child(theme::mark(11., p))
            .child(
                text(10., 12., p.text)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("convt"),
            )
            .child(div().flex_1())
            .child(icon(
                IconName::Info,
                14.,
                if pressed { p.text } else { p.secondary },
            ))
            .into_any_element()
    }

    pub fn content(scroll: f32, ring: f32, pressed: bool, p: &Palette) -> Div {
        let top = TOP - scroll;
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(top))
            .h(px(LENGTH))
            .child(label(10., "Open at Login", false, p))
            .child(group(
                24.,
                vec![
                    row(0x5E8BF0, 54., false, p),
                    row(0xE0A33B, 70., false, p),
                    row(0x9B7BE0, 46., false, p),
                ],
                p,
            ))
            .child(label(102., "Allow in the Background", false, p))
            .child(group(
                116.,
                vec![
                    row(0x5BB98C, 62., true, p),
                    row(0xE0705B, 44., true, p),
                    row(0x6CA8D9, 76., true, p),
                    row(0xC08A5B, 52., true, p),
                    row(0x8A8F8C, 66., true, p),
                ],
                p,
            ))
            .child(label(238., "Extensions", true, p))
            .child(group(
                256.,
                vec![
                    row(0xE0A33B, 48., false, p),
                    convt_row(ring, pressed, p),
                    row(0xE6C04A, 40., false, p),
                ],
                p,
            ))
    }

    pub fn scrollbar(scroll: f32, p: &Palette) -> Div {
        let track = STAGE.1 - TOP - 8.;
        let view = STAGE.1 - TOP;
        let thumb = track * view / LENGTH;
        let y = TOP + 4. + (track - thumb) * scroll / SCROLL;
        div()
            .absolute()
            .right(px(3.))
            .top(px(y))
            .w(px(4.))
            .h(px(thumb))
            .rounded(px(2.))
            .bg(p.text.opacity(0.25))
    }
}

/// The sheet ⓘ opens: "convt Extensions" with its switch. `shown` (0 to 1)
/// fades it in; `on` (0 to 1) slides the switch on.
fn sheet_layer(shown: f32, on: f32, t: f32, p: &Palette) -> Div {
    let blue = system_blue(p);
    let knob = 9.;
    let switch = div()
        .relative()
        .w(px(24.))
        .h(px(13.))
        .rounded(px(6.5))
        .bg(p.toggle_off)
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(6.5))
                .bg(blue.opacity(on)),
        )
        .child(
            div()
                .absolute()
                .top(px(2.))
                .left(px(2. + (24. - knob - 4.) * on))
                .size(px(knob))
                .rounded(px(knob / 2.))
                .bg(gpui_kit::white())
                .shadow(vec![theme::shadow(black().opacity(0.25), 0.5, 1.)]),
        );
    let done_pressed = during(t, beat::PRESS_DONE);
    div()
        .absolute()
        .inset_0()
        .child(
            div()
                .absolute()
                .inset_0()
                .bg(black().opacity(if p.dark { 0.45 } else { 0.14 } * shown)),
        )
        .child(
            div()
                .absolute()
                .left(px(46.))
                .top(px(38. + 6. * (1. - shown)))
                .w(px(196.))
                .opacity(shown)
                .flex()
                .flex_col()
                .gap(px(8.))
                .p(px(10.))
                .rounded(px(9.))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .shadow(vec![theme::shadow(black().opacity(0.18), 4., 14.)])
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(theme::mark(13., p))
                        .child(
                            text(10., 12., p.text)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("convt Extensions"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .py(px(6.))
                        .px(px(8.))
                        .rounded(px(6.))
                        .bg(p.recessed)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .flex_1()
                                .child(
                                    div()
                                        .w(px(64.))
                                        .h(px(5.))
                                        .rounded(px(2.5))
                                        .bg(p.text.opacity(0.3)),
                                )
                                .child(
                                    div()
                                        .w(px(104.))
                                        .h(px(4.))
                                        .rounded(px(2.))
                                        .bg(p.text.opacity(0.13)),
                                ),
                        )
                        .child(switch),
                )
                .child(
                    div().flex().justify_end().child(
                        div()
                            .px(px(9.))
                            .h(px(15.))
                            .flex()
                            .items_center()
                            .rounded(px(7.5))
                            .bg(if done_pressed {
                                blue.opacity(0.8)
                            } else {
                                blue
                            })
                            .child(
                                text(9., 11., gpui_kit::white())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Done"),
                            ),
                    ),
                ),
        )
}

/// The drawn pointer with its tip at `at`, a little smaller while pressed.
fn pointer_at(at: (f32, f32), pressed: bool, p: &Palette) -> Div {
    let scale = if pressed { 0.86 } else { 1. };
    let layer = |path: &'static str, color: Hsla| {
        svg()
            .absolute()
            .top_0()
            .left_0()
            .w(px(16.))
            .h(px(22.))
            .path(path)
            .text_color(color)
            .with_transformation(Transformation::scale(size(scale, scale)))
    };
    let (edge, fill) = if p.dark {
        (black(), gpui_kit::white())
    } else {
        (gpui_kit::white(), black())
    };
    // The tip sits 2 points in from the image's corner.
    div()
        .absolute()
        .left(px(at.0 - 2.))
        .top(px(at.1 - 2.))
        .w(px(16.))
        .h(px(22.))
        .child(layer("guide/pointer-edge.svg", edge))
        .child(layer("guide/pointer.svg", fill))
}

#[cfg(test)]
mod tests {
    use super::{LOOP, beat, step_at, through};

    #[test]
    fn the_loop_shows_each_step_in_order() {
        assert_eq!(step_at(0.), 0);
        assert_eq!(step_at(beat::SCROLL.1 - 0.01), 0);
        assert_eq!(step_at(beat::TO_INFO.0), 1);
        assert_eq!(step_at(beat::SWITCH_ON.0), 2);
        assert_eq!(step_at(LOOP - 0.01), 2);
        // Each moment starts after the one before it ends, inside the loop.
        let order = [
            beat::FADE_IN,
            beat::SCROLL,
            beat::RING,
            beat::TO_INFO,
            beat::PRESS_INFO,
            beat::SHEET_IN,
            beat::TO_SWITCH,
            beat::PRESS_SWITCH,
            beat::TO_DONE,
            beat::PRESS_DONE,
            beat::SHEET_OUT,
            beat::FADE_OUT,
        ];
        for pair in order.windows(2) {
            assert!(pair[0].1 <= pair[1].0, "{pair:?}");
        }
        assert!(beat::FADE_OUT.1 <= LOOP);
    }

    #[test]
    fn the_still_frame_has_the_switch_on_and_the_sheet_open() {
        let t = beat::STILL;
        assert_eq!(through(t, beat::SWITCH_ON), 1.);
        assert_eq!(through(t, beat::SHEET_IN), 1.);
        assert_eq!(through(t, beat::SHEET_OUT), 0.);
        assert_eq!(through(t, beat::SCROLL), 1.);
    }
}
