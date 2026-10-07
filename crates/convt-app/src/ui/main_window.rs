//! The main window: Activity (running jobs and history in one list, plus
//! Add files, which converts right away) and Automations.

use std::path::PathBuf;
use std::time::Instant;

use convt_core::{FORMATS, Format, format_by_extension, format_by_id};
use convt_license::client::{BUY_URL, State, TRIAL_DAYS};
use gpui_kit::component::IconName;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{self, Button, Palette, Tone, icon, mono, radius, size, space, styled, text};
use super::{LICENSE_PRICE, SettingsTab, error_text, file_size, human_size, time_left};
use crate::automation;
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

/// The sidebar's width.
const SIDEBAR: f32 = 220.;
/// The left and right margin of the content column.
const GUTTER: f32 = 24.;

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

    pub(super) fn pick_files(&mut self, cx: &mut Context<Self>) {
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
        let nav = |id: &'static str,
                   label: &'static str,
                   glyph: IconName,
                   selected: bool,
                   count: Option<usize>| {
            theme::clickable(id, label)
                .aria_selected(selected)
                .flex()
                .items_center()
                .gap(px(10.))
                .h(px(30.))
                .px(px(space::SM))
                .rounded(px(radius::CONTROL))
                .map(|d| {
                    if selected {
                        d.bg(p.selected)
                    } else {
                        d.hover(|s| s.bg(p.hover))
                    }
                })
                .child(icon(
                    glyph,
                    15.,
                    if selected { p.text } else { p.secondary },
                ))
                .child(
                    styled(size::BODY, if selected { p.text } else { p.secondary })
                        .flex_1()
                        .font_weight(FontWeight::MEDIUM)
                        .child(label),
                )
                .children(count.map(|n| theme::badge(n.to_string(), Tone::Green, p)))
        };
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(SIDEBAR))
            .h_full()
            .bg(p.chrome)
            .border_r_1()
            .border_color(p.chrome_border)
            .px(px(10.))
            .pb(px(space::MD))
            // Room for the traffic lights drawn over a transparent title bar.
            .pt(px(if theme::transparent_titlebar() {
                44.
            } else {
                16.
            }))
            .child(
                div()
                    .id("brand")
                    .flex()
                    .px(px(space::SM))
                    .pb(px(18.))
                    .child(theme::lockup(14., p)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        nav(
                            "nav-activity",
                            "Activity",
                            IconName::Inbox,
                            self.page == Page::Activity,
                            (active > 0).then_some(active),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.set_page(Page::Activity, cx))),
                    )
                    .child(
                        nav(
                            "nav-automations",
                            "Automations",
                            IconName::Bot,
                            self.page == Page::Automations,
                            None,
                        )
                        .on_click(
                            cx.listener(|this, _, _, cx| this.set_page(Page::Automations, cx)),
                        ),
                    )
                    .child(
                        nav("nav-settings", "Settings", IconName::Settings, false, None)
                            .on_click(|_, _, cx| super::show_settings(SettingsTab::General, cx)),
                    ),
            )
            .child(div().flex_1())
            .children(super::update::sidebar_card(&self.app, p, cx))
            .children(trial_card(&license, p))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(space::SM))
                    .pt(px(space::MD))
                    .child(icon(IconName::HardDrive, 12., p.tertiary))
                    .child(
                        styled(size::CAPTION, p.tertiary)
                            .child(format!("Files stay on {}", theme::this_machine())),
                    ),
            )
    }

    fn header(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let state = self.app.read(cx);
        let activity = self.page == Page::Activity;
        let any_finished =
            !state.recent.is_empty() || state.queue.entries.iter().any(|e| e.status.is_finished());
        let active = state.queue.active();
        let (title, subtitle) = if activity {
            (
                "Activity",
                match (active, state.recent.len()) {
                    (0, 0) => "Drop files anywhere in this window".to_string(),
                    (0, 1) => "1 recent conversion".to_string(),
                    (0, n) => format!("{n} recent conversions"),
                    (n, _) => format!("{n} converting"),
                },
            )
        } else {
            ("Automations", "Saved rules for folders".to_string())
        };
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(space::SM))
            .h(px(64.))
            .px(px(GUTTER))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(1.))
                    .child(
                        styled(size::TITLE, p.text)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .id("subtitle")
                            .test_support()
                            .aria_label(SharedString::from(subtitle.clone()))
                            .child(styled(size::SMALL, p.secondary).truncate().child(subtitle)),
                    ),
            )
            .when(activity && any_finished, |d| {
                d.child(
                    Button::ghost("clear-finished", "Clear finished")
                        .small()
                        .build(p)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.app.update(cx, |s, cx| s.clear_activity(cx));
                        })),
                )
            })
            .when(activity, |d| {
                d.child(
                    Button::primary("add-files", "Add files")
                        .icon(IconName::Plus)
                        .build(p)
                        .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
                )
            })
    }

    fn defaults_bar(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let state = self.app.read(cx);
        let defaults = state.settings.defaults.clone();
        let registry = state.registry.clone();
        let chip = |kind: &'static str, to: &'static str| {
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(5.))
                .h(px(24.))
                .px(px(9.))
                .rounded(px(radius::CONTROL))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .child(styled(size::SMALL, p.secondary).child(kind))
                .child(icon(IconName::ArrowRight, 11., p.tertiary))
                .child(
                    styled(size::SMALL, p.text)
                        .font_weight(FontWeight::MEDIUM)
                        .child(to),
                )
        };
        let summary = div()
            .flex()
            .items_center()
            .gap(px(space::MD))
            .h(px(48.))
            .child(
                styled(size::SMALL, p.secondary)
                    .flex_shrink_0()
                    .child("Add files converts to"),
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
                    .min_w_0()
                    .overflow_hidden()
                    .gap(px(6.))
                    .children(
                        Kind::ALL
                            .iter()
                            .filter_map(|k| Some(chip(k.label(), defaults.get(*k)?.name))),
                    ),
            )
            .child(
                Button::ghost(
                    "change-defaults",
                    if self.editing_defaults {
                        "Done"
                    } else {
                        "Change"
                    },
                )
                .small()
                .color(p.green_text)
                .build(p)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.editing_defaults = !this.editing_defaults;
                    cx.notify();
                })),
            );
        let editor = self.editing_defaults.then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .pb(px(14.))
                .children(Kind::ALL.iter().map(|&kind| {
                    let current = defaults.get(kind);
                    let choices = choices_for(&registry, kind);
                    div()
                        .flex()
                        .items_start()
                        .gap(px(space::MD))
                        .child(
                            styled(size::SMALL, p.secondary)
                                .w(px(84.))
                                .flex_shrink_0()
                                .pt(px(4.))
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
                                .flex()
                                .items_center()
                                .h(px(24.))
                                .px(px(9.))
                                .rounded(px(radius::CONTROL))
                                .map(|d| {
                                    if on {
                                        d.bg(p.green_tint)
                                            .shadow(vec![theme::inset_ring(p.green_border, 1.)])
                                    } else {
                                        d.bg(p.surface)
                                            .shadow(vec![theme::inset_ring(p.border, 1.)])
                                            .hover(|s| s.bg(p.hover))
                                    }
                                })
                                .on_click(move |_, _, cx| {
                                    app.update(cx, |s, cx| {
                                        s.update_settings(|s| s.defaults.set(kind, to), cx)
                                    })
                                })
                                .child(
                                    styled(size::SMALL, if on { p.green_text } else { p.text })
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(to.name),
                                )
                            }),
                        ))
                }))
        });
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .px(px(GUTTER))
            .bg(p.recessed)
            .border_t_1()
            .border_b_1()
            .border_color(p.hairline)
            .child(summary)
            .children(editor)
    }

    /// What goes above the list: what Add files couldn't do, and the way
    /// back to Finder setup.
    fn notices(&self, p: &Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let finder_off = self.app.read(cx).finder_on == Some(false);
        let mut notices = Vec::new();
        if let Some(e) = self.error.clone() {
            notices.push(
                theme::callout(IconName::TriangleAlert, Tone::Error, error_text(e, p), p)
                    .into_any_element(),
            );
        }
        if finder_off {
            notices.push(finder_setup_card(p).into_any_element());
        }
        notices
    }

    fn activity(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
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
                        .px(px(10.))
                        .pt(px(if rows.is_empty() { 4. } else { 18. }))
                        .pb(px(6.))
                        .child(
                            styled(size::CAPTION, p.tertiary)
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
        let finder_off = state.finder_on == Some(false);
        let notices = self.notices(p, cx);
        let notices = (!notices.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(space::SM))
                .px(px(GUTTER))
                .pt(px(space::LG))
                .children(notices)
        });
        if rows.is_empty() {
            return div()
                .id("activity")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .children(notices)
                .child(self.empty_state(finder_off, p, cx))
                .into_any_element();
        }
        div()
            .id("activity")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .children(notices)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px(px(GUTTER - 10.))
                    .pt(px(space::MD))
                    .pb(px(GUTTER))
                    .children(rows),
            )
            .into_any_element()
    }

    fn empty_state(&self, finder_off: bool, p: &Palette, cx: &mut Context<Self>) -> Div {
        let hint = if finder_off {
            "Drop files here, or use Add files. Turn on the Finder menu above to convert from a right-click.".to_string()
        } else {
            format!(
                "Drop files here, or right-click a file in {} and pick a format.",
                theme::file_manager()
            )
        };
        div().flex().flex_1().min_h_0().p(px(GUTTER)).child(
            div()
                .id("empty")
                .test_support()
                .aria_label("Nothing converted yet.")
                .flex()
                .flex_col()
                .flex_1()
                .items_center()
                .justify_center()
                .gap(px(space::LG))
                .px(px(space::XXL))
                .rounded(px(radius::PANEL))
                .border_1()
                .border_dashed()
                .border_color(p.control_border)
                .bg(p.recessed)
                .child(theme::mark(40., p))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(6.))
                        .max_w(px(440.))
                        .child(
                            text(15., 20., p.text)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Nothing converted yet"),
                        )
                        .child(styled(size::SMALL, p.secondary).text_center().child(hint)),
                )
                .child(
                    Button::secondary("empty-add-files", "Choose files…")
                        .icon(IconName::FolderOpen)
                        .build(p)
                        .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
                )
                .child(
                    mono(11., 14., p.tertiary).child("Images · Video · Audio · PDF · Documents"),
                ),
        )
    }

    fn automations(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let rules = self.app.read(cx).settings.automations.clone();
        let rows: Vec<AnyElement> = rules
            .into_iter()
            .enumerate()
            .map(|(i, rule)| {
                let to = format_by_id(&rule.to).map_or(rule.to.clone(), |f| f.name.to_string());
                let app = self.app.clone();
                let on = rule.enabled;
                let copy = rule.copies_to_clipboard();
                let detail = div()
                    .flex()
                    .flex_col()
                    .gap(px(space::SM))
                    .child(mono(11., 15., p.secondary).child(automation::source_line(&rule)))
                    .child({
                        let app = app.clone();
                        theme::checkbox(
                            SharedString::from(format!("automation-{i}-copy")),
                            "Copy the converted file",
                            copy,
                            p,
                        )
                        .on_click(move |_, _, cx| {
                            app.update(cx, |s, cx| s.set_automation_copy(i, !copy, cx))
                        })
                    });
                theme::row(
                    format!("{} → {to}", rule.name),
                    Some(detail.into_any_element()),
                    theme::switch(SharedString::from(format!("automation-{i}")), on, false, p)
                        .on_click(move |_, _, cx| {
                            app.update(cx, |s, cx| s.set_automation(i, !on, cx))
                        }),
                    p,
                )
                .into_any_element()
            })
            .collect();
        let empty = rows.is_empty();
        div()
            .id("automations")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .px(px(GUTTER))
            .pt(px(space::LG))
            .pb(px(GUTTER))
            .gap(px(space::XL))
            .child(
                styled(size::SMALL, p.secondary)
                    .id("automations-intro")
                    .test_support()
                    .aria_label(
                        "When a screenshot or screen recording appears in its folder, convt converts it.",
                    )
                    .child(
                        "When a screenshot or screen recording appears in its folder, convt converts it. Screenshots follow the folder this computer saves them to, which is not always the Desktop. Only that folder is watched, not folders inside it.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(theme::section_label("Rules", p))
                    .child(if empty {
                        theme::card(p)
                            .p(px(space::LG))
                            .child(styled(size::SMALL, p.secondary).child("No rules yet."))
                    } else {
                        theme::group(rows, p)
                    }),
            )
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
        .child(theme::callout(
            IconName::FolderOpen,
            Tone::Green,
            div()
                .flex()
                .items_center()
                .gap(px(space::LG))
                .child(theme::callout_words(
                    "Turn on the Finder menu",
                    "Right-click Convert with convt needs the Finder extension. Open System Settings, scroll to Extensions, and turn on convt.",
                    p,
                ).flex_1().min_w_0())
                .child(
                    Button::primary("enable-finder", "Open System Settings")
                        .small()
                        .build(p)
                        .on_click(|_, _, cx| cx.open_url(EXTENSION_SETTINGS)),
                ),
            p,
        ))
}

/// The trial or license card at the bottom of the sidebar. Nothing once
/// licensed or in a build that doesn't check licenses.
fn trial_card(state: &State, p: &Palette) -> Option<impl IntoElement + use<>> {
    let (title, left, used, ended, link) = match state {
        State::Unrestricted | State::Licensed(_) => return None,
        State::Trial { started: None, .. } => (
            "Free trial",
            format!("{TRIAL_DAYS} days"),
            0.,
            false,
            format!("Buy license · {LICENSE_PRICE}"),
        ),
        State::Trial { days_left, .. } => (
            "Free trial",
            match days_left {
                1 => "1 day left".to_string(),
                n => format!("{n} days left"),
            },
            (TRIAL_DAYS - days_left) as f32 / TRIAL_DAYS as f32,
            false,
            format!("Buy license · {LICENSE_PRICE}"),
        ),
        State::TrialEnded => (
            "Trial ended",
            String::new(),
            1.,
            true,
            format!("Buy license · {LICENSE_PRICE}"),
        ),
        State::NotCovered(_) => ("Updates ended", String::new(), 1., true, "Renew".into()),
    };
    let button = if ended {
        Button::primary("trial-buy", link)
    } else {
        Button::secondary("trial-buy", link)
    };
    Some(
        div()
            .id("trial-card")
            .test_support()
            .aria_label(SharedString::from(state.summary()))
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(space::MD))
            .rounded(px(radius::CARD))
            .bg(p.surface)
            .border_1()
            .border_color(if ended { p.error_border } else { p.border })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        styled(size::SMALL, if ended { p.error } else { p.text })
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(mono(11., 14., p.secondary).child(left)),
            )
            .child(theme::progress(
                used,
                p.track,
                if ended { p.error } else { p.green },
            ))
            .child(
                button
                    .small()
                    .build(p)
                    .w_full()
                    .on_click(|_, _, cx| cx.open_url(BUY_URL)),
            ),
    )
}

