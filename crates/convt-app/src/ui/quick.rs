//! Quick convert: the window for files sent without a target, from "More
//! options…" in the Finder menu, a `convt://` link, "Open with" or the
//! command line. It shows the files, picks the format most people want from
//! them (the user can pick another), offers options, and follows the
//! conversion until it is done.
//!
//! The same view is the main window's composer ([`QuickView::new_inline`]):
//! a card at the top of Activity for files dropped or added there. It hands
//! the conversion to Activity as soon as it starts, and emits [`Dismissed`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use convt_core::{
    Background, Category, Format, Options, Output, Registry, VideoCodec, format_by_id, format_size,
};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::IconName;

use super::theme::{
    self, Button, Choice, Palette, Segment, Tone, icon, mono, radius, size, space, styled,
};
use super::{
    QUICK_SIZE, blocked_banner, error_text, file_size, human_size, quick_height_for, time_left,
};
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

/// The Max file size choices in bytes, decimal like upload limits; `None`
/// is no limit.
const TARGET_SIZES: [Option<u64>; 6] = [
    None,
    Some(2_000_000),
    Some(8_000_000),
    Some(10_000_000),
    Some(25_000_000),
    Some(50_000_000),
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
    TargetSize,
    Codec,
    Background,
}

/// The inline composer is done: its conversion started, or it was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dismissed;

/// The formats offered first, in this order, before "More formats". Formats
/// of the source's own kind come first.
const POPULAR: [&str; 19] = [
    "jpeg", "webp", "png", "avif", "gif", "heic", "pdf", "mp4", "webm", "mov", "mkv", "mp3", "m4a",
    "wav", "flac", "aac", "docx", "xlsx", "pptx",
];

pub struct QuickView {
    app: Entity<AppState>,
    /// The main window's composer rather than a window of its own.
    pub(super) inline: bool,
    pub(super) focus: FocusHandle,
    /// "More formats" is open, showing the less common targets.
    pub(super) more_open: bool,
    /// Copy file was clicked; the button says Copied.
    pub(super) copied: bool,
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
    /// The Max file size control, in bytes. `None` is no limit.
    pub(super) target_size: Option<u64>,
    /// The Strip metadata checkbox.
    pub(super) strip_metadata: bool,
    /// The Background control for image targets. `None` shows the engine's
    /// default: Transparent where the format keeps it, White where it can't.
    pub(super) background: Option<Background>,
    open: Option<Open>,
    /// The Options row is expanded into its controls.
    pub(super) options_open: bool,
    /// The file name shows as a field instead of as text.
    pub(super) editing_name: bool,
    /// Where the files go. `None` is next to each file.
    pub(super) save_dir: Option<PathBuf>,
    pub(super) file_name: Entity<InputState>,
    /// The last name committed with Enter or Done, or the default; Escape
    /// puts the field back to it.
    pub(super) saved_name: String,
    /// Why the typed file name can't be used, shown under it.
    pub(super) name_error: Option<String>,
    pub(super) jobs: Vec<JobId>,
    /// The last state seen of each job, so results stay after the main
    /// window clears finished jobs.
    seen: HashMap<JobId, Entry>,
    pub(super) error: Option<String>,
    /// The target the request named, kept while the document pack it needs
    /// isn't installed, and picked again once it is.
    pub(super) wanted: Option<&'static Format>,
    /// The window waited on the pack: a file needed it, or a target that
    /// needs it was asked for or picked. Only then does it say once the
    /// pack is in.
    waited_for_pack: bool,
    /// Install & Convert was clicked: convert as soon as the pack is in.
    /// Cleared when the download fails or stops, so nothing converts later
    /// without another click.
    pub(super) convert_when_ready: bool,
    /// Convert on convt's cloud instead of on this computer.
    pub(super) cloud: bool,
    /// The registry the targets were computed with.
    generation: u64,
    /// The height the window was last sized to for its content, so a window
    /// the user resized keeps its size until what it shows changes.
    sized_for: Option<Pixels>,
    _observe: Subscription,
    _appearance: Subscription,
    /// Clears the file name's error once it's edited.
    _name: Subscription,
}

impl EventEmitter<Dismissed> for QuickView {}

