//! The Feedback window: a message, an optional email, and Send. The app
//! version and platform go along without asking; the note under the fields
//! says so. See `crate::feedback` for what is sent where.

use gpui_kit::component::input::{InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{self, Button, secondary_button, text};
use crate::feedback::{self, SendError};
use crate::model::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Editing,
    Sending,
    Sent,
    Failed(String),
}

pub struct FeedbackView {
    app: Entity<AppState>,
    pub(super) message: Entity<TextareaState>,
    pub(super) email: Entity<InputState>,
    pub(super) status: Status,
    _send: Option<Task<()>>,
    _appearance: Subscription,
}

impl FeedbackView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let message = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(6)
                .placeholder("A bug, a format you need, anything.")
        });
        let email = cx.new(|cx| InputState::new(window, cx).placeholder("you@example.com"));
        message.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            _appearance: theme::observe_appearance(window, cx),
            app,
            message,
            email,
            status: Status::Editing,
            _send: None,
        }
    }

    pub(super) fn send(&mut self, cx: &mut Context<Self>) {
        if self.status == Status::Sending {
            return;
        }
        let message = self.message.read(cx).value().to_string();
        let email = self.email.read(cx).value().to_string();
        if let Err(e) = feedback::check(&message, &email) {
            self.status = Status::Failed(e.message());
            cx.notify();
            return;
        }
        self.status = Status::Sending;
        cx.notify();
        let sender = self.app.read(cx).feedback.clone();
        let body = feedback::body(&message, &email);
        let (tx, rx) = futures::channel::oneshot::channel::<Result<(), SendError>>();
        // Network calls never run on the UI thread.
        std::thread::Builder::new()
            .name("convt-feedback".into())
            .spawn(move || drop(tx.send(sender.send(&body))))
            .expect("spawn the feedback thread");
        self._send = Some(cx.spawn(async move |this, cx| {
            let result = rx.await.unwrap_or(Err(SendError::Offline));
            let _ = this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(()) => Status::Sent,
                    Err(e) => Status::Failed(e.message()),
                };
                cx.notify();
            });
        }));
    }

    fn field_label(label: &'static str, hint: Option<&'static str>, p: &theme::Palette) -> Div {
        div()
            .flex()
            .gap(px(4.))
            .child(
                text(12., 16., p.text)
                    .font_weight(FontWeight::MEDIUM)
                    .child(label),
            )
            .children(hint.map(|h| text(12., 16., p.secondary).child(h)))
    }

    fn form(&self, p: &theme::Palette) -> Div {
        let sending = self.status == Status::Sending;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .gap(px(14.))
            .px(px(24.))
            .pt(px(20.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        text(17., 22., p.text)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Send feedback"),
                    )
                    .child(
                        text(13., 18., p.secondary)
                            .child("Tell us what's broken, missing or great. It goes straight to the convt team."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(Self::field_label("Message", None, p))
                    .child(
                        Textarea::new(&self.message)
                            .accessibility_id("feedback-message")
                            .aria_label("Message")
                            .h(px(128.))
                            .disabled(sending),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(Self::field_label("Email", Some("(optional, if you'd like a reply)"), p))
                    .child(theme::field(&self.email, "feedback-email").disabled(sending)),
            )
            .child(
                div()
                    .id("feedback-note")
                    .test_support()
                    .child(text(11., 15., p.tertiary).child(format!(
                        "Sent to convt.app with convt {} on {}. Nothing else about this computer or your files.",
                        crate::account::VERSION,
                        feedback::platform()
                    ))),
            )
            .when_some(
                match &self.status {
                    Status::Failed(e) => Some(e.clone()),
                    _ => None,
                },
                |d, e| d.child(super::error_text(e, p)),
            )
    }

    fn sent(&self, p: &theme::Palette, cx: &App) -> Div {
        let email = !self.email.read(cx).value().trim().is_empty();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .gap(px(6.))
            .px(px(24.))
            .pt(px(28.))
            .child(
                div()
                    .id("feedback-sent")
                    .test_support()
                    .aria_label("Thanks for the feedback")
                    .child(
                        text(17., 22., p.text)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Thanks for the feedback"),
                    ),
            )
            .child(text(13., 18., p.secondary).child(if email {
                "We read every message, and we'll reply by email if we have a question."
            } else {
                "We read every message."
            }))
    }
}

impl Render for FeedbackView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let sent = self.status == Status::Sent;
        let sending = self.status == Status::Sending;
        let body = if sent {
            self.sent(&p, cx)
        } else {
            self.form(&p)
        };
        let footer = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_end()
            .gap(px(8.))
            .px(px(24.))
            .py(px(14.))
            .bg(p.recessed)
            .border_t_1()
            .border_color(p.hairline)
            .when(sent, |d| {
                d.child(
                    Button::primary("feedback-done", "Done")
                        .build(&p)
                        .on_click(|_, window, _| window.remove_window()),
                )
            })
            .when(!sent, |d| {
                d.child(
                    secondary_button("feedback-cancel", "Cancel", &p)
                        .on_click(|_, window, _| window.remove_window()),
                )
                .child(
                    Button::primary(
                        "feedback-send",
                        if sending {
                            "Sending…"
                        } else {
                            "Send feedback"
                        },
                    )
                    .disabled(sending)
                    .build(&p)
                    .on_click(cx.listener(|this, _, _, cx| this.send(cx))),
                )
            });
        div()
            .id("feedback")
            .flex()
            .flex_col()
            .size_full()
            .bg(p.window)
            .font_family(theme::SANS)
            .text_color(p.text)
            .when(theme::transparent_titlebar(), |d| d.pt(px(28.)))
            .child(body)
            .child(footer)
    }
}
