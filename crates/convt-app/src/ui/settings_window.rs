//! Settings: General, Menu, Presets and License. It is a page of the main
//! window ([`SettingsView::new_embedded`]) and, for the tray, links and the
//! Settings… menu item, a window of its own.

use convt_core::{
    Category, FORMATS, Format, MENU_KINDS, MenuChoices, Options, Preset, category_key, format_by_id,
};
use convt_license::client::{State, TRIAL_DAYS};
use gpui_kit::component::input::InputState;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::IconName;

use super::theme::{
    self, Button, Choice, Palette, Tone, icon, mono, primary_button, radius, secondary_button,
    size, space, styled, text_button,
};
use super::{LICENSE_PRICE, describe, error_text, open_folder};
use crate::account::masked_email;
use crate::model::AppState;
use crate::settings::{Settings, auto_concurrency};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Menu,
    Presets,
    License,
}

const TABS: [(SettingsTab, &str, &str, IconName); 4] = [
    (
        SettingsTab::General,
        "tab-general",
        "General",
        IconName::Settings,
    ),
    (SettingsTab::Menu, "tab-menu", "Menu", IconName::RightClick),
    (
        SettingsTab::Presets,
        "tab-presets",
        "Presets",
        IconName::Star,
    ),
    (
        SettingsTab::License,
        "tab-license",
        "License",
        IconName::CircleCheck,
    ),
];

/// Space between the window edge and the settings content.
const GUTTER: f32 = 24.;
/// Space between the General tab's sections and above the first.
const GENERAL_GAP: f32 = 18.;
/// The width of the dropdowns in General, so they line up.
const SELECT_WIDTH: f32 = 200.;

/// The dropdown that is open, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    Output,
    Jobs,
}