/// The file name, an arrow and the target, "interview.mov → MP4".
fn name_line(input: &std::path::Path, to: &str, p: &Palette) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .min_w_0()
        .child(
            styled(size::BODY, p.text)
                .font_weight(FontWeight::MEDIUM)
                .truncate()
                .child(model::file_name(input)),
        )
        .child(icon(IconName::ArrowRight, 12., p.tertiary).flex_shrink_0())
        .child(theme::badge(to.to_string(), Tone::Neutral, p))
}

/// The status column: an icon and a line tests read by `id`.
fn status_cell(
    id: String,
    label: String,
    glyph: Option<(IconName, Hsla)>,
    el: Div,
) -> impl IntoElement {
    div()
        .id(SharedString::from(id))
        .test_support()
        .aria_label(SharedString::from(label))
        .flex()
        .items_center()
        .justify_end()
        .gap(px(6.))
        // Wide enough for "68% · under a minute left" on one line.
        .w(px(176.))
        .flex_shrink_0()
        .children(glyph.map(|(g, color)| icon(g, 14., color)))
        .child(el.whitespace_nowrap().overflow_hidden().text_ellipsis())
}

fn row(id: String, p: &Palette) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .flex()
        .items_center()
        .gap(px(space::MD))
        .min_h(px(60.))
        .px(px(10.))
        .py(px(10.))
        .rounded(px(radius::CARD))
        .hover(|s| s.bg(p.hover))
}

