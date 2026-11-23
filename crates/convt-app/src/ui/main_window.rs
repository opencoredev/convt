//! The main window: Activity (the composer for files dropped or added here,
//! running jobs and history in one list), Automations and Settings.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::theme::IconName;
use convt_core::{format_by_extension, format_by_id};
use convt_license::client::{BUY_URL, State, TRIAL_DAYS};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::quick::{Dismissed, QuickView};
use super::settings_window::SettingsView;
use super::theme::{self, Button, Palette, Tone, icon, mono, radius, size, space, styled, text};
use super::{LICENSE_PRICE, SettingsTab, file_size, friendly_error, human_size, time_left};
use crate::automation::{self, WatchStatus};
use crate::clock::Local;
use crate::history::{Outcome, Record};
use crate::jobs::{Entry, Status};
use crate::model::{self, AppState};
use crate::request::Request;
use crate::settings::{Automation, WatchKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Activity,
    Automations,
    Presets,
    Menu,
    Settings,
}

/// The sidebar's width.
const SIDEBAR: f32 = 216.;
/// The left and right margin of the content column.
const GUTTER: f32 = 24.;
/// How long a row's Copy button says Copied.
const COPIED_FOR: Duration = Duration::from_millis(1600);

/// State of the inline "+ New rule" builder on Automations.
#[derive(Debug, Clone)]
pub(super) struct RuleDraft {
    /// Which source kind the rule matches (`"HEIC"`, `"Screenshots"`, etc.).
    pub kind_index: usize,
    /// Target format id (`"webp"`, `"jpeg"`, `"png"`, `"mp4"`, `"mp3"`, `"pdf"`).
    pub to: &'static str,
    /// Explicit watched folder, if the user picked one.
    pub folder: Option<PathBuf>,
    pub copy: bool,
}

impl Default for RuleDraft {
    fn default() -> Self {
        Self {
            kind_index: 0,
            to: "webp",
            folder: None,
            copy: false,
        }
    }
}

/// Presets for what a new automation rule matches.
const RULE_KINDS: [(&str, &str, WatchKind, &str); 5] = [
    ("HEIC", "HEIC photos", WatchKind::Folder, "Downloads"),
    ("Images", "Any image", WatchKind::Folder, "Downloads"),
    ("Video", "Any video", WatchKind::Folder, "Downloads"),
    (
        "Screenshots",
        "Screenshots",
        WatchKind::Screenshot,
        "Screenshots",
    ),
    (
        "Screen recordings",
        "Recordings",
        WatchKind::Recording,
        "Screen recordings",
    ),
];

const RULE_TARGETS: [(&str, &str); 6] = [
    ("webp", "WebP"),
    ("jpeg", "JPEG"),
    ("png", "PNG"),
    ("mp4", "MP4"),
    ("mp3", "MP3"),
    ("pdf", "PDF"),
];

pub struct MainView {
    app: Entity<AppState>,
    pub(super) page: Page,
    /// The Settings view (powers Presets, Right-click menu, and Settings).
    pub(super) settings: Entity<SettingsView>,
    /// Quick convert for files dropped or added here, at the top of
    /// Activity until its conversion starts or it is closed.
    pub(super) composer: Option<Entity<QuickView>>,
    /// The inline "+ New rule" composer on Automations.
    pub(super) new_rule: Option<RuleDraft>,
    /// The record whose Copy was clicked last, while its button says Copied.
    pub(super) copied: Option<i64>,
    /// Failed records showing the engine's full message under the summary.
    pub(super) details_open: HashSet<i64>,
    handle: AnyWindowHandle,
    _composer: Option<Subscription>,
    _copied: Option<Task<()>>,
    _observe: Subscription,
    _appearance: Subscription,
}

