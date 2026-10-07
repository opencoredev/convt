//! The main window: Activity (running jobs and history in one list, plus
//! Add files, which converts right away) and Automations.

use std::path::PathBuf;
use std::time::Instant;

use convt_core::{FORMATS, Format, format_by_extension, format_by_id};
use convt_license::client::{BUY_URL, State, TRIAL_DAYS};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{self, Palette, mono, primary_button, text, text_button};
use super::{LICENSE_PRICE, SettingsTab, error_text, file_size, human_size, time_left};
use crate::clock::Local;
use crate::finder::EXTENSION_SETTINGS;
use crate::history::{Outcome, Record};
use crate::jobs::{Entry, Status};
use crate::model::{self, AppState};
use crate::request::Request;
use crate::settings::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Activity,
    Automations,
}

pub struct MainView {
    app: Entity<AppState>,
    pub(super) page: Page,
    /// The default formats are open for changing.
    pub(super) editing_defaults: bool,
    /// What the last Add files couldn't do.
    pub(super) error: Option<String>,
    _observe: Subscription,
    _appearance: Subscription,
}

impl MainView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            _observe: cx.observe(&app, |_, _, cx| cx.notify()),
            _appearance: theme::observe_appearance(window, cx),
            app,
            page: Page::Activity,
            editing_defaults: false,
            error: None,
        }
    }

    pub fn set_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        cx.notify();
    }

    /// Converts files right away to their default formats. Files with no
    /// usable default open Quick convert so the user can pick.
    pub fn add(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let added = self.app.update(cx, |s, cx| s.add_files(paths, cx));
        self.error = (!added.errors.is_empty()).then(|| added.errors.join("\n"));
        if !added.ask.is_empty() {
            super::open_quick(
                Request {
                    files: added.ask,
                    ..Request::default()
                },
                cx,
            );
        }
        cx.notify();
    }

    fn pick_files(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(add_files_prompt(cx.can_select_mixed_files_and_dirs()));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                let _ = this.update(cx, |this, cx| this.add(&paths, cx));
            }
        })
        .detach();
    }

    fn sidebar(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let state = self.app.read(cx);
        let active = state.queue.active();
        let license = state.license.clone();
        let nav = |id: &'static str, label: &'static str, selected: bool, count: Option<usize>| {
            theme::clickable(id, label)
                .aria_selected(selected)
                .flex()
                .items_center()
                .justify_between()
                .px(px(10.))
                .py(px(6.))
                .rounded(px(6.))
                .when(selected, |d| d.bg(p.nav_selected))
                .child(
                    text(13., 16., p.text)
                        .when(selected, |d| d.font_weight(FontWeight::MEDIUM))
                        .child(label),
                )
                .children(count.map(|n| mono(11., 14., p.secondary).child(n.to_string())))
        };
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(208.))
            .h_full()
            .bg(p.chrome)
            .border_r_1()
            .border_color(p.chrome_border)
            .px(px(10.))
            .pt(px(16.))
            .pb(px(12.))
            // Room for the traffic lights drawn over a transparent title bar.
            .when(theme::transparent_titlebar(), |d| {
                d.child(div().h(px(34.)).flex_shrink_0())
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        nav(
                            "nav-activity",
                            "Activity",
                            self.page == Page::Activity,
                            (active > 0).then_some(active),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.set_page(Page::Activity, cx))),
                    )
                    .child(
                        nav(
                            "nav-automations",
                            "Automations",
                            self.page == Page::Automations,
                            None,
                        )
                        .on_click(
                            cx.listener(|this, _, _, cx| this.set_page(Page::Automations, cx)),
                        ),
                    )
                    .child(
                        nav("nav-settings", "Settings", false, None)
                            .on_click(|_, _, cx| super::show_settings(SettingsTab::General, cx)),
                    ),
            )
            .child(div().flex_1())
            .children(super::update::sidebar_card(&self.app, p, cx))
            .children(trial_card(&license, p))
    }

    fn header(&self, title: &'static str, p: &Palette, cx: &mut Context<Self>) -> Div {
        let state = self.app.read(cx);
        let any_finished =
            !state.recent.is_empty() || state.queue.entries.iter().any(|e| e.status.is_finished());
        let activity = self.page == Page::Activity;
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(10.))
            .h(px(52.))
            .px(px(20.))
            .border_b_1()
            .border_color(p.hairline)
            .child(
                text(15., 18., p.text)
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .when(activity && any_finished, |d| {
                d.child(
                    text_button("clear-finished", "Clear finished", p.secondary, 12.)
                        .px(px(10.))
                        .py(px(5.))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.app.update(cx, |s, cx| s.clear_activity(cx));
                        })),
                )
            })
            .when(activity, |d| {
                d.child(
                    primary_button("add-files", "Add files", 12., false)
                        .px(px(12.))
                        .py(px(5.))
                        .rounded(px(7.))
                        .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
                )
            })
    }

    fn defaults_bar(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let state = self.app.read(cx);
        let defaults = state.settings.defaults.clone();
        let registry = state.registry.clone();
        let chip = |label: String| {
            div()
                .px(px(8.))
                .py(px(2.))
                .rounded(px(5.))
                .bg(p.chip)
                .border_1()
                .border_color(p.chip_border)
                .child(text(12., 16., p.text).whitespace_nowrap().child(label))
        };
        let summary = div()
            .flex()
            .items_center()
            .gap(px(16.))
            .child(
                text(12., 16., p.secondary)
                    .flex_shrink_0()
                    .child("Add files converts right away to"),
            )
            .child(
                div()
                    .id("defaults")
                    .test_support()
                    .aria_label(SharedString::from(
                        Kind::ALL
                            .iter()
                            .filter_map(|k| {
                                Some(format!("{} → {}", k.label(), defaults.get(*k)?.name))
                            })
                            .collect::<Vec<_>>()
                            .join(", "),
                    ))
                    .flex()
                    .flex_1()
                    .flex_wrap()
                    .gap(px(6.))
                    .children(Kind::ALL.iter().filter_map(|k| {
                        Some(chip(format!("{} → {}", k.label(), defaults.get(*k)?.name)))
                    })),
            )
            .child(
                text_button(
                    "change-defaults",
                    if self.editing_defaults {
                        "Done"
                    } else {
                        "Change"
                    },
                    p.green,
                    12.,
                )
                .flex_shrink_0()
                .font_weight(FontWeight::MEDIUM)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.editing_defaults = !this.editing_defaults;
                    cx.notify();
                })),
            );
        let editor = self.editing_defaults.then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .pt(px(10.))
                .children(Kind::ALL.iter().map(|&kind| {
                    let current = defaults.get(kind);
                    let choices = choices_for(&registry, kind);
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            text(12., 16., p.secondary)
                                .w(px(80.))
                                .flex_shrink_0()
                                .child(kind.label()),
                        )
                        .child(div().flex().flex_wrap().gap(px(6.)).children(
                            choices.into_iter().map(|to| {
                                let on = current == Some(to);
                                let app = self.app.clone();
                                theme::clickable(
                                    SharedString::from(format!("default-{}-{}", kind.id(), to.id)),
                                    to.name,
                                )
                                .aria_selected(on)
                                .px(px(8.))
                                .py(px(2.))
                                .rounded(px(5.))
                                .bg(if on { p.green_tint } else { p.chip })
                                .border_1()
                                .border_color(if on { p.green } else { p.chip_border })
                                .on_click(move |_, _, cx| {
                                    app.update(cx, |s, cx| {
                                        s.update_settings(|s| s.defaults.set(kind, to), cx)
                                    })
                                })
                                .child(
                                    text(12., 16., if on { p.green } else { p.text })
                                        .child(to.name),
                                )
                            }),
                        ))
                }))
        });
        div().flex().px(px(20.)).pt(px(16.)).pb(px(4.)).child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .px(px(14.))
                .py(px(10.))
                .rounded(px(8.))
                .bg(p.recessed)
                .border_1()
                .border_color(p.recessed_border)
                .child(summary)
                .children(editor),
        )
    }

    fn activity(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let state = self.app.read(cx);
        let now = Instant::now();
        let today = Local::now();
        let mut rows: Vec<AnyElement> = Vec::new();
        let active: Vec<Entry> = state
            .queue
            .entries
            .iter()
            .filter(|e| !e.status.is_finished())
            .cloned()
            .collect();
        let mut heading = None;
        let mut push_heading = |label: String, rows: &mut Vec<AnyElement>| {
            if heading.as_ref() != Some(&label) {
                rows.push(
                    div()
                        .flex()
                        .py(px(8.))
                        .child(
                            text(11., 14., p.tertiary)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(label.clone()),
                        )
                        .into_any_element(),
                );
                heading = Some(label);
            }
        };
        if !active.is_empty() {
            push_heading("Today".into(), &mut rows);
        }
        for entry in &active {
            rows.push(active_row(entry, now, &self.app, p).into_any_element());
        }
        for record in &state.recent {
            push_heading(Local::at(record.finished_at).day_label(&today), &mut rows);
            rows.push(record_row(record, &self.app, p).into_any_element());
        }
        let empty = rows.is_empty();
        let finder_off = state.finder_on == Some(false);
        let hint = if finder_off {
            "Drop files here, or use Add files. Turn on the Finder menu above to convert from a right-click.".to_string()
        } else {
            format!(
                "Drop files here, or right-click a file in {} and pick a format.",
                theme::file_manager()
            )
        };
        div()
            .id("activity")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .px(px(20.))
            .pt(px(8.))
            .pb(px(20.))
            .children(
                self.error
                    .clone()
                    .map(|e| div().pb(px(8.)).child(error_text(e, p))),
            )
            .children(finder_off.then(|| finder_setup_card(p)))
            .when(empty, |d| {
                d.child(
                    div()
                        .id("empty")
                        .test_support()
                        .aria_label("Nothing converted yet.")
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(4.))
                        .py(px(60.))
                        .child(text(13., 16., p.secondary).child("Nothing converted yet."))
                        .child(text(12., 16., p.tertiary).child(hint)),
                )
            })
            .children(rows)
    }

    fn automations(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let rules = self.app.read(cx).settings.automations.clone();
        div()
            .id("automations")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .px(px(20.))
            .pt(px(12.))
            .child(text(12., 16., p.secondary).pb(px(8.)).child(
                "Rules are saved here. Running them automatically comes in a later version.",
            ))
            .children(rules.into_iter().enumerate().map(|(i, rule)| {
                let to = format_by_id(&rule.to).map_or(rule.to.clone(), |f| f.name.to_string());
                let app = self.app.clone();
                let on = rule.enabled;
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .py(px(10.))
                    .border_b_1()
                    .border_color(p.row_divider)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .gap(px(2.))
                            .child(text(13., 16., p.text).child(format!("{} → {to}", rule.name)))
                            .child(
                                mono(11., 14., p.secondary)
                                    .child(format!("{} · {}", rule.source, rule.detail)),
                            ),
                    )
                    .child(
                        theme::switch(SharedString::from(format!("automation-{i}")), on, false, p)
                            .on_click(move |_, _, cx| {
                                app.update(cx, |s, cx| s.set_automation(i, !on, cx))
                            }),
                    )
            }))
    }
}