pub struct SettingsView {
    app: Entity<AppState>,
    pub(super) tab: SettingsTab,
    open: Option<Open>,
    /// Formats some input can become, for the preset form.
    outputs: Vec<&'static Format>,
    /// What each kind of file can become, for the Menu tab, and the
    /// registry rebuild it was worked out for.
    menu_options: Vec<(Category, Vec<&'static Format>)>,
    menu_options_generation: u64,
    /// Which file kind is selected in the Right-click menu designer.
    menu_kind: Category,
    /// Which target group is shown in the New preset form (0 = Popular).
    preset_group: usize,
    pub(super) preset_name: Entity<InputState>,
    pub(super) preset_quality: Entity<InputState>,
    pub(super) preset_max_size: Entity<InputState>,
    pub(super) preset_to: Option<&'static Format>,
    pub(super) preset_error: Option<String>,
    /// The preset loaded into the form, if the user is editing one.
    pub(super) editing: Option<String>,
    pub(super) license_key: Entity<InputState>,
    pub(super) license_error: Option<String>,
    /// What the last license action did, such as "License activated".
    pub(super) license_notice: Option<String>,
    /// Remove was clicked once under Documents; the row asks again.
    pub(super) confirm_remove_pack: bool,
    scroll: ScrollHandle,
    /// Drawn as a page of the main window, under its header: no title bar,
    /// and the tabs on the page rather than on a toolbar.
    pub(super) embedded: bool,
    _observe: Subscription,
    _appearance: Subscription,
}

impl SettingsView {
    /// Settings as a page of the main window.
    pub fn new_embedded(
        app: Entity<AppState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            embedded: true,
            ..Self::new(app, window, cx)
        }
    }

    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let registry = app.read(cx).registry.clone();
        let menu_options_generation = app.read(cx).registry_generation;
        // What right-click menus can offer, the pack's targets included
        // while it can be downloaded, as Finder and the Linux menus do.
        let menu_options = kinds_and_targets(app.read(cx).menu_registry());
        let outputs = FORMATS
            .iter()
            .filter(|f| {
                FORMATS
                    .iter()
                    .any(|from| registry.targets(from).contains(f))
            })
            .collect();
        let input = |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        Self {
            _observe: cx.observe(&app, |this, app, cx| {
                // Document support was installed or removed.
                let state = app.read(cx);
                if state.registry_generation != this.menu_options_generation {
                    this.menu_options_generation = state.registry_generation;
                    this.menu_options = kinds_and_targets(state.menu_registry());
                }
                cx.notify()
            }),
            _appearance: theme::observe_appearance(window, cx),
            app,
            tab: SettingsTab::General,
            open: None,
            outputs,
            menu_kind: menu_options
                .first()
                .map_or(Category::Image, |(kind, _)| *kind),
            menu_options,
            menu_options_generation,
            preset_group: 0,
            preset_name: input("e.g. web", window, cx),
            preset_quality: input("e.g. 82", window, cx),
            preset_max_size: input("e.g. 1600", window, cx),
            preset_to: None,
            preset_error: None,
            editing: None,
            license_key: input("License key", window, cx),
            license_error: None,
            license_notice: None,
            confirm_remove_pack: false,
            scroll: ScrollHandle::new(),
            embedded: false,
        }
    }

    pub fn set_tab(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        if tab == SettingsTab::Menu {
            self.app.update(cx, |s, cx| s.reload_menu(cx));
        }
        self.tab = tab;
        self.open = None;
        cx.notify();
    }

    /// Shows General scrolled to the end, where Updates sits above the
    /// network list.
    pub fn reveal_updates(&mut self, cx: &mut Context<Self>) {
        self.set_tab(SettingsTab::General, cx);
        self.scroll.scroll_to_bottom();
    }

    /// Shows the License tab, filling in `key` if given.
    pub fn fill_license(
        &mut self,
        key: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab = SettingsTab::License;
        if let Some(key) = key {
            self.license_key
                .update(cx, |s, cx| s.set_value(key, window, cx));
            self.license_error = None;
            self.license_notice = None;
        }
        cx.notify();
    }

    fn change(&self, change: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        self.app.update(cx, |s, cx| s.update_settings(change, cx));
    }

    pub(super) fn activate_license(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.license_key.read(cx).value().trim().to_string();
        if key.is_empty() {
            self.license_error = Some("Paste your license key first.".into());
            self.license_notice = None;
            cx.notify();
            return;
        }
        match self.app.update(cx, |s, cx| s.activate(&key, cx)) {
            Ok(license) => {
                self.license_error = None;
                // A key whose updates ended before this build is kept, but
                // the status above says it can't convert here.
                let allowed = self.app.read(cx).license.allows_conversion();
                self.license_notice = Some(if allowed {
                    format!("License activated for {}.", masked_email(&license.email))
                } else {
                    format!("Saved the license for {}.", masked_email(&license.email))
                });
                self.license_key
                    .update(cx, |s, cx| s.set_value("", window, cx));
            }
            Err(e) => {
                self.license_error = Some(e);
                self.license_notice = None;
            }
        }
        cx.notify();
    }

    pub(super) fn remove_license(&mut self, cx: &mut Context<Self>) {
        match self.app.update(cx, |s, cx| s.deactivate(cx)) {
            Ok(()) => {
                self.license_error = None;
                self.license_notice = Some("Removed the license from this machine.".into());
            }
            Err(e) => {
                self.license_error = Some(format!("The license couldn't be removed: {e}"));
                self.license_notice = None;
            }
        }
        cx.notify();
    }

    fn choose_output(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Save here".into()),
        });
        let app = self.app.clone();
        cx.spawn(async move |_, cx| {
            if let Ok(Ok(Some(mut dirs))) = picked.await
                && let Some(dir) = dirs.pop()
            {
                app.update(cx, |s, cx| {
                    s.update_settings(|s| s.output_dir = Some(dir), cx)
                });
            }
        })
        .detach();
    }

    fn toggle(&mut self, which: Open, cx: &mut Context<Self>) {
        self.open = if self.open == Some(which) {
            None
        } else {
            Some(which)
        };
        cx.notify();
    }

    fn general(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let state = self.app.read(cx);
        let settings = state.settings.clone();
        let errors = state.errors.clone();
        let output = match &settings.output_dir {
            Some(dir) => super::tilde(dir),
            None => "Next to the original".into(),
        };
        let auto = auto_concurrency();
        let jobs = match settings.concurrency {
            Some(n) => n.to_string(),
            None => format!("Auto ({auto})"),
        };
        let mut job_choices = vec![Choice::new("auto", format!("Auto ({auto})"))];
        for n in [1, 2, 4, 8, 16] {
            job_choices.push(Choice::new(n.to_string(), n.to_string()));
        }

        let weak = cx.entity().downgrade();
        let output_select = theme::select(
            "output",
            output,
            if settings.output_dir.is_none() {
                "beside"
            } else {
                ""
            },
            SELECT_WIDTH,
            false,
            self.open == Some(Open::Output),
            vec![
                Choice::new("beside", "Next to the original"),
                Choice::new("choose", "Choose a folder…"),
            ],
            p,
            {
                let weak = weak.clone();
                move |_, cx| {
                    let _ = weak.update(cx, |this, cx| this.toggle(Open::Output, cx));
                }
            },
            {
                let weak = weak.clone();
                move |id, _, cx| {
                    let choose = id == "choose";
                    let _ = weak.update(cx, |this, cx| {
                        this.open = None;
                        if choose {
                            this.choose_output(cx);
                        } else {
                            this.change(|s| s.output_dir = None, cx);
                        }
                        cx.notify();
                    });
                }
            },
        );
        let jobs_select = theme::select(
            "jobs",
            jobs,
            &settings
                .concurrency
                .map_or_else(|| "auto".to_string(), |n| n.to_string()),
            SELECT_WIDTH,
            false,
            self.open == Some(Open::Jobs),
            job_choices,
            p,
            {
                let weak = weak.clone();
                move |_, cx| {
                    let _ = weak.update(cx, |this, cx| this.toggle(Open::Jobs, cx));
                }
            },
            move |id, _, cx| {
                let value = id.parse::<usize>().ok();
                let _ = weak.update(cx, |this, cx| {
                    this.open = None;
                    this.change(|s| s.concurrency = value, cx);
                    cx.notify();
                });
            },
        );

        let app = self.app.clone();
        let reveal_label = format!("Reveal it in {}", theme::file_manager_name());
        let notifications = theme::switch(
            "notifications",
            "Show a notification",
            settings.notifications,
            false,
            p,
        )
        .on_click({
            let app = app.clone();
            let on = settings.notifications;
            move |_, _, cx| app.update(cx, |s, cx| s.update_settings(|s| s.notifications = !on, cx))
        });
        let reveal = theme::switch(
            "reveal",
            reveal_label.clone(),
            settings.reveal_when_done,
            false,
            p,
        )
        .on_click({
            let app = app.clone();
            let on = settings.reveal_when_done;
            move |_, _, cx| {
                app.update(cx, |s, cx| {
                    s.update_settings(|s| s.reveal_when_done = !on, cx)
                })
            }
        });
        let menu_bar = theme::switch(
            "menu-bar-icon",
            "Keep running in the background",
            settings.menu_bar_icon,
            false,
            p,
        )
        .on_click({
            let app = app.clone();
            let on = settings.menu_bar_icon;
            move |_, _, cx| app.update(cx, |s, cx| s.update_settings(|s| s.menu_bar_icon = !on, cx))
        });

        let documents = {
            let weak = cx.entity().downgrade();
            super::pack::settings_row(
                &self.app,
                self.confirm_remove_pack,
                move |click, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.confirm_remove_pack = click == super::pack::Remove::Ask;
                        if click == super::pack::Remove::Confirm {
                            let _ = this.app.update(cx, |s, cx| s.remove_pack(cx));
                        }
                        cx.notify();
                    });
                },
                p,
                cx,
            )
        };

        // Always on macOS; on other platforms only when a test set finder_on.
        let finder = (cfg!(target_os = "macos") || state.finder_on.is_some()).then(|| {
            let (status, button, on) = match state.finder_on {
                Some(true) => (
                    "On. Right-click a file in Finder to convert.",
                    "Manage in System Settings",
                    true,
                ),
                Some(false) => (
                    "Off. Open System Settings, scroll to Extensions, and turn on convt.",
                    "Open System Settings",
                    false,
                ),
                None => (
                    "Open System Settings, scroll to Extensions, and turn on convt.",
                    "Open System Settings",
                    false,
                ),
            };
            theme::row(
                "Finder menu",
                Some(
                    div()
                        .id("finder-status")
                        .test_support()
                        .aria_label(status)
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .size(px(6.))
                                .flex_shrink_0()
                                .rounded(px(3.))
                                .bg(if on { p.green } else { p.toggle_off }),
                        )
                        .child(styled(size::SMALL, p.secondary).child(status))
                        .into_any_element(),
                ),
                Button::secondary("manage-finder", button)
                    .small()
                    .build(p)
                    .on_click(|_, _, cx| crate::finder::open_settings(cx)),
                p,
            )
            .into_any_element()
        });
        // Every platform: the tray icon keeps convt running with no window.
        let menu_bar = theme::row(
            "Keep running in the background",
            Some(
                div()
                    .id("menu-bar-icon-note")
                    .test_support()
                    .aria_label(SharedString::from(background_note()))
                    .child(styled(size::SMALL, p.secondary).child(background_note()))
                    .into_any_element(),
            ),
            menu_bar,
            p,
        )
        .into_any_element();
        let desktop = section(
            if finder.is_some() {
                "Finder and menu bar"
            } else {
                "Background"
            },
            theme::group(finder.into_iter().chain([menu_bar]), p),
            p,
        );

        let automation_count = settings.automations.len();
        let automation_active = settings.automations.iter().filter(|a| a.enabled).count();
        let automation_detail = match (automation_count, automation_active) {
            (0, _) => "Watch folders and convert new files automatically.".to_string(),
            (1, 1) => "1 rule active.".to_string(),
            (1, 0) => "1 rule, paused.".to_string(),
            (total, 0) => format!("{total} rules, all paused."),
            (total, active) if active == total => format!("{total} rules active."),
            (total, active) => format!("{total} rules ({active} active)."),
        };
        let automations_row = theme::row(
            "Folder automations",
            Some(theme::detail(automation_detail, p)),
            Button::secondary("open-automations", "Manage")
                .small()
                .build(p)
                .on_click(|_, _, cx| {
                    super::show_main_page(super::main_window::Page::Automations, cx)
                }),
            p,
        )
        .into_any_element();
        let telemetry_save_error = state.telemetry_save_error.clone();
        let mut telemetry_rows = vec![
            theme::row(
                "Share usage statistics",
                Some(theme::detail(
                    if crate::telemetry::do_not_track() {
                        "Off because DO_NOT_TRACK is set."
                    } else {
                        "No file names, paths or content. Signed-in usage is linked to your account."
                    },
                    p,
                )),
                theme::switch(
                    "telemetry",
                    "Share usage statistics",
                    settings.telemetry && !crate::telemetry::do_not_track(),
                    crate::telemetry::do_not_track(),
                    p,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    if !crate::telemetry::do_not_track() {
                        let had_error = this.app.update(cx, |s, cx| {
                            s.telemetry_notice(!s.settings.telemetry, cx);
                            s.telemetry_save_error.is_some()
                        });
                        if had_error {
                            this.scroll.scroll_to_bottom();
                        }
                    }
                })),
                p,
            )
            .into_any_element(),
        ];
        if let Some(err) = telemetry_save_error {
            let retry = self.app.clone();
            telemetry_rows.push(
                div()
                    .id("telemetry-save-error")
                    .test_support()
                    .aria_label(SharedString::from(err.clone()))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(space::MD))
                    .px(px(space::MD))
                    .py(px(10.))
                    .bg(p.error_tint)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::SM))
                            .flex_1()
                            .min_w_0()
                            .child(icon(IconName::TriangleAlert, 14., p.error))
                            .child(styled(size::SMALL, p.error).child(err)),
                    )
                    .child(
                        Button::secondary("telemetry-retry-save", "Retry save")
                            .small()
                            .build(p)
                            .on_click(move |_, _, cx| {
                                retry.update(cx, |s, cx| s.retry_telemetry_save(cx))
                            }),
                    )
                    .into_any_element(),
            );
        }

        // A little tighter than the other tabs, so the cards above the fold
        // at the window's opening height end on a card's edge.
        div()
            .flex()
            .flex_col()
            .gap(px(GENERAL_GAP))
            .px(px(GUTTER))
            .pt(px(GENERAL_GAP))
            .pb(px(space::XXL))
            .children((!errors.is_empty()).then(|| {
                theme::callout(
                    IconName::TriangleAlert,
                    Tone::Error,
                    div().flex().flex_col().gap(px(4.)).children(
                        errors
                            .into_iter()
                            .map(|e| styled(size::SMALL, p.error).child(e)),
                    ),
                    p,
                )
            }))
            .child(section(
                "Converting",
                theme::group(
                    [
                        theme::row(
                            "Save converted files",
                            Some(theme::detail(
                                "Quick convert can pick a folder each time",
                                p,
                            )),
                            output_select,
                            p,
                        )
                        .into_any_element(),
                        theme::row(
                            "Jobs at once",
                            None,
                            div()
                                .id("concurrency")
                                .test_support()
                                .aria_label(SharedString::from(match settings.concurrency {
                                    Some(n) => n.to_string(),
                                    None => format!("Auto ({auto})"),
                                }))
                                .child(jobs_select),
                            p,
                        )
                        .into_any_element(),
                    ],
                    p,
                ),
                p,
            ))
            .child(section(
                "When a file is done",
                theme::group(
                    [
                        theme::row("Show a notification", None, notifications, p)
                            .into_any_element(),
                        theme::row(reveal_label, None, reveal, p).into_any_element(),
                    ],
                    p,
                ),
                p,
            ))
            .child(desktop)
            .child(section(
                "Automations",
                theme::group([automations_row], p),
                p,
            ))
            .child(section(
                "Documents",
                theme::group([documents.into_any_element()], p),
                p,
            ))
            .child(section(
                "Updates",
                theme::group(super::update::settings_rows(&self.app, p, cx), p),
                p,
            ))
            .child(section("What reaches the network", network(p), p))
            .child(section(
                "Usage statistics",
                theme::group(telemetry_rows, p),
                p,
            ))
    }

    /// Master-detail right-click menu designer: category switcher across the
    /// top, a live native context-menu preview on the left, and an organized
    /// format grid on the right (no wall of tag-cloud pills).
    fn menu(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let state = self.app.read(cx);
        let choices = state.menu.clone();
        let error = state.menu_error.clone();
        let active_kind = self
            .menu_options
            .iter()
            .find(|(k, _)| *k == self.menu_kind)
            .or_else(|| self.menu_options.first())
            .map(|(k, _)| *k)
            .unwrap_or(Category::Image);

        let kind_tabs =
            theme::pill_track_wrap(p).children(self.menu_options.iter().map(|(kind, options)| {
                let kind = *kind;
                let key = category_key(kind);
                let on = kind == active_kind;
                let (picked, _) = shown_and_hidden(&choices, kind, options);
                theme::clickable(
                    SharedString::from(format!("menu-kind-{key}")),
                    kind_label(kind),
                )
                .aria_selected(on)
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.))
                .h(px(28.))
                .pl(px(10.))
                .pr(px(12.))
                .rounded(px(radius::CONTROL))
                .map(|d| {
                    if on {
                        theme::pill_segment_on(d, p)
                    } else {
                        d.hover(|s| s.bg(p.hover))
                    }
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.menu_kind = kind;
                    cx.notify();
                }))
                .child(icon(
                    kind_icon(kind),
                    14.,
                    if on { p.green_text } else { p.tertiary },
                ))
                .child(
                    styled(size::SMALL, if on { p.text } else { p.secondary })
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .child(kind_label(kind)),
                )
                .child(
                    mono(10., 12., if on { p.green_text } else { p.tertiary })
                        .child(picked.len().to_string()),
                )
            }));

        let workspace =
            self.menu_options
                .iter()
                .find(|(k, _)| *k == active_kind)
                .map(|(kind, options)| {
                    let kind = *kind;
                    let key = category_key(kind);
                    let (picked, _hidden) = shown_and_hidden(&choices, kind, options);
                    let custom = choices.get(kind).is_some();
                    let summary = format!("Offers {}", list_names(&picked));
                    let gif_note = (kind == Category::Image && !custom)
                        .then_some("GIFs offer MP4, WebP and PNG until you customize Images.");
                    let app = self.app.clone();

                    // Left column: live Finder/Explorer submenu preview.
                    let can_remove = picked.len() > 1;
                    let preview_rows: Vec<AnyElement> = picked
                        .iter()
                        .map(|&to| {
                            let app = self.app.clone();
                            let options = options.clone();
                            let group = SharedString::from(format!("menu-row-{key}-{}", to.id));
                            div()
                                .id(group.clone())
                                .group(group.clone())
                                .flex()
                                .items_center()
                                .gap(px(9.))
                                .h(px(28.))
                                .pl(px(8.))
                                .pr(px(5.))
                                .rounded(px(radius::SM))
                                .hover(|s| s.bg(p.hover))
                                .child(icon(format_icon(to), 15., p.secondary))
                                .child(
                                    styled(size::SMALL, p.text)
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .child(to.name),
                                )
                                .children(can_remove.then(|| {
                                    theme::clickable(
                                        SharedString::from(format!("menu-remove-{key}-{}", to.id)),
                                        format!("Remove {}", to.name),
                                    )
                                    .flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .justify_center()
                                    .size(px(20.))
                                    .rounded(px(5.))
                                    .opacity(0.45)
                                    .group_hover(group.clone(), |s| s.opacity(1.))
                                    .hover(|s| s.bg(p.error_tint))
                                    .on_click(move |_, _, cx| {
                                        app.update(cx, |s, cx| {
                                            s.update_menu(|m| toggle(m, kind, to, &options), cx)
                                        });
                                    })
                                    .child(icon(
                                        IconName::Close,
                                        11.,
                                        p.secondary,
                                    ))
                                }))
                                .into_any_element()
                        })
                        .collect();

                    let separator = || div().h(px(1.)).mx(px(8.)).my(px(3.)).bg(p.hairline);
                    let preview_card = div()
                        .flex()
                        .flex_col()
                        .w(px(236.))
                        .flex_shrink_0()
                        .gap(px(space::SM))
                        .child(theme::section_label("Menu preview", p))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .bg(p.overlay)
                                .rounded(px(10.))
                                .p(px(5.))
                                .gap(px(1.))
                                .shadow(menu_shadow(p))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(9.))
                                        .h(px(26.))
                                        .px(px(8.))
                                        .child(theme::mark(14., p))
                                        .child(
                                            styled(size::SMALL, p.text)
                                                .flex_1()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child("Convert with convt"),
                                        )
                                        .child(icon(IconName::ChevronDown, 12., p.tertiary)),
                                )
                                .child(separator())
                                .children(preview_rows)
                                .child(separator())
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(9.))
                                        .h(px(28.))
                                        .px(px(8.))
                                        .rounded(px(radius::SM))
                                        .child(icon(IconName::Settings, 15., p.tertiary))
                                        .child(
                                            styled(size::SMALL, p.secondary).child("More options…"),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .px(px(2.))
                                .child(
                                    div()
                                        .id(SharedString::from(format!("menu-summary-{key}")))
                                        .test_support()
                                        .aria_label(SharedString::from(summary.clone()))
                                        .child(styled(size::CAPTION, p.secondary).child(summary)),
                                )
                                .children(
                                    gif_note
                                        .map(|note| styled(size::CAPTION, p.tertiary).child(note)),
                                )
                                .children(custom.then(|| {
                                    div().pt(px(2.)).child(
                                        text_button(
                                            SharedString::from(format!("menu-reset-{key}")),
                                            "Reset to default",
                                            p.green_text,
                                            12.,
                                        )
                                        .on_click(
                                            move |_, _, cx| {
                                                app.update(cx, |s, cx| {
                                                    s.update_menu(|m| m.reset(kind), cx)
                                                })
                                            },
                                        ),
                                    )
                                })),
                        );

                    // Right column: curated category-specific 3-col grid + compact Other 4-col grid.
                    let curated = popular_for_kind(kind);
                    let mut popular: Vec<&'static Format> = Vec::new();
                    for id in curated {
                        if let Some(f) = options.iter().copied().find(|f| f.id == *id) {
                            popular.push(f);
                        }
                    }
                    let other: Vec<&'static Format> = options
                        .iter()
                        .copied()
                        .filter(|f| !popular.contains(f))
                        .collect();
                    let primary_list = if popular.is_empty() {
                        other.clone()
                    } else {
                        popular
                    };
                    let secondary_list = if primary_list.len() == options.len() {
                        Vec::new()
                    } else {
                        other
                    };

                    let tile = |to: &'static Format, compact: bool| {
                        let place = picked.iter().position(|f| *f == to);
                        let on = place.is_some();
                        let app = self.app.clone();
                        let options = options.clone();
                        theme::choice(
                            theme::clickable(
                                SharedString::from(format!("menu-{key}-{}", to.id)),
                                to.name,
                            ),
                            on,
                            radius::CONTROL,
                            p,
                        )
                        .aria_selected(on)
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(6.))
                        .px(px(10.))
                        .py(px(if compact { 7. } else { 9. }))
                        .min_w_0()
                        .on_click(move |_, _, cx| {
                            app.update(cx, |s, cx| {
                                s.update_menu(|m| toggle(m, kind, to, &options), cx)
                            });
                        })
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .gap(px(1.))
                                .child(
                                    mono(12., 15., p.text)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .truncate()
                                        .child(to.name),
                                )
                                .when(!compact, |d| {
                                    d.child(
                                        styled(
                                            size::CAPTION,
                                            if on { p.secondary } else { p.tertiary },
                                        )
                                        .truncate()
                                        .child(format_note(to)),
                                    )
                                }),
                        )
                        .children(place.map(|i| {
                            div()
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .justify_center()
                                .size(px(18.))
                                .rounded(px(5.))
                                .bg(p.green)
                                .child(
                                    mono(10., 12., p.on_green)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child((i + 1).to_string()),
                                )
                        }))
                        .into_any_element()
                    };

                    let picker_col =
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap(px(space::MD))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(theme::section_label(
                                        format!("Formats for {}", kind_label(kind)),
                                        p,
                                    ))
                                    .child(div().grid().grid_cols(3).gap(px(space::SM)).children(
                                        primary_list.into_iter().map(|f| tile(f, false)),
                                    )),
                            )
                            .when(!secondary_list.is_empty(), |d| {
                                // An animated GIF can become a video, so the
                                // Images menu offers video containers too.
                                // They get their own labelled row so they
                                // don't read as still-image formats.
                                let (animated, still): (Vec<_>, Vec<_>) =
                                    secondary_list.into_iter().partition(|f| {
                                        kind == Category::Image && f.category == Category::Video
                                    });
                                d.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(theme::section_label("Other formats", p))
                                        .when(!still.is_empty(), |d| {
                                            d.child(
                                                div().grid().grid_cols(4).gap(px(6.)).children(
                                                    still.into_iter().map(|f| tile(f, true)),
                                                ),
                                            )
                                        })
                                        .when(!animated.is_empty(), |d| {
                                            d.child(
                                                div()
                                                    .id(SharedString::from(format!(
                                                        "menu-animated-{key}"
                                                    )))
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(6.))
                                                    .px(px(2.))
                                                    .pt(px(space::MD))
                                                    .pb(px(space::SM))
                                                    .child(icon(IconName::Video, 13., p.tertiary))
                                                    .child(
                                                        styled(size::CAPTION, p.secondary)
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .child("For animated GIFs"),
                                                    )
                                                    .child(
                                                        styled(size::CAPTION, p.tertiary)
                                                            .child("· only GIFs offer these"),
                                                    ),
                                            )
                                            .child(
                                                div().grid().grid_cols(4).gap(px(6.)).children(
                                                    animated.into_iter().map(|f| tile(f, true)),
                                                ),
                                            )
                                        }),
                                )
                            });

                    div()
                        .flex()
                        .items_start()
                        .gap(px(space::XL))
                        .child(preview_card)
                        .child(picker_col)
                });

        div()
            .flex()
            .flex_col()
            .gap(px(space::LG))
            .px(px(GUTTER))
            .pt(px(space::LG))
            .pb(px(space::XXL))
            .children(error.map(|e| {
                theme::callout(
                    IconName::TriangleAlert,
                    Tone::Error,
                    div()
                        .id("menu-error")
                        .test_support()
                        .aria_label(SharedString::from(e.clone()))
                        .child(styled(size::SMALL, p.error).child(e)),
                    p,
                )
            }))
            .child(kind_tabs)
            .children(workspace)
    }

    fn presets(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let state = self.app.read(cx);
        let dir = state.presets_dir.clone();
        let empty = state.presets.is_empty();
        let list: Vec<(String, Result<Preset, String>)> = state
            .presets
            .iter()
            .map(|(name, preset)| (name.clone(), preset.clone()))
            .collect();
        let rows: Vec<AnyElement> = list
            .into_iter()
            .map(|(name, preset)| {
                let valid = preset.is_ok();
                let edit_name = name.clone();
                let edit = valid.then(|| {
                    Button::secondary(SharedString::from(format!("edit-preset-{name}")), "Edit")
                        .small()
                        .build(p)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.edit_preset(&edit_name, window, cx)
                        }))
                });
                let app = self.app.clone();
                let delete_name = name.clone();
                let detail = match &preset {
                    Ok(preset) => {
                        let to_badge = preset
                            .to
                            .as_deref()
                            .and_then(format_by_id)
                            .map(|f| theme::badge(f.name, Tone::Green, p));
                        let specs = describe(preset);
                        div()
                            .flex()
                            .items_center()
                            .flex_wrap()
                            .gap(px(6.))
                            .children(to_badge)
                            .child(mono(11., 15., p.secondary).child(specs))
                            .child(mono(11., 15., p.tertiary).child(format!("· --preset {name}")))
                            .into_any_element()
                    }
                    Err(e) => mono(11., 15., p.error).child(e.clone()).into_any_element(),
                };
                theme::row(
                    name.clone(),
                    Some(detail),
                    div().flex().gap(px(space::SM)).children(edit).child(
                        Button::ghost(
                            SharedString::from(format!("delete-preset-{name}")),
                            "Delete",
                        )
                        .small()
                        .build(p)
                        .on_click(move |_, _, cx| {
                            if let Err(e) =
                                app.update(cx, |s, cx| s.delete_preset(&delete_name, cx))
                            {
                                tracing::warn!(error = %e, "could not delete a preset");
                            }
                        }),
                    ),
                    p,
                )
                .into_any_element()
            })
            .collect();

        // Starter recipes shown when the user has no presets yet, so the page
        // teaches by example and lets you add a useful recipe in one click.
        let starters =
            div()
                .grid()
                .grid_cols(3)
                .gap(px(space::SM))
                .children(STARTER_PRESETS.iter().map(|starter| {
                    let app = self.app.clone();
                    let s = *starter;
                    theme::card(p)
                        .p(px(14.))
                        .gap(px(8.))
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(6.))
                                        .child(
                                            styled(size::BODY, p.text)
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(s.name),
                                        )
                                        .child(theme::badge(s.to_label, Tone::Green, p)),
                                )
                                .child(styled(size::CAPTION, p.secondary).child(s.note))
                                .child(mono(11., 14., p.tertiary).child(s.spec)),
                        )
                        .child(
                            div().pt(px(2.)).child(
                                Button::secondary(
                                    SharedString::from(format!("starter-preset-{}", s.name)),
                                    "Add preset",
                                )
                                .icon(IconName::Plus)
                                .small()
                                .build(p)
                                .on_click(move |_, _, cx| {
                                    let preset = Preset {
                                        to: Some(s.to.into()),
                                        options: Options {
                                            quality: s.quality,
                                            max_size: s.max_size,
                                            video_height: s.video_height,
                                            ..Options::default()
                                        },
                                    };
                                    let _ = app
                                        .update(cx, |st, cx| st.save_preset(s.name, &preset, cx));
                                }),
                            ),
                        )
                }));

        // Target picker: category segmented filter (default Popular = 6 clean
        // tiles) instead of dumping all 37 formats at once.
        let group_tabs = theme::pill_track(p).children(PRESET_TARGET_TABS.iter().enumerate().map(
            |(i, label)| {
                let on = self.preset_group == i;
                theme::clickable(SharedString::from(format!("preset-group-{i}")), *label)
                    .aria_selected(on)
                    .flex()
                    .items_center()
                    .h(px(24.))
                    .px(px(10.))
                    .rounded(px(radius::SM))
                    .map(|d| {
                        if on {
                            theme::pill_segment_on(d, p)
                        } else {
                            d.hover(|s| s.bg(p.hover))
                        }
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.preset_group = i;
                        cx.notify();
                    }))
                    .child(
                        styled(size::CAPTION, if on { p.text } else { p.secondary })
                            .font_weight(FontWeight::MEDIUM)
                            .child(*label),
                    )
            },
        ));
        let group_formats: Vec<&'static Format> = match self.preset_group {
            0 => PRESET_POPULAR
                .iter()
                .filter_map(|id| self.outputs.iter().copied().find(|f| f.id == *id))
                .collect(),
            idx => {
                let kinds = TARGET_GROUPS
                    .get(idx - 1)
                    .map_or(&[][..], |(_, kinds)| *kinds);
                self.outputs
                    .iter()
                    .copied()
                    .filter(|f| kinds.contains(&f.category))
                    .collect()
            }
        };
        let target_cols: u16 = if self.preset_group == 0 { 6 } else { 5 };
        let target_grid = div().grid().grid_cols(target_cols).gap(px(6.)).children(
            group_formats.into_iter().map(|to| {
                let on = self.preset_to == Some(to);
                let weak = cx.entity().downgrade();
                theme::choice(
                    theme::clickable(SharedString::from(format!("new-to-{}", to.id)), to.name),
                    on,
                    radius::CONTROL,
                    p,
                )
                .aria_selected(on)
                .flex()
                .items_center()
                .justify_center()
                .h(px(32.))
                .px(px(8.))
                .on_click(move |_, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.preset_to = (this.preset_to != Some(to)).then_some(to);
                        cx.notify();
                    });
                })
                .child(
                    mono(12., 15., p.text)
                        .font_weight(if on {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::MEDIUM
                        })
                        .truncate()
                        .child(to.name),
                )
            }),
        );

        let labelled = |label: &'static str, field: AnyElement| {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .gap(px(6.))
                .child(
                    styled(size::CAPTION, p.secondary)
                        .font_weight(FontWeight::MEDIUM)
                        .child(label),
                )
                .child(field)
        };
        let typed_name = self.preset_name.read(cx).value().trim().to_string();
        let cli_preview = format!(
            "convt file --preset {}",
            if typed_name.is_empty() {
                "web"
            } else {
                &typed_name
            }
        );
        let form = theme::card(p)
            .p(px(0.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .p(px(space::LG))
                    .child(
                        div()
                            .flex()
                            .gap(px(space::MD))
                            .child(labelled(
                                "Name",
                                theme::field(&self.preset_name, "preset-name").into_any_element(),
                            ))
                            .child(labelled(
                                "Quality (1–100)",
                                theme::field(&self.preset_quality, "preset-quality")
                                    .into_any_element(),
                            ))
                            .child(labelled(
                                "Longest edge (px)",
                                theme::field(&self.preset_max_size, "preset-max-size")
                                    .into_any_element(),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(space::SM))
                                    .child(
                                        styled(size::CAPTION, p.secondary)
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(match self.preset_to {
                                                Some(to) => format!("Target format · {}", to.name),
                                                None => "Target format (optional)".to_string(),
                                            }),
                                    )
                                    .child(group_tabs),
                            )
                            .child(target_grid),
                    )
                    .children(self.preset_error.clone().map(|e| error_text(e, p))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(space::SM))
                    .px(px(space::LG))
                    .py(px(10.))
                    .bg(p.recessed)
                    .rounded_b(px(radius::CARD - 1.))
                    .border_t_1()
                    .border_color(p.hairline)
                    .child(mono(11., 14., p.tertiary).truncate().child(cli_preview))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::SM))
                            .when(self.editing.is_some(), |row| {
                                row.child(secondary_button("cancel-edit", "Cancel", p).on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.reset_preset_form(window, cx)
                                    }),
                                ))
                            })
                            .child(primary_button("save-preset", "Save preset", p).on_click(
                                cx.listener(|this, _, window, cx| this.save_preset(window, cx)),
                            )),
                    ),
            );
        let list = if empty {
            starters.into_any_element()
        } else {
            theme::group(rows, p).into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(space::XL))
            .px(px(GUTTER))
            .pt(px(space::LG))
            .pb(px(space::XXL))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(theme::section_label(
                                if empty {
                                    "Starter presets"
                                } else {
                                    "Saved presets"
                                },
                                p,
                            ))
                            .children(dir.map(|dir| {
                                div().pb(px(space::SM)).child(
                                    text_button(
                                        "open-presets",
                                        "Open presets folder",
                                        p.green_text,
                                        12.,
                                    )
                                    .on_click(move |_, _, cx| open_folder(&dir, cx)),
                                )
                            })),
                    )
                    .child(list),
            )
            .child(section(
                match &self.editing {
                    Some(name) => format!("Edit preset \"{name}\""),
                    None => "Create custom preset".to_string(),
                },
                form,
                p,
            ))
    }

    fn license(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let state = self.app.read(cx).license.clone();
        let can_start_trial =
            self.app.read(cx).account.can_start_trial() && !state.allows_conversion();
        let summary = SharedString::from(match &state {
            State::Licensed(l) if l.plan == convt_license::Plan::Pro => {
                "You have convt Pro".to_string()
            }
            State::Licensed(_) => "You have a convt license".to_string(),
            _ if can_start_trial => format!("Start your {TRIAL_DAYS}-day Pro trial."),
            _ => state.summary(),
        });
        let allowed = state.allows_conversion();
        let (glyph, tone, about) = match &state {
            // An offer, not an error, though nothing converts yet.
            _ if can_start_trial => (
                IconName::Calendar,
                Tone::Green,
                "Your convt.app account includes a free Pro trial. Start it below, or enter a license key.".to_string(),
            ),
            State::Unrestricted => (
                IconName::CircleCheck,
                Tone::Green,
                "Built from source, so every feature is on.".to_string(),
            ),
            State::Trial { .. } => (
                IconName::Calendar,
                Tone::Green,
                format!(
                    "Every feature works during the trial. A {LICENSE_PRICE} license keeps them."
                ),
            ),
            State::AccountTrial { .. } => (
                IconName::Calendar,
                Tone::Green,
                "Every feature works during your Pro trial.".to_string(),
            ),
            State::SignInNeeded => (
                IconName::TriangleAlert,
                Tone::Error,
                "The 7-day Pro trial comes with a convt.app account. Sign in below, or enter a license key.".to_string(),
            ),
            State::TrialEnded => (
                IconName::TriangleAlert,
                Tone::Error,
                "Buy a license or paste your key to keep converting.".to_string(),
            ),
            State::Licensed(l) => (
                IconName::CircleCheck,
                Tone::Green,
                format!(
                    "{} · updates until {}. Checked offline; the key stays on {}.",
                    masked_email(&l.email),
                    l.updates_until,
                    theme::this_machine()
                ),
            ),
            State::NotCovered(_) => (
                IconName::TriangleAlert,
                Tone::Error,
                "Renew to use this version, or download a build your license covers.".to_string(),
            ),
        };
        let status = theme::card(p)
            .relative()
            .overflow_hidden()
            .flex_row()
            .items_center()
            .gap(px(14.))
            .px(px(20.))
            .py(px(22.))
            .rounded(px(radius::PANEL))
            .when(allowed, |d| d.child(theme::glow(0.6, 300., p)))
            .child(theme::icon_tile(glyph, tone, 40., p))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        div()
                            .id("license-status")
                            .test_support()
                            .aria_label(summary.clone())
                            .child(
                                styled(
                                    size::BODY,
                                    if allowed || can_start_trial {
                                        p.text
                                    } else {
                                        p.error
                                    },
                                )
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(summary),
                            ),
                    )
                    .child(styled(size::SMALL, p.secondary).child(about)),
            );
        let body = div()
            .flex()
            .flex_col()
            .gap(px(space::XL))
            .px(px(GUTTER))
            .pt(px(space::XL))
            .pb(px(space::XXL))
            .child(status);
        let account = super::account::section(&self.app, p, cx);
        if state == State::Unrestricted {
            return body.child(account);
        }
        let licensed = matches!(state, State::Licensed(_) | State::NotCovered(_));
        let notice = self.license_notice.clone().map(|message| {
            div()
                .id("license-notice")
                .test_support()
                .aria_label(SharedString::from(message.clone()))
                .flex()
                .items_center()
                .gap(px(6.))
                .children(allowed.then(|| icon(IconName::CircleCheck, 13., p.green)))
                .child(
                    styled(
                        size::SMALL,
                        if allowed { p.green_text } else { p.secondary },
                    )
                    .child(message),
                )
        });
        let key = theme::card(p)
            .gap(px(space::MD))
            .p(px(space::LG))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::SM))
                    .child(
                        div()
                            .flex_1()
                            .font_family(theme::MONO)
                            .child(theme::field(&self.license_key, "license-key")),
                    )
                    .child(primary_button("activate", "Activate", p).on_click(
                        cx.listener(|this, _, window, cx| this.activate_license(window, cx)),
                    )),
            )
            .children(notice)
            .children(self.license_error.clone().map(|e| error_text(e, p)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::LG))
                    .when(!matches!(state, State::Licensed(_)), |row| {
                        row.child(
                            text_button(
                                "settings-buy",
                                if matches!(state, State::NotCovered(_)) {
                                    "Renew"
                                } else {
                                    "Buy a license"
                                },
                                p.green_text,
                                12.,
                            )
                            .on_click(|_, _, cx| super::open_upgrade("settings", cx)),
                        )
                    })
                    .when(licensed, |row| {
                        row.child(
                            text_button("remove-license", "Remove license", p.secondary, 12.)
                                .on_click(cx.listener(|this, _, _, cx| this.remove_license(cx))),
                        )
                    }),
            );
        body.child(section("License key", key, p)).child(account)
    }

    fn save_preset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.preset_name.read(cx).value().trim().to_string();
        let state = self.app.read(cx);
        // Names differing only in case share a file on macOS and Windows.
        let taken = state
            .presets
            .keys()
            .any(|k| k.eq_ignore_ascii_case(&name) && self.editing.as_deref() != Some(k.as_str()));
        if taken {
            self.preset_error = Some(format!(
                "There is already a preset named \"{name}\". Use Edit to change it."
            ));
            cx.notify();
            return;
        }
        // Keep the options the form doesn't show, such as video height or DPI.
        let base = self
            .editing
            .as_deref()
            .and_then(|old| state.preset(old).ok())
            .map(|p| p.options.clone())
            .unwrap_or_default();
        let quality = number::<u8>(&self.preset_quality.read(cx).value(), "Quality");
        let max_size = number::<u32>(&self.preset_max_size.read(cx).value(), "The longest edge");
        let result = quality.and_then(|quality| {
            let preset = Preset {
                to: self.preset_to.map(|f| f.id.to_string()),
                options: Options {
                    quality,
                    max_size: max_size?,
                    ..base
                },
            };
            self.app
                .update(cx, |s, cx| s.save_preset(&name, &preset, cx))
        });
        match result {
            Ok(()) => self.reset_preset_form(window, cx),
            Err(e) => self.preset_error = Some(e),
        }
        cx.notify();
    }

    fn reset_preset_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preset_error = None;
        self.preset_to = None;
        self.editing = None;
        for input in [
            &self.preset_name,
            &self.preset_quality,
            &self.preset_max_size,
        ] {
            input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        cx.notify();
    }

    /// Loads a saved preset into the form.
    pub(super) fn edit_preset(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(preset) = self.app.read(cx).preset(name).cloned() else {
            return;
        };
        let text = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
        let fields = [
            (&self.preset_name, name.to_string()),
            (
                &self.preset_quality,
                text(preset.options.quality.map(u32::from)),
            ),
            (&self.preset_max_size, text(preset.options.max_size)),
        ];
        for (input, value) in fields {
            input.update(cx, |s, cx| s.set_value(value, window, cx));
        }
        self.preset_to = preset.to.as_deref().and_then(format_by_id);
        if let Some(to) = self.preset_to {
            self.preset_group = if PRESET_POPULAR.contains(&to.id) {
                0
            } else {
                TARGET_GROUPS
                    .iter()
                    .position(|(_, kinds)| kinds.contains(&to.category))
                    .map_or(0, |i| i + 1)
            };
        }
        self.preset_error = None;
        self.editing = Some(name.to_string());
        cx.notify();
    }
}