impl QuickView {
    /// Quick convert as the main window's composer.
    pub fn new_inline(
        app: Entity<AppState>,
        request: Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            inline: true,
            ..Self::new(app, request, window, cx)
        }
    }

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
        let targets = state.targets_with_pack(&files);
        let generation = state.registry_generation;
        let file_needs_pack = files.iter().any(|f| pack::needs_pack(&state.registry, f));
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
        let asked = request.to.is_some() || request.preset.is_some();
        let to = to.filter(|f| {
            let reachable = targets.formats.contains(f);
            if targets.pack.contains(f) {
                // Picked, with the pack's offer: Install & Convert.
                wanted = Some(*f);
            } else if !reachable && file_needs_pack {
                // A document the pack can't help with here (a build with
                // nothing to download): the card says so.
                wanted = Some(*f);
            } else if !reachable && error.is_none() {
                error = Some(unreachable(f, files.len()));
            }
            reachable
        });
        // Nothing asked for: start from what most people want from these
        // files. Documents waiting for the pack wait for a pick.
        let to = match to {
            None if !asked && !file_needs_pack => recommended_target(&files, &targets),
            to => to,
        };
        let more_open = to.or(wanted).is_some_and(|f| !is_popular(f));
        if !expanded.unreadable.is_empty() {
            error = Some(expanded.unreadable.join("\n"));
        } else if files.is_empty() {
            error = Some("There are no files to convert here.".into());
        }
        let waited_for_pack = file_needs_pack || wanted.is_some_and(|f| targets.pack.contains(&f));
        let file_name = cx.new(|cx| InputState::new(window, cx));
        let mut view = Self {
            _name: cx.subscribe(&file_name, |this: &mut Self, _, event, cx| {
                if matches!(event, InputEvent::Change) && this.name_error.take().is_some() {
                    cx.notify();
                }
            }),
            _observe: cx.observe_in(&app, window, |this: &mut Self, app, window, cx| {
                this.remember(&app, cx);
                this.follow_registry(window, cx);
                // A license banner or the pack card came or went.
                this.sync_window_height(window, cx);
                cx.notify();
            }),
            _appearance: theme::observe_appearance(window, cx),
            app,
            inline: false,
            focus: cx.focus_handle(),
            more_open,
            copied: false,
            files,
            targets,
            to,
            preset: request.preset,
            options,
            quality: None,
            size: None,
            video_codec: None,
            strip_audio: false,
            target_size: None,
            strip_metadata: false,
            background: None,
            open: None,
            options_open: false,
            editing_name: false,
            save_dir,
            file_name,
            saved_name: String::new(),
            name_error: None,
            jobs: Vec::new(),
            seen: HashMap::new(),
            error,
            waited_for_pack,
            wanted,
            convert_when_ready: false,
            cloud: false,
            generation,
            sized_for: None,
        };
        view.load_controls();
        view.reset_name(window, cx);
        view
    }

    /// Adds more files, from another drop or Add files while the composer is
    /// open. Files already listed are listed once. The picked format, the
    /// options, the folder and Cloud stay, as long as every file can still
    /// become that format; otherwise the usual one for the files is picked.
    pub fn add_files(&mut self, paths: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        if paths.is_empty() || !self.jobs.is_empty() {
            return;
        }
        let state = self.app.read(cx);
        let expanded = model::expand_keeping_documents(&state.registry, paths);
        let before = self.files.len();
        for file in expanded.files {
            if !self.files.contains(&file) {
                self.files.push(file);
            }
        }
        if !expanded.unreadable.is_empty() {
            self.error = Some(expanded.unreadable.join("\n"));
        }
        if self.files.len() == before {
            cx.notify();
            return;
        }
        let state = self.app.read(cx);
        self.targets = state.targets_with_pack(&self.files);
        self.generation = state.registry_generation;
        let file_needs_pack = self
            .files
            .iter()
            .any(|f| pack::needs_pack(&state.registry, f));
        if file_needs_pack {
            self.waited_for_pack = true;
        }
        let kept = self.to.filter(|to| self.targets.formats.contains(to));
        if kept.is_none() {
            let to = if file_needs_pack {
                None
            } else {
                recommended_target(&self.files, &self.targets)
            };
            self.to = to;
            self.size = self.preset_size();
            self.convert_when_ready = false;
            if let Some(to) = to {
                self.set_target(to);
            }
        }
        if self.files.is_empty() {
            self.error = Some("There are no files to convert here.".into());
        } else if expanded.unreadable.is_empty() {
            self.error = None;
        }
        self.reset_name(window, cx);
        cx.notify();
    }

    /// Recomputes the targets after the document pack was installed or
    /// removed, and picks the target the request named once it is reachable.
    /// After Install & Convert, converts as soon as the pack is in; a failed
    /// or stopped download forgets that, so converting takes a new click.
    fn follow_registry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.jobs.is_empty() {
            return;
        }
        let state = self.app.read(cx);
        if state.registry_generation != self.generation {
            self.generation = state.registry_generation;
            self.targets = state.targets_with_pack(&self.files);
            if self
                .to
                .is_some_and(|to| !self.targets.formats.contains(&to))
            {
                self.to = None;
            }
            if self.to.is_none()
                && let Some(wanted) = self.wanted.filter(|f| self.targets.formats.contains(f))
            {
                self.pick(wanted, window, cx);
            }
            if self.wanted.is_some_and(|f| !self.targets.pack.contains(&f)) {
                self.wanted = None;
            }
        }
        if !self.convert_when_ready {
            return;
        }
        let working = matches!(self.app.read(cx).pack.phase, PackPhase::Working(_));
        match self.to {
            Some(to) if !self.targets.pack.contains(&to) => {
                self.convert_when_ready = false;
                self.convert(cx);
            }
            Some(_) if working => {}
            // Failed, stopped, or the target went away: wait for a click.
            _ => self.convert_when_ready = false,
        }
    }

    /// Whether the picked target needs the document pack first.
    pub(super) fn needs_pack(&self) -> bool {
        self.to.is_some_and(|to| self.targets.pack.contains(&to))
    }

    /// Install & Convert, after its button started the download or found
    /// one running: convert once the pack is in.
    fn install_then_convert(&mut self, cx: &mut Context<Self>) {
        if !self.needs_pack() || !self.jobs.is_empty() {
            return;
        }
        // The download didn't start (Remove is running, or documents are
        // converting); the card says why.
        self.convert_when_ready = matches!(self.app.read(cx).pack.phase, PackPhase::Working(_));
        self.error = None;
        cx.notify();
    }

    /// The callback the pack buttons run after starting the download.
    fn after_install(&self, cx: &mut Context<Self>) -> super::pack::After {
        let weak = cx.entity().downgrade();
        std::rc::Rc::new(move |_, cx| {
            let _ = weak.update(cx, |this, cx| this.install_then_convert(cx));
        })
    }

    /// The files in a sentence: "this PDF", "these PDF files" or "these
    /// files".
    fn files_named(&self) -> String {
        let mut formats = self
            .files
            .iter()
            .map(|f| convt_core::format_by_extension(f));
        let first = formats.next().flatten();
        let same = first.filter(|f| formats.all(|o| o == Some(*f)));
        match (self.files.len(), same) {
            (1, Some(f)) => format!("this {}", f.name),
            (_, Some(f)) => format!("these {} files", f.name),
            _ => "these files".to_string(),
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
        self.target_size = o.target_size;
        self.strip_metadata = o.strip_metadata;
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
        self.name_error = None;
        self.saved_name.clone_from(&name);
        self.file_name
            .update(cx, |s, cx| s.set_value(name, window, cx));
    }

    /// Makes `to` the target, from a card or a preset.
    fn set_target(&mut self, to: &'static Format) {
        if self.to != Some(to) {
            // Install & Convert was for the other target.
            self.convert_when_ready = false;
        }
        self.to = Some(to);
        if !is_popular(to) {
            self.more_open = true;
        }
        if self.targets.pack.contains(&to) {
            self.waited_for_pack = true;
            // The pack's targets convert on this computer once it's in.
            self.cloud = false;
        }
    }

    pub(super) fn pick(
        &mut self,
        to: &'static Format,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = self.to != Some(to);
        self.set_target(to);
        // Size means something else for each kind of target.
        if changed {
            self.size = self.preset_size();
        }
        self.error = None;
        self.reset_name(window, cx);
        self.sync_window_height(window, cx);
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
                        self.set_target(to);
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
        self.sync_window_height(window, cx);
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
        // Where the row is hidden (lossless audio), a preset's limit would only
        // make the conversion fail, with nothing in the window to clear it.
        options.target_size = if target_size_applies(to) {
            self.target_size
        } else {
            None
        };
        if media(to) {
            options.strip_metadata = self.strip_metadata;
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

    /// Where the output goes, or why the typed file name can't be used.
    fn output(&self, cx: &App) -> Result<Output, String> {
        if let ([file], Some(default), Some(to)) =
            (self.files.as_slice(), self.default_name(), self.to)
        {
            let name = output_name(&self.file_name.read(cx).value(), to.extension())?;
            if name != default {
                let dir = self
                    .save_dir
                    .clone()
                    .or_else(|| file.parent().map(Path::to_path_buf))
                    .unwrap_or_default();
                return Ok(Output::Exact(dir.join(name)));
            }
        }
        Ok(match &self.save_dir {
            Some(dir) => Output::Dir(dir.clone()),
            None => Output::Beside,
        })
    }

    pub(super) fn convert(&mut self, cx: &mut Context<Self>) {
        let Some(to) = self.to else { return };
        let files = self.supported();
        // A target that needs the pack converts through Install & Convert.
        if files.is_empty() || !self.jobs.is_empty() || self.needs_pack() {
            return;
        }
        let cloud = self.in_cloud(cx);
        let options = self.conversion_options();
        if let Err(e) = options.validate() {
            self.error = Some(e.to_string());
            cx.notify();
            return;
        }
        let output = match self.output(cx) {
            Ok(output) => output,
            Err(e) => {
                self.name_error = Some(e);
                self.editing_name = true;
                cx.notify();
                return;
            }
        };
        if let Output::Exact(path) = &output
            && files.iter().any(|f| same_file(f, path))
        {
            self.name_error = Some("That's the original file's name. Pick another one.".into());
            self.editing_name = true;
            cx.notify();
            return;
        }
        self.name_error = None;
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
        // Activity follows the conversion from here.
        if self.inline {
            cx.emit(Dismissed);
        }
    }

    /// Cancel, ×, Escape and Close: the window closes, or the composer
    /// leaves Activity.
    pub(super) fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.inline {
            cx.emit(Dismissed);
        } else {
            window.remove_window();
        }
    }

    /// Enter converts once a target is picked and nothing else is being
    /// typed; Escape cancels.
    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keys = &event.keystroke;
        if keys.modifiers.modified() {
            return;
        }
        match keys.key.as_str() {
            // The file name field takes Enter and Escape for itself, so
            // neither converts nor closes the window.
            "enter" if self.editing_name => {
                cx.stop_propagation();
                self.commit_name(window, cx);
            }
            "escape" if self.editing_name => {
                cx.stop_propagation();
                self.cancel_name(window, cx);
            }
            "enter" if self.jobs.is_empty() && self.ready(cx) => {
                cx.stop_propagation();
                self.convert(cx);
            }
            "escape" if self.open.is_some() => {
                cx.stop_propagation();
                self.open = None;
                cx.notify();
            }
            "escape" => {
                cx.stop_propagation();
                self.dismiss(window, cx);
            }
            _ => {}
        }
    }

    /// Whether Convert would run now.
    pub(super) fn ready(&self, cx: &App) -> bool {
        let state = self.app.read(cx);
        self.to.is_some()
            && !self.needs_pack()
            && !self.supported().is_empty()
            && state.license.allows_conversion()
            && (state.settings.cloud_consent || !self.in_cloud(cx))
    }

    /// Puts the finished files on the clipboard.
    fn copy_outputs(&mut self, cx: &mut Context<Self>) {
        let outputs = self.outputs();
        if outputs.is_empty() {
            return;
        }
        crate::clipboard::copy_files(&outputs, cx);
        self.copied = true;
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
        self.cloud && self.cloud_access(cx).ready() && self.cloud_blocked().is_none()
    }

    /// Why Cloud can't take this target although the account could: it
    /// doesn't compress a file into its own format.
    fn cloud_blocked(&self) -> Option<&'static str> {
        let to = self.to?;
        sources(&self.files)
            .any(|f| f == to)
            .then_some("Compressing a file into its own format runs on this computer.")
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
    fn consent(&self, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.jobs.is_empty() || !self.in_cloud(cx) || self.app.read(cx).settings.cloud_consent {
            return None;
        }
        let what = if self.files.len() == 1 {
            "this file"
        } else {
            "these files"
        };
        let them = if self.files.len() == 1 { "it" } else { "them" };
        // A strip on the footer, so the question sits next to the choice
        // that asked it.
        Some(
            div()
                .id("cloud-consent")
                .test_support()
                .aria_label("Upload to convt's cloud?")
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(space::LG))
                .px(px(self.gutter()))
                .pt(px(14.))
                .pb(px(2.))
                .bg(p.chrome)
                .border_t_1()
                .border_color(p.chrome_border)
                .child(
                    theme::callout_words(
                        "Upload to convt's cloud?",
                        format!(
                            "Cloud uploads {what} to convt's servers to convert, then deletes {them}."
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
                            Button::secondary("cloud-consent-cancel", "Not now")
                                .small()
                                .build(p)
                                .on_click(cx.listener(|this, _, _, cx| this.set_cloud(false, cx))),
                        )
                        .child(
                            Button::primary("cloud-consent-agree", "Agree")
                                .small()
                                .build(p)
                                .on_click(cx.listener(|this, _, _, cx| this.agree_to_cloud(cx))),
                        ),
                )
                .into_any_element(),
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

    /// Space between the edge and the content: the window's gutter, or the
    /// composer card's tighter one.
    fn gutter(&self) -> f32 {
        if self.inline { INLINE_GUTTER } else { GUTTER }
    }

    pub(super) fn finished(&self) -> bool {
        !self.jobs.is_empty() && self.entries().all(|e| e.status.is_finished())
    }

    fn header(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
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
            Some(first) => theme::thumbnail(first, 48., 36., p),
            None => div().w(px(48.)).h(px(36.)).into_any_element(),
        };
        let inline = self.inline;
        let close = inline.then(|| {
            let hover = p.hover;
            theme::clickable("composer-close", "Clear")
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(px(28.))
                .rounded(px(radius::CONTROL))
                .hover(move |s| s.bg(hover))
                .tooltip(|window, cx| Tooltip::new("Clear").build(window, cx))
                .on_click(cx.listener(|this, _, window, cx| this.dismiss(window, cx)))
                .child(icon(IconName::Close, 14., p.secondary))
        });
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(14.))
            .px(px(self.gutter()))
            .pt(px(if inline { space::LG } else { space::XS }))
            .pb(px(12.))
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
            .children(close)
    }

    fn picker(&self, p: &Palette, cx: &mut Context<Self>) -> Option<Div> {
        if self.targets.formats.is_empty() && self.waiting_for_pack(cx) > 0 {
            // The document card says what to do instead.
            return None;
        }
        let video_input = self.files.iter().any(|f| {
            convt_core::format_by_extension(f).is_some_and(|f| f.category == Category::Video)
        });
        let (popular, more) = popular_and_more(&self.targets, &self.files);
        let card = |format: &'static Format, cx: &mut Context<Self>| {
            self.format_card(format, video_input, p, cx)
        };
        let grid = |cards: Vec<AnyElement>| {
            // Four to a row, whatever the window's width.
            div()
                .grid()
                .grid_cols(CARDS_PER_ROW)
                .gap(px(space::SM))
                .children(cards)
        };
        let popular_cards: Vec<AnyElement> = popular.iter().map(|&f| card(f, cx)).collect();
        // With nothing popular to show, or only a couple of others, every
        // target shows, with no toggle.
        let toggle = (more.len() >= MIN_HIDDEN && !popular.is_empty()).then(|| {
            let open = self.more_open;
            let label = SharedString::from(format!("More formats ({})", more.len()));
            theme::clickable("more-formats-toggle", label.clone())
                .aria_expanded(open)
                .flex()
                .items_center()
                .gap(px(4.))
                .h(px(22.))
                .px(px(6.))
                .mr(px(-6.))
                .rounded(px(radius::SM))
                .hover(|s| s.bg(p.hover))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.more_open = !this.more_open;
                    this.sync_window_height(window, cx);
                    cx.notify();
                }))
                .child(
                    styled(size::SMALL, p.secondary)
                        .font_weight(FontWeight::MEDIUM)
                        .child(label),
                )
                .child(icon(
                    if open {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    },
                    12.,
                    p.tertiary,
                ))
        });
        let more_cards: Vec<AnyElement> = if self.more_open || toggle.is_none() {
            more.iter().map(|&f| card(f, cx)).collect()
        } else {
            Vec::new()
        };
        let more_grid = (!more_cards.is_empty()).then(|| {
            div()
                .when(!popular.is_empty(), |d| d.pt(px(space::SM)))
                .child(grid(more_cards))
        });
        let png_hint = self.png_hint(p, cx);
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
                .pt(px(10.))
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
                        .h(px(26.))
                        .px(px(11.))
                        .rounded(px(radius::CONTROL))
                        .bg(if on { p.green_tint } else { p.surface })
                        .shadow(vec![theme::inset_ring(
                            if on { p.green } else { p.border },
                            if on { 1.5 } else { 1. },
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
        let picker = section_with("Convert to", self.gutter(), toggle, p)
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
                    .flex_col()
                    .children((!popular_cards.is_empty()).then(|| grid(popular_cards)))
                    .children(more_grid)
                    .into_any_element()
            })
            .children(png_hint)
            .children(preset_row);
        Some(picker)
    }

    /// One format card of the picker.
    fn format_card(
        &self,
        format: &'static Format,
        video_input: bool,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on = self.to == Some(format);
        // A file's own format makes a smaller or cleaner copy of it.
        let same = sources(&self.files).all(|f| f == format);
        let needs_pack = self.targets.pack.contains(&format);
        let weak = cx.entity().downgrade();
        let blurb = if same {
            "Compress"
        } else {
            blurb(format, video_input)
        };
        theme::choice(
            theme::clickable(SharedString::from(format!("to-{}", format.id)), format.name),
            on,
            radius::CARD,
            p,
        )
        .aria_selected(on)
        .relative()
        .flex()
        .flex_col()
        .min_w_0()
        .gap(px(3.))
        .px(px(space::MD))
        .py(px(8.))
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
                        .child(icon(IconName::Check, 10., p.on_green))
                })),
        )
        .map(|d| {
            if needs_pack {
                d.child(super::pack::tag(
                    SharedString::from(format!("pack-tag-{}", format.id)),
                    on,
                    p,
                ))
            } else {
                d.child(
                    div()
                        .id(SharedString::from(format!("blurb-{}", format.id)))
                        .test_support()
                        .aria_label(blurb)
                        .child(
                            styled(size::CAPTION, if on { p.green_text } else { p.secondary })
                                .truncate()
                                .child(blurb),
                        ),
                )
            }
        })
        .into_any_element()
    }

    /// A photo becoming PNG grows a lot; says so and offers the formats
    /// that keep it small.
    fn png_hint(&self, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.shows_png_hint() {
            return None;
        }
        let switch = |id: &'static str, label: &'static str, cx: &mut Context<Self>| {
            let to = self.targets.formats.iter().copied().find(|f| f.id == id)?;
            let weak = cx.entity().downgrade();
            Some(
                Button::secondary(SharedString::from(format!("switch-to-{id}")), label)
                    .small()
                    .build(p)
                    .on_click(move |_, window, cx| {
                        let _ = weak.update(cx, |this, cx| this.pick(to, window, cx));
                    }),
            )
        };
        let actions: Vec<_> = [
            switch("jpeg", "Switch to JPEG", cx),
            switch("webp", "Switch to WebP", cx),
        ]
        .into_iter()
        .flatten()
        .collect();
        let title = "PNG files are much larger for photos";
        Some(
            div()
                .id("png-size-hint")
                .test_support()
                .aria_label(title)
                .pt(px(space::SM))
                .child(
                    theme::callout(
                        IconName::TriangleAlert,
                        Tone::Amber,
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(theme::callout_words(
                                title,
                                "PNG is lossless, so photos take much more space than JPEG or \
                                 WebP.",
                                p,
                            ))
                            .when(!actions.is_empty(), |d| {
                                d.child(div().flex().flex_wrap().gap(px(8.)).children(actions))
                            }),
                        p,
                    )
                    .py(px(10.))
                    .px(px(12.)),
                )
                .into_any_element(),
        )
    }

    /// Whether the PNG size warning shows.
    fn shows_png_hint(&self) -> bool {
        self.jobs.is_empty()
            && self.to.is_some_and(|t| t.id == "png")
            && sources(&self.files).any(is_photo)
    }

    /// How many rows the open Options show.
    fn option_rows(&self) -> usize {
        let Some(to) = self.to else {
            return 0;
        };
        [
            quality_applies(to),
            size_choices(to).is_some(),
            target_size_applies(to),
            codec_applies(to),
            background_applies(to, &self.files),
            audio_applies(to),
            media(to),
        ]
        .into_iter()
        .filter(|applies| *applies)
        .count()
    }

    /// The window height, before the title bar, that shows everything
    /// without scrolling, with the same air above the footer as the opening
    /// size: [`QUICK_SIZE`] plus what shows beyond it. `None` while the
    /// window shows its conversions, which keep the size it had.
    fn wanted_height(&self, cx: &App) -> Option<f32> {
        if self.inline || !self.jobs.is_empty() {
            return None;
        }
        let state = self.app.read(cx);
        let mut height = QUICK_SIZE.1;
        if state.license.blocked_reason().is_some() {
            height += BANNER_HEIGHT;
        }
        if self.shows_png_hint() {
            height += PNG_HINT_HEIGHT;
        }
        if self.options_open {
            height += self.option_rows() as f32 * OPTION_ROW_HEIGHT + 1.;
        }
        let (popular, more) = popular_and_more(&self.targets, &self.files);
        if self.more_open && more.len() >= MIN_HIDDEN && !popular.is_empty() {
            height += more.len().div_ceil(CARDS_PER_ROW.into()) as f32 * CARD_ROW_HEIGHT;
        }
        let waiting = self.waiting_for_pack(cx);
        let saves = !self.supported().is_empty() || waiting == 0 || self.to.is_some();
        if (waiting > 0 || self.needs_pack()) && saves {
            height += PACK_CARD_HEIGHT;
        }
        Some(height)
    }

    /// Fits the window's height to what it shows now: tight by default, and
    /// taller for the PNG warning, open Options or More formats, a license
    /// banner or the pack card, at most 90% of the display's usable height.
    /// It changes the window only when that height changes, so a window the
    /// user resized stays as it is until then.
    pub(super) fn sync_window_height(&mut self, window: &mut Window, cx: &mut App) {
        let Some(wanted) = self.wanted_height(cx) else {
            return;
        };
        let mut height = quick_height_for(wanted);
        if let Some(display) = window.display(cx) {
            let visible = display.visible_bounds();
            let max_by_display = (visible.size.height * 0.9).floor();
            let room_below = (visible.bottom() - window.bounds().origin.y - px(16.)).floor();
            let cap = if room_below >= quick_height_for(QUICK_SIZE.1) {
                max_by_display.min(room_below)
            } else {
                max_by_display
            };
            height = height.min(cap);
        }
        if self.sized_for == Some(height) {
            return;
        }
        self.sized_for = Some(height);
        let current = window.viewport_size();
        if current.height != height {
            window.resize(size(current.width, height));
            // The test platform resizes without telling the window.
            #[cfg(test)]
            window.bounds_changed(cx);
        }
    }

    /// The document pack card, or the line saying it was installed.
    fn pack_section(&self, p: &Palette, cx: &mut Context<Self>) -> Option<Div> {
        let waiting = self.waiting_for_pack(cx);
        let content = if waiting > 0 || self.needs_pack() {
            let mixed = waiting < self.files.len();
            let need = match self.to {
                // The files open without the pack; only the target needs it.
                Some(to) if self.needs_pack() && mixed => super::pack::Need::Target {
                    from: self.files_named(),
                    to: to.name,
                },
                _ => super::pack::Need::Files { mixed },
            };
            // With a target that needs the pack, Install & Convert below
            // is the one action.
            super::pack::card(&self.app, &need, !self.needs_pack(), p, cx).into_any_element()
        } else if self.waited_for_pack && self.app.read(cx).pack.phase == PackPhase::Done {
            super::pack::installed_notice(self.to.is_some(), p).into_any_element()
        } else {
            return None;
        };
        Some(
            div()
                .flex()
                .flex_col()
                .px(px(self.gutter()))
                .pb(px(space::XL))
                .child(content),
        )
    }

    /// Shows the file name as a field, focused, for typing a new one.
    pub(super) fn edit_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editing_name {
            self.saved_name = self.file_name.read(cx).value().to_string();
        }
        self.editing_name = true;
        let focus = self.file_name.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Enter or Done: keeps a usable name and shows it as text again; an
    /// unusable one stays in the field with the reason under it.
    pub(super) fn commit_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = self.output(cx) {
            self.name_error = Some(e);
            cx.notify();
            return;
        }
        self.saved_name = self.file_name.read(cx).value().to_string();
        self.name_error = None;
        self.editing_name = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Escape: puts back the last committed name and shows it as text.
    pub(super) fn cancel_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let saved = self.saved_name.clone();
        self.file_name
            .update(cx, |s, cx| s.set_value(saved, window, cx));
        self.name_error = None;
        self.editing_name = false;
        window.focus(&self.focus, cx);
        cx.notify();
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
                    &self.size.map_or("original".into(), |v| v.to_string()),
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
                    self.video_codec.unwrap_or(VideoCodec::H264).id(),
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
                    &current.map_or_else(|| "automatic".to_string(), Background::id),
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
        let target_size = target_size_applies(to).then(|| {
            let mut choices: Vec<Option<u64>> = TARGET_SIZES.to_vec();
            // A preset's own limit gets an entry of its own, which stays
            // pickable after choosing another.
            for custom in [self.options.target_size, self.target_size] {
                if !choices.contains(&custom) {
                    choices.push(custom);
                }
            }
            let id = |v: Option<u64>| v.map_or("none".into(), |v| v.to_string());
            let toggle = cx.entity().downgrade();
            let pick = cx.entity().downgrade();
            row_label(
                "Max file size",
                theme::select(
                    "target-size",
                    target_size_label(self.target_size),
                    &id(self.target_size),
                    SELECT_WIDTH,
                    false,
                    self.open == Some(Open::TargetSize),
                    choices
                        .iter()
                        .map(|&v| Choice::new(id(v), target_size_label(v)))
                        .collect(),
                    p,
                    move |_, cx| {
                        let _ = toggle.update(cx, |this, cx| this.toggle(Open::TargetSize, cx));
                    },
                    move |id, _, cx| {
                        let value = id.parse().ok();
                        let _ = pick.update(cx, |this, cx| {
                            this.target_size = value;
                            this.open = None;
                            cx.notify();
                        });
                    },
                ),
                p,
            )
        });
        let strip_metadata = media(to).then(|| {
            row_label(
                "Metadata",
                theme::checkbox("strip-metadata", "Strip metadata", self.strip_metadata, p)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.strip_metadata = !this.strip_metadata;
                        cx.notify();
                    })),
                p,
            )
        });
        let mut rows: Vec<AnyElement> = [
            quality,
            size,
            target_size,
            codec,
            background,
            audio,
            strip_metadata,
        ]
        .into_iter()
        .flatten()
        .collect();
        if rows.is_empty() {
            return None;
        }
        // One row that says what the options are; a click opens them.
        let summary = SharedString::from(self.options_summary(to, cx));
        let open = self.options_open;
        let header = theme::clickable("options-toggle", summary.clone())
            .aria_expanded(open)
            .flex()
            .items_center()
            .gap(px(space::MD))
            .min_h(px(42.))
            .px(px(space::LG))
            .hover(|s| s.bg(p.hover))
            .on_click(cx.listener(|this, _, window, cx| {
                this.options_open = !this.options_open;
                this.open = None;
                this.sync_window_height(window, cx);
                cx.notify();
            }))
            .child(
                styled(size::BODY, p.text)
                    .font_weight(FontWeight::MEDIUM)
                    .flex_shrink_0()
                    .child("Options"),
            )
            .child(
                div()
                    .id("options-summary")
                    .test_support()
                    .aria_label(summary.clone())
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .justify_end()
                    .when(!open, |d| {
                        d.child(
                            styled(size::SMALL, p.secondary)
                                .truncate()
                                .child(summary.clone()),
                        )
                    }),
            )
            .child(icon(
                if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                },
                14.,
                p.tertiary,
            ))
            .into_any_element();
        if !open {
            rows.clear();
        }
        rows.insert(0, header);
        Some(
            div()
                .flex()
                .flex_col()
                .px(px(self.gutter()))
                .pb(px(SECTION_GAP))
                .child(theme::group(rows, p)),
        )
    }

    /// "Balanced · Original size · Transparent": the options as they are now.
    pub(super) fn options_summary(&self, to: &'static Format, cx: &App) -> String {
        let mut parts: Vec<String> = Vec::new();
        if quality_applies(to) {
            parts.push(
                match QUALITY
                    .iter()
                    .find(|(k, ..)| *k == quality_key(self.quality))
                {
                    Some((_, label, _)) => label.to_string(),
                    None => format!("Quality {}", self.quality.unwrap_or_default()),
                },
            );
        }
        if let Some((_, choices)) = size_choices(to) {
            parts.push(
                choices
                    .iter()
                    .find(|(v, _)| *v == self.size)
                    .map(|(v, l)| {
                        if v.is_none() {
                            "Original size".to_string()
                        } else {
                            l.to_string()
                        }
                    })
                    .or_else(|| self.size.map(|v| size_label(to, v)))
                    .unwrap_or_else(|| "Original size".into()),
            );
        }
        if codec_applies(to) {
            parts.push(
                self.video_codec
                    .unwrap_or(VideoCodec::H264)
                    .name()
                    .to_string(),
            );
        }
        if background_applies(to, &self.files) {
            let default = default_background(&self.app.read(cx).registry, to, &self.files);
            parts.push(
                shown_background(to, self.background)
                    .or(default)
                    .map_or_else(|| "Automatic".to_string(), |b| b.name().to_string()),
            );
        }
        if audio_applies(to) && self.strip_audio {
            parts.push("No audio".into());
        }
        parts.join(" · ")
    }

    /// Where the conversion runs: "Convert on", then This computer or
    /// Cloud. Cloud stays readable while it can't run and says why on hover.
    fn where_row(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let access = self.cloud_access(cx);
        let cloud = self.in_cloud(cx);
        let weak = cx.entity().downgrade();
        row_label(
            "Convert on",
            theme::segmented_with(
                "where",
                &[
                    Segment {
                        key: "local",
                        icon: Some(IconName::Computer),
                        label: theme::this_machine_label().into(),
                        disabled: None,
                    },
                    Segment {
                        key: "cloud",
                        icon: Some(IconName::Cloud),
                        label: "Cloud".into(),
                        disabled: access
                            .reason()
                            .or_else(|| self.cloud_blocked().map(Into::into))
                            .map(Into::into),
                    },
                ],
                if cloud { "cloud" } else { "local" },
                p,
                window,
                cx,
                move |key, _, cx| {
                    let _ = weak.update(cx, |this, cx| this.set_cloud(key == "cloud", cx));
                },
            ),
            p,
        )
    }

    /// Where it runs, the file name and the folder, in one group.
    fn save_section(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Div {
        // A target that needs the pack converts here once it's in.
        let place_to_run =
            (self.to.is_some() && !self.needs_pack()).then(|| self.where_row(p, window, cx));
        let place = match &self.save_dir {
            Some(dir) => super::tilde(dir),
            None => "Same folder".to_string(),
        };
        let name = (self.files.len() == 1 && self.to.is_some()).then(|| {
            let control = if self.editing_name {
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .w(px(260.))
                            .flex_shrink_0()
                            .font_family(theme::MONO)
                            .text_size(px(12.))
                            .child(theme::small_field(&self.file_name, "file-name")),
                    )
                    .child(
                        Button::ghost("file-name-done", "Done")
                            .icon(IconName::Check)
                            .small()
                            .build(p)
                            .h(px(theme::SMALL_FIELD_HEIGHT))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.commit_name(window, cx)),
                            ),
                    )
                    .into_any_element()
            } else {
                let current = SharedString::from(self.file_name.read(cx).value().to_string());
                theme::clickable("file-name-edit", current.clone())
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .max_w(px(320.))
                    .h(px(theme::SMALL_FIELD_HEIGHT))
                    .px(px(8.))
                    .mr(px(-8.))
                    .rounded(px(radius::CONTROL))
                    .hover(|s| s.bg(p.hover))
                    .on_click(cx.listener(|this, _, window, cx| this.edit_name(window, cx)))
                    .child(mono(12., 16., p.text).min_w_0().truncate().child(current))
                    .child(icon(IconName::Edit, 13., p.tertiary))
                    .into_any_element()
            };
            let error = self.name_error.clone().map(|e| {
                div()
                    .id("file-name-error")
                    .test_support()
                    .aria_label(SharedString::from(e.clone()))
                    .flex()
                    .justify_end()
                    .px(px(space::LG))
                    .pb(px(10.))
                    .mt(px(-4.))
                    .child(error_text(e, p))
            });
            div()
                .flex()
                .flex_col()
                .child(row_label("File name", control, p))
                .children(error)
                .into_any_element()
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
        section("Save", self.gutter(), p).child(theme::group(
            place_to_run
                .into_iter()
                .chain(name)
                .chain(std::iter::once(folder)),
            p,
        ))
    }

    fn progress_section(&self, p: &Palette) -> Stateful<Div> {
        let now = std::time::Instant::now();
        // The heading follows the jobs, as the footer's status does.
        let title = if !self.finished() {
            "Converting"
        } else if self.entries().all(|e| matches!(e.status, Status::Done(_))) {
            "Converted"
        } else {
            "Finished"
        };
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
                Status::Failed(e) => super::friendly_error(&e.message),
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
            .px(px(self.gutter()))
            .pb(px(space::XL))
            .overflow_y_scroll()
            .child(
                div()
                    .id("jobs-title")
                    .test_support()
                    .aria_label(title)
                    .child(theme::section_label(title, p)),
            )
            .child(theme::group(rows.collect::<Vec<_>>(), p))
    }

    fn footer(
        &self,
        asking: bool,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let gutter = self.gutter();
        let bar = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(space::SM))
            .px(px(gutter))
            .h(px(56.))
            .bg(if self.inline { p.recessed } else { p.chrome })
            // The card's overflow clip is square, so the fill takes the
            // card's corners itself, less its 1px border.
            .when(self.inline, |d| d.rounded_b(px(radius::CARD - 1.)))
            .when(!asking, |d| {
                d.border_t_1().border_color(if self.inline {
                    p.border
                } else {
                    p.chrome_border
                })
            });
        if !self.jobs.is_empty() {
            let outputs = self.outputs();
            let first = outputs.first().cloned();
            let only = (outputs.len() == 1).then(|| outputs[0].clone());
            let done = self.finished();
            let failed: Vec<PathBuf> = self
                .entries()
                .filter(|e| matches!(e.status, Status::Failed(_)))
                .map(|e| e.input.clone())
                .collect();
            let saved = self
                .entries()
                .filter(|e| matches!(e.status, Status::Done(_)))
                .count();
            // Where the jobs run, not what was picked: a same-format
            // target stays here even with Cloud chosen.
            let status: SharedString = match (done, self.in_cloud(cx)) {
                (true, _) if !failed.is_empty() && saved == 0 => "Failed".into(),
                (true, _) if !failed.is_empty() => {
                    format!("{saved} saved · {} failed", failed.len()).into()
                }
                (true, _)
                    if self
                        .entries()
                        .all(|e| matches!(e.status, Status::Cancelled)) =>
                {
                    "Cancelled".into()
                }
                (true, _) => "Done".into(),
                (false, true) => "Converting in the cloud…".into(),
                (false, false) => "Converting…".into(),
            };
            // The full error waits in Activity, with Retry beside it.
            let details = (done && !failed.is_empty()).then(|| {
                Button::secondary("quick-details", "Details")
                    .build(p)
                    .on_click(move |_, _, cx| super::show_failures(failed.clone(), cx))
            });
            return bar
                .child(
                    div()
                        .id("quick-status")
                        .test_support()
                        .aria_label(status.clone())
                        .flex_1()
                        .min_w_0()
                        .child(styled(size::SMALL, p.secondary).truncate().child(status)),
                )
                .children(details)
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
                .children((done && !outputs.is_empty()).then(|| {
                    Button::secondary(
                        "copy-output",
                        if self.copied { "Copied" } else { "Copy file" },
                    )
                    .build(p)
                    .on_click(cx.listener(|this, _, _, cx| this.copy_outputs(cx)))
                }))
                .child(
                    Button::primary("close", "Close")
                        .build(p)
                        .px(px(20.))
                        .on_click(cx.listener(|this, _, window, cx| this.dismiss(window, cx))),
                );
        }
        let state = self.app.read(cx);
        let allowed = state.license.allows_conversion();
        let convert = if self.needs_pack() {
            let pack = &state.pack;
            let working = matches!(pack.phase, PackPhase::Working(_));
            let label = if self.convert_when_ready {
                "Converts when installed"
            } else {
                "Install & Convert"
            };
            // Remove is running, or a reinstall waits for documents: the
            // card says why.
            let disabled = !allowed
                || self.convert_when_ready
                || pack.removing
                || !pack.offer.configured
                || (!working && state.documents_locked().is_some());
            let after = self.after_install(cx);
            super::pack::install_and_convert_button(&self.app, label.into(), disabled, after, p)
                .px(px(20.))
        } else {
            let disabled = !self.ready(cx);
            let label = match self.to {
                Some(to) if !disabled => format!("Convert to {}", to.name),
                _ => "Convert".to_string(),
            };
            Button::primary("convert", label)
                .disabled(disabled)
                .build(p)
                .px(px(20.))
                .when(!disabled, |d| {
                    d.on_click(cx.listener(|this, _, _, cx| this.convert(cx)))
                })
        };
        let _ = window;
        bar.child(div().flex_1())
            .child(
                Button::secondary("cancel", "Cancel")
                    .build(p)
                    .on_click(cx.listener(|this, _, window, cx| this.dismiss(window, cx))),
            )
            .child(convert)
    }
}