impl MainView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = cx.new(|cx| SettingsView::new_embedded(app.clone(), window, cx));
        #[cfg(all(debug_assertions, not(test)))]
        dev_stage(window, cx);
        Self {
            _observe: cx.observe(&app, |_, _, cx| cx.notify()),
            _appearance: theme::observe_appearance(window, cx),
            app,
            page: Page::Activity,
            settings,
            composer: None,
            new_rule: None,
            copied: None,
            details_open: HashSet::new(),
            handle: window.window_handle(),
            _composer: None,
            _copied: None,
        }
    }

    pub fn set_page(&mut self, page: Page, cx: &mut Context<Self>) {
        if matches!(page, Page::Settings | Page::Menu | Page::Presets) && self.page != page {
            // Status is read offline; it may have changed through the CLI.
            self.app.update(cx, |s, cx| s.refresh_pack(cx));
        }
        match page {
            Page::Presets => self
                .settings
                .update(cx, |s, cx| s.set_tab(SettingsTab::Presets, cx)),
            Page::Menu => self
                .settings
                .update(cx, |s, cx| s.set_tab(SettingsTab::Menu, cx)),
            Page::Settings => {
                let current = self.settings.read(cx).tab;
                if !matches!(current, SettingsTab::General | SettingsTab::License) {
                    self.settings
                        .update(cx, |s, cx| s.set_tab(SettingsTab::General, cx));
                }
            }
            _ => {}
        }
        self.page = page;
        cx.notify();
    }

    /// Shows the Settings tab, routing Menu and Presets to their top-level
    /// sidebar pages.
    #[cfg(test)]
    pub fn show_settings(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        let page = match tab {
            SettingsTab::Presets => Page::Presets,
            SettingsTab::Menu => Page::Menu,
            SettingsTab::General | SettingsTab::License => Page::Settings,
        };
        self.set_page(page, cx);
        self.settings.update(cx, |s, cx| s.set_tab(tab, cx));
    }

    /// Opens the composer for the files at the top of Activity, as a
    /// right-click without a target opens Quick convert, so the user picks
    /// the format, or adds them to the composer already open. Done once this
    /// update is over, since it needs the window.
    pub fn add(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        let request = add_request(paths);
        let handle = self.handle;
        let this = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                this.update(cx, |this, cx| this.compose(request, window, cx))
            });
        });
    }

    /// [`MainView::add`], with the window at hand.
    pub(super) fn compose(
        &mut self,
        request: Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Status is read offline; it may have changed through the CLI.
        self.app.update(cx, |s, cx| s.refresh_pack(cx));
        self.page = Page::Activity;
        if let Some(open) = self.composer.clone() {
            // More files for the conversion being set up, which keeps its
            // format, options and folder.
            open.update(cx, |c, cx| c.add_files(&request.files, window, cx));
            let focus = open.read(cx).focus.clone();
            window.focus(&focus, cx);
            cx.notify();
            return;
        }
        let app = self.app.clone();
        let composer = cx.new(|cx| QuickView::new_inline(app, request, window, cx));
        // Enter converts and Escape closes it from the start.
        let focus = composer.read(cx).focus.clone();
        window.focus(&focus, cx);
        self._composer = Some(cx.subscribe(&composer, |this, _, _: &Dismissed, cx| {
            this.composer = None;
            cx.notify();
        }));
        self.composer = Some(composer);
        self.page = Page::Activity;
        cx.notify();
    }

    /// Shows or hides a failed record's full message.
    fn toggle_details(&mut self, id: i64, cx: &mut Context<Self>) {
        if !self.details_open.remove(&id) {
            self.details_open.insert(id);
        }
        cx.notify();
    }

    /// Opens the full message of each failed conversion for Details, matching
    /// its exact recorded id when known and falling back to the newest failed
    /// record for that input.
    pub(super) fn expand_failures(
        &mut self,
        failures: &[(Option<i64>, PathBuf)],
        cx: &mut Context<Self>,
    ) {
        let state = self.app.read(cx);
        let ids: Vec<i64> = failures
            .iter()
            .filter_map(|(id, input)| {
                id.filter(|id| state.recent.iter().any(|r| r.id == *id))
                    .or_else(|| {
                        state
                            .recent
                            .iter()
                            .filter(|r| {
                                r.input == *input && matches!(r.outcome, Outcome::Failed(_))
                            })
                            .max_by_key(|r| (r.finished_at, r.id))
                            .map(|r| r.id)
                    })
            })
            .collect();
        self.details_open.extend(ids);
        self.set_page(Page::Activity, cx);
    }

    /// Copies a finished conversion's files, and says so on its button for
    /// a moment.
    fn copy(&mut self, id: i64, outputs: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        crate::clipboard::copy_files(outputs, cx);
        self.copied = Some(id);
        cx.notify();
        self._copied = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = this.update(cx, |this, cx| {
                if this.copied == Some(id) {
                    this.copied = None;
                    cx.notify();
                }
            });
        }));
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
        let preset_count = state.valid_presets().count();
        let license = state.license.clone();
        let can_start_trial = state.account.can_start_trial();
        let nav = |id: &'static str,
                   label: &'static str,
                   glyph: IconName,
                   selected: bool,
                   count: Option<(usize, Tone)>| {
            theme::clickable(id, label)
                .aria_selected(selected)
                .flex()
                .items_center()
                .gap(px(10.))
                .h(px(32.))
                .px(px(10.))
                .rounded(px(radius::CONTROL))
                .map(|d| {
                    if selected && p.dark {
                        d.bg(p.selected)
                            .shadow(vec![theme::inset_ring(p.border, 1.)])
                    } else if selected {
                        d.bg(p.surface).shadow(theme::bevel(p.border, p))
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
                        .truncate()
                        .child(label),
                )
                .children(count.map(|(n, tone)| theme::badge(n.to_string(), tone, p)))
        };
        let page = self.page;
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
                18.
            }))
            .child(
                div()
                    .id("brand")
                    .flex()
                    .items_center()
                    .h(px(28.))
                    .px(px(space::SM))
                    .mb(px(space::MD))
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
                            page == Page::Activity,
                            (active > 0).then_some((active, Tone::Green)),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.set_page(Page::Activity, cx))),
                    )
                    .child(
                        nav(
                            "nav-presets",
                            "Presets",
                            IconName::Star,
                            page == Page::Presets,
                            (preset_count > 0).then_some((preset_count, Tone::Neutral)),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.set_page(Page::Presets, cx))),
                    )
                    .child(
                        nav(
                            "nav-menu",
                            "Right-click menu",
                            IconName::RightClick,
                            page == Page::Menu,
                            None,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.set_page(Page::Menu, cx))),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::SM))
                    .children(super::update::sidebar_card(&self.app, p, cx))
                    .children(trial_card(&license, can_start_trial, &self.app, p)),
            )
            .child(
                div()
                    .h(px(1.))
                    .mx(px(space::XS))
                    .my(px(space::SM))
                    .bg(p.hairline),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        nav(
                            "nav-settings",
                            "Settings",
                            IconName::Settings,
                            page == Page::Settings,
                            None,
                        )
                        .h(px(30.))
                        .on_click(cx.listener(|this, _, _, cx| this.set_page(Page::Settings, cx))),
                    )
                    .child(
                        nav("nav-feedback", "Send feedback", IconName::Edit, false, None)
                            .h(px(30.))
                            .on_click(|_, _, cx| super::show_feedback(cx)),
                    ),
            )
    }

    fn header(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let state = self.app.read(cx);
        let (title, subtitle) = match self.page {
            Page::Activity => (
                "Activity",
                match (state.queue.progress_line(), state.recent.len()) {
                    (Some(line), _) => line,
                    (None, 0) => "Nothing converted yet".to_string(),
                    (None, 1) => "1 recent conversion".to_string(),
                    (None, n) => format!("{n} recent conversions"),
                },
            ),
            Page::Automations => (
                "Automations",
                "Watch folders and convert new files automatically".to_string(),
            ),
            Page::Presets => (
                "Presets",
                "Reusable conversion recipes for Quick convert and the CLI".to_string(),
            ),
            Page::Menu => (
                "Right-click menu",
                "Choose which formats appear in your file manager menu".to_string(),
            ),
            Page::Settings => (
                "Settings",
                "Preferences, document support, updates and your license".to_string(),
            ),
        };
        let has_config = state.ensure_settings_file_exists_hint();
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
            .when(self.page == Page::Activity, |d| {
                d.child(
                    Button::primary("add-files", "Add files")
                        .icon(IconName::Plus)
                        .build(p)
                        .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
                )
            })
            .when(self.page == Page::Automations, |d| {
                let open = self.new_rule.is_some();
                d.when(has_config, |d| {
                    d.child(
                        Button::secondary("open-automations-config", "Edit TOML")
                            .icon(IconName::Edit)
                            .build(p)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(path) =
                                    this.app.update(cx, |s, _| s.ensure_settings_file())
                                {
                                    cx.open_with_system(&path);
                                }
                            })),
                    )
                })
                .child(
                    Button::primary("new-automation", if open { "Close" } else { "New rule" })
                        .icon(if open {
                            IconName::Close
                        } else {
                            IconName::Plus
                        })
                        .build(p)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.new_rule = if this.new_rule.is_some() {
                                None
                            } else {
                                Some(RuleDraft::default())
                            };
                            cx.notify();
                        })),
                )
            })
    }

    /// What goes above the list: the way back to Finder setup.
    fn notices(&self, p: &Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let state = self.app.read(cx);
        let finder_off = state.finder_on == Some(false);
        let mut notices: Vec<AnyElement> = super::pack::activity_notice(&state.pack, p)
            .into_iter()
            .collect();
        // The offer of the pack; the download itself is the notice above.
        // The composer makes its own offer for the files in it.
        let app = self.app.clone();
        if self.composer.is_none() {
            notices.extend(super::pack::activity_card(
                &self.app,
                move |_, cx| {
                    app.update(cx, |s, cx| {
                        s.update_settings(|s| s.pack_hint_dismissed = true, cx)
                    })
                },
                p,
                cx,
            ));
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
        // Rows by day, newest first; each day with finished rows can be
        // cleared on its own.
        let mut days: Vec<(String, Vec<AnyElement>, Vec<i64>)> = Vec::new();
        let day = |label: String, days: &mut Vec<(String, Vec<AnyElement>, Vec<i64>)>| {
            if days.last().map(|(l, ..)| l) != Some(&label) {
                days.push((label, Vec::new(), Vec::new()));
            }
            days.len() - 1
        };
        for entry in &active {
            let i = day("Today".into(), &mut days);
            days[i]
                .1
                .push(active_row(entry, now, &self.app, p).into_any_element());
        }
        let records = state.recent.clone();
        for record in &records {
            let i = day(Local::at(record.finished_at).day_label(&today), &mut days);
            days[i]
                .1
                .push(self.record_row(record, p, cx).into_any_element());
            days[i].2.push(record.id);
        }
        for (label, day_rows, ids) in days {
            let first = rows.is_empty();
            let id = format!("clear-{}", label.to_lowercase().replace([' ', ','], "-"));
            let app = self.app.clone();
            let clear = (!ids.is_empty()).then(|| {
                theme::text_button(SharedString::from(id), "Clear", p.tertiary, 12.)
                    .on_click(move |_, _, cx| app.update(cx, |s, cx| s.clear_records(&ids, cx)))
            });
            rows.push(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(2.))
                    .pt(px(if first { 0. } else { 20. }))
                    .pb(px(space::SM))
                    .child(
                        styled(size::CAPTION, p.tertiary)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(label),
                    )
                    .children(clear)
                    .into_any_element(),
            );
            rows.push(theme::group(day_rows, p).into_any_element());
        }
        let finder_off = self.app.read(cx).finder_on == Some(false);
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
        let composer = self.composer.clone().map(|composer| {
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .px(px(GUTTER))
                .pt(px(space::LG))
                .child(composer)
        });
        if rows.is_empty() {
            // Scrolls too: notices over the empty state can outgrow a short
            // window, and Choose files must stay reachable.
            return div()
                .id("activity")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(notices)
                .map(|d| match composer {
                    Some(composer) => d.child(composer.pb(px(GUTTER))),
                    None => d.child(self.empty_state(finder_off, p, cx)),
                })
                .into_any_element();
        }
        let strip = composer.is_none().then(|| drop_strip(p, cx));
        div()
            .id("activity")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(notices)
            .children(composer)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px(px(GUTTER))
                    .pt(px(space::LG))
                    .pb(px(GUTTER))
                    .children(strip.map(|s| s.mb(px(space::LG))))
                    .children(rows),
            )
            .into_any_element()
    }

    fn empty_state(&self, finder_off: bool, p: &Palette, cx: &mut Context<Self>) -> Div {
        let hint = if finder_off {
            "Turn on the Finder menu above to convert from a right-click.".to_string()
        } else {
            format!(
                "Or right-click any file in {} and pick a format.",
                theme::file_manager()
            )
        };
        // Fills the page when it fits; never shrinks below its content.
        div()
            .flex()
            .flex_grow(1.)
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .p(px(GUTTER))
            .child(
                div()
                    .id("empty")
                    .test_support()
                    .aria_label("Nothing converted yet.")
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(space::LG))
                    .w_full()
                    .max_w(px(520.))
                    .py(px(40.))
                    .px(px(space::XXL))
                    .rounded(px(radius::PANEL))
                    .bg(p.recessed)
                    .shadow(theme::well(p))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(48.))
                            .rounded(px(radius::CARD))
                            .bg(p.surface)
                            .shadow(theme::bevel(p.border, p))
                            .child(theme::mark(24., p)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(4.))
                            .max_w(px(400.))
                            .child(
                                text(16., 22., p.text)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Drop files to convert"),
                            )
                            .child(styled(size::BODY, p.secondary).text_center().child(hint)),
                    )
                    .child(
                        Button::secondary("empty-add-files", "Choose files…")
                            .icon(IconName::FolderOpen)
                            .build(p)
                            .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
                    )
                    .child(
                        mono(11., 14., p.tertiary)
                            .id("empty-formats")
                            .child("HEIC · PNG · WebP · MP4 · MOV · MP3 · PDF"),
                    ),
            )
    }

    /// A finished conversion from history.
    fn record_row(&self, record: &Record, p: &Palette, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = record.id;
        let app = &self.app;
        let to = format_by_id(&record.to);
        let to_name = to.map_or(record.to.clone(), |f| f.name.to_string());
        let (status, color, glyph, detail, actions): (
            String,
            Hsla,
            (IconName, Hsla),
            Option<Div>,
            Vec<AnyElement>,
        ) =
            match &record.outcome {
                Outcome::Done(outputs) => {
                    // The sizes kept when it finished, so a file moved or
                    // edited since doesn't change what the row says.
                    let before = record.input_bytes.or_else(|| file_size(&record.input));
                    let after = record.output_bytes.or_else(|| match outputs.as_slice() {
                        [one] => file_size(one),
                        _ => None,
                    });
                    let detail = match (before, outputs.as_slice()) {
                        (Some(a), [_]) => after.map(|b| {
                            let (delta, tone) = size_delta(a, b);
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(mono(11., 14., p.tertiary).child(format!(
                                    "{} → {}",
                                    human_size(a),
                                    human_size(b)
                                )))
                                .child(
                                    div()
                                        .id(SharedString::from(format!("delta-{id}")))
                                        .test_support()
                                        .aria_label(SharedString::from(delta.clone()))
                                        .child(theme::badge(delta, tone, p)),
                                )
                        }),
                        (_, many) if many.len() > 1 => {
                            Some(div().child(mono(11., 14., p.tertiary).child(match after {
                                Some(b) => format!("{} files · {}", many.len(), human_size(b)),
                                None => format!("{} files", many.len()),
                            })))
                        }
                        _ => None,
                    };
                    let mut actions: Vec<AnyElement> = Vec::new();
                    if !outputs.is_empty() {
                        let copied = self.copied == Some(id);
                        let outs = outputs.clone();
                        actions.push(
                            Button::ghost(
                                SharedString::from(format!("copy-{id}")),
                                if copied { "Copied" } else { "Copy" },
                            )
                            .small()
                            .build(p)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.copy(id, &outs, window, cx)
                            }))
                            .into_any_element(),
                        );
                    }
                    if let [one] = outputs.as_slice() {
                        let path = one.clone();
                        actions.push(
                            Button::ghost(SharedString::from(format!("open-{id}")), "Open")
                                .small()
                                .build(p)
                                .on_click(move |_, _, cx| cx.open_with_system(&path))
                                .into_any_element(),
                        );
                    }
                    if let Some(path) = outputs.first().cloned() {
                        actions.push(
                            Button::secondary(SharedString::from(format!("show-{id}")), "Show")
                                .small()
                                .build(p)
                                .on_click(move |_, _, cx| cx.reveal_path(&path))
                                .into_any_element(),
                        );
                    }
                    (
                        format!("Done · {}", Local::at(record.finished_at).time()),
                        p.secondary,
                        (IconName::CircleCheck, p.green),
                        detail,
                        actions,
                    )
                }
                Outcome::Failed(message) => (
                    "Failed".into(),
                    p.error,
                    (IconName::CircleX, p.error),
                    Some(self.failure(id, message, p, cx)),
                    retry(record, app, p).into_iter().collect(),
                ),
                Outcome::Cancelled => (
                    "Cancelled".into(),
                    p.tertiary,
                    (IconName::Ban, p.tertiary),
                    None,
                    retry(record, app, p).into_iter().collect(),
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
                    .child(name_line(
                        &record.input,
                        &to_name,
                        record
                            .setup
                            .as_ref()
                            .is_some_and(|s| s.cloud)
                            .then(|| format!("record-cloud-{id}")),
                        p,
                    ))
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
            .child(action_cell(actions))
    }

    /// A failed record's reason on one line, and the engine's own message
    /// behind Details when the line is a summary of it.
    fn failure(&self, id: i64, raw: &str, p: &Palette, cx: &mut Context<Self>) -> Div {
        let summary = friendly_error(raw);
        let raw = raw.trim().to_string();
        let open = self.details_open.contains(&id);
        let toggle = (summary != raw).then(|| {
            theme::text_button(
                SharedString::from(format!("error-details-{id}")),
                if open { "Hide details" } else { "Details" },
                p.secondary,
                12.,
            )
            .aria_expanded(open)
            .flex_shrink_0()
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_details(id, cx)))
        });
        let full = (open && toggle.is_some()).then(|| {
            div()
                .id(SharedString::from(format!("error-raw-{id}")))
                .test_support()
                .aria_label(SharedString::from(raw.clone()))
                .max_h(px(160.))
                .overflow_y_scroll()
                .px(px(10.))
                .py(px(8.))
                .rounded(px(radius::SM))
                .bg(p.recessed)
                .border_1()
                .border_color(p.border)
                .child(mono(11., 16., p.secondary).child(raw))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::SM))
                    .min_w_0()
                    .child(
                        div()
                            .id(SharedString::from(format!("error-summary-{id}")))
                            .test_support()
                            .aria_label(SharedString::from(summary.clone()))
                            .min_w_0()
                            .child(styled(size::SMALL, p.error).truncate().child(summary)),
                    )
                    .children(toggle),
            )
            .children(full)
    }

    fn pick_automation_folder(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Watch folder".into()),
        });
        let this = cx.entity().downgrade();
        let app = self.app.clone();
        cx.spawn(async move |_, cx| {
            if let Ok(Ok(Some(mut dirs))) = picked.await
                && let Some(dir) = dirs.pop()
            {
                match index {
                    Some(i) => {
                        app.update(cx, |s, cx| s.set_automation_folder(i, dir, cx));
                    }
                    None => {
                        let _ = this.update(cx, |this, cx| {
                            if let Some(draft) = &mut this.new_rule {
                                draft.folder = Some(dir);
                                cx.notify();
                            }
                        });
                    }
                }
            }
        })
        .detach();
    }

    fn save_new_rule(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.new_rule.take() else {
            return;
        };
        let (name, _label, watch, default_source) = RULE_KINDS
            .get(draft.kind_index)
            .copied()
            .unwrap_or(RULE_KINDS[0]);
        let source = draft
            .folder
            .as_deref()
            .map(super::tilde)
            .unwrap_or_else(|| default_source.to_string());
        let rule = Automation {
            name: name.to_string(),
            to: draft.to.to_string(),
            source,
            detail: if draft.copy {
                "copy to clipboard".into()
            } else {
                "save beside original".into()
            },
            enabled: true,
            watch: Some(watch),
            folder: draft.folder,
            copy_to_clipboard: Some(draft.copy),
        };
        self.app.update(cx, |s, cx| s.add_automation(rule, cx));
        cx.notify();
    }

    fn new_rule_card(
        &self,
        draft: &RuleDraft,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (_, _, _, default_source) = RULE_KINDS
            .get(draft.kind_index)
            .copied()
            .unwrap_or(RULE_KINDS[0]);
        let folder_label = draft
            .folder
            .as_deref()
            .map(super::tilde)
            .unwrap_or_else(|| format!("Default ({default_source})"));
        let pill = |id: SharedString, label: &'static str, on: bool| {
            theme::clickable(id, label)
                .aria_selected(on)
                .flex()
                .items_center()
                .h(px(28.))
                .px(px(11.))
                .rounded(px(radius::CONTROL))
                .bg(if on { p.green_tint } else { p.surface })
                .shadow(vec![theme::inset_ring(
                    if on { p.green } else { p.border },
                    if on { 1.5 } else { 1. },
                )])
                .when(!on, |d| d.hover(|s| s.bg(p.recessed)))
                .child(
                    styled(size::SMALL, if on { p.green_text } else { p.text })
                        .font_weight(FontWeight::MEDIUM)
                        .child(label),
                )
        };
        let field_row = |label: &'static str, control: AnyElement| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(space::LG))
                .px(px(space::LG))
                .py(px(12.))
                .child(
                    styled(size::BODY, p.text)
                        .font_weight(FontWeight::MEDIUM)
                        .w(px(108.))
                        .flex_shrink_0()
                        .child(label),
                )
                .child(div().flex_1().min_w_0().child(control))
        };
        let kinds = div()
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .children(
                RULE_KINDS
                    .into_iter()
                    .enumerate()
                    .map(|(i, (_, label, ..))| {
                        pill(
                            SharedString::from(format!("new-rule-kind-{i}")),
                            label,
                            draft.kind_index == i,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(d) = &mut this.new_rule {
                                d.kind_index = i;
                                cx.notify();
                            }
                        }))
                    }),
            )
            .into_any_element();
        let targets = div()
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .children(RULE_TARGETS.into_iter().map(|(id, label)| {
                pill(
                    SharedString::from(format!("new-rule-to-{id}")),
                    label,
                    draft.to == id,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(d) = &mut this.new_rule {
                        d.to = id;
                        cx.notify();
                    }
                }))
            }))
            .into_any_element();
        let folder_control = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(space::SM))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .min_w_0()
                    .child(icon(IconName::Folder, 14., p.tertiary))
                    .child(mono(12., 16., p.secondary).truncate().child(folder_label)),
            )
            .child(
                Button::secondary("new-rule-folder", "Choose folder…")
                    .small()
                    .build(p)
                    .on_click(cx.listener(|this, _, _, cx| this.pick_automation_folder(None, cx))),
            )
            .into_any_element();
        let copy = draft.copy;
        let after_control =
            theme::checkbox("new-rule-copy", "Copy converted file to clipboard", copy, p)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(d) = &mut this.new_rule {
                        d.copy = !d.copy;
                        cx.notify();
                    }
                }))
                .into_any_element();
        let rows = vec![
            field_row("Files", kinds).into_any_element(),
            field_row("Convert to", targets).into_any_element(),
            field_row("Folder", folder_control).into_any_element(),
            field_row("After", after_control).into_any_element(),
        ];
        let footer = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(space::SM))
            .px(px(space::LG))
            .py(px(10.))
            .bg(p.recessed)
            .rounded_b(px(radius::CARD - 1.))
            .border_t_1()
            .border_color(p.hairline)
            .child(
                Button::secondary("cancel-new-automation", "Cancel")
                    .small()
                    .build(p)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.new_rule = None;
                        cx.notify();
                    })),
            )
            .child(
                Button::primary("add-automation", "Add rule")
                    .small()
                    .build(p)
                    .on_click(cx.listener(|this, _, _, cx| this.save_new_rule(cx))),
            );
        div()
            .id("new-rule-card")
            .test_support()
            .pb(px(space::XL))
            .child(theme::section_label("New rule", p))
            .child(theme::group(rows, p).child(footer))
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
                let status = automation::watch_status(&rule);
                let (dot_color, status_text, needs_folder) = match &status {
                    WatchStatus::Folder(dir) if on => {
                        (p.green, format!("Watching · {}", super::tilde(dir)), false)
                    }
                    WatchStatus::Folder(dir) => {
                        (p.tertiary, format!("Paused · {}", super::tilde(dir)), false)
                    }
                    WatchStatus::Clipboard => (
                        p.amber,
                        "Screenshots go to clipboard (no folder to watch)".into(),
                        true,
                    ),
                    WatchStatus::Waiting(msg) => (p.amber, msg.clone(), true),
                    WatchStatus::Skipped(msg) => (p.error, (*msg).to_string(), true),
                };
                let title = format!("{} → {to}", rule.name);
                let title_row = div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        styled(size::BODY, p.text)
                            .font_weight(FontWeight::MEDIUM)
                            .child(rule.name.clone()),
                    )
                    .child(icon(IconName::ArrowRight, 12., p.tertiary))
                    .child(theme::badge(
                        to,
                        if on { Tone::Green } else { Tone::Neutral },
                        p,
                    ));
                let status_row = div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .size(px(6.))
                            .rounded(px(3.))
                            .flex_shrink_0()
                            .bg(dot_color),
                    )
                    .child(mono(11., 15., p.secondary).truncate().child(status_text))
                    .when(needs_folder, |d| {
                        d.child(
                            theme::text_button(
                                SharedString::from(format!("automation-{i}-folder")),
                                "Choose folder…",
                                p.green_text,
                                12.,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.pick_automation_folder(Some(i), cx),
                            )),
                        )
                    });
                let controls_row = div()
                    .flex()
                    .items_center()
                    .gap(px(space::MD))
                    .pt(px(2.))
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
                    })
                    .when(!needs_folder, |d| {
                        d.child(
                            theme::text_button(
                                SharedString::from(format!("automation-{i}-folder")),
                                "Change folder",
                                p.tertiary,
                                12.,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.pick_automation_folder(Some(i), cx),
                            )),
                        )
                    })
                    .child({
                        let app = app.clone();
                        theme::text_button(
                            SharedString::from(format!("remove-automation-{i}")),
                            "Remove",
                            p.tertiary,
                            12.,
                        )
                        .on_click(move |_, _, cx| {
                            app.update(cx, |s, cx| s.remove_automation(i, cx))
                        })
                    });
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(space::LG))
                    .px(px(space::LG))
                    .py(px(14.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap(px(6.))
                            .child(title_row)
                            .child(status_row)
                            .child(controls_row),
                    )
                    .child(
                        theme::switch(
                            SharedString::from(format!("automation-{i}")),
                            title,
                            on,
                            false,
                            p,
                        )
                        .on_click(move |_, _, cx| {
                            app.update(cx, |s, cx| s.set_automation(i, !on, cx))
                        }),
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
            .children(self.new_rule.as_ref().map(|d| self.new_rule_card(d, p, cx)))
            .child(theme::section_label("Rules", p))
            .child(if empty {
                theme::card(p).p(px(space::LG)).child(
                    styled(size::SMALL, p.secondary).child(
                        "No rules yet. Click New rule above to watch a folder automatically.",
                    ),
                )
            } else {
                theme::group(rows, p)
            })
            .child(
                styled(size::SMALL, p.secondary)
                    .id("automations-intro")
                    .test_support()
                    .aria_label(AUTOMATIONS_INTRO)
                    .max_w(px(600.))
                    .px(px(2.))
                    .pt(px(space::SM))
                    .child(AUTOMATIONS_INTRO),
            )
    }
}

