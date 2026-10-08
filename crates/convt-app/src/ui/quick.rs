//! Quick convert: the window for files sent without a target, from "More
//! options…" in the Finder menu, a `convt://` link, "Open with" or the
//! command line. It shows the files, lets the user pick a target and options,
//! and follows the conversion until it is done.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use convt_core::{
    Background, Category, Format, Options, Output, Registry, VideoCodec, format_by_id,
};
use gpui_kit::component::input::InputState;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use gpui_kit::component::IconName;

use super::theme::{
    self, Button, Choice, Palette, Segment, Tone, icon, mono, radius, size, space, styled,
};
use super::{blocked_banner, error_text, file_size, human_size, time_left};
use crate::cloud::CloudAccess;
use crate::jobs::{Entry, JobId, Status};
use crate::model::{self, AppState, PackPhase, Targets};
use crate::pack;
use crate::request::Request;

/// The quality segments: key, label and the quality option they set.
/// Balanced leaves quality to the engine.
const QUALITY: [(&str, &str, Option<u8>); 3] = [
    ("smaller", "Smaller", Some(60)),
    ("balanced", "Balanced", None),
    ("best", "Best", Some(95)),
];

/// The segment key for a quality value; "preset" for a value a preset set
/// that no segment matches.
fn quality_key(quality: Option<u8>) -> &'static str {
    QUALITY
        .iter()
        .find(|(_, _, q)| *q == quality)
        .map_or("preset", |(key, ..)| key)
}

/// The dropdown that is open, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    Size,
    Codec,
    Background,
}

pub struct QuickView {
    app: Entity<AppState>,
    pub(super) files: Vec<PathBuf>,
    pub(super) targets: Targets,
    pub(super) to: Option<&'static Format>,
    pub(super) preset: Option<String>,
    /// The preset's options, which the controls start from.
    options: Options,
    /// The Quality control. `None` is Balanced: the engine's default.
    pub(super) quality: Option<u8>,
    /// Video height, longest image edge or audio bitrate, by target kind.
    /// `None` is Original.
    pub(super) size: Option<u32>,
    /// The Codec control for MP4, MOV and MKV. `None` shows H.264, the
    /// engine's default.
    pub(super) video_codec: Option<VideoCodec>,
    /// The Keep audio checkbox, unchecked.
    pub(super) strip_audio: bool,
    /// The Background control for image targets. `None` shows the engine's
    /// default: Transparent where the format keeps it, White where it can't.
    pub(super) background: Option<Background>,
    open: Option<Open>,
    /// Where the files go. `None` is next to each file.
    pub(super) save_dir: Option<PathBuf>,
    pub(super) file_name: Entity<InputState>,
    pub(super) jobs: Vec<JobId>,
    /// The last state seen of each job, so results stay after the main
    /// window clears finished jobs.
    seen: HashMap<JobId, Entry>,
    pub(super) error: Option<String>,
    /// The target the request named, kept while the document pack it needs
    /// isn't installed, and picked once it is.
    pub(super) wanted: Option<&'static Format>,
    /// Some file needed the document pack when the window opened.
    pub(super) offered_pack: bool,
    /// Convert on convt's cloud instead of on this computer.
    pub(super) cloud: bool,
    /// The registry the targets were computed with.
    generation: u64,
    _observe: Subscription,
    _appearance: Subscription,
}

impl QuickView {
    pub fn new(
        app: Entity<AppState>,
        request: Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = app.read(cx);
        let expanded = model::expand_keeping_documents(&state.registry, &request.files);
        let mut files = expanded.files;
        let mut seen = std::collections::HashSet::new();
        files.retain(|f| seen.insert(f.clone()));
        let targets = model::common_targets(&state.registry, &files);
        let generation = state.registry_generation;
        let offered_pack = files.iter().any(|f| pack::needs_pack(&state.registry, f));
        let mut wanted = None;
        let save_dir = state.settings.output_dir.clone();
        let mut error = None;
        let (to, options) =
            match model::resolve(state, request.to.as_deref(), request.preset.as_deref()) {
                Ok(resolved) => resolved,
                Err(e) => {
                    error = Some(e);
                    (None, Options::default())
                }
            };
        let to = to.filter(|f| {
            let reachable = targets.formats.contains(f);
            if !reachable && offered_pack {
                // The card offers the pack; the target waits for it.
                wanted = Some(*f);
            } else if !reachable && error.is_none() {
                error = Some(unreachable(f, files.len()));
            }
            reachable
        });
        if !expanded.unreadable.is_empty() {
            error = Some(expanded.unreadable.join("\n"));
        } else if files.is_empty() {
            error = Some("There are no files to convert here.".into());
        }
        let file_name = cx.new(|cx| InputState::new(window, cx));
        let mut view = Self {
            _observe: cx.observe_in(&app, window, |this: &mut Self, app, window, cx| {
                this.remember(&app, cx);
                this.follow_registry(window, cx);
                cx.notify();
            }),
            _appearance: theme::observe_appearance(window, cx),
            app,
            files,
            targets,
            to,
            preset: request.preset,
            options,
            quality: None,
            size: None,
            video_codec: None,
            strip_audio: false,
            background: None,
            open: None,
            save_dir,
            file_name,
            jobs: Vec::new(),
            seen: HashMap::new(),
            error,
            wanted,
            offered_pack,
            cloud: false,
            generation,
        };
        view.load_controls();
        view.reset_name(window, cx);
        view
    }