/// Tests set this to show Cloud as the account would allow it.
#[cfg(test)]
pub(super) struct TestCloud(pub CloudAccess);

#[cfg(test)]
impl Global for TestCloud {}

/// Space between the window edge and the content.
const GUTTER: f32 = 24.;
/// The composer card's gutter.
const INLINE_GUTTER: f32 = 20.;
/// Space under each part of the window.
const SECTION_GAP: f32 = 16.;
/// Format cards to a row.
const CARDS_PER_ROW: u16 = 4;
/// The fewest formats "More formats" hides; fewer show with the rest.
const MIN_HIDDEN: usize = 3;
/// What each part adds to Quick convert's height beyond [`QUICK_SIZE`], as
/// drawn at its width: the license banner, the PNG warning, an open Options
/// row, a row of format cards with the gap above it, and the pack card.
const BANNER_HEIGHT: f32 = 118.;
const PNG_HINT_HEIGHT: f32 = 118.;
const OPTION_ROW_HEIGHT: f32 = 44.;
const CARD_ROW_HEIGHT: f32 = 57.;
const PACK_CARD_HEIGHT: f32 = 96.;
const SELECT_WIDTH: f32 = 190.;

/// A titled part of the window.
fn section(label: &'static str, gutter: f32, p: &Palette) -> Div {
    section_with(label, gutter, None::<Div>, p)
}