/// The row action: Show, Retry, Stop or Remove.
fn action_cell(action: Option<AnyElement>) -> Div {
    div()
        .w(px(84.))
        .flex_shrink_0()
        .flex()
        .justify_end()
        .children(action)
}

/// A running or waiting job.
fn active_row(
    entry: &Entry,
    now: Instant,
    app: &Entity<AppState>,
    p: &Palette,
) -> impl IntoElement {
    let id = entry.id;
    let (status, el, action, glyph) = match entry.status {
        Status::Running(progress) => {
            let mut status = match progress {
                Some(f) => format!("{:.0}%", f * 100.),
                None => "Converting".into(),
            };
            if let Some(left) = entry.remaining(now) {
                status = format!("{status} · {}", time_left(left));
            }
            let el = mono(11., 14., p.secondary).child(status.clone());
            (status, el, "Stop", None)
        }
        _ => {
            let el = styled(size::SMALL, p.tertiary).child("Waiting");
            (
                "Waiting".to_string(),
                el,
                "Remove",
                Some((IconName::Loader, p.tertiary)),
            )
        }
    };
    let bar = match entry.status {
        Status::Running(f) => Some(div().w_full().max_w(px(380.)).child(theme::progress(
            f.unwrap_or(0.),
            p.track,
            p.green,
        ))),
        _ => None,
    };
    let app = app.clone();
    row(format!("job-{id}"), p)
        .child(theme::thumbnail(&entry.input, 48., 36., p))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(8.))
                .child(name_line(&entry.input, entry.to.name, p))
                .children(bar),
        )
        .child(status_cell(format!("status-{id}"), status, glyph, el))
        .child(action_cell(Some(
            Button::ghost(
                SharedString::from(format!("{}-{id}", action.to_lowercase())),
                action,
            )
            .small()
            .build(p)
            .on_click(move |_, _, cx| app.update(cx, |s, _| s.cancel(id)))
            .into_any_element(),
        )))
}