/// What the Automations page says above its rules.
const AUTOMATIONS_INTRO: &str = "Each rule watches one folder, not the folders inside it. For \
     screenshots, that's the folder this computer saves them to, which isn't always the Desktop.";

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
                        .on_click(|_, _, cx| crate::finder::open_settings(cx)),
                ),
            p,
        ))
}

/// The trial or license card at the bottom of the sidebar. Nothing once
/// licensed or in a build that doesn't check licenses. `can_start_trial`:
/// the signed-in account can start its Pro trial, which the card offers.
fn trial_card(
    state: &State,
    can_start_trial: bool,
    app: &Entity<AppState>,
    p: &Palette,
) -> Option<impl IntoElement + use<>> {
    let start_trial = can_start_trial && !state.allows_conversion();
    let (title, left, used, ended, link) = match state {
        State::Unrestricted | State::Licensed(_) => return None,
        _ if start_trial => (
            "Pro trial",
            format!("{TRIAL_DAYS} days"),
            0.,
            false,
            "Start free trial".into(),
        ),
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
        State::AccountTrial { days_left, .. } => (
            "Pro trial",
            match days_left {
                1 => "1 day left".to_string(),
                n => format!("{n} days left"),
            },
            ((TRIAL_DAYS - days_left) as f32 / TRIAL_DAYS as f32).clamp(0., 1.),
            false,
            format!("Buy license · {LICENSE_PRICE}"),
        ),
        State::SignInNeeded => (
            "Pro trial",
            "Sign in to start".into(),
            0.,
            false,
            "Sign in".into(),
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
    let button = if start_trial || matches!(state, State::SignInNeeded) {
        Button::primary("trial-buy", link)
    } else if ended {
        Button::brand("trial-buy", link)
    } else {
        Button::secondary("trial-buy", link)
    };
    // Signing in starts from the License tab, which follows the flow.
    let sign_in = !start_trial && matches!(state, State::SignInNeeded);
    let summary = if start_trial {
        format!("Start your {TRIAL_DAYS}-day Pro trial.")
    } else {
        state.summary()
    };
    let app = app.clone();
    Some(
        div()
            .id("trial-card")
            .test_support()
            .aria_label(SharedString::from(summary))
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(space::MD))
            .rounded(px(radius::PANEL))
            .bg(p.surface)
            .shadow({
                let mut s = vec![theme::inset_ring(
                    if ended { p.error_border } else { p.border },
                    1.,
                )];
                s.extend(theme::soft(p));
                s
            })
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
            .child(button.small().build(p).w_full().on_click(move |_, _, cx| {
                if start_trial {
                    app.update(cx, AppState::start_trial)
                } else if sign_in {
                    super::show_settings(SettingsTab::License, cx)
                } else {
                    cx.open_url(BUY_URL)
                }
            })),
    )
}

/// The file name, an arrow and the target, "interview.mov → MP4", then
/// "Cloud" for a job that ran on convt's cloud. `cloud` is that mark's id.
fn name_line(input: &std::path::Path, to: &str, cloud: Option<String>, p: &Palette) -> Div {
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
        .children(cloud.map(|id| {
            div()
                .id(SharedString::from(id))
                .test_support()
                .aria_label("Cloud")
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(4.))
                .pl(px(2.))
                .child(icon(IconName::Cloud, 13., p.tertiary))
                .child(styled(size::SMALL, p.tertiary).child("Cloud"))
        }))
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

/// A row of a day's group.
fn row(id: String, p: &Palette) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .flex()
        .items_center()
        .gap(px(space::MD))
        .min_h(px(60.))
        .px(px(space::MD))
        .py(px(10.))
        .hover(|s| s.bg(p.hover))
}