    /// Recomputes the targets after the document pack was installed or
    /// removed, and picks the target the request named once it is reachable.
    /// Converting still takes a click on Convert.
    fn follow_registry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.app.read(cx);
        if state.registry_generation == self.generation || !self.jobs.is_empty() {
            return;
        }
        self.generation = state.registry_generation;
        self.targets = model::common_targets(&state.registry, &self.files);
        if self
            .to
            .is_some_and(|to| !self.targets.formats.contains(&to))
        {
            self.to = None;
        }
        if self.to.is_none()
            && let Some(wanted) = self.wanted.filter(|f| self.targets.formats.contains(f))
        {
            self.wanted = None;
            self.pick(wanted, window, cx);
        }
    }

    /// Files that only the document pack could convert.
    fn waiting_for_pack(&self, cx: &App) -> usize {
        let registry = &self.app.read(cx).registry;
        self.files
            .iter()
            .filter(|f| pack::needs_pack(registry, f))
            .count()
    }

    /// Sets the controls to the preset's options for the current target.
    fn load_controls(&mut self) {
        let o = &self.options;
        self.quality = o.quality;
        self.video_codec = o.video_codec;
        self.strip_audio = o.strip_audio;
        self.background = o.background;
        self.size = self.preset_size();
    }

    /// The preset's size for the current target's kind.
    fn preset_size(&self) -> Option<u32> {
        let o = &self.options;
        self.to.and_then(|to| match to.category {
            Category::Video => o.video_height,
            Category::Audio => o.audio_bitrate,
            _ => o.max_size,
        })
    }

    /// The output name for one file and the current target.
    fn default_name(&self) -> Option<String> {
        let ([file], Some(to)) = (self.files.as_slice(), self.to) else {
            return None;
        };
        let stem = file.file_stem()?.to_string_lossy();
        Some(format!("{stem}.{}", to.extension()))
    }

    fn reset_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.default_name().unwrap_or_default();
        self.file_name
            .update(cx, |s, cx| s.set_value(name, window, cx));
    }

    pub(super) fn pick(
        &mut self,
        to: &'static Format,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = self.to != Some(to);
        self.to = Some(to);
        // Size means something else for each kind of target.
        if changed {
            self.size = self.preset_size();
        }
        self.error = None;
        self.reset_name(window, cx);
        cx.notify();
    }

    fn pick_preset(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        match model::resolve(self.app.read(cx), None, Some(name)) {
            Ok((to, options)) => {
                self.preset = Some(name.to_string());
                self.options = options;
                self.error = None;
                if let Some(to) = to {
                    if self.targets.formats.contains(&to) {
                        self.to = Some(to);
                        self.reset_name(window, cx);
                    } else {
                        self.to = None;
                        self.error = Some(unreachable(to, self.files.len()));
                    }
                }
                self.load_controls();
            }
            Err(e) => self.error = Some(e),
        }
        cx.notify();
    }

    fn supported(&self) -> Vec<PathBuf> {
        self.files
            .iter()
            .filter(|f| !self.targets.unsupported.contains(f))
            .cloned()
            .collect()
    }

    /// The options to convert with: the preset's, replaced by every control
    /// the target shows. Balanced and Original clear what a preset set.
    pub(super) fn conversion_options(&self) -> Options {
        let mut options = self.options.clone();
        let Some(to) = self.to else { return options };
        if quality_applies(to) {
            options.quality = self.quality;
        }
        if size_choices(to).is_some() {
            match to.category {
                Category::Video => options.video_height = self.size,
                Category::Audio => options.audio_bitrate = self.size,
                _ => options.max_size = self.size,
            }
        }
        if codec_applies(to) {
            options.video_codec = self.video_codec;
        }
        if audio_applies(to) {
            options.strip_audio = self.strip_audio;
        }
        // Where the control is hidden (video to GIF), a preset's color would
        // only make the conversion fail, with nothing in the window to clear it.
        options.background = if background_applies(to, &self.files) {
            shown_background(to, self.background)
        } else {
            None
        };
        options
    }

    fn output(&self, cx: &App) -> Output {
        let name = self.file_name.read(cx).value().trim().to_string();
        if let ([file], Some(default)) = (self.files.as_slice(), self.default_name())
            && !name.is_empty()
            && name != default
        {
            let dir = self
                .save_dir
                .clone()
                .or_else(|| file.parent().map(Path::to_path_buf))
                .unwrap_or_default();
            return Output::Exact(dir.join(name));
        }
        match &self.save_dir {
            Some(dir) => Output::Dir(dir.clone()),
            None => Output::Beside,
        }
    }

    pub(super) fn convert(&mut self, cx: &mut Context<Self>) {
        let Some(to) = self.to else { return };
        let files = self.supported();
        if files.is_empty() || !self.jobs.is_empty() {
            return;
        }
        let options = self.conversion_options();
        if let Err(e) = options.validate() {
            self.error = Some(e.to_string());
            cx.notify();
            return;
        }
        let output = self.output(cx);
        let cloud = self.in_cloud(cx);
        if cloud && !self.app.read(cx).settings.cloud_consent {
            return;
        }
        let queued = self.app.update(cx, |s, cx| {
            if cloud {
                s.convert_in_cloud(&files, to, &options, output, cx)
            } else {
                s.convert_to(&files, to, &options, output, cx)
            }
        });
        let jobs = match queued {
            Ok(jobs) => jobs,
            Err(e) => {
                // When the license state stops conversions, the banner says
                // why; anything else (such as a trial that couldn't be
                // recorded) is shown here.
                if self.app.read(cx).license.allows_conversion() {
                    self.error = Some(e);
                }
                cx.notify();
                return;
            }
        };
        self.jobs = jobs;
        let app = self.app.clone();
        self.remember(&app, cx);
        cx.notify();
    }

    /// Whether Cloud conversions can run, and if not, why.
    fn cloud_access(&self, cx: &App) -> CloudAccess {
        #[cfg(test)]
        if let Some(access) = cx.try_global::<TestCloud>() {
            return access.0.clone();
        }
        self.app.read(cx).cloud_access()
    }

    /// Cloud is picked and can run.
    fn in_cloud(&self, cx: &App) -> bool {
        self.cloud && self.cloud_access(cx).ready()
    }

    pub(super) fn set_cloud(&mut self, cloud: bool, cx: &mut Context<Self>) {
        self.cloud = cloud && self.cloud_access(cx).ready();
        self.error = None;
        cx.notify();
    }

    /// The user agreed that Cloud uploads their files. Asked once.
    pub(super) fn agree_to_cloud(&mut self, cx: &mut Context<Self>) {
        self.app.update(cx, |s, cx| {
            s.update_settings(|s| s.cloud_consent = true, cx)
        });
        cx.notify();
    }

    /// Asks, the first time Cloud is picked, whether the files may be
    /// uploaded.
    fn consent(&self, p: &Palette, cx: &mut Context<Self>) -> Option<Div> {
        if !self.jobs.is_empty() || !self.in_cloud(cx) || self.app.read(cx).settings.cloud_consent {
            return None;
        }
        let what = if self.files.len() == 1 {
            "this file"
        } else {
            "these files"
        };
        Some(
            div().flex_shrink_0().px(px(GUTTER)).pb(px(space::LG)).child(
                div()
                    .id("cloud-consent")
                    .test_support()
                    .aria_label("Upload to convt's cloud?")
                    .child(theme::callout(
                        IconName::Info,
                        Tone::Neutral,
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::LG))
                            .child(
                                theme::callout_words(
                                    "Upload to convt's cloud?",
                                    format!(
                                        "Cloud uploads {what} to convt's servers to convert, then deletes {}.",
                                        if self.files.len() == 1 { "it" } else { "them" }
                                    ),
                                    p,
                                )
                                .flex_1()
                                .min_w_0(),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .gap(px(space::SM))
                                    .child(
                                        Button::secondary("cloud-consent-cancel", "Cancel")
                                            .small()
                                            .build(p)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_cloud(false, cx)
                                            })),
                                    )
                                    .child(
                                        Button::primary("cloud-consent-agree", "Agree")
                                            .small()
                                            .build(p)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.agree_to_cloud(cx)
                                            })),
                                    ),
                            ),
                        p,
                    )),
            ),
        )
    }

    fn choose_folder(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Save here".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut dirs))) = picked.await
                && let Some(dir) = dirs.pop()
            {
                let _ = this.update(cx, |this, cx| {
                    this.save_dir = Some(dir);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn remember(&mut self, app: &Entity<AppState>, cx: &App) {
        let state = app.read(cx);
        for id in &self.jobs {
            if let Some(entry) = state.entry(*id) {
                self.seen.insert(*id, entry.clone());
            }
        }
    }

    /// This window's jobs in order, as last seen.
    fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.jobs.iter().filter_map(|id| self.seen.get(id))
    }

    /// Every output of the finished jobs, in job order.
    fn outputs(&self) -> Vec<PathBuf> {
        self.entries()
            .filter_map(|e| match &e.status {
                Status::Done(out) => Some(out.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    pub(super) fn finished(&self) -> bool {
        !self.jobs.is_empty() && self.entries().all(|e| e.status.is_finished())
    }

    fn header(&self, p: &Palette) -> Div {
        let (title, meta) = match self.files.as_slice() {
            [one] => (
                model::file_name(one).to_string(),
                [
                    convt_core::format_by_extension(one).map(|f| f.name.to_string()),
                    file_size(one).map(human_size),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
            ),
            many => {
                let total: u64 = many.iter().filter_map(|f| file_size(f)).sum();
                (
                    format!("{} files", many.len()),
                    format!("{} files · {}", many.len(), human_size(total)),
                )
            }
        };
        let thumb = match self.files.first() {
            Some(first) => theme::thumbnail(first, 56., 42., p),
            None => div().w(px(56.)).h(px(42.)).into_any_element(),
        };
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(14.))
            .px(px(GUTTER))
            .pt(px(space::XS))
            .pb(px(space::LG))
            .child(thumb)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(3.))
                    .child(
                        styled(size::TITLE, p.text)
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(title),
                    )
                    .child(mono(11., 14., p.secondary).truncate().child(meta)),
            )
            .children(common_folder(&self.files).map(|dir| {
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(5.))
                    .max_w(px(180.))
                    .child(icon(IconName::Folder, 13., p.tertiary))
                    .child(
                        div().min_w_0().child(
                            styled(size::SMALL, p.secondary)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis_start()
                                .child(dir),
                        ),
                    )
            }))
    }

    fn picker(&self, p: &Palette, cx: &mut Context<Self>) -> Option<Div> {
        if self.targets.formats.is_empty() && self.waiting_for_pack(cx) > 0 {
            // The document card says what to do instead.
            return None;
        }
        let video_input = self.files.iter().any(|f| {
            convt_core::format_by_extension(f).is_some_and(|f| f.category == Category::Video)
        });
        let cards = self.targets.formats.iter().map(|&format| {
            let on = self.to == Some(format);
            let weak = cx.entity().downgrade();
            theme::clickable(SharedString::from(format!("to-{}", format.id)), format.name)
                .aria_selected(on)
                .relative()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .gap(px(3.))
                .w(px(CARD_WIDTH))
                .px(px(space::MD))
                .py(px(10.))
                .rounded(px(radius::CARD))
                .map(|d| {
                    if on {
                        d.bg(p.green_tint)
                            .shadow(vec![theme::inset_ring(p.green, 1.5)])
                    } else {
                        d.bg(p.surface)
                            .shadow(vec![theme::inset_ring(p.border, 1.)])
                            .hover(|s| s.bg(p.recessed))
                    }
                })
                .on_click(move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| this.pick(format, window, cx));
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            mono(13., 16., p.text)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format.name),
                        )
                        .children(on.then(|| {
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(16.))
                                .rounded(px(8.))
                                .bg(p.green)
                                .child(icon(IconName::Check, 10., rgb(0xFFFFFF).into()))
                        })),
                )
                .child(
                    styled(size::CAPTION, if on { p.green_text } else { p.secondary })
                        .truncate()
                        .child(blurb(format, video_input)),
                )
        });
        let presets: Vec<(String, String)> = self
            .app
            .read(cx)
            .valid_presets()
            // A preset these files can't reach would only show an error.
            .filter(|(_, preset)| match preset.to.as_deref() {
                None => true,
                Some(to) => convt_core::format_by_id(to)
                    .is_some_and(|to| self.targets.formats.contains(&to)),
            })
            .map(|(name, preset)| (name.clone(), super::describe(preset)))
            .collect();
        let preset_row = (!presets.is_empty()).then(|| {
            div()
                .flex()
                .items_center()
                .flex_wrap()
                .gap(px(6.))
                .pt(px(space::MD))
                .child(
                    styled(size::SMALL, p.secondary)
                        .font_weight(FontWeight::MEDIUM)
                        .pr(px(2.))
                        .child("Presets"),
                )
                .children(presets.into_iter().map(|(name, about)| {
                    let on = self.preset.as_deref() == Some(&name);
                    let weak = cx.entity().downgrade();
                    let label = name.clone();
                    theme::clickable(SharedString::from(format!("preset-{name}")), name.clone())
                        .aria_selected(on)
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .h(px(24.))
                        .px(px(9.))
                        .rounded(px(12.))
                        .bg(if on { p.green_tint } else { p.surface })
                        .shadow(vec![theme::inset_ring(
                            if on { p.green_border } else { p.control_border },
                            1.,
                        )])
                        .when(!on, |d| d.hover(|s| s.bg(p.recessed)))
                        .tooltip(move |window, cx| Tooltip::new(about.clone()).build(window, cx))
                        .on_click(move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| this.pick_preset(&name, window, cx));
                        })
                        .children(on.then(|| icon(IconName::Check, 11., p.green_text)))
                        .child(
                            styled(size::SMALL, if on { p.green_text } else { p.text })
                                .font_weight(FontWeight::MEDIUM)
                                .child(label),
                        )
                }))
        });
        let picker = section("Convert to", p)
            .child(if self.targets.formats.is_empty() {
                theme::callout(
                    IconName::Info,
                    Tone::Neutral,
                    theme::callout_words(
                        "No format fits every file",
                        "Convert files of one kind together, or pick fewer files.",
                        p,
                    ),
                    p,
                )
                .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(space::SM))
                    .children(cards)
                    .into_any_element()
            })
            .children(preset_row);
        Some(picker)
    }

    /// The document pack card, or the line saying it was installed.
    fn pack_section(&self, p: &Palette, cx: &mut Context<Self>) -> Option<Div> {
        let waiting = self.waiting_for_pack(cx);
        let content = if waiting > 0 {
            let mixed = waiting < self.files.len();
            super::pack::card(&self.app, mixed, p, cx).into_any_element()
        } else if self.offered_pack && self.app.read(cx).pack.phase == PackPhase::Done {
            super::pack::installed_notice(p).into_any_element()
        } else {
            return None;
        };
        Some(
            div()
                .flex()
                .flex_col()
                .px(px(GUTTER))
                .pb(px(space::XL))
                .child(content),
        )
    }

    fn toggle(&mut self, which: Open, cx: &mut Context<Self>) {
        self.open = if self.open == Some(which) {
            None
        } else {
            Some(which)
        };
        cx.notify();
    }

    fn options_section(&self, p: &Palette, cx: &mut Context<Self>) -> Option<Div> {
        let to = self.to?;
        let quality = quality_applies(to).then(|| {
            let mut choices: Vec<(&'static str, SharedString)> = QUALITY
                .iter()
                .map(|(key, label, _)| (*key, SharedString::from(*label)))
                .collect();
            // A preset's own quality gets a segment of its own.
            if let Some(q) = self
                .quality
                .filter(|_| quality_key(self.quality) == "preset")
            {
                choices.push(("preset", format!("Preset ({q})").into()));
            }
            let weak = cx.entity().downgrade();
            row_label(
                "Quality",
                theme::segmented(
                    "quality",
                    &choices,
                    quality_key(self.quality),
                    p,
                    move |key, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            if let Some((_, _, q)) = QUALITY.iter().find(|(k, ..)| *k == key) {
                                this.quality = *q;
                            }
                            cx.notify();
                        });
                    },
                ),
                p,
            )
        });
        let size = size_choices(to).map(|(label, choices)| {
            let current = choices
                .iter()
                .find(|(v, _)| *v == self.size)
                .map(|(_, l)| l.to_string())
                .or_else(|| self.size.map(|v| size_label(to, v)))
                .unwrap_or_else(|| "Original".into());
            let toggle = cx.entity().downgrade();
            let pick = cx.entity().downgrade();
            row_label(
                label,
                theme::select(
                    "size",
                    current,
                    SELECT_WIDTH,
                    false,
                    self.open == Some(Open::Size),
                    choices
                        .iter()
                        .map(|(v, l)| {
                            Choice::new(v.map_or("original".into(), |v| v.to_string()), *l)
                        })
                        .collect(),
                    p,
                    move |_, cx| {
                        let _ = toggle.update(cx, |this, cx| this.toggle(Open::Size, cx));
                    },
                    move |id, _, cx| {
                        let value = id.parse().ok();
                        let _ = pick.update(cx, |this, cx| {
                            this.size = value;
                            this.open = None;
                            cx.notify();
                        });
                    },
                ),
                p,
            )
        });
        let codec = codec_applies(to).then(|| {
            let toggle = cx.entity().downgrade();
            let pick = cx.entity().downgrade();
            row_label(
                "Codec",
                theme::select(
                    "codec",
                    self.video_codec.unwrap_or(VideoCodec::H264).name(),
                    SELECT_WIDTH,
                    false,
                    self.open == Some(Open::Codec),
                    VideoCodec::ALL
                        .iter()
                        .map(|c| Choice::new(c.id(), c.name()))
                        .collect(),
                    p,
                    move |_, cx| {
                        let _ = toggle.update(cx, |this, cx| this.toggle(Open::Codec, cx));
                    },
                    move |id, _, cx| {
                        let codec = id.parse().ok();
                        let _ = pick.update(cx, |this, cx| {
                            this.video_codec = codec;
                            this.open = None;
                            cx.notify();
                        });
                    },
                ),
                p,
            )
        });
        let background = background_applies(to, &self.files).then(|| {
            let toggle = cx.entity().downgrade();
            let pick = cx.entity().downgrade();
            let current = shown_background(to, self.background);
            let mut choices: Vec<Background> = Background::CHOICES
                .into_iter()
                .filter(|b| *b != Background::Transparent || to.keeps_transparency())
                .collect();
            // A color a preset set stays pickable.
            for custom in [current, shown_background(to, self.options.background)]
                .into_iter()
                .flatten()
            {
                if !choices.contains(&custom) {
                    choices.push(custom);
                }
            }
            let default = default_background(&self.app.read(cx).registry, to, &self.files);
            row_label(
                "Background",
                theme::select(
                    "background",
                    current
                        .or(default)
                        .map_or_else(|| "Automatic".into(), Background::name),
                    SELECT_WIDTH,
                    false,
                    self.open == Some(Open::Background),
                    std::iter::once(Choice::new("automatic", "Automatic"))
                        .chain(choices.iter().map(|b| Choice::new(b.id(), b.name())))
                        .collect(),
                    p,
                    move |_, cx| {
                        let _ = toggle.update(cx, |this, cx| this.toggle(Open::Background, cx));
                    },
                    move |id, _, cx| {
                        let background = id.parse().ok();
                        let _ = pick.update(cx, |this, cx| {
                            this.background = background;
                            this.open = None;
                            cx.notify();
                        });
                    },
                ),
                p,
            )
        });
        let audio = audio_applies(to).then(|| {
            row_label(
                "Audio",
                theme::checkbox("keep-audio", "Keep audio", !self.strip_audio, p).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.strip_audio = !this.strip_audio;
                        cx.notify();
                    }),
                ),
                p,
            )
        });
        let rows: Vec<AnyElement> = [quality, size, codec, background, audio]
            .into_iter()
            .flatten()
            .collect();
        if rows.is_empty() {
            return None;
        }
        Some(section("Options", p).child(theme::group(rows, p)))
    }

    fn save_section(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let place = match &self.save_dir {
            Some(dir) => super::tilde(dir),
            None => "Same folder".to_string(),
        };
        let name = (self.files.len() == 1 && self.to.is_some()).then(|| {
            row_label(
                "File name",
                div()
                    .w(px(260.))
                    .flex_shrink_0()
                    .font_family(theme::MONO)
                    .text_size(px(12.))
                    .child(theme::small_field(&self.file_name, "file-name")),
                p,
            )
        });
        let folder = row_label(
            "Save to",
            div()
                .flex()
                .items_center()
                .gap(px(space::SM))
                .max_w(px(360.))
                .child(icon(IconName::Folder, 13., p.tertiary))
                .child(
                    div()
                        .id("save-to")
                        .test_support()
                        .aria_label(SharedString::from(place.clone()))
                        .min_w_0()
                        .child(
                            styled(size::SMALL, p.text)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis_start()
                                .child(place),
                        ),
                )
                .when(self.save_dir.is_some(), |d| {
                    d.child(
                        Button::ghost("same-folder", "Same folder")
                            .small()
                            .build(p)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_dir = None;
                                cx.notify();
                            })),
                    )
                })
                .child(
                    Button::secondary("change-folder", "Change…")
                        .small()
                        .build(p)
                        .on_click(cx.listener(|this, _, _, cx| this.choose_folder(cx))),
                ),
            p,
        );
        section("Save", p).child(theme::group(std::iter::once(folder).chain(name), p))
    }

    fn progress_section(&self, p: &Palette) -> Stateful<Div> {
        let now = std::time::Instant::now();
        let rows = self.entries().map(|entry| {
            let id = entry.id;
            let status = match &entry.status {
                Status::Queued => "Waiting".to_string(),
                Status::Running(f) => {
                    let pct = f.map_or("Converting".into(), |f| format!("{:.0}%", f * 100.));
                    match entry.remaining(now) {
                        Some(left) => format!("{pct} · {}", time_left(left)),
                        None => pct,
                    }
                }
                Status::Done(outputs) => match outputs.as_slice() {
                    [one] => format!("Saved {}", model::file_name(one)),
                    many => format!("Saved {} files", many.len()),
                },
                Status::Failed(e) => e.message.clone(),
                Status::Cancelled => "Cancelled".into(),
            };
            let (color, glyph) = match entry.status {
                Status::Done(_) => (p.green_text, Some((IconName::CircleCheck, p.green))),
                Status::Failed(_) => (p.error, Some((IconName::CircleX, p.error))),
                Status::Cancelled => (p.secondary, Some((IconName::Ban, p.tertiary))),
                _ => (p.secondary, None),
            };
            let bar = match entry.status {
                Status::Running(f) => Some(theme::progress(f.unwrap_or(0.), p.track, p.green)),
                Status::Queued => Some(theme::progress(0., p.track, p.green)),
                _ => None,
            };
            div()
                .id(SharedString::from(format!("job-{id}")))
                .flex()
                .items_center()
                .gap(px(14.))
                .px(px(space::LG))
                .py(px(space::MD))
                .child(theme::thumbnail(&entry.input, 48., 36., p))
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
                                .items_center()
                                .gap(px(6.))
                                .min_w_0()
                                .child(
                                    styled(size::BODY, p.text)
                                        .font_weight(FontWeight::MEDIUM)
                                        .truncate()
                                        .child(model::file_name(&entry.input).to_string()),
                                )
                                .child(icon(IconName::ArrowRight, 12., p.tertiary))
                                .child(theme::badge(entry.to.name, Tone::Neutral, p)),
                        )
                        .children(bar)
                        .child(
                            div()
                                .id(SharedString::from(format!("status-{id}")))
                                .test_support()
                                .aria_label(SharedString::from(status.clone()))
                                .flex()
                                .items_center()
                                .gap(px(5.))
                                .children(glyph.map(|(g, c)| icon(g, 13., c)))
                                .child(styled(size::SMALL, color).child(status)),
                        ),
                )
                .into_any_element()
        });
        div()
            .id("jobs")
            .flex()
            .flex_col()
            .flex_1()
            .px(px(GUTTER))
            .pb(px(space::XL))
            .overflow_y_scroll()
            .child(theme::section_label("Converting", p))
            .child(theme::group(rows.collect::<Vec<_>>(), p))
    }

    fn footer(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let bar = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(space::SM))
            .px(px(GUTTER))
            .h(px(60.))
            .bg(p.chrome)
            .border_t_1()
            .border_color(p.chrome_border);
        if !self.jobs.is_empty() {
            let outputs = self.outputs();
            let first = outputs.first().cloned();
            let only = (outputs.len() == 1).then(|| outputs[0].clone());
            let done = self.finished();
            let status = match (done, self.cloud) {
                (true, _) => "Done",
                (false, true) => "Converting in the cloud…",
                (false, false) => "Converting…",
            };
            return bar
                .child(
                    styled(size::SMALL, p.secondary)
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(status),
                )
                .children(first.filter(|_| done).map(|path| {
                    Button::secondary("show-in-folder", "Show in folder")
                        .icon(IconName::FolderOpen)
                        .build(p)
                        .on_click(move |_, _, cx| cx.reveal_path(&path))
                }))
                .children(only.filter(|_| done).map(|path| {
                    Button::secondary("open", "Open")
                        .build(p)
                        .on_click(move |_, _, cx| cx.open_with_system(&path))
                }))
                .child(
                    Button::primary("close", "Close")
                        .build(p)
                        .px(px(18.))
                        .on_click(|_, window, _| window.remove_window()),
                );
        }
        let access = self.cloud_access(cx);
        let cloud = self.in_cloud(cx);
        let state = self.app.read(cx);
        let disabled = self.to.is_none()
            || self.supported().is_empty()
            || !state.license.allows_conversion()
            || (cloud && !state.settings.cloud_consent);
        let label = match self.to {
            Some(to) if !disabled => format!("Convert to {}", to.name),
            _ => "Convert".to_string(),
        };
        let weak = cx.entity().downgrade();
        bar.child(div().flex().flex_1().min_w_0().child(theme::segmented_with(
            "where",
            &[
                Segment::new("local", theme::this_machine_label()),
                Segment::new("cloud", "Cloud").disabled(access.reason()),
            ],
            if cloud { "cloud" } else { "local" },
            p,
            move |key, _, cx| {
                let _ = weak.update(cx, |this, cx| this.set_cloud(key == "cloud", cx));
            },
        )))
        .child(
            Button::ghost("cancel", "Cancel")
                .build(p)
                .on_click(|_, window, _| window.remove_window()),
        )
        .child(
            Button::primary("convert", label)
                .disabled(disabled)
                .build(p)
                .px(px(18.))
                .when(!disabled, |d| {
                    d.on_click(cx.listener(|this, _, _, cx| this.convert(cx)))
                }),
        )
    }
}

