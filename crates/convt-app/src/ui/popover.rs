//! The menu bar popover: a drop bar that converts a file and copies the
//! result to the clipboard, the running jobs, the automation rules with their
//! switches, and links to the main window and Settings.
//!
//! It opens only from the tray icon (see `tray.rs`), which GPUI can't draw
//! yet, so for now only tests open it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use convt_core::format_by_id;
use convt_license::client::State;
use gpui_kit::component::IconName;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::main_window::Page;
use super::theme::{self, Button, Palette, icon, mono, size, space, styled};
use super::{SettingsTab, error_text, file_size, human_size, time_left};
use crate::automation;
use crate::clipboard::clipboard_item;
use crate::jobs::{Entry, JobId, Status};
use crate::model::{self, AppState};
use crate::request::Request;

/// How many dropped files the popover lists. Every drop is tracked until
/// its result is copied, however many there are.
const DROPS_SHOWN: usize = 3;

/// A file dropped on the popover and what became of it.
#[derive(Debug, Clone)]
pub struct Dropped {
    pub job: JobId,
    /// Set once the result is on the clipboard.
    pub copied: bool,
}

pub struct PopoverView {
    app: Entity<AppState>,
    pub(super) drops: Vec<Dropped>,
    /// Every job whose result was copied, in order, for tests.
    #[cfg(test)]
    pub(super) copied: Vec<JobId>,
    /// The last state seen of each dropped job.
    seen: HashMap<JobId, Entry>,
    pub(super) error: Option<String>,
    _observe: Subscription,
    _appearance: Subscription,
}