/// A titled part of the window with a control at the right of its title.
fn section_with(
    label: &'static str,
    gutter: f32,
    trailing: Option<impl IntoElement>,
    p: &Palette,
) -> Div {
    div()
        .flex()
        .flex_col()
        .px(px(gutter))
        .pb(px(SECTION_GAP))
        .child(
            div()
                .flex()
                .items_end()
                .justify_between()
                .child(theme::section_label(label, p))
                .children(trailing.map(|t| div().pb(px(4.)).child(t))),
        )
}

/// An option row: the label on the left, the control on the right.
fn row_label(label: &'static str, control: impl IntoElement, p: &Palette) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(space::LG))
        .min_h(px(40.))
        .px(px(space::LG))
        .py(px(6.))
        .child(
            styled(size::BODY, p.text)
                .font_weight(FontWeight::MEDIUM)
                .flex_1()
                .child(label),
        )
        .child(div().flex().flex_shrink_0().items_center().child(control))
        .into_any_element()
}

/// The typed output name as one file name in the save folder, ending in
/// `extension`: a path, `..` or a separator would put the file somewhere
/// else, so they're refused rather than followed.
pub(super) fn output_name(typed: &str, extension: &str) -> Result<String, String> {
    let name = typed.trim();
    if name.is_empty() {
        return Err("Type a name for the file.".into());
    }
    let forbidden: &[char] = if cfg!(windows) {
        &['/', '\\', '\0', ':', '<', '>', '"', '|', '?', '*']
    } else {
        &['/', '\\', '\0']
    };
    if let Some(c) = name
        .chars()
        .find(|c| forbidden.contains(c) || c.is_control())
    {
        return Err(match c {
            '/' | '\\' => "Use a file name, not a folder path.".into(),
            c if c.is_control() => "The name has a character files can't use.".into(),
            c => format!("File names can't contain “{c}”."),
        });
    }
    if name.trim_matches('.').is_empty() {
        return Err("That isn't a file name.".into());
    }
    let has_extension = Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(extension));
    let name = if has_extension {
        name.to_string()
    } else {
        format!("{name}.{extension}")
    };
    // Belt and braces: exactly one plain component.
    let mut parts = Path::new(&name).components();
    match (parts.next(), parts.next()) {
        (Some(std::path::Component::Normal(_)), None) => Ok(name),
        _ => Err("That isn't a file name.".into()),
    }
}