/// The formats a kind of file can default to: every format some file of
/// that kind can become, in table order.
fn choices_for(registry: &convt_core::Registry, kind: Kind) -> Vec<&'static Format> {
    let inputs: Vec<&Format> = FORMATS
        .iter()
        .filter(|f| Kind::of(f) == Some(kind))
        .collect();
    FORMATS
        .iter()
        .filter(|to| {
            inputs
                .iter()
                .any(|from| registry.targets(from).contains(to))
        })
        .take(10)
        .collect()
}

/// Shown on Activity until the Finder extension is on, so skipping or
/// closing first run still has a way back.
fn finder_setup_card(p: &Palette) -> impl IntoElement {
    div()
        .id("finder-setup")
        .test_support()
        .aria_label("Turn on the Finder menu")
        .flex()
        .flex_col()
        .gap(px(8.))
        .mb(px(8.))
        .p(px(14.))
        .rounded(px(8.))
        .bg(p.green_tint)
        .border_1()
        .border_color(p.green)
        .child(
            text(13., 16., p.text)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Turn on the Finder menu"),
        )
        .child(
            text(12., 16., p.secondary).child(
                "Right-click Convert with convt needs the Finder extension. Open System Settings, scroll to Extensions, and turn on convt.",
            ),
        )
        .child(
            div().flex().child(
                primary_button("enable-finder", "Open System Settings", 12., false)
                    .on_click(|_, _, cx| cx.open_url(EXTENSION_SETTINGS)),
            ),
        )
}