impl PopoverView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            _observe: cx.observe(&app, |this: &mut Self, app, cx| {
                this.update_drops(&app, cx);
                cx.notify();
            }),
            _appearance: theme::observe_appearance(window, cx),
            app,
            drops: Vec::new(),
            #[cfg(test)]
            copied: Vec::new(),
            seen: HashMap::new(),
            error: None,
        }
    }

    /// Converts dropped files to their default formats. Each result is
    /// copied to the clipboard when it is done. Files with no default open
    /// Quick convert.
    pub fn drop_files(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let added = self.app.update(cx, |s, cx| s.add_files(paths, cx));
        self.error = (!added.errors.is_empty()).then(|| added.errors.join("\n"));
        for job in added.jobs {
            self.drops.insert(0, Dropped { job, copied: false });
        }
        if !added.ask.is_empty() {
            super::open_quick(
                Request {
                    files: added.ask,
                    ..Request::default()
                },
                cx,
            );
        }
        let app = self.app.clone();
        self.update_drops(&app, cx);
        cx.notify();
    }

    fn update_drops(&mut self, app: &Entity<AppState>, cx: &mut Context<Self>) {
        let state = app.read(cx);
        for drop in &self.drops {
            if let Some(entry) = state.entry(drop.job) {
                self.seen.insert(drop.job, entry.clone());
            }
        }
        for drop in &mut self.drops {
            if drop.copied {
                continue;
            }
            if let Some(Entry {
                status: Status::Done(outputs),
                ..
            }) = self.seen.get(&drop.job)
            {
                cx.write_to_clipboard(clipboard_item(outputs));
                drop.copied = true;
                #[cfg(test)]
                self.copied.push(drop.job);
            }
        }
        // Forget settled drops that are no longer listed. Pending ones stay,
        // so their results are still copied when they finish.
        let seen = &self.seen;
        let settled = |drop: &Dropped| {
            drop.copied || seen.get(&drop.job).is_some_and(|e| e.status.is_finished())
        };
        let mut index = 0;
        self.drops.retain(|drop| {
            index += 1;
            index <= DROPS_SHOWN || !settled(drop)
        });
        let kept: Vec<JobId> = self.drops.iter().map(|d| d.job).collect();
        self.seen.retain(|job, _| kept.contains(job));
    }

    fn running(&self, p: &Palette, cx: &App) -> Option<Div> {
        let now = Instant::now();
        let state = self.app.read(cx);
        let dropped: Vec<JobId> = self.drops.iter().take(DROPS_SHOWN).map(|d| d.job).collect();
        let rows: Vec<Div> = state
            .queue
            .entries
            .iter()
            .filter(|e| !e.status.is_finished() && !dropped.contains(&e.id))
            .map(|e| job_row(e, now, p))
            .collect();
        (!rows.is_empty()).then(|| div().flex().flex_col().children(rows))
    }

    fn dropped(&self, p: &Palette) -> Option<Div> {
        let now = Instant::now();
        let rows: Vec<Div> = self
            .drops
            .iter()
            .take(DROPS_SHOWN)
            .filter_map(|drop| {
                let entry = self.seen.get(&drop.job)?;
                let row = match &entry.status {
                    Status::Done(outputs) => {
                        let sizes = match (file_size(&entry.input), outputs.as_slice()) {
                            (Some(a), [one]) => file_size(one)
                                .map(|b| format!("{} → {}", human_size(a), human_size(b))),
                            (_, many) => Some(format!("{} files", many.len())),
                        };
                        let thumb = outputs
                            .first()
                            .map_or(entry.input.as_path(), PathBuf::as_path);
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .px(px(16.))
                                    .py(px(6.))
                                    .child(theme::thumbnail(thumb, 44., 32., p))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .flex_1()
                                            .min_w_0()
                                            .gap(px(2.))
                                            .child(title(&entry.input, entry.to.name, p))
                                            .children(
                                                sizes.map(|s| mono(11., 14., p.secondary).child(s)),
                                            ),
                                    ),
                            )
                            .when(drop.copied, |d| d.child(copied_chip(drop.job, p)))
                    }
                    Status::Failed(e) => div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .px(px(16.))
                        .py(px(6.))
                        .child(title(&entry.input, entry.to.name, p))
                        .child(styled(size::SMALL, p.error).child(e.message.clone())),
                    Status::Cancelled => return None,
                    _ => job_row(entry, now, p),
                };
                Some(row)
            })
            .collect();
        (!rows.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .pt(px(10.))
                .pb(px(8.))
                .border_t_1()
                .border_color(p.hairline)
                .children(rows)
        })
    }

    fn automations(&self, p: &Palette, cx: &App) -> Div {
        let rules = self.app.read(cx).settings.automations.clone();
        div()
            .flex()
            .flex_col()
            .py(px(8.))
            .border_t_1()
            .border_color(p.hairline)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(16.))
                    .pt(px(4.))
                    .pb(px(6.))
                    .child(
                        styled(size::CAPTION, p.tertiary)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Automations"),
                    )
                    .child(
                        Button::ghost("new-rule", "New rule")
                            .icon(IconName::Plus)
                            .color(p.green_text)
                            .small()
                            .build(p)
                            .on_click(|_, _, cx| show_page(Page::Automations, cx)),
                    ),
            )
            .children(rules.into_iter().enumerate().map(|(i, rule)| {
                let to = format_by_id(&rule.to).map_or(rule.to.clone(), |f| f.name.to_string());
                let app = self.app.clone();
                let on = rule.enabled;
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(16.))
                    .py(px(6.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap(px(2.))
                            .child(
                                styled(size::BODY, p.text)
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(format!("{} → {to}", rule.name)),
                            )
                            .child(mono(11., 14., p.secondary).truncate().child(format!(
                                "{} · {}",
                                automation::source_line(&rule),
                                rule.detail
                            ))),
                    )
                    .child(
                        theme::switch(
                            SharedString::from(format!("popover-automation-{i}")),
                            on,
                            true,
                            p,
                        )
                        .flex_shrink_0()
                        .on_click(move |_, _, cx| {
                            app.update(cx, |s, cx| s.set_automation(i, !on, cx))
                        }),
                    )
            }))
    }
}

/// Opens the main window on `page`.
fn show_page(page: Page, cx: &mut App) {
    if let Some((handle, view)) = super::open_main(cx) {
        let _ = handle.update(cx, |_, _, cx| view.update(cx, |v, cx| v.set_page(page, cx)));
    }
}

fn title(input: &Path, to: &str, p: &Palette) -> Div {
    styled(size::BODY, p.text)
        .font_weight(FontWeight::MEDIUM)
        .truncate()
        .child(format!("{} → {to}", model::file_name(input)))
}

fn copied_chip(job: JobId, p: &Palette) -> Div {
    div()
        .flex()
        .pl(px(72.))
        .pr(px(16.))
        .pt(px(4.))
        .pb(px(2.))
        .child(
            div()
                .id(SharedString::from(format!("copied-{job}")))
                .test_support()
                .aria_label("Copied to your clipboard")
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .py(px(4.))
                .rounded(px(6.))
                .bg(p.green_tint)
                .shadow(vec![theme::inset_ring(p.green_border, 1.)])
                .child(icon(IconName::Check, 12., p.green_text))
                .child(
                    styled(size::SMALL, p.green_text)
                        .font_weight(FontWeight::MEDIUM)
                        .child("Copied to your clipboard"),
                ),
        )
}

