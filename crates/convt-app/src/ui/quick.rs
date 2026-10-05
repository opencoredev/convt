//! Quick convert: the window for files sent without a target, from "More
//! options…" in the Finder menu, a `convt://` link, "Open with" or the
//! command line. It shows the files, lets the user pick a target and options,
//! and follows the conversion until it is done.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use convt_core::{Category, Format, Options, Output, VideoCodec};
use gpui_kit::component::input::InputState;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{
    self, Choice, Palette, mono, primary_button, secondary_button, text, text_button,
};
use super::{blocked_banner, error_text, file_size, human_size, time_left};
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
        let files = expanded.files;
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
            open: None,
            save_dir,
            file_name,
            jobs: Vec::new(),
            seen: HashMap::new(),
            error,
            wanted,
            offered_pack,
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
        let queued = self
            .app
            .update(cx, |s, cx| s.convert_to(&files, to, &options, output, cx));
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
            Some(first) => theme::thumbnail(first, 64., 44., p),
            None => div().w(px(64.)).h(px(44.)).into_any_element(),
        };
        div()
            .flex()
            .items_center()
            .gap(px(14.))
            .px(px(24.))
            .pt(px(4.))
            .pb(px(20.))
            .child(thumb)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.))
                    .child(
                        text(15., 18., p.text)
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(title),
                    )
                    .child(mono(11., 14., p.secondary).truncate().child(meta)),
            )
            .children(
                common_folder(&self.files)
                    .map(|dir| text(12., 16., p.secondary).flex_shrink_0().child(dir)),
            )
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
                .flex()
                .flex_col()
                .flex_shrink_0()
                .gap(px(2.))
                .w(px(100.))
                .px(px(12.))
                .py(px(10.))
                .rounded(px(8.))
                .map(|d| {
                    if on {
                        d.bg(p.green_tint)
                            .shadow(vec![theme::inset_ring(p.green, 1.5)])
                    } else {
                        d.shadow(vec![theme::inset_ring(p.card_border, 1.)])
                    }
                })
                .on_click(move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| this.pick(format, window, cx));
                })
                .child(
                    mono(13., 16., p.text)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format.name),
                )
                .child(
                    text(11., 14., if on { p.green } else { p.secondary })
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
                .flex_col()
                .gap(px(8.))
                .pt(px(6.))
                .child(section_label("Presets", p))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .children(presets.into_iter().map(|(name, about)| {
                            let on = self.preset.as_deref() == Some(&name);
                            let weak = cx.entity().downgrade();
                            let label = name.clone();
                            theme::clickable(
                                SharedString::from(format!("preset-{name}")),
                                name.clone(),
                            )
                            .aria_selected(on)
                            .px(px(8.))
                            .py(px(2.))
                            .rounded(px(5.))
                            .bg(if on { p.green_tint } else { p.chip })
                            .border_1()
                            .border_color(if on { p.green } else { p.chip_border })
                            .tooltip(move |window, cx| {
                                Tooltip::new(about.clone()).build(window, cx)
                            })
                            .on_click(move |_, window, cx| {
                                let _ =
                                    weak.update(cx, |this, cx| this.pick_preset(&name, window, cx));
                            })
                            .child(text(12., 16., if on { p.green } else { p.text }).child(label))
                        })),
                )
        });
        let picker = section(p)
            .gap(px(10.))
            .child(section_label("Convert to", p))
            .child(if self.targets.formats.is_empty() {
                text(13., 16., p.text)
                    .child("No format fits every file.")
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
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
        Some(section(p).child(content))
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
                    180.,
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
                    180.,
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
        let audio = audio_applies(to).then(|| {
            div().flex().pl(px(110.)).child(
                theme::checkbox("keep-audio", "Keep audio", !self.strip_audio, p).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.strip_audio = !this.strip_audio;
                        cx.notify();
                    }),
                ),
            )
        });
        if quality.is_none() && size.is_none() && codec.is_none() && audio.is_none() {
            return None;
        }
        Some(
            section(p)
                .gap(px(14.))
                .children(quality)
                .children(size)
                .children(codec)
                .children(audio),
        )
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
                    .w(px(240.))
                    .flex_shrink_0()
                    .font_family(theme::MONO)
                    .text_size(px(12.))
                    .child(theme::small_field(&self.file_name, "file-name")),
                p,
            )
        });
        section(p)
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        text(13., 16., p.text)
                            .w(px(110.))
                            .flex_shrink_0()
                            .child("Save to"),
                    )
                    .child(
                        div()
                            .id("save-to")
                            .test_support()
                            .aria_label(SharedString::from(place.clone()))
                            .child(text(13., 16., p.text).truncate().child(place)),
                    )
                    .child(
                        text_button("change-folder", "Change", p.green, 12.)
                            .pl(px(10.))
                            .on_click(cx.listener(|this, _, _, cx| this.choose_folder(cx))),
                    )
                    .when(self.save_dir.is_some(), |d| {
                        d.child(
                            text_button("same-folder", "Same folder", p.secondary, 12.)
                                .pl(px(10.))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.save_dir = None;
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .children(name)
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
            let color = match entry.status {
                Status::Done(_) => p.green,
                Status::Failed(_) => p.error,
                _ => p.secondary,
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
                .py(px(8.))
                .child(theme::thumbnail(&entry.input, 48., 34., p))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap(px(6.))
                        .child(text(13., 16., p.text).truncate().child(format!(
                            "{} → {}",
                            model::file_name(&entry.input),
                            entry.to.name
                        )))
                        .children(bar)
                        .child(
                            div()
                                .id(SharedString::from(format!("status-{id}")))
                                .test_support()
                                .aria_label(SharedString::from(status.clone()))
                                .child(text(12., 16., color).child(status)),
                        ),
                )
        });
        section(p)
            .id("jobs")
            .flex_1()
            .overflow_y_scroll()
            .children(rows)
    }

    fn footer(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let bar = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(10.))
            .px(px(24.))
            .py(px(14.))
            .bg(p.recessed)
            .border_t_1()
            .border_color(p.hairline);
        if !self.jobs.is_empty() {
            let outputs = self.outputs();
            let first = outputs.first().cloned();
            let only = (outputs.len() == 1).then(|| outputs[0].clone());
            let done = self.finished();
            return bar
                .child(mono(11., 14., p.secondary).flex_1().child(if done {
                    "Done"
                } else {
                    "Converting…"
                }))
                .children(first.filter(|_| done).map(|path| {
                    secondary_button("show-in-folder", "Show in folder", p)
                        .on_click(move |_, _, cx| cx.reveal_path(&path))
                }))
                .children(only.filter(|_| done).map(|path| {
                    secondary_button("open", "Open", p)
                        .on_click(move |_, _, cx| cx.open_with_system(&path))
                }))
                .child(
                    primary_button("close", "Close", 13., false)
                        .px(px(16.))
                        .on_click(|_, window, _| window.remove_window()),
                );
        }
        let state = self.app.read(cx);
        let disabled =
            self.to.is_none() || self.supported().is_empty() || !state.license.allows_conversion();
        let count = match self.supported().len() {
            0 | 1 => String::new(),
            n => format!("{n} files · "),
        };
        bar.child(
            mono(11., 14., p.secondary)
                .flex_1()
                .child(format!("{count}Runs on {}", theme::this_machine())),
        )
        .child(
            secondary_button("cancel", "Cancel", p).on_click(|_, window, _| window.remove_window()),
        )
        .child(
            primary_button("convert", "Convert", 13., disabled)
                .px(px(16.))
                .on_click(cx.listener(|this, _, _, cx| this.convert(cx))),
        )
    }
}

fn section(p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .px(px(24.))
        .py(px(18.))
        .border_t_1()
        .border_color(p.hairline)
}

fn section_label(label: &'static str, p: &Palette) -> Div {
    text(12., 16., p.secondary)
        .font_weight(FontWeight::SEMIBOLD)
        .child(label)
}

fn row_label(label: &'static str, control: impl IntoElement, p: &Palette) -> Div {
    div()
        .flex()
        .items_center()
        .child(
            text(13., 16., p.text)
                .w(px(110.))
                .flex_shrink_0()
                .child(label),
        )
        .child(control)
}

/// Whether the Codec control does anything for `to`.
fn codec_applies(to: &Format) -> bool {
    matches!(to.id, "mp4" | "mov" | "mkv")
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
                    .child(theme::text(12., 16., p.secondary).child(text))
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
                .gap(px(8.))
                .px(px(24.))
                .pb(px(14.))
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
                    .overflow_y_scroll()
                    .children(body),
            )
            .child(self.footer(&p, cx))
    }
}