/// The target picked for files sent without one: what most people want
/// from the first file's kind, else the first target, never one that waits
/// for the document pack.
pub(super) fn recommended_target(files: &[PathBuf], targets: &Targets) -> Option<&'static Format> {
    let from = sources(files).next()?;
    let wanted: &[&str] = match from.id {
        "heic" => &["jpeg", "webp", "png"],
        "png" => &["webp", "jpeg"],
        "jpeg" => &["webp", "png"],
        "webp" | "avif" => &["jpeg", "png"],
        "bmp" | "tiff" | "tga" | "ppm" | "qoi" | "exr" | "gif" => &["png", "jpeg"],
        "svg" => &["png", "pdf"],
        "mov" | "mkv" | "avi" | "webm" => &["mp4", "webm"],
        "mp4" => &["webm", "mp3", "gif"],
        "mp3" => &["m4a", "wav"],
        "wav" | "flac" | "aiff" | "caf" | "m4a" | "m4r" | "ogg" | "opus" | "aac" => &["mp3", "m4a"],
        "pdf" => &["png", "jpeg"],
        _ if matches!(
            from.category,
            Category::Document | Category::Presentation | Category::Spreadsheet
        ) =>
        {
            &["pdf"]
        }
        _ => &[],
    };
    let usable = |f: &&'static Format| !targets.pack.contains(f);
    wanted
        .iter()
        .find_map(|id| {
            targets
                .formats
                .iter()
                .copied()
                .find(|f| f.id == *id)
                .filter(usable)
        })
        .or_else(|| targets.formats.iter().copied().find(usable))
}