/// The trial or license card at the bottom of the sidebar. Nothing once
/// licensed or in a build that doesn't check licenses.
fn trial_card(state: &State, p: &Palette) -> Option<impl IntoElement + use<>> {
    let (title, left, used, color, link) = match state {
        State::Unrestricted | State::Licensed(_) => return None,
        State::Trial { started: None, .. } => (
            "Trial",
            format!("{TRIAL_DAYS} days"),
            0.,
            p.green,
            format!("Buy license · {LICENSE_PRICE}"),
        ),
        State::Trial { days_left, .. } => (
            "Trial",
            match days_left {
                1 => "1 day left".to_string(),
                n => format!("{n} days left"),
            },
            (TRIAL_DAYS - days_left) as f32 / TRIAL_DAYS as f32,
            p.green,
            format!("Buy license · {LICENSE_PRICE}"),
        ),
        State::TrialEnded => (
            "Trial ended",
            "0 days left".into(),
            1.,
            p.error,
            format!("Buy license · {LICENSE_PRICE}"),
        ),
        State::NotCovered(_) => ("Updates ended", String::new(), 1., p.error, "Renew".into()),
    };
    Some(
        div()
            .id("trial-card")
            .test_support()
            .aria_label(SharedString::from(state.summary()))
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(12.))
            .rounded(px(8.))
            .bg(p.trial_card)
            .border_1()
            .border_color(p.chrome_border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        text(12., 16., p.text)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(mono(11., 14., p.secondary).child(left)),
            )
            .child(theme::progress(used, p.track, color))
            .child(
                text_button("trial-buy", link, p.green, 12.)
                    .font_weight(FontWeight::MEDIUM)
                    .on_click(|_, _, cx| cx.open_url(BUY_URL)),
            ),
    )
}

