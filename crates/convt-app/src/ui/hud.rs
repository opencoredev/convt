//! The progress HUD: a small floating card near the top right of the screen
//! for conversions started without a window (a right-click with a target, or
//! the command line). It never takes focus, follows its jobs, and closes
//! itself a few seconds after they finish, or a little later if one failed.
//!
//! Two rows: what happened (a picture of the file, the title, ×), then the
//! detail (sizes, progress or the reason) with Copy and Show beside it for
//! saved files, and Details, which opens the failures in Activity, when any
//! failed.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{self, Button, IconName, Palette, Tone, icon, mono, radius, size, styled};
use super::{file_size, friendly_error, human_size};
use crate::jobs::{Entry, JobId, Status};
use crate::model::{self, AppState};

/// The HUD's size, in pixels.
pub const SIZE: (f32, f32) = (408., 86.);
/// Its distance from the screen's top and right edges.
pub const MARGIN: f32 = 16.;
/// How long it stays once every job is done.
const DONE_FOR: Duration = Duration::from_secs(4);
/// How long it stays when some jobs failed and others were saved.
const MIXED_FOR: Duration = Duration::from_secs(6);
/// How long it stays when every job failed, so the reason can be read.
const FAILED_FOR: Duration = Duration::from_secs(7);
/// How long it stays after the pointer leaves a finished HUD.
const HOVER_LEAVE_FOR: Duration = Duration::from_millis(2500);
/// The picture at the start of the title row.
const LEAD: f32 = 32.;
/// Space between the picture and the words.
const LEAD_GAP: f32 = 12.;

/// Where the jobs are, as the HUD shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Running,
    Done,
    /// Some files were saved and some failed.
    Mixed,
    Failed,
}

impl Phase {
    /// What tests read from the HUD.
    fn name(self) -> &'static str {
        match self {
            Phase::Running => "running",
            Phase::Done => "done",
            Phase::Mixed => "mixed",
            Phase::Failed => "failed",
        }
    }
}

pub struct HudView {
    app: Entity<AppState>,
    pub(super) jobs: Vec<JobId>,
    /// The last state seen of each job, so the result stays after the queue
    /// clears finished jobs.
    seen: HashMap<JobId, Entry>,
    /// Copy was clicked; the button says Copied.
    pub(super) copied: bool,
    /// The pointer is over the HUD; pause auto-dismiss until it leaves.
    pub(super) hovered: bool,
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
            hovered: false,
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
        let (mut saved, mut failed) = (false, false);
        for entry in entries {
            match &entry.status {
                Status::Done(_) => saved = true,
                Status::Cancelled => {}
                Status::Failed(_) => failed = true,
                _ => return Phase::Running,
            }
        }
        match (saved, failed) {
            (true, true) => Phase::Mixed,
            (false, true) => Phase::Failed,
            _ => Phase::Done,
        }
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