/// Curated primary formats per file kind in the Right-click menu's main 3-col grid.
/// Keeps each category focused and balanced without cross-category video/container clutter.
fn popular_for_kind(kind: Category) -> &'static [&'static str] {
    match kind {
        Category::Image => &["jpeg", "png", "webp", "avif", "gif", "pdf"],
        Category::Vector => &["png", "pdf", "webp", "jpeg", "avif", "ico"],
        Category::Video => &["mp4", "mov", "webm", "mkv", "gif", "mp3"],
        Category::Audio => &["mp3", "m4a", "wav", "flac", "ogg", "opus"],
        Category::Pdf => &["png", "jpeg", "webp", "txt", "docx", "html"],
        Category::Document | Category::Spreadsheet | Category::Presentation => {
            &["pdf", "docx", "xlsx", "pptx", "txt", "html", "csv"]
        }
    }
}

/// Popular targets shown on the default tab of the New preset builder.
const PRESET_POPULAR: &[&str] = &["webp", "jpeg", "png", "mp4", "mp3", "pdf"];

const PRESET_TARGET_TABS: [&str; 5] = ["Popular", "Images", "Video", "Audio", "Documents"];

#[derive(Clone, Copy)]
struct StarterPreset {
    name: &'static str,
    to: &'static str,
    to_label: &'static str,
    quality: Option<u8>,
    max_size: Option<u32>,
    video_height: Option<u32>,
    spec: &'static str,
    note: &'static str,
}