/// Tests set this to show Cloud as the account would allow it.
#[cfg(test)]
pub(super) struct TestCloud(pub CloudAccess);

#[cfg(test)]
impl Global for TestCloud {}

/// Space between the window edge and the content.
const GUTTER: f32 = 24.;
/// Four format cards to a row.
const CARD_WIDTH: f32 = 132.;
const SELECT_WIDTH: f32 = 190.;

/// A titled part of the window.
fn section(label: &'static str, p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .px(px(GUTTER))
        .pb(px(space::XL))
        .child(theme::section_label(label, p))
}

/// An option row: the label on the left, the control on the right.
fn row_label(label: &'static str, control: impl IntoElement, p: &Palette) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(space::LG))
        .min_h(px(46.))
        .px(px(space::LG))
        .py(px(space::SM))
        .child(
            styled(size::BODY, p.text)
                .font_weight(FontWeight::MEDIUM)
                .flex_1()
                .child(label),
        )
        .child(div().flex().flex_shrink_0().items_center().child(control))
        .into_any_element()
}

/// Whether the Codec control does anything for `to`.
fn codec_applies(to: &Format) -> bool {
    matches!(to.id, "mp4" | "mov" | "mkv")
}

/// The source formats of `files`, by extension.
fn sources(files: &[PathBuf]) -> impl Iterator<Item = &'static Format> + '_ {
    files
        .iter()
        .filter_map(|f| convt_core::format_by_extension(f))
}