/// The row actions: Copy, Open and Show, or Retry, Stop or Remove.
/// Fixed width so Done, Failed and Running status columns align vertically.
fn action_cell(actions: Vec<AnyElement>) -> Div {
    div()
        .w(px(164.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_end()
        .gap(px(2.))
        .children(actions)
}

/// How much smaller or larger a conversion made the file: "-78%", "+45%",
/// "4.2× larger" or "23× larger". Smaller is green; at least 20% larger is
/// amber.
pub(super) fn size_delta(before: u64, after: u64) -> (String, Tone) {
    let (a, b) = (before.max(1) as f64, after as f64);
    if b < a && (a - b) / a >= 0.01 {
        let shrunk = ((a - b) / a * 100.).round().min(99.);
        return (format!("-{shrunk:.0}%"), Tone::Green);
    }
    if b > a && (b - a) / a >= 0.01 {
        let times = b / a;
        let tone = if times >= 1.2 {
            Tone::Amber
        } else {
            Tone::Neutral
        };
        let label = if times >= 9.95 {
            format!("{times:.0}× larger")
        } else if times >= 2. {
            format!("{times:.1}× larger")
        } else {
            format!("+{:.0}%", (times - 1.) * 100.)
        };
        return (label, tone);
    }
    ("Same size".into(), Tone::Neutral)
}

/// Debug builds only: `CONVT_DEV_STAGE=<path>` opens the composer with that
/// file at startup, so the Xvfb screenshot harness can capture it without a
/// drop. Not compiled into release builds or tests.
#[cfg(all(debug_assertions, not(test)))]
fn dev_stage(window: &Window, cx: &mut Context<MainView>) {
    let Some(path) = std::env::var_os("CONVT_DEV_STAGE").map(PathBuf::from) else {
        return;
    };
    if !path.exists() {
        return;
    }
    let request = add_request(&[path]);
    let handle = window.window_handle();
    let this = cx.entity().downgrade();
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, cx| {
            this.update(cx, |this, cx| this.compose(request, window, cx))
        });
    });
}