const STARTER_PRESETS: [StarterPreset; 3] = [
    StarterPreset {
        name: "web",
        to: "webp",
        to_label: "WebP",
        quality: Some(82),
        max_size: Some(1600),
        video_height: None,
        spec: "Quality 82 · 1600 px",
        note: "Small, sharp images for websites",
    },
    StarterPreset {
        name: "email",
        to: "jpeg",
        to_label: "JPEG",
        quality: Some(75),
        max_size: Some(1920),
        video_height: None,
        spec: "Quality 75 · 1920 px",
        note: "Compact photos ready to attach",
    },
    StarterPreset {
        name: "share",
        to: "mp4",
        to_label: "MP4",
        quality: Some(75),
        max_size: None,
        video_height: Some(1080),
        spec: "Quality 75 · 1080p",
        note: "Lightweight clips for chat & Slack",
    },
];

fn format_note(f: &Format) -> &'static str {
    match f.id {
        "jpeg" => "Photos",
        "png" => "Lossless",
        "webp" => "Small, sharp",
        "avif" => "Smallest",
        "gif" => "Simple graphics",
        "heic" => "Apple photos",
        "pdf" => "Print, share",
        "mp4" => "Plays anywhere",
        "mov" => "Apple video",
        "webm" => "Web video",
        "mkv" => "Container",
        "mp3" => "Plays anywhere",
        "m4a" => "Apple audio",
        "wav" => "Uncompressed",
        "flac" => "Lossless",
        "aac" => "Compact audio",
        "docx" => "Word",
        "xlsx" => "Excel",
        "pptx" => "PowerPoint",
        "md" => "Markdown",
        "txt" => "Plain text",
        "html" => "Web page",
        "json" => "Structured data",
        "csv" => "Spreadsheet",
        _ => "Format",
    }
}