/// Whether `to` is one of the formats the picker offers first.
fn is_popular(to: &Format) -> bool {
    POPULAR.contains(&to.id)
}

/// The targets split into the popular ones, those of the source's own kind
/// first, and the rest, in the registry's order.
pub(super) fn popular_and_more(
    targets: &Targets,
    files: &[PathBuf],
) -> (Vec<&'static Format>, Vec<&'static Format>) {
    let kind = sources(files).next().map(|f| f.category);
    let (mut popular, more): (Vec<&'static Format>, Vec<&'static Format>) =
        targets.formats.iter().partition(|f| is_popular(f));
    popular.sort_by_key(|f| {
        (
            Some(f.category) != kind,
            POPULAR.iter().position(|id| *id == f.id),
        )
    });
    (popular, more)
}

/// Photos, which grow several times over as PNG.
fn is_photo(from: &'static Format) -> bool {
    matches!(from.id, "heic" | "jpeg" | "webp" | "avif")
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

/// Whether `to` is video, audio or an image, for Max file size and Strip
/// metadata.
fn media(to: &Format) -> bool {
    matches!(
        to.category,
        Category::Video | Category::Audio | Category::Image
    )
}

/// Whether Max file size shows for `to`. Lossless audio (WAV, FLAC, AIFF,
/// CAF) can't trade quality for size, so the engine refuses a limit there.
fn target_size_applies(to: &Format) -> bool {
    media(to) && !matches!(to.id, "wav" | "flac" | "aiff" | "caf")
}

/// "No limit" or the size as presets write it: "10MB".
fn target_size_label(bytes: Option<u64>) -> String {
    bytes.map_or_else(|| "No limit".into(), format_size)
}

/// Whether `a` and `b` name the same file, even through another spelling of
/// the folder.
fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
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
        Category::Audio if !matches!(to.id, "wav" | "flac" | "aiff" | "caf") => Some((
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
        "gif" if video_input => "Short loops",
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
        "gif" => "Simple graphics",
        "bmp" => "Uncompressed",
        "tiff" => "Print, archive",
        "ico" => "App icons",
        "exr" => "HDR",
        "tga" => "Textures",
        "ppm" => "Raw pixels",
        "qoi" => "Fast, lossless",
        "svg" => "Scales cleanly",
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                .px(px(self.gutter()))
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
            // Nothing to save while every file waits for the document pack
            // and no target is picked yet.
            if !self.supported().is_empty() || waiting == 0 || self.to.is_some() {
                body.push(self.save_section(&p, window, cx).into_any_element());
            }
            body
        } else {
            vec![self.progress_section(&p).into_any_element()]
        };
        let consent = self.consent(&p, cx);
        let asking = consent.is_some();
        let header = self.header(&p, cx);
        let footer = self.footer(asking, &p, window, cx);
        let on_key = cx.listener(Self::on_key);
        if self.inline {
            // A card at the top of Activity; the page scrolls, not the card.
            return div()
                .id("composer")
                .test_support()
                .aria_label("Convert")
                .track_focus(&self.focus)
                .on_key_down(on_key)
                .flex()
                .flex_col()
                .w_full()
                .overflow_hidden()
                .rounded(px(radius::CARD))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .shadow(theme::soft(&p))
                .font_family(theme::SANS)
                .text_color(p.text)
                .child(header)
                .children(notices)
                .child(div().id("composer-body").flex().flex_col().children(body))
                .children(consent)
                .child(footer)
                .into_any_element();
        }
        div()
            .id("quick")
            .track_focus(&self.focus)
            .on_key_down(on_key)
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
            .when(!theme::transparent_titlebar(), |d| d.pt(px(16.)))
            .child(header)
            .children(notices)
            .child(
                div()
                    .id("quick-body")
                    .test_support()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .pt(px(space::XS))
                    .overflow_y_scroll()
                    .children(body),
            )
            .children(consent)
            .child(footer)
            .into_any_element()
    }
}