/// A running or waiting job with its progress.
fn job_row(entry: &Entry, now: Instant, p: &Palette) -> Div {
    let (pct, fraction) = match entry.status {
        Status::Running(Some(f)) => (format!("{:.0}%", f * 100.), f),
        Status::Running(None) => (String::new(), 0.),
        _ => ("Waiting".into(), 0.),
    };
    let size = file_size(&entry.input).map(human_size).unwrap_or_default();
    let left = entry.remaining(now).map(time_left).unwrap_or_default();
    div()
        .flex()
        .gap(px(12.))
        .px(px(16.))
        .pt(px(10.))
        .pb(px(14.))
        .child(theme::thumbnail(&entry.input, 44., 32., p))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(6.))
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .justify_between()
                        .gap(px(8.))
                        .child(title(&entry.input, entry.to.name, p).flex_1())
                        .child(mono(11., 14., p.secondary).child(pct)),
                )
                .child(theme::progress(fraction, p.track, p.green))
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .child(mono(11., 14., p.secondary).child(size))
                        .child(mono(11., 14., p.secondary).child(left)),
                ),
        )
}

/// "Trial · 5 days left", for the footer.
fn license_line(state: &State) -> Option<String> {
    match state {
        State::Unrestricted => None,
        State::Trial { started: None, .. } => Some("Trial · 7 days".into()),
        State::Trial { days_left: 1, .. } => Some("Trial · last day".into()),
        State::Trial { days_left, .. } => Some(format!("Trial · {days_left} days left")),
        State::TrialEnded => Some("Trial ended".into()),
        State::Licensed(_) => Some("Licensed".into()),
        State::NotCovered(_) => Some("Updates ended".into()),
    }
}

impl Render for PopoverView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let state = self.app.read(cx);
        let active = state.queue.active();
        let license = license_line(&state.license);
        let drop_bar = div()
            .id("drop-bar")
            .test_support()
            .aria_label("Drop a file to convert and copy")
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(space::SM))
            .h(px(52.))
            .rounded(px(theme::radius::CARD))
            .bg(p.recessed)
            .border_1()
            .border_dashed()
            .border_color(p.control_border)
            .drag_over::<ExternalPaths>(move |style, _, _, _| {
                style.border_color(p.green).bg(p.green_tint)
            })
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.drop_files(paths.paths(), cx);
            }))
            .child(icon(IconName::ArrowDown, 14., p.secondary))
            .child(styled(size::SMALL, p.secondary).child("Drop a file to convert and copy"));
        div()
            .id("popover")
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
                    .justify_between()
                    .px(px(16.))
                    .pt(px(14.))
                    .pb(px(10.))
                    .child(theme::lockup(13., &p))
                    .when(active > 0, |d| {
                        d.child(theme::badge(
                            format!("{active} converting"),
                            theme::Tone::Green,
                            &p,
                        ))
                    }),
            )
            .child(div().flex().px(px(12.)).pb(px(10.)).child(drop_bar))
            .children(
                self.error
                    .clone()
                    .map(|e| div().px(px(16.)).pb(px(8.)).child(error_text(e, &p))),
            )
            .children(self.running(&p, cx))
            .children(self.dropped(&p))
            .child(self.automations(&p, cx))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .px(px(12.))
                    .py(px(8.))
                    .bg(p.chrome)
                    .border_t_1()
                    .border_color(p.chrome_border)
                    .child(
                        styled(size::SMALL, p.secondary)
                            .pl(px(4.))
                            .child(license.unwrap_or_default()),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(space::XS))
                            .child(
                                Button::ghost("open-convt", "Open convt")
                                    .small()
                                    .build(&p)
                                    .on_click(|_, _, cx| show_page(Page::Activity, cx)),
                            )
                            .child(
                                Button::ghost("open-settings", "Settings")
                                    .icon(IconName::Settings)
                                    .small()
                                    .build(&p)
                                    .on_click(|_, _, cx| {
                                        super::show_settings(SettingsTab::General, cx)
                                    }),
                            ),
                    ),
            )
    }
}
