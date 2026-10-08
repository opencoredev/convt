//! The Settings window: General, Presets and License.

use convt_core::{Category, FORMATS, Format, Options, Preset, format_by_id};
use convt_license::client::{BUY_URL, State};
use gpui_kit::component::input::InputState;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::IconName;

use super::theme::{
    self, Button, Choice, Palette, Tone, icon, mono, primary_button, radius, secondary_button,
    size, space, styled, text_button,
};
use super::{LICENSE_PRICE, describe, error_text, open_folder};
use crate::finder::EXTENSION_SETTINGS;
use crate::model::AppState;
use crate::settings::{Settings, auto_concurrency};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Presets,
    License,
}

const TABS: [(SettingsTab, &str, &str, IconName); 3] = [
    (
        SettingsTab::General,
        "tab-general",
        "General",
        IconName::Settings,
    ),
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
    _observe: Subscription,
    _appearance: Subscription,
}

impl SettingsView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let registry = app.read(cx).registry.clone();
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
            _observe: cx.observe(&app, |_, _, cx| cx.notify()),
            _appearance: theme::observe_appearance(window, cx),
            app,
            tab: SettingsTab::General,
            open: None,
            outputs,
            preset_name: input("Name, e.g. web", window, cx),
            preset_quality: input("Quality 1-100", window, cx),
            preset_max_size: input("Longest edge in px", window, cx),
            preset_to: None,
            preset_error: None,
            editing: None,
            license_key: input("License key", window, cx),
            license_error: None,
            license_notice: None,
            confirm_remove_pack: false,
            scroll: ScrollHandle::new(),
        }
    }

    pub fn set_tab(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
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
                    format!("License activated for {}.", license.email)
                } else {
                    format!("Saved the license for {}.", license.email)
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
            "Menu bar icon",
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
                    .on_click(|_, _, cx| cx.open_url(EXTENSION_SETTINGS)),
                p,
            )
            .into_any_element()
        });
        // Only macOS keeps running for a menu bar icon; elsewhere the switch
        // would do nothing.
        let menu_bar = cfg!(target_os = "macos").then(|| {
            theme::row(
                "Menu bar icon",
                Some(theme::detail("Shows progress and takes dropped files", p)),
                menu_bar,
                p,
            )
            .into_any_element()
        });
        let desktop_label = match (finder.is_some(), menu_bar.is_some()) {
            (true, true) => Some("Finder and menu bar"),
            (true, false) => Some("Finder"),
            (false, true) => Some("Menu bar"),
            (false, false) => None,
        };
        let desktop = desktop_label.map(|label| {
            section(
                label,
                theme::group(finder.into_iter().chain(menu_bar), p),
                p,
            )
        });

        div()
            .flex()
            .flex_col()
            .gap(px(space::XL))
            .px(px(GUTTER))
            .pt(px(space::XL))
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
            .children(desktop)
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
    }

    fn presets(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let state = self.app.read(cx);
        let dir = state.presets_dir.clone();
        let empty = state.presets.is_empty();
        let list: Vec<(String, Result<String, String>)> = state
            .presets
            .iter()
            .map(|(name, preset)| {
                (
                    name.clone(),
                    preset.as_ref().map(describe).map_err(Clone::clone),
                )
            })
            .collect();
        let rows: Vec<AnyElement> = list
            .into_iter()
            .map(|(name, about)| {
                let valid = about.is_ok();
                let (about, color) = match about {
                    Ok(text) => (text, p.secondary),
                    Err(e) => (e, p.error),
                };
                let edit_name = name.clone();
                let edit = valid.then(|| {
                    Button::ghost(SharedString::from(format!("edit-preset-{name}")), "Edit")
                        .small()
                        .build(p)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.edit_preset(&edit_name, window, cx)
                        }))
                });
                let app = self.app.clone();
                let delete_name = name.clone();
                theme::row(
                    name.clone(),
                    Some(mono(11., 15., color).child(about).into_any_element()),
                    div().flex().gap(px(2.)).children(edit).child(
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
        let chip = |to: &'static Format| {
            let on = self.preset_to == Some(to);
            let weak = cx.entity().downgrade();
            theme::clickable(SharedString::from(format!("new-to-{}", to.id)), to.name)
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
                    let _ = weak.update(cx, |this, cx| {
                        this.preset_to = (this.preset_to != Some(to)).then_some(to);
                        cx.notify();
                    });
                })
                .child(
                    mono(11., 14., if on { p.green_text } else { p.text })
                        .font_weight(FontWeight::MEDIUM)
                        .child(to.name),
                )
        };
        // The targets by kind, so the list reads in groups.
        let chips =
            div()
                .flex()
                .flex_col()
                .gap(px(space::SM))
                .children(TARGET_GROUPS.iter().filter_map(|(label, kinds)| {
                    let formats: Vec<&'static Format> = self
                        .outputs
                        .iter()
                        .copied()
                        .filter(|f| kinds.contains(&f.category))
                        .collect();
                    (!formats.is_empty()).then(|| {
                        div()
                            .flex()
                            .items_start()
                            .gap(px(space::MD))
                            .child(
                                styled(size::SMALL, p.secondary)
                                    .w(px(76.))
                                    .flex_shrink_0()
                                    .pt(px(4.))
                                    .child(*label),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .flex_wrap()
                                    .gap(px(6.))
                                    .children(formats.into_iter().map(chip)),
                            )
                    })
                }));
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
        let form = theme::card(p)
            .gap(px(14.))
            .p(px(space::LG))
            .child(labelled(
                "Name",
                theme::field(&self.preset_name, "preset-name").into_any_element(),
            ))
            .child(labelled("Target (optional)", chips.into_any_element()))
            .child(
                div()
                    .flex()
                    .gap(px(space::MD))
                    .child(labelled(
                        "Quality",
                        theme::field(&self.preset_quality, "preset-quality").into_any_element(),
                    ))
                    .child(labelled(
                        "Longest edge",
                        theme::field(&self.preset_max_size, "preset-max-size").into_any_element(),
                    )),
            )
            .children(self.preset_error.clone().map(|e| error_text(e, p)))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(space::SM))
                    .when(self.editing.is_some(), |row| {
                        row.child(secondary_button("cancel-edit", "Cancel", p).on_click(
                            cx.listener(|this, _, window, cx| this.reset_preset_form(window, cx)),
                        ))
                    })
                    .child(
                        primary_button("save-preset", "Save preset", p).on_click(
                            cx.listener(|this, _, window, cx| this.save_preset(window, cx)),
                        ),
                    ),
            );
        let list = if empty {
            theme::card(p)
                .items_center()
                .gap(px(4.))
                .py(px(space::XL))
                .px(px(space::LG))
                .child(
                    styled(size::BODY, p.text)
                        .font_weight(FontWeight::MEDIUM)
                        .child("No presets yet."),
                )
                .child(
                    styled(size::SMALL, p.secondary)
                        .text_center()
                        .child("Save one below. Quick convert and the CLI's --preset use them."),
                )
        } else {
            theme::group(rows, p)
        };
        div()
            .flex()
            .flex_col()
            .gap(px(space::XL))
            .px(px(GUTTER))
            .pt(px(space::XL))
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
                            .child(theme::section_label("Presets", p))
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
                    None => "New preset".to_string(),
                },
                form,
                p,
            ))
    }

    fn license(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let state = self.app.read(cx).license.clone();
        let summary = SharedString::from(match &state {
            State::Licensed(l) if l.plan == convt_license::Plan::Pro => {
                "You have convt Pro".to_string()
            }
            State::Licensed(_) => "You have a convt license".to_string(),
            _ => state.summary(),
        });
        let allowed = state.allows_conversion();
        let (glyph, tone, about) = match &state {
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
                "Sign in to start your free trial.".to_string(),
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
                    l.email,
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
            .flex_row()
            .items_center()
            .gap(px(14.))
            .p(px(space::LG))
            .child(theme::icon_tile(glyph, tone, 36., p))
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
                                styled(size::BODY, if allowed { p.text } else { p.error })
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
                            .on_click(|_, _, cx| cx.open_url(BUY_URL)),
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
        self.preset_error = None;
        self.editing = Some(name.to_string());
        cx.notify();
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
         convt.app. The request carries the app version and nothing about your files.",
    ),
    (
        "network-refresh",
        "License refresh: only while you're signed in to convt.app, once a day at launch, \
         to fetch your current Pro key. See License.",
    ),
    (
        "network-other",
        "Anything else waits for your click, such as downloading document support. Files \
         leave this computer only when you pick Cloud in Quick convert and agree to the upload.",
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

/// A labelled section of a tab: the small label, then its card.
fn section(label: impl Into<SharedString>, content: impl IntoElement, p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .child(theme::section_label(label, p))
        .child(content)
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
        let tabs = div()
            .flex()
            .flex_shrink_0()
            .justify_center()
            .px(px(space::LG))
            .pb(px(10.))
            .when(!theme::transparent_titlebar(), |d| d.pt(px(12.)))
            .bg(p.chrome)
            .border_b_1()
            .border_color(p.chrome_border)
            .child(
                div()
                    .flex()
                    .gap(px(2.))
                    .p(px(2.))
                    .rounded(px(radius::CONTROL + 2.))
                    .bg(if p.dark { p.recessed } else { p.track })
                    .children(TABS.into_iter().map(|(tab, id, label, glyph)| {
                        let on = self.tab == tab;
                        theme::clickable(id, label)
                            .aria_selected(on)
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .h(px(26.))
                            .px(px(14.))
                            .rounded(px(radius::CONTROL))
                            .map(|d| {
                                if on {
                                    d.bg(if p.dark { p.selected } else { p.surface })
                                        .shadow(vec![theme::shadow(p.shadow_soft, 1., 2.)])
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
                    })),
            );
        let body = match self.tab {
            SettingsTab::General => self.general(&p, cx),
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
            .children(theme::title_bar(Some(title), 40., Some(p.chrome), &p))
            .child(tabs)
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