/// Whether the Background control applies: image targets, except GIF from
/// video, where the engines don't take a background color yet.
fn background_applies(to: &Format, files: &[PathBuf]) -> bool {
    to.category == Category::Image
        && !(to.id == "gif" && sources(files).any(|f| f.category == Category::Video))
}

/// What the Background control shows with nothing picked: what the engines
/// do by default, or `None` (Automatic) when the files would differ.
fn default_background(registry: &Registry, to: &Format, files: &[PathBuf]) -> Option<Background> {
    let mut defaults = sources(files).map(|from| route_default(registry, from, to));
    let first = defaults.next()?;
    defaults.all(|d| d == first).then_some(first)
}

/// The default background for one route: white for formats without
/// transparency and for anything rendered from PDF (documents go through
/// PDF too), which renders on white like a PDF viewer; otherwise kept.
fn route_default(registry: &Registry, from: &'static Format, to: &Format) -> Background {
    let via_pdf = from.id == "pdf"
        || format_by_id(to.id)
            .and_then(|to| registry.plan(from, to).ok())
            .is_some_and(|plan| plan.hops.iter().any(|(_, step)| step.from.id == "pdf"));
    if !to.keeps_transparency() || via_pdf {
        Background::WHITE
    } else {
        Background::Transparent
    }
}

