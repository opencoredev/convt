//! The progress HUD: a small floating card near the top right of the screen
//! for conversions started without a window (a right-click with a target, or
//! the command line). It never takes focus, follows its jobs, and closes
//! itself a few seconds after they finish, or a little later if one failed.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{
    self, Button, IconName, Palette, Tone, icon, mono, radius, size, space, styled,
};
use super::{file_size, human_size};
use crate::jobs::{Entry, JobId, Status};
use crate::model::{self, AppState};

/// The HUD's size, in pixels.
pub const SIZE: (f32, f32) = (348., 84.);
/// Its distance from the screen's top and right edges.
pub const MARGIN: f32 = 16.;
/// How long it stays once every job is done.
const DONE_FOR: Duration = Duration::from_secs(4);
/// How long it stays when a job failed, so the reason can be read.
const FAILED_FOR: Duration = Duration::from_secs(7);

/// Where the jobs are, as the HUD shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Running,
    Done,
    Failed,
}

pub struct HudView {
    app: Entity<AppState>,
    pub(super) jobs: Vec<JobId>,
    /// The last state seen of each job, so the result stays after the queue
    /// clears finished jobs.
    seen: HashMap<JobId, Entry>,
    /// Copy was clicked; the button says Copied.
    pub(super) copied: bool,
    /// The jobs the close timer was set for; new jobs reset it.
    closing: Option<usize>,
    _close: Option<Task<()>>,
    _observe: Subscription,
    _appearance: Subscription,
}