/// The groups the preset form lists targets in.
const TARGET_GROUPS: [(&str, &[Category]); 4] = [
    ("Images", &[Category::Image, Category::Vector]),
    ("Video", &[Category::Video]),
    ("Audio", &[Category::Audio]),
    (
        "Documents",
        &[
            Category::Pdf,
            Category::Document,
            Category::Presentation,
            Category::Spreadsheet,
        ],
    ),
];

/// What reaches the network without a click, for the General tab. Update
/// checks and license refresh are listed together, as the privacy policy
/// lists them.
pub(super) const NETWORK_LINES: [(&str, &str); 3] = [
    (
        "network-updates",
        "Update checks: while automatic checks are on, at every launch and every 5 hours, and \
         whenever you click Check now, convt downloads the signed list of releases from \
         convt.app. The request carries the app version and nothing about your files. When a \
         new version your license covers is out and automatic checks are on, convt downloads \
         its installer from the release's host (GitHub) and checks it against that list before \
         offering to restart.",
    ),
    (
        "network-refresh",
        "License refresh: only while you're signed in to convt.app, at most once a day, \
         to fetch your current Pro key. See License.",
    ),
    (
        "network-other",
        "Usage statistics: when enabled, the app sends PostHog app opens, conversion formats, \
         engine, coarse file size, success or error category and duration, trial and license \
         activity, and upgrade clicks. A random installation ID links repeat use; signing in \
         links it to your account ID. No file names, paths, content or error messages are sent. \
         Turn this off in Usage statistics. Other requests wait for your click, such as \
         downloading document support or sending feedback. Files leave this computer only when \
         you pick Cloud in Quick convert and agree to the upload.",
    ),
];

