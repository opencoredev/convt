//! The Settings window: General, Presets and License.

use convt_core::{FORMATS, Format, Options, Preset, format_by_id};
use convt_license::client::{BUY_URL, State};
use gpui_kit::component::input::InputState;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{
    self, Choice, Palette, mono, primary_button, secondary_button, text, text_button,
};
use super::{describe, error_text, open_folder};
use crate::finder::EXTENSION_SETTINGS;
use crate::model::AppState;
use crate::settings::{Settings, auto_concurrency};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Presets,
    License,
}

const TABS: [(SettingsTab, &str, &str); 3] = [
    (SettingsTab::General, "tab-general", "General"),
    (SettingsTab::Presets, "tab-presets", "Presets"),
    (SettingsTab::License, "tab-license", "License"),
];

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
        }
    }

    pub fn set_tab(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.open = None;
        cx.notify();
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
            220.,
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
            120.,
            true,
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
        let notifications = theme::checkbox(
            "notifications",
            "Show a notification",
            settings.notifications,
            p,
        )
        .on_click({
            let app = app.clone();
            let on = settings.notifications;
            move |_, _, cx| app.update(cx, |s, cx| s.update_settings(|s| s.notifications = !on, cx))
        });
        let reveal = theme::checkbox(
            "reveal",
            format!("Reveal it in {}", theme::file_manager_name()),
            settings.reveal_when_done,
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
        let menu_bar = theme::switch("menu-bar-icon", settings.menu_bar_icon, false, p).on_click({
            let app = app.clone();
            let on = settings.menu_bar_icon;
            move |_, _, cx| app.update(cx, |s, cx| s.update_settings(|s| s.menu_bar_icon = !on, cx))
        });
        // GPUI has no tray icon on Linux yet; the switch would do nothing.
        let show_menu_bar = !cfg!(target_os = "linux");
        let linux_menu = cfg!(target_os = "linux").then(|| linux_menu_row(&self.app, p, cx));

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
            let (status, button) = match state.finder_on {
                Some(true) => (
                    "On. Right-click a file in Finder to convert.",
                    "Manage in System Settings",
                ),
                Some(false) => (
                    "Off. Open System Settings, scroll to Extensions, and turn on convt.",
                    "Open System Settings",
                ),
                None => (
                    "Open System Settings, scroll to Extensions, and turn on convt.",
                    "Open System Settings",
                ),
            };
            field_top(
                "Finder menu",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .id("finder-status")
                            .test_support()
                            .aria_label(status)
                            .child(text(12., 16., p.secondary).child(status)),
                    )
                    .child(
                        text_button("manage-finder", button, p.green, 12.)
                            .on_click(|_, _, cx| cx.open_url(EXTENSION_SETTINGS)),
                    ),
                p,
            )
        });

        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .px(px(40.))
                    .pt(px(28.))
                    .pb(px(30.))
                    .child(field("Save converted files", output_select, p))
                    .child(field_top(
                        "When a file is done",
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(notifications)
                            .child(reveal),
                        p,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .px(px(40.))
                    .pt(px(22.))
                    .pb(px(28.))
                    .border_t_1()
                    .border_color(p.hairline)
                    .children(finder)
                    .children(linux_menu)
                    .when(show_menu_bar, |d| {
                        d.child(field("Menu bar icon", menu_bar, p))
                    })
                    .child(field(
                        "Jobs at once",
                        div()
                            .flex()
                            .items_center()
                            .gap(px(14.))
                            .child(
                                div()
                                    .id("concurrency")
                                    .test_support()
                                    .aria_label(SharedString::from(match settings.concurrency {
                                        Some(n) => n.to_string(),
                                        None => format!("Auto ({auto})"),
                                    }))
                                    .child(jobs_select),
                            )
                            .child(text(12., 16., p.tertiary).child("Auto runs one per CPU core")),
                        p,
                    ))
                    .children(errors.into_iter().map(|e| text(12., 16., p.error).child(e))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .px(px(40.))
                    .pt(px(22.))
                    .pb(px(28.))
                    .border_t_1()
                    .border_color(p.hairline)
                    .child(field_top("Documents", documents, p)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .px(px(40.))
                    .pt(px(22.))
                    .pb(px(28.))
                    .border_t_1()
                    .border_color(p.hairline)
                    .child(field_top(
                        "Update checks",
                        super::update::settings_row(&self.app, p, cx),
                        p,
                    ))
                    .child(field_top("Network", network(p), p)),
            )
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
        let rows = list.into_iter().map(|(name, about)| {
            let valid = about.is_ok();
            let (about, color) = match about {
                Ok(text) => (text, p.secondary),
                Err(e) => (e, p.error),
            };
            let edit_name = name.clone();
            let edit =
                valid.then(|| {
                    text_button(
                        SharedString::from(format!("edit-preset-{name}")),
                        "Edit",
                        p.text,
                        12.,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.edit_preset(&edit_name, window, cx)
                    }))
                });
            let app = self.app.clone();
            let delete_name = name.clone();
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .py(px(10.))
                .border_b_1()
                .border_color(p.row_divider)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .gap(px(2.))
                        .child(text(13., 16., p.text).child(name.clone()))
                        .child(mono(11., 14., color).child(about)),
                )
                .children(edit)
                .child(
                    text_button(
                        SharedString::from(format!("delete-preset-{name}")),
                        "Delete",
                        p.secondary,
                        12.,
                    )
                    .on_click(move |_, _, cx| {
                        if let Err(e) = app.update(cx, |s, cx| s.delete_preset(&delete_name, cx)) {
                            tracing::warn!(error = %e, "could not delete a preset");
                        }
                    }),
                )
        });
        let chips = div()
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .children(self.outputs.iter().map(|&to| {
                let on = self.preset_to == Some(to);
                let weak = cx.entity().downgrade();
                theme::clickable(SharedString::from(format!("new-to-{}", to.id)), to.name)
                    .aria_selected(on)
                    .px(px(8.))
                    .py(px(2.))
                    .rounded(px(5.))
                    .bg(if on { p.green_tint } else { p.chip })
                    .border_1()
                    .border_color(if on { p.green } else { p.chip_border })
                    .on_click(move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.preset_to = (this.preset_to != Some(to)).then_some(to);
                            cx.notify();
                        });
                    })
                    .child(text(12., 16., if on { p.green } else { p.text }).child(to.name))
            }));
        let form = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .pt(px(18.))
            .child(
                text(12., 16., p.secondary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(match &self.editing {
                        Some(name) => format!("Edit preset \"{name}\""),
                        None => "New preset".to_string(),
                    }),
            )
            .child(theme::field(&self.preset_name, "preset-name"))
            .child(text(11., 14., p.secondary).child("Target (optional)"))
            .child(chips)
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(theme::field(&self.preset_quality, "preset-quality"))
                    .child(theme::field(&self.preset_max_size, "preset-max-size")),
            )
            .children(self.preset_error.clone().map(|e| error_text(e, p)))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(10.))
                    .when(self.editing.is_some(), |row| {
                        row.child(secondary_button("cancel-edit", "Cancel", p).on_click(
                            cx.listener(|this, _, window, cx| this.reset_preset_form(window, cx)),
                        ))
                    })
                    .child(
                        primary_button("save-preset", "Save preset", 13., false).on_click(
                            cx.listener(|this, _, window, cx| this.save_preset(window, cx)),
                        ),
                    ),
            );
        div()
            .flex()
            .flex_col()
            .px(px(40.))
            .pt(px(24.))
            .pb(px(28.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pb(px(4.))
                    .child(
                        text(12., 16., p.secondary)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(if empty { "No presets yet." } else { "Presets" }),
                    )
                    .children(dir.map(|dir| {
                        text_button("open-presets", "Open presets folder", p.green, 12.)
                            .on_click(move |_, _, cx| open_folder(&dir, cx))
                    })),
            )
            .children(rows)
            .child(form)
    }

    fn license(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let state = self.app.read(cx).license.clone();
        let summary = SharedString::from(state.summary());
        let status = div()
            .id("license-status")
            .test_support()
            .aria_label(summary.clone())
            .child(
                text(
                    13.,
                    19.,
                    if state.allows_conversion() {
                        p.text
                    } else {
                        p.error
                    },
                )
                .child(summary),
            );
        let body = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(40.))
            .pt(px(28.))
            .pb(px(30.));
        let account = super::account::section(&self.app, p, cx);
        if state == State::Unrestricted {
            return div()
                .flex()
                .flex_col()
                .child(body.child(status))
                .child(account);
        }
        let licensed = matches!(state, State::Licensed(_) | State::NotCovered(_));
        let notice = self.license_notice.clone().map(|message| {
            div()
                .id("license-notice")
                .test_support()
                .aria_label(SharedString::from(message.clone()))
                .child(
                    text(
                        12.,
                        16.,
                        if state.allows_conversion() {
                            p.green
                        } else {
                            p.secondary
                        },
                    )
                    .child(message),
                )
        });
        let license = body
            .child(status)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .child(theme::field(&self.license_key, "license-key")),
                    )
                    .child(primary_button("activate", "Activate", 13., false).on_click(
                        cx.listener(|this, _, window, cx| this.activate_license(window, cx)),
                    )),
            )
            .children(notice)
            .children(self.license_error.clone().map(|e| error_text(e, p)))
            .child(
                div()
                    .flex()
                    .gap(px(14.))
                    .when(!matches!(state, State::Licensed(_)), |row| {
                        row.child(
                            text_button(
                                "settings-buy",
                                if matches!(state, State::NotCovered(_)) {
                                    "Renew"
                                } else {
                                    "Buy a license"
                                },
                                p.green,
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
        div().flex().flex_col().child(license).child(account)
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

/// What reaches the network without a click, for the General tab. Update
/// checks and license refresh are listed together, as the privacy policy
/// lists them.
pub(super) const NETWORK_LINES: [(&str, &str); 3] = [
    (
        "network-updates",
        "Update checks: while they're on, once a day at launch and when you click Check now, \
         convt downloads the signed list of releases from convt.app. The request carries the \
         app version and nothing about your files.",
    ),
    (
        "network-refresh",
        "License refresh: only while you're signed in to convt.app, once a day at launch, \
         to fetch your current Pro key. See License.",
    ),
    (
        "network-other",
        "Anything else, such as downloading document support, waits for your click. \
         Your files never leave this computer.",
    ),
];

fn network(p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.))
        .gap(px(6.))
        .children(NETWORK_LINES.iter().map(|&(id, line)| {
            div()
                .id(id)
                .test_support()
                .aria_label(line)
                .child(text(12., 17., p.secondary).child(line))
        }))
}

fn linux_menu_row(app: &Entity<AppState>, p: &Palette, cx: &App) -> Div {
    use crate::linux_menu::Status;
    let status = app.read(cx).linux_menu.clone();
    let (summary, color) = match &status {
        Status::Installing => ("Installing…".to_string(), p.secondary),
        Status::Removing => ("Removing…".to_string(), p.secondary),
        Status::Installed(kinds) => (
            format!("On. Right-click a file in {} to convert.", kinds.join(", ")),
            p.green,
        ),
        Status::System(kinds) => (
            format!("On. The package installed menus for {}.", kinds.join(", ")),
            p.green,
        ),
        Status::MissingInstaller => ("This build has no menu installer.".to_string(), p.secondary),
        Status::Failed(e) => (e.clone(), p.error),
        Status::NotInstalled | Status::Unavailable => (
            "Not set up. GNOME Files, Dolphin and Nemo get a Convert with convt menu.".to_string(),
            p.secondary,
        ),
    };
    let busy = matches!(status, Status::Installing | Status::Removing);
    let show_setup = matches!(
        status,
        Status::NotInstalled | Status::Failed(_) | Status::MissingInstaller
    );
    let show_remove = matches!(status, Status::Installed(_));
    field_top(
        "Right-click menu",
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .id("linux-menu-status")
                    .test_support()
                    .aria_label(SharedString::from(summary.clone()))
                    .child(text(12., 16., color).child(summary)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .when(show_setup && !busy, |row| {
                        let app = app.clone();
                        row.child(
                            text_button(
                                "setup-linux-menu",
                                "Set up right-click menu",
                                p.green,
                                12.,
                            )
                            .on_click(move |_, _, cx| {
                                app.update(cx, |s, cx| s.install_linux_menu(cx))
                            }),
                        )
                    })
                    .when(show_remove && !busy, |row| {
                        let app = app.clone();
                        row.child(
                            text_button("remove-linux-menu", "Remove", p.secondary, 12.).on_click(
                                move |_, _, cx| app.update(cx, |s, cx| s.remove_linux_menu(cx)),
                            ),
                        )
                    }),
            ),
        p,
    )
}

/// A right-aligned label and its control, as in the design's settings rows.
fn field(label: &'static str, control: impl IntoElement, p: &Palette) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(14.))
        .child(
            text(13., 16., p.text)
                .w(px(170.))
                .flex_shrink_0()
                .text_right()
                .child(label),
        )
        .child(control)
}

/// [`field`] for a control taller than one line.
fn field_top(label: &'static str, control: impl IntoElement, p: &Palette) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(14.))
        .child(
            text(13., 16., p.text)
                .w(px(170.))
                .flex_shrink_0()
                .text_right()
                .child(label),
        )
        .child(control)
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
            .map_or("Settings", |(_, _, label)| label);
        let tabs = div()
            .flex()
            .flex_shrink_0()
            .justify_center()
            .gap(px(4.))
            .px(px(16.))
            .pb(px(10.))
            .when(!theme::transparent_titlebar(), |d| d.pt(px(10.)))
            .bg(p.chrome)
            .border_b_1()
            .border_color(p.chrome_border)
            .children(TABS.iter().map(|&(tab, id, label)| {
                let on = self.tab == tab;
                theme::clickable(id, label)
                    .aria_selected(on)
                    .px(px(14.))
                    .py(px(5.))
                    .rounded(px(6.))
                    .when(on, |d| d.bg(p.tab_selected))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_tab(tab, cx)))
                    .child(
                        text(12., 16., if on { p.text } else { p.secondary })
                            .when(on, |d| d.font_weight(FontWeight::MEDIUM))
                            .child(label),
                    )
            }));
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
                    .child(body),
            )
    }
}