/// What a drop or Add files hands the composer.
fn add_request(paths: &[PathBuf]) -> Request {
    Request {
        files: paths.to_vec(),
        ..Request::default()
    }
}

/// The slim drop zone above the list once Activity has rows.
fn drop_strip(p: &Palette, cx: &mut Context<MainView>) -> theme::Clickable {
    let label = "Drop files here to convert, or click Add files";
    let hover = p.hover;
    theme::clickable("drop-strip", label)
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(10.))
        .h(px(44.))
        .px(px(space::LG))
        .rounded(px(radius::CARD))
        .bg(p.recessed)
        .shadow(theme::well(p))
        .hover(move |s| s.bg(hover))
        .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx)))
        .child(icon(IconName::FolderOpen, 15., p.tertiary))
        .child(styled(size::SMALL, p.secondary).truncate().child(label))
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
                .child(name_line(
                    &entry.input,
                    entry.to.name,
                    entry.setup.cloud.then(|| format!("job-cloud-{id}")),
                    p,
                ))
                .children(bar),
        )
        .child(status_cell(format!("status-{id}"), status, glyph, el))
        .child(action_cell(vec![
            Button::secondary(
                SharedString::from(format!("{}-{id}", action.to_lowercase())),
                action,
            )
            .small()
            .build(p)
            .on_click(move |_, _, cx| app.update(cx, |s, _| s.cancel(id)))
            .into_any_element(),
        ]))
}

fn retry(record: &Record, app: &Entity<AppState>, p: &Palette) -> Option<AnyElement> {
    let to = format_by_id(&record.to)?;
    format_by_extension(&record.input)?;
    let input = record.input.clone();
    let setup = record.setup.clone();
    let app = app.clone();
    Some(
        Button::secondary(SharedString::from(format!("retry-{}", record.id)), "Retry")
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
        let body = match self.page {
            Page::Activity => self.activity(&p, cx),
            Page::Automations => self.automations(&p, cx).into_any_element(),
            // Presets, Right-click menu and Settings are rendered by the
            // embedded SettingsView; only Settings shows the General/License
            // sub-tabs below the header.
            Page::Presets | Page::Menu | Page::Settings => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(self.settings.clone())
                .into_any_element(),
        };
        let header = if self.page == Page::Settings {
            header
        } else {
            header.border_b_1().border_color(p.hairline)
        };
        div()
            .id("main")
            .flex()
            .size_full()
            .bg(p.window)
            .font_family(theme::SANS)
            .text_color(p.text)
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.bg(p.green_tint))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                if !paths.paths().is_empty() {
                    this.compose(add_request(paths.paths()), window, cx);
                }
            }))
            .child(sidebar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(header)
                    .child(body),
            )
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