fn network(p: &Palette) -> Div {
    let glyphs = [
        IconName::RefreshCw,
        IconName::CircleUser,
        IconName::HardDrive,
    ];
    theme::group(
        NETWORK_LINES
            .iter()
            .zip(glyphs)
            .map(|(&(id, line), glyph)| {
                div()
                    .flex()
                    .items_start()
                    .gap(px(space::MD))
                    .px(px(space::LG))
                    .py(px(space::MD))
                    .child(div().pt(px(2.)).child(icon(glyph, 14., p.tertiary)))
                    .child(
                        div()
                            .id(id)
                            .test_support()
                            .aria_label(line)
                            .flex_1()
                            .min_w(px(0.))
                            .child(styled(size::SMALL, p.secondary).child(line)),
                    )
                    .into_any_element()
            }),
        p,
    )
}

/// Every kind of file with something to convert to, and what it can become.
fn kinds_and_targets(registry: &convt_core::Registry) -> Vec<(Category, Vec<&'static Format>)> {
    MENU_KINDS
        .into_iter()
        .map(|kind| (kind, registry.category_targets(kind)))
        .filter(|(_, options)| !options.is_empty())
        .collect()
}

/// A kind of file as the Menu tab names it.
/// The picks for `kind` this machine can convert to, in menu order, and the
/// ones it can't.
fn shown_and_hidden(
    choices: &MenuChoices,
    kind: Category,
    options: &[&'static Format],
) -> (Vec<&'static Format>, Vec<&'static Format>) {
    choices
        .effective(kind)
        .into_iter()
        .partition(|f| options.contains(f))
}

/// Adds `to` at the end of the menu for `kind`, or takes it out. It works on
/// the choices as saved, so a `convt menu` change made since the tab drew is
/// kept. The menu keeps at least one format; Reset brings the defaults back.
fn toggle(
    choices: &mut MenuChoices,
    kind: Category,
    to: &'static Format,
    options: &[&'static Format],
) {
    let (mut next, hidden) = shown_and_hidden(choices, kind, options);
    match next.iter().position(|f| *f == to) {
        Some(_) if next.len() == 1 => return,
        Some(i) => {
            next.remove(i);
        }
        None => next.push(to),
    }
    next.extend(hidden);
    choices.set(kind, next);
}

fn kind_label(kind: Category) -> &'static str {
    match kind {
        Category::Image => "Images",
        Category::Vector => "SVG",
        Category::Video => "Video",
        Category::Audio => "Audio",
        Category::Pdf => "PDFs",
        Category::Document => "Documents",
        Category::Spreadsheet => "Spreadsheets",
        Category::Presentation => "Presentations",
    }
}

/// The icon for a kind of file, on the Right-click menu's category tabs.
fn kind_icon(kind: Category) -> IconName {
    match kind {
        Category::Image => IconName::Image,
        Category::Vector => IconName::Vector,
        Category::Video => IconName::Video,
        Category::Audio => IconName::Music,
        Category::Pdf => IconName::Pdf,
        Category::Document => IconName::Document,
        Category::Spreadsheet => IconName::Spreadsheet,
        Category::Presentation => IconName::Presentation,
    }
}

/// The icon beside a target in the menu preview: what kind of file it makes.
fn format_icon(to: &Format) -> IconName {
    kind_icon(to.category)
}

/// The menu preview's surface: a hairline ring, a highlight along the top
/// edge and the soft drop of a floating menu, like the native popovers.
fn menu_shadow(p: &Palette) -> Vec<BoxShadow> {
    let highlight = if p.dark {
        hsla(0., 0., 1., 0.07)
    } else {
        hsla(0., 0., 1., 0.9)
    };
    vec![
        theme::inset_ring(p.border, 1.),
        theme::inset_edge(highlight, 1.),
        theme::shadow(p.shadow_soft, 6., 18.),
        theme::shadow_s(p),
    ]
}

/// "MP3, M4A and WAV".
fn list_names(formats: &[&Format]) -> String {
    match formats {
        [] => String::new(),
        [one] => one.name.to_string(),
        [rest @ .., last] => format!(
            "{} and {}",
            rest.iter().map(|f| f.name).collect::<Vec<_>>().join(", "),
            last.name
        ),
    }
}

/// A labelled section of a tab: the small label, then its card.
fn section(label: impl Into<SharedString>, content: impl IntoElement, p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .child(theme::section_label(label, p))
        .child(content)
}

/// What "Keep running in the background" does, in this platform's words.
fn background_note() -> &'static str {
    if cfg!(target_os = "macos") {
        "Shows convt in the menu bar so conversions start instantly. Quit from there or with ⌘Q."
    } else {
        "Shows convt in the system tray so conversions start instantly. Quit from there or with \
         Ctrl+Q."
    }
}