/// The file name with the target, "interview.mov → MP4".
fn name_line(input: &std::path::Path, to: &str, p: &Palette) -> Div {
    div()
        .flex()
        .items_baseline()
        .gap(px(8.))
        .min_w_0()
        .child(
            text(13., 16., p.text)
                .truncate()
                .child(model::file_name(input)),
        )
        .child(
            text(13., 16., p.tertiary)
                .flex_shrink_0()
                .child(format!("→ {to}")),
        )
}

fn status_cell(id: String, label: String, el: Div) -> impl IntoElement {
    div()
        .id(SharedString::from(id))
        .test_support()
        .aria_label(SharedString::from(label))
        // Wide enough for "68% · under a minute left" on one line.
        .w(px(176.))
        .flex_shrink_0()
        .child(el.whitespace_nowrap().overflow_hidden().text_ellipsis())
}

fn row(id: String, p: &Palette) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .flex()
        .items_center()
        .gap(px(14.))
        .py(px(10.))
        .border_b_1()
        .border_color(p.row_divider)
}

/// A running or waiting job.
fn active_row(
    entry: &Entry,
    now: Instant,
    app: &Entity<AppState>,
    p: &Palette,
) -> impl IntoElement {
    let id = entry.id;
    let (status, el, action) = match entry.status {
        Status::Running(progress) => {
            let mut status = match progress {
                Some(f) => format!("{:.0}%", f * 100.),
                None => "Converting".into(),
            };
            if let Some(left) = entry.remaining(now) {
                status = format!("{status} · {}", time_left(left));
            }
            let el = mono(11., 14., p.secondary).child(status.clone());
            (status, el, "Stop")
        }
        _ => {
            let el = mono(11., 14., p.tertiary).child("Waiting");
            ("Waiting".to_string(), el, "Remove")
        }
    };
    let bar = match entry.status {
        Status::Running(f) => Some(div().w(px(360.)).max_w_full().child(theme::progress(
            f.unwrap_or(0.),
            p.track,
            p.green,
        ))),
        _ => None,
    };
    let app = app.clone();
    row(format!("job-{id}"), p)
        .child(theme::thumbnail(&entry.input, 48., 34., p))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(6.))
                .child(name_line(&entry.input, entry.to.name, p))
                .children(bar),
        )
        .child(status_cell(format!("status-{id}"), status, el))
        .child(
            text_button(
                SharedString::from(format!("{}-{id}", action.to_lowercase())),
                action,
                p.secondary,
                12.,
            )
            .w(px(60.))
            .flex_shrink_0()
            .flex()
            .justify_end()
            .on_click(move |_, _, cx| app.update(cx, |s, _| s.cancel(id))),
        )
}