/// A finished conversion from history.
fn record_row(record: &Record, app: &Entity<AppState>, p: &Palette) -> impl IntoElement {
    let id = record.id;
    let to = format_by_id(&record.to);
    let to_name = to.map_or(record.to.clone(), |f| f.name.to_string());
    let (status, color, glyph, detail, action): (
        String,
        Hsla,
        (IconName, Hsla),
        Option<Div>,
        Option<AnyElement>,
    ) = match &record.outcome {
        Outcome::Done(outputs) => {
            let sizes = match (file_size(&record.input), outputs.as_slice()) {
                (Some(a), [one]) => {
                    file_size(one).map(|b| format!("{} → {}", human_size(a), human_size(b)))
                }
                (_, many) if many.len() > 1 => Some(format!("{} files", many.len())),
                _ => None,
            };
            let show = outputs.first().cloned().map(|path| {
                Button::ghost(SharedString::from(format!("show-{id}")), "Show")
                    .small()
                    .build(p)
                    .on_click(move |_, _, cx| cx.reveal_path(&path))
                    .into_any_element()
            });
            (
                format!("Done · {}", Local::at(record.finished_at).time()),
                p.secondary,
                (IconName::CircleCheck, p.green),
                sizes.map(|s| mono(11., 14., p.tertiary).child(s)),
                show,
            )
        }
        Outcome::Failed(message) => (
            "Failed".into(),
            p.error,
            (IconName::CircleX, p.error),
            Some(styled(size::SMALL, p.error).child(message.clone())),
            retry(record, app, p),
        ),
        Outcome::Cancelled => (
            "Cancelled".into(),
            p.tertiary,
            (IconName::Ban, p.tertiary),
            None,
            retry(record, app, p),
        ),
    };
    let input_exists = record.input.exists();
    row(format!("record-{id}"), p)
        .child(theme::thumbnail(
            match &record.outcome {
                Outcome::Done(out) if !input_exists => out.first().unwrap_or(&record.input),
                _ => &record.input,
            },
            48.,
            36.,
            p,
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(4.))
                .child(name_line(&record.input, &to_name, p))
                .children(detail),
        )
        .child(status_cell(
            format!("record-status-{id}"),
            status.clone(),
            Some(glyph),
            styled(size::SMALL, color)
                .font_weight(FontWeight::MEDIUM)
                .child(status),
        ))
        .child(action_cell(action))
}

fn retry(record: &Record, app: &Entity<AppState>, p: &Palette) -> Option<AnyElement> {
    let to = format_by_id(&record.to)?;
    format_by_extension(&record.input)?;
    let input = record.input.clone();
    let setup = record.setup.clone();
    let app = app.clone();
    Some(
        Button::ghost(SharedString::from(format!("retry-{}", record.id)), "Retry")
            .icon(IconName::RotateCw)
            .small()
            .build(p)
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
        let header = self.header(&p, cx);
        let main = match self.page {
            Page::Activity => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(header)
                .child(self.defaults_bar(&p, cx))
                .child(self.activity(&p, cx)),
            Page::Automations => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(header.border_b_1().border_color(p.hairline))
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
