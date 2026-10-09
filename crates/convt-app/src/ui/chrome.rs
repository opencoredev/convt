//! The title bar a Linux window draws itself when the compositor leaves the
//! decorations to the app, as GNOME does on Wayland. GPUI asks for
//! server-side decorations first; where the compositor has them, and on
//! macOS and Windows, the system draws the title bar and this adds nothing.
//!
//! gpui-component's root already draws the frame, the shadow and the resize
//! edges of a client-decorated window, so this only adds the bar: the title,
//! dragging to move, double-clicking to maximize, the window menu on a right
//! click, and minimize, maximize and close.

use super::theme::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{self, Palette};

/// The height of the drawn title bar.
pub const TITLE_BAR_HEIGHT: f32 = 38.;

/// Whether a window with these decorations draws its own title bar.
pub fn draws_title_bar(decorations: Decorations) -> bool {
    cfg!(target_os = "linux") && matches!(decorations, Decorations::Client { .. })
}

/// Wraps the view of every window, between gpui-component's root and the
/// view, so the bar sits inside the frame the root draws.
pub struct Chrome {
    view: AnyView,
    /// GPUI reads a window's title back only on macOS, so this keeps it.
    title: Option<SharedString>,
    /// A press on the bar that hasn't moved yet. The first move after it,
    /// anywhere in the window, hands the drag to the compositor; a press that
    /// doesn't move stays a click, so a double click still reaches the bar.
    pressed: bool,
}

/// Set in tests to draw the bar: the test platform always reports
/// server-side decorations. It can't move windows either, so the bar counts
/// the moves it would start here.
#[cfg(test)]
#[derive(Default)]
pub struct ForceTitleBar {
    pub moves: usize,
}

#[cfg(test)]
impl Global for ForceTitleBar {}

impl Chrome {
    pub fn new(view: impl Into<AnyView>, title: Option<SharedString>) -> Self {
        Self {
            view: view.into(),
            title,
            pressed: false,
        }
    }

    #[cfg_attr(not(test), allow(unused_variables))]
    fn draws_title_bar(window: &Window, cx: &App) -> bool {
        #[cfg(test)]
        if cx.has_global::<ForceTitleBar>() {
            return true;
        }
        draws_title_bar(window.window_decorations())
    }

    fn title_bar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let controls = window.window_controls();
        div()
            .id("title-bar")
            .test_support()
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_end()
            .h(px(TITLE_BAR_HEIGHT))
            .px(px(8.))
            .gap(px(8.))
            .bg(p.chrome)
            .border_b_1()
            .border_color(p.chrome_border)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.pressed = true;
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.pressed = false;
                    cx.notify();
                }),
            )
            .when(self.pressed, |d| d.child(self.drag_watch(cx)))
            .on_click(|e, window, _| {
                if e.standard_click() && e.click_count() == 2 {
                    window.zoom_window();
                }
            })
            .when(controls.window_menu, |d| {
                d.on_mouse_down(MouseButton::Right, |e, window, _| {
                    window.show_window_menu(e.position)
                })
            })
            .child(
                // The title is centered on the whole bar, not on the space
                // the buttons leave.
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .children(self.title.clone().map(|title| {
                        theme::text(13., 16., p.text)
                            .id("window-title")
                            .test_support()
                            .aria_label(title.clone())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title)
                    })),
            )
            .when(controls.minimize, |d| {
                d.child(
                    control("window-minimize", "Minimize", IconName::Minus, &p).on_click(
                        |_, window, cx| {
                            cx.stop_propagation();
                            window.minimize_window();
                        },
                    ),
                )
            })
            .when(controls.maximize, |d| {
                let (label, icon) = if window.is_maximized() {
                    ("Restore", IconName::WindowRestore)
                } else {
                    ("Maximize", IconName::WindowMaximize)
                };
                d.child(
                    control("window-maximize", label, icon, &p).on_click(|_, window, cx| {
                        cx.stop_propagation();
                        window.zoom_window();
                    }),
                )
            })
            .child(
                control("window-close", "Close", IconName::Close, &p).on_click(|_, window, cx| {
                    cx.stop_propagation();
                    window.remove_window();
                }),
            )
    }
}

impl Chrome {
    /// Watches the first move after a press anywhere in the window, so a fast
    /// drag whose first motion already left the bar still moves the window.
    fn drag_watch(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let chrome = cx.entity().downgrade();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let Ok(armed) = chrome.update(cx, |this, cx| {
                        cx.notify();
                        std::mem::take(&mut this.pressed)
                    }) else {
                        return;
                    };
                    // A release the bar never saw (the press moved focus
                    // away) must not leave a move armed for the next hover.
                    if armed && e.pressed_button == Some(MouseButton::Left) {
                        start_window_move(window, cx);
                    }
                });
            },
        )
        .absolute()
        .size_0()
    }
}

#[cfg_attr(not(test), allow(unused_variables))]
fn start_window_move(window: &Window, cx: &mut App) {
    #[cfg(test)]
    if cx.has_global::<ForceTitleBar>() {
        cx.global_mut::<ForceTitleBar>().moves += 1;
        return;
    }
    window.start_window_move();
}

impl Render for Chrome {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !Self::draws_title_bar(window, cx) {
            return self.view.clone().into_any_element();
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.title_bar(window, cx))
            .child(div().flex_1().min_h_0().w_full().child(self.view.clone()))
            .into_any_element()
    }
}

/// A round window button, like GNOME's. A press on it never starts a drag.
fn control(id: &'static str, label: &'static str, icon: IconName, p: &Palette) -> theme::Clickable {
    theme::clickable(id, label)
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(24.))
        .rounded_full()
        .bg(p.text.opacity(0.07))
        .hover(|s| s.bg(p.text.opacity(0.13)))
        .active(|s| s.bg(p.text.opacity(0.2)))
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .child(Icon::new(icon).size(px(12.)).text_color(p.text))
}