impl HudView {
    pub fn new(
        app: Entity<AppState>,
        jobs: Vec<JobId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            _observe: cx.observe_in(&app, window, |this: &mut Self, _, window, cx| {
                this.remember(cx);
                this.close_when_done(window, cx);
                cx.notify();
            }),
            _appearance: theme::observe_appearance(window, cx),
            app,
            jobs,
            seen: HashMap::new(),
            copied: false,
            closing: None,
            _close: None,
        };
        view.remember(cx);
        view.close_when_done(window, cx);
        view
    }

    /// Follows more jobs, from another right-click while this one shows.
    pub fn add(&mut self, jobs: Vec<JobId>, window: &mut Window, cx: &mut Context<Self>) {
        self.jobs.extend(jobs);
        self.copied = false;
        self.remember(cx);
        self.close_when_done(window, cx);
        cx.notify();
    }

    fn remember(&mut self, cx: &App) {
        let state = self.app.read(cx);
        for id in &self.jobs {
            if let Some(entry) = state.entry(*id) {
                self.seen.insert(*id, entry.clone());
            }
        }
    }

    fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.jobs.iter().filter_map(|id| self.seen.get(id))
    }

    pub(super) fn phase(&self) -> Phase {
        let mut entries = self.entries().peekable();
        if entries.peek().is_none() {
            return Phase::Running;
        }
        let mut failed = false;
        for entry in entries {
            match &entry.status {
                Status::Done(_) | Status::Cancelled => {}
                Status::Failed(_) => failed = true,
                _ => return Phase::Running,
            }
        }
        if failed { Phase::Failed } else { Phase::Done }
    }

    /// The finished files, in job order.
    fn outputs(&self) -> Vec<PathBuf> {
        self.entries()
            .filter_map(|e| match &e.status {
                Status::Done(out) => Some(out.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// Once every job is finished, closes after a few seconds.
    fn close_when_done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let phase = self.phase();
        if phase == Phase::Running {
            self.closing = None;
            self._close = None;
            return;
        }
        if self.closing == Some(self.jobs.len()) {
            return;
        }
        self.closing = Some(self.jobs.len());
        let wait = if phase == Phase::Failed {
            FAILED_FOR
        } else {
            DONE_FOR
        };
        self._close = Some(cx.spawn_in(window, async move |_, cx| {
            cx.background_executor().timer(wait).await;
            let _ = cx.update(|window, _| window.remove_window());
        }));
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let outputs = self.outputs();
        if outputs.is_empty() {
            return;
        }
        crate::clipboard::copy_files(&outputs, cx);
        self.copied = true;
        cx.notify();
    }

    /// The title and the line under it, for the current phase.
    pub(super) fn words(&self) -> (String, String) {
        let entries: Vec<&Entry> = self.entries().collect();
        let to = entries.first().map_or("", |e| e.to.name);
        match self.phase() {
            Phase::Running => {
                let title = match entries.as_slice() {
                    [one] => format!("Converting {} → {to}", model::file_name(&one.input)),
                    many => format!("Converting {} files → {to}", many.len().max(1)),
                };
                let done = entries.iter().filter(|e| e.status.is_finished()).count();
                let line = if entries.len() > 1 {
                    format!("{done} of {} done", entries.len())
                } else {
                    match entries.first().map(|e| &e.status) {
                        Some(Status::Running(Some(f))) => format!("{:.0}%", f * 100.),
                        Some(Status::Running(None)) => "Converting".into(),
                        _ => "Waiting".into(),
                    }
                };
                (title, line)
            }
            Phase::Done => {
                let outputs = self.outputs();
                let title = match outputs.as_slice() {
                    [one] => format!("Saved {}", model::file_name(one)),
                    [] => "Cancelled".to_string(),
                    many => format!("Saved {} files", many.len()),
                };
                let line = match (entries.as_slice(), outputs.as_slice()) {
                    ([entry], [one]) => match (file_size(&entry.input), file_size(one)) {
                        (Some(a), Some(b)) => {
                            let (delta, _) = super::main_window::size_delta(a, b);
                            format!("{} → {} · {delta}", human_size(a), human_size(b))
                        }
                        _ => format!("Saved beside the original as {to}"),
                    },
                    _ => format!("Saved beside the originals as {to}"),
                };
                (title, line)
            }
            Phase::Failed => {
                let failed: Vec<&Entry> = entries
                    .iter()
                    .copied()
                    .filter(|e| matches!(e.status, Status::Failed(_)))
                    .collect();
                let title = match failed.as_slice() {
                    [one] => format!("Couldn't convert {}", model::file_name(&one.input)),
                    many => format!("Couldn't convert {} files", many.len()),
                };
                let line = failed
                    .first()
                    .and_then(|e| match &e.status {
                        Status::Failed(e) => Some(e.message.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                (title, line)
            }
        }
    }

    /// The overall progress of the running jobs, 0 to 1.
    fn fraction(&self) -> f32 {
        let entries: Vec<&Entry> = self.entries().collect();
        if entries.is_empty() {
            return 0.;
        }
        let sum: f32 = entries
            .iter()
            .map(|e| match e.status {
                Status::Running(f) => f.unwrap_or(0.),
                Status::Queued => 0.,
                _ => 1.,
            })
            .sum();
        sum / entries.len() as f32
    }

    fn lead(&self, phase: Phase, p: &Palette) -> AnyElement {
        match phase {
            Phase::Done => {
                theme::icon_tile(IconName::CircleCheck, Tone::Green, 40., p).into_any_element()
            }
            Phase::Failed => {
                theme::icon_tile(IconName::CircleX, Tone::Error, 40., p).into_any_element()
            }
            Phase::Running => match self.entries().next() {
                Some(entry) => theme::thumbnail(&entry.input, 40., 40., p),
                None => {
                    theme::icon_tile(IconName::Loader, Tone::Neutral, 40., p).into_any_element()
                }
            },
        }
    }
}

/// Whether the HUD draws its own rounded corners: where the window can be
/// transparent around them.
pub fn transparent() -> bool {
    cfg!(target_os = "macos")
}

impl Render for HudView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let phase = self.phase();
        let (title, line) = self.words();
        let state = match phase {
            Phase::Running => "running",
            Phase::Done => "done",
            Phase::Failed => "failed",
        };
        let first = self.outputs().first().cloned();
        let hover = p.hover;
        let close = theme::clickable("hud-close", "Dismiss")
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(24.))
            .rounded(px(radius::SM))
            .hover(move |s| s.bg(hover))
            .on_click(|_, window, _| window.remove_window())
            .child(icon(IconName::Close, 12., p.tertiary));
        let actions = (phase == Phase::Done && first.is_some()).then(|| {
            let path = first.clone().unwrap_or_default();
            div()
                .flex()
                .items_center()
                .gap(px(2.))
                .child(
                    Button::ghost("hud-copy", if self.copied { "Copied" } else { "Copy" })
                        .small()
                        .build(&p)
                        .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                )
                .child(
                    Button::secondary("hud-show", "Show")
                        .small()
                        .build(&p)
                        .on_click(move |_, _, cx| cx.reveal_path(&path)),
                )
        });
        let line_color = match phase {
            Phase::Failed => p.error,
            _ => p.secondary,
        };
        let below = if phase == Phase::Running {
            div()
                .flex()
                .items_center()
                .gap(px(space::SM))
                .child(
                    div()
                        .flex_1()
                        .child(theme::progress(self.fraction(), p.track, p.green)),
                )
                .child(mono(11., 14., p.secondary).flex_shrink_0().child(line))
                .into_any_element()
        } else {
            let words = if phase == Phase::Done {
                mono(11., 14., line_color)
            } else {
                styled(size::SMALL, line_color)
            };
            words.truncate().child(line).into_any_element()
        };
        div()
            .id("hud")
            .test_support()
            .aria_label(SharedString::from(state))
            .flex()
            .items_center()
            .gap(px(space::MD))
            .size_full()
            .px(px(14.))
            .bg(p.overlay)
            .font_family(theme::SANS)
            .text_color(p.text)
            .when(transparent(), |d| {
                d.rounded(px(radius::PANEL))
                    .border_1()
                    .border_color(p.border)
            })
            .child(self.lead(phase, &p))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.))
                    .child(
                        div()
                            .id("hud-title")
                            .test_support()
                            .aria_label(SharedString::from(title.clone()))
                            .child(
                                styled(size::BODY, p.text)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(title),
                            ),
                    )
                    .child(
                        div()
                            .id("hud-line")
                            .test_support()
                            .aria_label(SharedString::from(self.words().1))
                            .child(below),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .items_end()
                    .justify_between()
                    .h_full()
                    .py(px(10.))
                    .child(close)
                    .children(actions),
            )
    }
}