    /// Once every job is finished, closes after a few seconds unless the
    /// pointer is over the HUD. Closing is the last window closing, so with
    /// nothing else open convt quits then
    /// (`model::AppState::maybe_quit_after_hud`).
    fn close_when_done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let phase = self.phase();
        if phase == Phase::Running || self.hovered {
            self.closing = None;
            self._close = None;
            return;
        }
        if self.closing == Some(self.jobs.len()) {
            return;
        }
        let wait = match phase {
            Phase::Failed => FAILED_FOR,
            Phase::Mixed => MIXED_FOR,
            _ => DONE_FOR,
        };
        self.schedule_close(wait, window, cx);
    }

    fn schedule_close(&mut self, wait: Duration, window: &mut Window, cx: &mut Context<Self>) {
        self.closing = Some(self.jobs.len());
        self._close = Some(cx.spawn_in(window, async move |_, cx| {
            cx.background_executor().timer(wait).await;
            let _ = cx.update(|window, _| window.remove_window());
        }));
    }

    /// Pauses auto-dismiss while the pointer is over the HUD, and gives a
    /// short grace period after it leaves.
    pub(super) fn set_hovered(
        &mut self,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.hovered == hovered {
            return;
        }
        self.hovered = hovered;
        if hovered {
            self.closing = None;
            self._close = None;
        } else if self.phase() != Phase::Running {
            self.schedule_close(HOVER_LEAVE_FOR, window, cx);
        }
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

    /// The target every job shares, or `None` for a mix.
    fn shared_target(&self) -> Option<&'static str> {
        let mut entries = self.entries();
        let first = entries.next()?.to;
        entries.all(|e| e.to == first).then_some(first.name)
    }

    /// The title, the line under it, and the size change for one saved file.
    pub(super) fn words(&self) -> (String, String, Option<(String, Tone)>) {
        let entries: Vec<&Entry> = self.entries().collect();
        let failed: Vec<&Entry> = entries
            .iter()
            .copied()
            .filter(|e| matches!(e.status, Status::Failed(_)))
            .collect();
        let reason = || {
            failed
                .first()
                .and_then(|e| match &e.status {
                    Status::Failed(error) => Some(friendly_error(&error.message)),
                    _ => None,
                })
                .unwrap_or_default()
        };
        match self.phase() {
            Phase::Running => {
                let title = match (entries.as_slice(), self.shared_target()) {
                    ([one], _) => format!(
                        "Converting {} → {}",
                        model::file_name(&one.input),
                        one.to.name
                    ),
                    (many, Some(to)) => format!("Converting {} files → {to}", many.len().max(1)),
                    (many, None) => format!("Converting {} files…", many.len()),
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
                (title, line, None)
            }
            Phase::Done => {
                let outputs = self.outputs();
                let title = match outputs.as_slice() {
                    [one] => format!("Saved {}", model::file_name(one)),
                    [] => "Cancelled".to_string(),
                    many => format!("Saved {} files", many.len()),
                };
                let saved: Vec<&Entry> = entries
                    .iter()
                    .copied()
                    .filter(|e| matches!(e.status, Status::Done(_)))
                    .collect();
                match (saved.as_slice(), outputs.as_slice()) {
                    ([entry], [one]) => match (file_size(&entry.input), file_size(one)) {
                        (Some(a), Some(b)) => (
                            title,
                            format!("{} → {}", human_size(a), human_size(b)),
                            Some(super::main_window::size_delta(a, b)),
                        ),
                        _ => (title, in_original_folder(&saved), None),
                    },
                    ([], _) => (title, String::new(), None),
                    _ => (title, in_original_folder(&saved), None),
                }
            }
            Phase::Mixed => {
                let saved = entries
                    .iter()
                    .filter(|e| matches!(e.status, Status::Done(_)))
                    .count();
                let title = format!("{saved} saved · {} failed", failed.len());
                // Short enough to sit whole beside Copy, Show and Details;
                // Details opens Activity with the full error.
                let line = match failed.as_slice() {
                    [one] => format!("{} failed", model::file_name(&one.input)),
                    many => format!("{} files failed", many.len()),
                };
                (title, line, None)
            }
            Phase::Failed => {
                let title = match failed.as_slice() {
                    [one] => format!("Couldn't convert {}", model::file_name(&one.input)),
                    many => format!("Couldn't convert {} files", many.len()),
                };
                (title, reason(), None)
            }
        }
    }

    /// The failed jobs' recorded history ids (when known) and inputs, in job order.
    fn failed_records(&self, cx: &App) -> Vec<(Option<i64>, PathBuf)> {
        let state = self.app.read(cx);
        self.entries()
            .filter(|e| matches!(e.status, Status::Failed(_)))
            .map(|e| (state.record_id(e.id), e.input.clone()))
            .collect()
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

    /// The picture at the start of the title row: the file while it
    /// converts, the saved file with a check once done, and a cross when
    /// nothing was saved.
    fn lead(&self, phase: Phase, p: &Palette) -> AnyElement {
        let marked = |path: &std::path::Path, glyph: IconName, color: Hsla| {
            div()
                .relative()
                .flex_shrink_0()
                .size(px(LEAD))
                .child(theme::thumbnail(path, LEAD, LEAD, p))
                .child(
                    div()
                        .absolute()
                        .right(px(-4.))
                        .bottom(px(-4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(16.))
                        .rounded(px(8.))
                        .bg(p.overlay)
                        .child(icon(glyph, 14., color)),
                )
                .into_any_element()
        };
        let tile = |glyph: IconName, tone: Tone| {
            theme::icon_tile(glyph, tone, LEAD, p)
                .rounded(px(radius::SM))
                .into_any_element()
        };
        match phase {
            Phase::Done | Phase::Mixed => match self.outputs().first() {
                Some(out) if phase == Phase::Done => marked(out, IconName::CircleCheck, p.green),
                Some(out) => marked(out, IconName::TriangleAlert, p.amber),
                None => tile(IconName::CircleCheck, Tone::Neutral),
            },
            Phase::Failed => tile(IconName::CircleX, Tone::Error),
            Phase::Running => match self.entries().next() {
                Some(entry) => theme::thumbnail(&entry.input, LEAD, LEAD, p),
                None => tile(IconName::Loader, Tone::Neutral),
            },
        }
    }
}

/// "In the original folder", or "folders" when the files came from several.
fn in_original_folder(saved: &[&Entry]) -> String {
    let first = saved.first().and_then(|e| e.input.parent());
    if saved.iter().all(|e| e.input.parent() == first) {
        "In the original folder".into()
    } else {
        "In the original folders".into()
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
        let (title, line, delta) = self.words();
        // What tests read: the line as one string, with the size change.
        let line_label = match &delta {
            Some((change, _)) => format!("{line} · {change}"),
            None => line.clone(),
        };
        let first = self.outputs().first().cloned();
        let hover = p.hover;
        let close = theme::clickable("hud-close", "Dismiss")
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(22.))
            .mr(px(-4.))
            .rounded(px(radius::SM))
            .hover(move |s| s.bg(hover))
            .on_click(|_, window, _| window.remove_window())
            .child(icon(IconName::Close, 12., p.tertiary));
        // Saved files can be copied or shown, even when others failed.
        let actions = matches!(phase, Phase::Done | Phase::Mixed)
            .then_some(first)
            .flatten()
            .map(|path| {
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(4.))
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
        // A failure's full message is in Activity, beside Retry, even when
        // other files of the batch were saved.
        let details = matches!(phase, Phase::Mixed | Phase::Failed).then(|| {
            let failures = self.failed_records(cx);
            div().flex_shrink_0().child(
                Button::secondary("hud-details", "Details")
                    .small()
                    .build(&p)
                    .on_click(move |_, window, cx| {
                        super::show_failures(failures.clone(), cx);
                        window.remove_window();
                    }),
            )
        });
        let detail = match phase {
            Phase::Running => div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .flex_1()
                        .child(theme::progress(self.fraction(), p.track, p.green)),
                )
                .child(mono(11., 14., p.secondary).flex_shrink_0().child(line)),
            Phase::Done => div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(6.))
                .child(
                    mono(11., 14., p.secondary)
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .child(line),
                )
                .children(delta.map(|(change, tone)| theme::badge(change, tone, &p))),
            // A reason can be long; it is the one line that may be cut.
            Phase::Mixed | Phase::Failed => div().flex().flex_1().min_w_0().child(
                styled(size::SMALL, p.error)
                    .min_w_0()
                    .truncate()
                    .child(line),
            ),
        };
        div()
            .id("hud")
            .test_support()
            .aria_label(SharedString::from(phase.name()))
            // Pause auto-dismiss when the pointer moves onto the HUD (not just
            // if a stationary cursor happened to sit at (0, 0) on spawn), and
            // restart the grace timer when it leaves.
            .on_mouse_move(cx.listener(|this, _, window, cx| {
                this.set_hovered(true, window, cx);
            }))
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                if !*hovered {
                    this.set_hovered(false, window, cx);
                }
            }))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(8.))
            .size_full()
            .px(px(14.))
            .bg(p.overlay)
            .font_family(theme::SANS)
            .text_color(p.text)
            .when(transparent(), |d| {
                d.rounded(px(radius::CARD))
                    .border_1()
                    .border_color(p.border)
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(LEAD_GAP))
                    .child(self.lead(phase, &p))
                    .child(
                        div()
                            .id("hud-title")
                            .test_support()
                            .aria_label(SharedString::from(title.clone()))
                            .flex_1()
                            .min_w_0()
                            .child(
                                styled(size::BODY, p.text)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(title),
                            ),
                    )
                    .child(close),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .h(px(24.))
                    .pl(px(LEAD + LEAD_GAP))
                    .child(
                        div()
                            .id("hud-line")
                            .test_support()
                            .aria_label(SharedString::from(line_label))
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .child(detail),
                    )
                    .children(actions)
                    .children(details),
            )
    }
}