/// The background to convert with: Transparent picked for a format that
/// keeps it, then a switch to one that can't (JPEG), falls back to the
/// default white instead of failing.
fn shown_background(to: &Format, picked: Option<Background>) -> Option<Background> {
    match picked {
        Some(Background::Transparent) if !to.keeps_transparency() => None,
        other => other,
    }
}

/// Whether `to` is video that can carry audio, for Keep audio.
fn audio_applies(to: &Format) -> bool {
    to.category == Category::Video && to.id != "gif"
}

/// A size no menu entry matches, such as one a preset set.
fn size_label(to: &Format, value: u32) -> String {
    match to.category {
        Category::Video => format!("{value}p"),
        Category::Audio => format!("{value} kbit/s"),
        _ => format!("{value} px"),
    }
}

/// Whether the quality control does anything for `to`.
fn quality_applies(to: &Format) -> bool {
    to.category == Category::Video || matches!(to.id, "jpeg" | "avif" | "webp")
}

/// A size value (`None` for the original) and its label.
type SizeChoice = (Option<u32>, &'static str);

/// The size control for `to`: its label and the choices, `None` being the
/// original.
fn size_choices(to: &Format) -> Option<(&'static str, &'static [SizeChoice])> {
    match to.category {
        Category::Video => Some((
            "Resolution",
            &[
                (None, "Original"),
                (Some(2160), "2160p"),
                (Some(1080), "1080p"),
                (Some(720), "720p"),
                (Some(480), "480p"),
            ],
        )),
        Category::Image if to.id != "ico" => Some((
            "Size",
            &[
                (None, "Original"),
                (Some(4096), "4096 px"),
                (Some(2048), "2048 px"),
                (Some(1024), "1024 px"),
                (Some(512), "512 px"),
            ],
        )),
        Category::Audio if !matches!(to.id, "wav" | "flac") => Some((
            "Bitrate",
            &[
                (None, "Original"),
                (Some(320), "320 kbit/s"),
                (Some(192), "192 kbit/s"),
                (Some(128), "128 kbit/s"),
            ],
        )),
        _ => None,
    }
}

/// A few words on what a target is good for.
fn blurb(to: &Format, video_input: bool) -> &'static str {
    if video_input && to.category == Category::Audio {
        return "Audio only";
    }
    match to.id {
        "mp4" => "Most players",
        "webm" => "For the web",
        "gif" => "Short loops",
        "mkv" => "Keeps tracks",
        "mov" => "For editing",
        "avi" => "Older players",
        "mp3" => "Any player",
        "wav" => "Uncompressed",
        "flac" => "Lossless",
        "aac" | "m4a" => "Small, clear",
        "jpeg" => "Photos",
        "png" => "Lossless",
        "webp" => "Small, sharp",
        "avif" => "Smallest",
        "pdf" => "Print, share",
        _ => match to.category {
            Category::Image | Category::Vector => "Image",
            Category::Video => "Video",
            Category::Audio => "Audio",
            Category::Pdf => "Document",
            Category::Document => "Document",
            Category::Presentation => "Slides",
            Category::Spreadsheet => "Spreadsheet",
        },
    }
}