/// Parses an optional number field. Empty means unset.
fn number<T: std::str::FromStr>(text: &str, what: &str) -> Result<Option<T>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    text.parse()
        .map(Some)
        .map_err(|_| format!("{what} must be a whole number."))
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let title = TABS
            .iter()
            .find(|(t, ..)| *t == self.tab)
            .map_or("Settings", |(_, _, label, _)| label);
        let embedded = self.embedded;
        // When embedded in the main window, Menu and Presets have their own
        // top-level sidebar pages and headers, so Settings only shows the
        // General / License tabs. Standalone Settings still offers all four.
        let show_tabs =
            !embedded || matches!(self.tab, SettingsTab::General | SettingsTab::License);
        let visible_tabs: Vec<_> = TABS
            .into_iter()
            .filter(|(tab, ..)| {
                !embedded || matches!(tab, SettingsTab::General | SettingsTab::License)
            })
            .collect();
        let tabs = show_tabs.then(|| {
            div()
                .flex()
                .flex_shrink_0()
                .when(embedded, |d| {
                    d.px(px(GUTTER))
                        .pb(px(space::MD))
                        .border_b_1()
                        .border_color(p.hairline)
                })
                .when(!embedded, |d| {
                    d.justify_center()
                        .px(px(space::LG))
                        .pb(px(10.))
                        .when(!theme::transparent_titlebar(), |d| d.pt(px(12.)))
                        .bg(p.chrome)
                        .border_b_1()
                        .border_color(p.chrome_border)
                })
                .child(theme::pill_track(&p).children(visible_tabs.into_iter().map(
                    |(tab, id, label, glyph)| {
                        let on = self.tab == tab;
                        theme::clickable(id, label)
                            .aria_selected(on)
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .h(px(28.))
                            .px(px(16.))
                            .rounded(px(radius::CONTROL))
                            .map(|d| {
                                if on {
                                    theme::pill_segment_on(d, &p)
                                } else {
                                    d.hover(|s| s.bg(p.hover))
                                }
                            })
                            .on_click(cx.listener(move |this, _, _, cx| this.set_tab(tab, cx)))
                            .child(icon(glyph, 13., if on { p.text } else { p.tertiary }))
                            .child(
                                styled(size::SMALL, if on { p.text } else { p.secondary })
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(label),
                            )
                    },
                )))
        });
        let body = match self.tab {
            SettingsTab::General => self.general(&p, cx),
            SettingsTab::Menu => self.menu(&p, cx),
            SettingsTab::Presets => self.presets(&p, cx),
            SettingsTab::License => self.license(&p, cx),
        };
        div()
            .id("settings")
            .flex()
            .flex_col()
            .size_full()
            .bg(p.window)
            .font_family(theme::SANS)
            .text_color(p.text)
            .when(!embedded, |d| {
                d.children(theme::title_bar(Some(title), 40., Some(p.chrome), &p))
            })
            .children(tabs)
            .child(
                div()
                    .id("settings-body")
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(body),
            )
    }
}