/// A finished conversion from history.
fn record_row(record: &Record, app: &Entity<AppState>, p: &Palette) -> impl IntoElement {
    let id = record.id;
    let to = format_by_id(&record.to);
    let to_name = to.map_or(record.to.clone(), |f| f.name.to_string());
    let (status, color, detail, action): (String, Hsla, Option<Div>, Option<AnyElement>) =
        match &record.outcome {
            Outcome::Done(outputs) => {
                let sizes = match (file_size(&record.input), outputs.as_slice()) {
                    (Some(a), [one]) => {
                        file_size(one).map(|b| format!("{} → {}", human_size(a), human_size(b)))
                    }
                    (_, many) if many.len() > 1 => Some(format!("{} files", many.len())),
                    _ => None,
                };
                let show = outputs.first().cloned().map(|path| {
                    text_button(
                        SharedString::from(format!("show-{id}")),
                        "Show",
                        p.text,
                        12.,
                    )
                    .on_click(move |_, _, cx| cx.reveal_path(&path))
                    .into_any_element()
                });
                (
                    format!("Done · {}", Local::at(record.finished_at).time()),
                    p.green,
                    sizes.map(|s| mono(11., 14., p.secondary).child(s)),
                    show,
                )
            }
            Outcome::Failed(message) => (
                "Failed".into(),
                p.error,
                Some(text(12., 16., p.error).child(message.clone())),
                retry(record, app, p),
            ),
            Outcome::Cancelled => ("Cancelled".into(), p.tertiary, None, retry(record, app, p)),
        };
    let input_exists = record.input.exists();
    row(format!("record-{id}"), p)
        .child(theme::thumbnail(
            match &record.outcome {
                Outcome::Done(out) if !input_exists => out.first().unwrap_or(&record.input),
                _ => &record.input,
            },
            48.,
            34.,
            p,
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(3.))
                .child(name_line(&record.input, &to_name, p))
                .children(detail),
        )
        .child(status_cell(
            format!("record-status-{id}"),
            status.clone(),
            text(12., 16., color)
                .font_weight(FontWeight::MEDIUM)
                .child(status),
        ))
        .child(
            div()
                .w(px(60.))
                .flex_shrink_0()
                .flex()
                .justify_end()
                .children(action),
        )
}

fn retry(record: &Record, app: &Entity<AppState>, p: &Palette) -> Option<AnyElement> {
    let to = format_by_id(&record.to)?;
    format_by_extension(&record.input)?;
    let input = record.input.clone();
    let setup = record.setup.clone();
    let app = app.clone();
    Some(
        text_button(
            SharedString::from(format!("retry-{}", record.id)),
            "Retry",
            p.text,
            12.,
        )
        .on_click(move |_, _, cx| {
            if let Err(e) = app.update(cx, |s, cx| s.retry(&input, to, setup.as_ref(), cx)) {
                tracing::warn!(error = %e, "could not retry");
            }
        })
        .into_any_element(),
    )
}

impl Render for MainView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let sidebar = self.sidebar(&p, cx);
        let main = match self.page {
            Page::Activity => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(self.header("Activity", &p, cx))
                .child(self.defaults_bar(&p, cx))
                .child(self.activity(&p, cx)),
            Page::Automations => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(self.header("Automations", &p, cx))
                .child(self.automations(&p, cx)),
        };
        div()
            .id("main")
            .flex()
            .size_full()
            .bg(p.window)
            .font_family(theme::SANS)
            .text_color(p.text)
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.bg(p.green_tint))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.add(paths.paths(), cx);
            }))
            .child(sidebar)
            .child(main)
    }
}

/// What Add files asks the system picker for. Where one picker can't offer
/// files and folders together (the XDG portal on Linux, and Windows), asking
/// for folders makes it a folder picker, so ask for files there.
pub(super) fn add_files_prompt(mixed: bool) -> PathPromptOptions {
    PathPromptOptions {
        files: true,
        directories: mixed,
        multiple: true,
        prompt: Some("Convert".into()),
    }
}