/// The folder every file is in, as "~/Downloads", or `None` if they differ.
fn common_folder(files: &[PathBuf]) -> Option<String> {
    let first = files.first()?.parent()?;
    files
        .iter()
        .all(|f| f.parent() == Some(first))
        .then(|| super::tilde(first))
}

fn unreachable(to: &Format, files: usize) -> String {
    let ext = to.extension().to_uppercase();
    if files == 1 {
        format!("This file can't become {ext}.")
    } else {
        format!("Not every file can become {ext}.")
    }
}

impl Render for QuickView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        // Documents waiting for the pack have their own card.
        let waiting = self.waiting_for_pack(cx);
        let skipped = match self.targets.unsupported.len().saturating_sub(waiting) {
            0 => None,
            1 => Some("1 file can't be converted and will be skipped.".to_string()),
            n => Some(format!("{n} files can't be converted and will be skipped.")),
        };
        let notices: Vec<AnyElement> = [
            skipped.filter(|_| self.jobs.is_empty()).map(|text| {
                div()
                    .id("skipped")
                    .test_support()
                    .aria_label(SharedString::from(text.clone()))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon(IconName::Info, 13., p.tertiary))
                    .child(styled(size::SMALL, p.secondary).child(text))
                    .into_any_element()
            }),
            self.error
                .clone()
                .map(|e| error_text(e, &p).into_any_element()),
            blocked_banner(&self.app.read(cx).license, &p).map(IntoElement::into_any_element),
        ]
        .into_iter()
        .flatten()
        .collect();
        let notices = (!notices.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .gap(px(space::SM))
                .px(px(GUTTER))
                .pb(px(space::LG))
                .children(notices)
        });
        let body: Vec<AnyElement> = if self.jobs.is_empty() {
            let mut body: Vec<AnyElement> = Vec::new();
            if let Some(pack) = self.pack_section(&p, cx) {
                body.push(pack.into_any_element());
            }
            if let Some(picker) = self.picker(&p, cx) {
                body.push(picker.into_any_element());
            }
            if let Some(options) = self.options_section(&p, cx) {
                body.push(options.into_any_element());
            }
            // Nothing to save while every file waits for the document pack.
            if !self.supported().is_empty() || waiting == 0 {
                body.push(self.save_section(&p, cx).into_any_element());
            }
            body
        } else {
            vec![self.progress_section(&p).into_any_element()]
        };
        div()
            .id("quick")
            .flex()
            .flex_col()
            .size_full()
            .bg(p.window)
            .font_family(theme::SANS)
            .text_color(p.text)
            .children(theme::title_bar(
                Some("Convert"),
                44.,
                p.dark.then_some(p.chrome),
                &p,
            ))
            .when(!theme::transparent_titlebar(), |d| d.pt(px(20.)))
            .child(self.header(&p))
            .children(notices)
            .child(
                div()
                    .id("quick-body")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .pt(px(space::XS))
                    .overflow_y_scroll()
                    .children(body),
            )
            .children(self.consent(&p, cx))
            .child(self.footer(&p, cx))
    }
}
