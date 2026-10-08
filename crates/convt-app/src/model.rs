//! The app's shared state: the job runner and queue, history, settings and
//! presets. Every window reads one [`AppState`] entity and changes it through
//! the methods here.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use convt_core::{
    Format, Job, Options, Output, Preset, Registry, expand_inputs, format_by_extension,
    format_by_id,
};
use convt_license::License;
use convt_license::account::{self, Api};
use convt_license::client::{self, Licensing};
use futures::StreamExt;
use futures::channel::mpsc::unbounded;
use gpui_kit::{App, Context, Entity, Global, SharedString, SystemNotification, Task};

use crate::account::Account;
use crate::history::{History, Outcome, Record, Setup};
use crate::jobs::{Entry, JobId, Queue, Runner, Status};
use crate::pack::{self, Failure};
use crate::request::Request;
use crate::settings::{Kind, Settings, write_atomic};
use crate::update::{Update, UpdateConfig};

/// How many rows the History tab shows.
const RECENT: usize = 200;

/// Where the app keeps its files. Tests point these at a temp directory.
pub struct Paths {
    /// `None` keeps settings in memory only.
    pub settings: Option<PathBuf>,
    /// `None` keeps history in memory only.
    pub history: Option<PathBuf>,
    pub presets: Option<PathBuf>,
    /// The trial file, the key store and whether this build checks licenses.
    pub license: client::Config,
    /// The site desktop sign-in and renewal talk to, and how. Tests script
    /// their own [`Api`] so they never reach the network.
    pub account_url: String,
    pub account_api: Arc<dyn Api>,
    /// The update check's key, transport and install target. Tests script
    /// their own transport.
    pub update: UpdateConfig,
}

impl Paths {
    pub fn from_env() -> Self {
        use convt_engines::paths;
        Self {
            settings: Settings::path(),
            history: History::path(),
            presets: paths::presets_dir(),
            license: client::Config::from_env(paths::config_dir(), paths::data_dir()),
            account_url: account::account_url(),
            account_api: Arc::new(account::Http::new(&account::account_url())),
            update: UpdateConfig::from_env(),
        }
    }
}

/// Finished jobs since the queue was last idle, for the summary notification.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Batch {
    pub done: usize,
    pub failed: usize,
    pub cancelled: usize,
}

impl Batch {
    /// The notification for a finished batch, or `None` if the user
    /// cancelled everything.
    pub fn summary(&self) -> Option<(String, String)> {
        let files = |n: usize| {
            if n == 1 {
                "1 file".to_string()
            } else {
                format!("{n} files")
            }
        };
        match (self.done, self.failed) {
            (0, 0) => None,
            (done, 0) => Some((
                "Conversion finished".into(),
                format!("Converted {}.", files(done)),
            )),
            (0, failed) => Some((
                "Conversion failed".into(),
                format!("{} could not be converted.", files(failed)),
            )),
            (done, failed) => Some((
                "Conversion finished with errors".into(),
                format!("Converted {}; {} failed.", files(done), files(failed)),
            )),
        }
    }
}

/// The formats every file in a selection can become, in the first file's
/// order, plus the files that can't be converted at all.
#[derive(Debug, Default, PartialEq)]
pub struct Targets {
    pub formats: Vec<&'static Format>,
    pub unsupported: Vec<PathBuf>,
}

pub fn common_targets(registry: &Registry, files: &[PathBuf]) -> Targets {
    let mut out = Targets::default();
    let mut first = true;
    for file in files {
        let targets = format_by_extension(file)
            .map(|f| registry.targets(f))
            .unwrap_or_default();
        if targets.is_empty() {
            out.unsupported.push(file.clone());
        } else if first {
            out.formats = targets;
            first = false;
        } else {
            out.formats.retain(|f| targets.contains(f));
        }
    }
    out
}

/// What [`AppState::add_files`] did.
#[derive(Debug, Default)]
pub struct Added {
    pub jobs: Vec<JobId>,
    /// Files with no default format they can reach.
    pub ask: Vec<PathBuf>,
    /// Folders that could not be read, or why the license stopped the batch.
    pub errors: Vec<String>,
}

/// Files to convert, plus a message for each folder that could not be read.
#[derive(Debug, Default)]
pub struct Expanded {
    pub files: Vec<PathBuf>,
    pub unreadable: Vec<String>,
}

/// Expands folders to the files directly inside them. Folder files that no
/// engine reads are skipped; explicit files are kept so the user hears why.
/// A folder that can't be read is reported without losing the others.
pub fn expand(registry: &Registry, paths: &[PathBuf]) -> Expanded {
    expand_with(registry, paths, false)
}

/// [`expand`] that also keeps documents in folders that no engine reads
/// yet, so a window can offer the document pack instead of dropping them.
pub fn expand_keeping_documents(registry: &Registry, paths: &[PathBuf]) -> Expanded {
    expand_with(registry, paths, true)
}

fn expand_with(registry: &Registry, paths: &[PathBuf], documents: bool) -> Expanded {
    let mut out = Expanded::default();
    for path in paths {
        let items = match expand_inputs(std::slice::from_ref(path), false) {
            Ok(items) => items,
            Err(e) => {
                out.unreadable
                    .push(format!("Could not read {}: {e}", path.display()));
                continue;
            }
        };
        out.files.extend(
            items
                .into_iter()
                .filter(|item| {
                    item.explicit
                        || format_by_extension(&item.input).is_some_and(|f| {
                            !registry.targets(f).is_empty() || (documents && pack::is_document(f))
                        })
                })
                .map(|item| item.input),
        );
    }
    out
}

pub fn valid_preset_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-'))
        && !name.starts_with([' ', '-'])
        && !name.ends_with(' ')
}

/// Where the document pack stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackPhase {
    Idle,
    Working(pack::Progress),
    /// Installed this session.
    Done,
    Failed(Failure),
}

/// The document pack as the windows show it.
#[derive(Debug, Clone)]
pub struct PackState {
    pub status: pack::Status,
    pub offer: pack::Offer,
    pub phase: PackPhase,
    pub removing: bool,
    /// Why the last download or removal couldn't start, or why a removal
    /// failed.
    pub notice: Option<String>,
}

enum PackEvent {
    Progress(pack::Progress),
    Installed(Result<PathBuf, Failure>),
    Removed(Result<(), String>),
}

pub struct AppState {
    pub registry: Arc<Registry>,
    /// Counts registry rebuilds, so windows know to recompute targets.
    pub registry_generation: u64,
    packs: Arc<dyn pack::Backend>,
    pub pack: PackState,
    pack_cancel: Option<Arc<AtomicBool>>,
    _pack_task: Option<Task<()>>,
    runner: Runner,
    pub queue: Queue,
    history: History,
    /// The newest history rows, refreshed whenever history changes.
    pub recent: Vec<Record>,
    pub settings: Settings,
    settings_path: Option<PathBuf>,
    pub presets: BTreeMap<String, Result<Preset, String>>,
    pub presets_dir: Option<PathBuf>,
    /// Problems loading or saving app files, shown in the Settings tab.
    pub errors: Vec<String>,
    pub(crate) licensing: Licensing,
    /// Where this machine stands, refreshed whenever it can change.
    pub license: client::State,
    /// Desktop sign-in and Pro renewal.
    pub account: Account,
    pub(crate) update_config: UpdateConfig,
    /// What the last update check found.
    pub update: Update,
    pub(crate) _update_task: Option<Task<()>>,
    /// The last manifest accepted this session, to select again when the
    /// license changes.
    pub(crate) update_manifest: Option<Arc<Vec<u8>>>,
    /// The update download and install, when this install updates itself.
    pub(crate) updater: crate::update::Updater,
    /// Runs the day's renewal and update check while convt runs.
    pub(crate) _daily_checks: Option<Task<()>>,
    batch: Batch,
    /// Jobs from silent conversions (a target picked in a background menu).
    /// Explorer requests that ask to show progress are tracked like normal
    /// batches so the Activity window gets a summary and reveal behavior.
    silent: HashSet<JobId>,
    /// What [`Self::apply`] would have revealed, in tests.
    #[cfg(test)]
    pub revealed: Vec<PathBuf>,
    /// Quit once the queue drains if no window is open.
    pub quit_when_idle: bool,
    /// Whether the Finder extension is on. `None` when this platform has
    /// none, or macOS has not registered it. Tests set this directly.
    pub finder_on: Option<bool>,
    _finder_watch: Option<Task<()>>,
    /// Jobs whose result should be copied when they finish.
    automation_copies: HashSet<JobId>,
    watch: crate::automation::WatchState,
    _automations: Option<Task<()>>,
    _drain: Task<()>,
}

/// Why a conversion can't start while an update installs.
pub const INSTALLING: &str =
    "convt is installing an update and restarts in a moment. Convert again after it does.";

/// The app's one [`AppState`].
pub struct Shared(pub Entity<AppState>);

impl Global for Shared {}

impl AppState {
    pub fn new(packs: Arc<dyn pack::Backend>, paths: Paths, cx: &mut Context<Self>) -> Self {
        let registry = Arc::new(packs.registry());
        let pack = PackState {
            status: packs.status(),
            offer: packs.offer(),
            phase: PackPhase::Idle,
            removing: false,
            notice: None,
        };
        let mut errors = Vec::new();
        let settings = match &paths.settings {
            Some(path) => Settings::load(path).unwrap_or_else(|e| {
                errors.push(format!(
                    "Settings were not loaded: {e}. Saving a change replaces the file."
                ));
                Settings::default()
            }),
            None => Settings::default(),
        };
        let history = paths
            .history
            .as_deref()
            .map(History::open)
            .unwrap_or_else(History::in_memory)
            .or_else(|e| {
                errors.push(format!("History is not saved this session: {e}"));
                History::in_memory()
            })
            .expect("an in-memory SQLite database opens");
        let (runner, mut rx) = Runner::new(registry.clone(), settings.concurrency());
        let drain = cx.spawn(async move |this, cx| {
            while let Some(update) = rx.next().await {
                let Ok(()) = this.update(cx, |state, cx| state.apply(update, cx)) else {
                    break;
                };
            }
        });
        let licensing = Licensing::new(paths.license);
        let account = Account::new(paths.account_url, paths.account_api, licensing.session());
        let mut state = Self {
            registry,
            registry_generation: 0,
            packs,
            pack,
            pack_cancel: None,
            _pack_task: None,
            runner,
            queue: Queue::default(),
            history,
            recent: Vec::new(),
            settings,
            settings_path: paths.settings,
            presets: BTreeMap::new(),
            presets_dir: paths.presets,
            errors,
            license: licensing.state(),
            licensing,
            account,
            update_config: paths.update,
            update: Update::Idle,
            _update_task: None,
            update_manifest: None,
            updater: Default::default(),
            _daily_checks: None,
            batch: Batch::default(),
            silent: HashSet::new(),
            #[cfg(test)]
            revealed: Vec::new(),
            quit_when_idle: false,
            finder_on: None,
            // Tests set `finder_on` themselves; a live poll would overwrite it.
            _finder_watch: if cfg!(all(target_os = "macos", not(test))) {
                Some(crate::finder::watch(cx))
            } else {
                None
            },
            automation_copies: HashSet::new(),
            watch: crate::automation::WatchState::default(),
            // Tests drive the watcher themselves so a live poll cannot see
            // the real Desktop or race a fixture.
            _automations: if cfg!(not(test)) {
                Some(crate::automation::watch(cx))
            } else {
                None
            },
            _drain: drain,
        };
        state.reload_presets();
        state.refresh_history();
        state
    }

    /// Queues one job per file and returns their ids, in file order. Fails
    /// with the reason, for the user, when the license stops conversions.
    pub fn convert(
        &mut self,
        files: &[PathBuf],
        to: &'static Format,
        options: &Options,
        cx: &mut Context<Self>,
    ) -> Result<Vec<JobId>, String> {
        let output = self.settings.output();
        self.convert_to(files, to, options, output, cx)
    }

    /// [`Self::convert`] with an output other than the one in Settings.
    pub fn convert_to(
        &mut self,
        files: &[PathBuf],
        to: &'static Format,
        options: &Options,
        output: Output,
        cx: &mut Context<Self>,
    ) -> Result<Vec<JobId>, String> {
        self.queue_jobs(files, to, options, output, false, cx)
    }

    fn queue_jobs(
        &mut self,
        files: &[PathBuf],
        to: &'static Format,
        options: &Options,
        output: Output,
        silent: bool,
        cx: &mut Context<Self>,
    ) -> Result<Vec<JobId>, String> {
        // Quitting after the install would stop them.
        if self.installing() {
            return Err(INSTALLING.into());
        }
        if let Some(reason) = self.documents_locked()
            && files
                .iter()
                .any(|f| format_by_extension(f).is_some_and(pack::is_document))
        {
            return Err(reason.into());
        }
        let allowed = self.licensing.begin_conversion();
        self.license = self.licensing.state();
        if let Err(blocked) = allowed {
            cx.notify();
            return Err(blocked.to_string());
        }
        // Retry runs from history, maybe after a restart in another working
        // directory, so a relative folder must not keep its meaning open.
        let output = absolute_output(output);
        let ids = files
            .iter()
            .map(|file| {
                let job = Job {
                    input: file.clone(),
                    to,
                    options: options.clone(),
                    output: output.clone(),
                };
                let id = self.queue.add(&job);
                if silent {
                    self.silent.insert(id);
                }
                self.runner.submit(id, job);
                id
            })
            .collect();
        cx.notify();
        Ok(ids)
    }

    /// The format Add files converts `file` to: the default for its kind, if
    /// the file can reach it.
    pub fn default_target(&self, file: &Path) -> Option<&'static Format> {
        let from = format_by_extension(file)?;
        let to = self.settings.defaults.get(Kind::of_file(file, from)?)?;
        (from.id != to.id && self.registry.targets(from).contains(&to)).then_some(to)
    }

    /// Converts files (and the files directly inside folders) to their
    /// default formats right away. Files with no usable default come back in
    /// [`Added::ask`] so the caller can ask what to do with them.
    pub fn add_files(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) -> Added {
        let expanded = expand_keeping_documents(&self.registry, paths);
        let mut added = Added {
            errors: expanded.unreadable,
            ..Added::default()
        };
        // One batch per target, in the order the targets first appear.
        let mut groups: Vec<(&'static Format, Vec<PathBuf>)> = Vec::new();
        let mut files = expanded.files;
        let mut seen = std::collections::HashSet::new();
        files.retain(|f| seen.insert(f.clone()));
        for file in files {
            match self.default_target(&file) {
                Some(to) => match groups.iter_mut().find(|(t, _)| *t == to) {
                    Some((_, files)) => files.push(file),
                    None => groups.push((to, vec![file])),
                },
                None => added.ask.push(file),
            }
        }
        for (to, files) in groups {
            match self.convert(&files, to, &Options::default(), cx) {
                Ok(ids) => added.jobs.extend(ids),
                Err(e) => {
                    added.errors.push(e);
                    break;
                }
            }
        }
        added
    }

    /// Converts a request that named its target, in place and without a
    /// window. Fails, with the reason, when the request needs a window to ask:
    /// the target is unknown or unreachable, a file can't be converted, or
    /// the license stops conversions.
    pub fn convert_silently(
        &mut self,
        request: &Request,
        cx: &mut Context<Self>,
    ) -> Result<Vec<JobId>, String> {
        if !request.auto_start() {
            return Err("This request needs a window.".into());
        }
        let expanded = expand(&self.registry, &request.files);
        if let Some(e) = expanded.unreadable.first() {
            return Err(e.clone());
        }
        let targets = common_targets(&self.registry, &expanded.files);
        let (to, options) = resolve(self, request.to.as_deref(), request.preset.as_deref())?;
        let to = to.ok_or("The preset doesn't name a format.")?;
        if expanded.files.is_empty()
            || !targets.unsupported.is_empty()
            || !targets.formats.contains(&to)
        {
            return Err(format!(
                "Not every file can become {}.",
                to.extension().to_uppercase()
            ));
        }
        // "In place" means next to the original, whatever Settings says.
        let ids = self.queue_jobs(
            &expanded.files,
            to,
            &options,
            Output::Beside,
            !request.show_progress,
            cx,
        )?;
        if cx.windows().is_empty() {
            // Nothing else keeps the app open, so quit once the files are done.
            self.quit_when_idle = true;
        }
        Ok(ids)
    }

    /// Runs a finished conversion again with the same input, target, options
    /// and output. Records from before setups were kept use the defaults.
    pub fn retry(
        &mut self,
        input: &Path,
        to: &'static Format,
        setup: Option<&Setup>,
        cx: &mut Context<Self>,
    ) -> Result<Vec<JobId>, String> {
        let files = [input.to_path_buf()];
        match setup {
            Some(setup) => self.convert_to(&files, to, &setup.options, setup.output.clone(), cx),
            None => self.convert(&files, to, &Options::default(), cx),
        }
    }

    /// Whether some conversion this registry offers reads documents: the
    /// document pack or a LibreOffice on this computer is in use.
    pub fn documents_supported(&self) -> bool {
        convt_core::FORMATS
            .iter()
            .any(|f| pack::is_document(f) && !self.registry.targets(f).is_empty())
    }

    /// Why documents can't convert right now, if the pack they use is
    /// being removed or replaced.
    pub fn documents_locked(&self) -> Option<&'static str> {
        if self.pack.removing {
            Some("Document support is being removed, so documents can't convert right now.")
        } else if matches!(self.pack.phase, PackPhase::Working(_))
            && matches!(self.pack.status, pack::Status::Installed(_))
        {
            Some(
                "Document support is being reinstalled. Documents can convert again once it's done.",
            )
        } else {
            None
        }
    }

    /// Whether a document is converting or waiting to.
    fn documents_converting(&self) -> bool {
        self.queue.entries.iter().any(|e| {
            !e.status.is_finished() && format_by_extension(&e.input).is_some_and(pack::is_document)
        })
    }

    /// Starts downloading and installing the document pack on a worker
    /// thread. Only the Download button calls this: nothing else may start a
    /// network request. A reinstall waits until no document is converting,
    /// since it replaces the files those conversions run.
    pub fn download_pack(&mut self, cx: &mut Context<Self>) {
        if matches!(self.pack.phase, PackPhase::Working(_))
            || self.pack.removing
            || !self.pack.offer.configured
        {
            return;
        }
        if matches!(self.pack.status, pack::Status::Installed(_)) && self.documents_converting() {
            self.pack.notice =
                Some("Wait for the document conversions to finish, then download it again.".into());
            cx.notify();
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.pack_cancel = Some(cancel.clone());
        self.pack.phase = PackPhase::Working(pack::Progress::Download {
            bytes: 0,
            total: None,
        });
        self.pack.notice = None;
        let backend = self.packs.clone();
        let (tx, rx) = unbounded();
        std::thread::Builder::new()
            .name("convt-pack".into())
            .spawn(move || {
                let result = backend.install(
                    &|progress| drop(tx.unbounded_send(PackEvent::Progress(progress))),
                    &|| cancel.load(Ordering::Relaxed),
                );
                let _ = tx.unbounded_send(PackEvent::Installed(result));
            })
            .expect("spawn the pack thread");
        self.follow_pack(rx, cx);
        cx.notify();
    }

    /// Stops the download. The partial file stays for the next try.
    pub fn cancel_pack_download(&mut self) {
        if let Some(cancel) = &self.pack_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Removes the document pack, once no document is converting.
    pub fn remove_pack(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if matches!(self.pack.phase, PackPhase::Working(_)) || self.pack.removing {
            return Err("Wait for the download to finish first.".into());
        }
        if self.documents_converting() {
            let error = "Wait for the document conversions to finish, then remove it.";
            self.pack.notice = Some(error.into());
            cx.notify();
            return Err(error.into());
        }
        self.pack.removing = true;
        self.pack.notice = None;
        // Nothing may start LibreOffice from the folder being deleted.
        self.registry = Arc::new(self.packs.registry_without_documents());
        self.runner.set_registry(self.registry.clone());
        self.registry_generation += 1;
        let backend = self.packs.clone();
        let (tx, rx) = unbounded();
        std::thread::Builder::new()
            .name("convt-pack".into())
            .spawn(move || {
                let _ = tx.unbounded_send(PackEvent::Removed(backend.remove()));
            })
            .expect("spawn the pack thread");
        self.follow_pack(rx, cx);
        cx.notify();
        Ok(())
    }

    fn follow_pack(
        &mut self,
        mut rx: futures::channel::mpsc::UnboundedReceiver<PackEvent>,
        cx: &mut Context<Self>,
    ) {
        self._pack_task = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                let Ok(()) = this.update(cx, |state, cx| state.pack_event(event, cx)) else {
                    break;
                };
            }
        }));
    }

    fn pack_event(&mut self, event: PackEvent, cx: &mut Context<Self>) {
        match event {
            PackEvent::Progress(progress) => {
                if matches!(self.pack.phase, PackPhase::Working(_)) {
                    self.pack.phase = PackPhase::Working(progress);
                }
            }
            PackEvent::Installed(result) => {
                self.pack_cancel = None;
                self.pack.phase = match result {
                    Ok(_) => PackPhase::Done,
                    Err(failure) => PackPhase::Failed(failure),
                };
                self.rebuild_registry();
            }
            PackEvent::Removed(result) => {
                self.pack.removing = false;
                self.pack.phase = PackPhase::Idle;
                self.pack.notice = result.err();
                self.rebuild_registry();
            }
        }
        cx.notify();
    }

    /// Reads the pack status again, offline, and rebuilds the registry if it
    /// changed, such as after `convt pack install` in a terminal.
    pub fn refresh_pack(&mut self, cx: &mut Context<Self>) {
        if matches!(self.pack.phase, PackPhase::Working(_)) || self.pack.removing {
            return;
        }
        if self.packs.status() != self.pack.status {
            self.rebuild_registry();
            cx.notify();
        }
    }

    fn rebuild_registry(&mut self) {
        self.pack.status = self.packs.status();
        self.registry = Arc::new(self.packs.registry());
        self.runner.set_registry(self.registry.clone());
        self.registry_generation += 1;
    }

    /// Switches an automation rule on or off.
    pub fn set_automation(&mut self, index: usize, enabled: bool, cx: &mut Context<Self>) {
        self.update_settings(
            |s| {
                if let Some(rule) = s.automations.get_mut(index) {
                    rule.enabled = enabled;
                }
            },
            cx,
        );
    }

    /// Whether a finished automation should copy its result.
    pub fn set_automation_copy(&mut self, index: usize, copy: bool, cx: &mut Context<Self>) {
        self.update_settings(
            |s| {
                if let Some(rule) = s.automations.get_mut(index) {
                    rule.copy_to_clipboard = Some(copy);
                    rule.detail = if copy {
                        "copy to clipboard".into()
                    } else {
                        "save beside original".into()
                    };
                }
            },
            cx,
        );
    }

    /// Lists each watched folder and converts new matching files. Tests call
    /// this twice after writing a file: once to see it, once after it is
    /// the same size.
    pub fn poll_automations(&mut self, cx: &mut Context<Self>) {
        let rules = self.settings.automations.clone();
        let ready = self.watch.drain_ready(&rules);
        for (index, path) in ready {
            let Some(rule) = rules.get(index) else {
                continue;
            };
            let Some(to) = format_by_id(&rule.to) else {
                continue;
            };
            let Some(from) = format_by_extension(&path) else {
                continue;
            };
            if !self.registry.targets(from).contains(&to) {
                continue;
            }
            let copy = rule.copies_to_clipboard();
            match self.convert(std::slice::from_ref(&path), to, &Options::default(), cx) {
                Ok(ids) => {
                    if copy {
                        self.automation_copies.extend(ids);
                    }
                }
                Err(e) => self.errors.push(e),
            }
        }
    }

    /// Verifies and stores a license key.
    pub fn activate(&mut self, key: &str, cx: &mut Context<Self>) -> Result<License, String> {
        let result = self.licensing.activate(key).map_err(|e| e.to_string());
        self.license = self.licensing.state();
        self.reselect_update();
        cx.notify();
        result
    }

    /// Removes the license from this machine.
    pub fn deactivate(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let result = self.licensing.deactivate();
        self.license = self.licensing.state();
        self.reselect_update();
        cx.notify();
        result
    }

    pub fn cancel(&mut self, id: JobId) {
        self.runner.cancel(id);
    }

    pub fn entry(&self, id: JobId) -> Option<&Entry> {
        self.queue.get(id)
    }

    pub fn clear_finished(&mut self, cx: &mut Context<Self>) {
        self.queue.clear_finished();
        cx.notify();
    }

    /// Clears everything finished from the Activity list: finished jobs and
    /// the history behind them.
    pub fn clear_activity(&mut self, cx: &mut Context<Self>) {
        self.clear_finished(cx);
        self.clear_history(cx);
    }

    /// Whether this build checks licenses.
    pub fn license_enforced(&self) -> bool {
        self.licensing.enforced()
    }

    fn apply(&mut self, update: crate::jobs::Update, cx: &mut Context<Self>) {
        if let Some(entry) = self.queue.apply(update).cloned() {
            // Silent jobs stay out of the batch summary and are never revealed.
            let silent = self.silent.remove(&entry.id);
            let mut ignored = Batch::default();
            let batch = if silent {
                &mut ignored
            } else {
                &mut self.batch
            };
            let outcome = match &entry.status {
                Status::Done(outputs) => {
                    batch.done += 1;
                    if self.automation_copies.remove(&entry.id) {
                        cx.write_to_clipboard(crate::clipboard::clipboard_item(outputs));
                    }
                    if !silent
                        && self.settings.reveal_when_done
                        && let Some(first) = outputs.first()
                    {
                        // GPUI's test platform can't reveal files.
                        #[cfg(not(test))]
                        cx.reveal_path(first);
                        #[cfg(test)]
                        self.revealed.push(first.clone());
                    }
                    Outcome::Done(outputs.clone())
                }
                Status::Failed(e) => {
                    self.automation_copies.remove(&entry.id);
                    batch.failed += 1;
                    Outcome::Failed(e.message.clone())
                }
                _ => {
                    batch.cancelled += 1;
                    Outcome::Cancelled
                }
            };
            if let Err(e) = self
                .history
                .add(&entry.input, entry.to.id, &entry.setup, &outcome)
            {
                tracing::warn!(error = %e, "could not record history");
            }
            self.refresh_history();
            if self.queue.active() == 0 {
                self.batch_finished(cx);
            }
        }
        cx.notify();
    }

    fn batch_finished(&mut self, cx: &mut Context<Self>) {
        let batch = std::mem::take(&mut self.batch);
        let in_background = cx.active_window().is_none();
        if self.settings.notifications
            && in_background
            && let Some((title, body)) = batch.summary()
        {
            cx.show_system_notification(SystemNotification {
                tag: "convt-batch".into(),
                title: title.into(),
                body: body.into(),
                actions: Vec::new(),
            });
        }
        if self.quit_when_idle
            && cx.windows().is_empty()
            && !crate::tray::keeps_running(self.settings.menu_bar_icon, cx)
        {
            crate::menu::quit(cx);
        }
    }

    fn refresh_history(&mut self) {
        self.recent = self.history.recent(RECENT).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "could not read history");
            Vec::new()
        });
    }

    pub fn clear_history(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = self.history.clear() {
            self.errors.push(format!("History was not cleared: {e}"));
        }
        self.refresh_history();
        cx.notify();
    }

    pub fn reload_presets(&mut self) {
        self.presets = match &self.presets_dir {
            Some(dir) => match Preset::load_dir(dir) {
                Ok(map) => map
                    .into_iter()
                    .map(|(name, p)| (name, p.map_err(|e| e.to_string())))
                    .collect(),
                Err(e) => {
                    self.errors.push(format!("Presets were not loaded: {e}"));
                    BTreeMap::new()
                }
            },
            None => BTreeMap::new(),
        };
    }

    /// Presets that loaded, by name.
    pub fn valid_presets(&self) -> impl Iterator<Item = (&String, &Preset)> {
        self.presets
            .iter()
            .filter_map(|(n, p)| p.as_ref().ok().map(|p| (n, p)))
    }

    pub fn preset(&self, name: &str) -> Result<&Preset, String> {
        match self.presets.get(name) {
            Some(Ok(p)) => Ok(p),
            Some(Err(e)) => Err(e.clone()),
            None => Err(format!("There is no preset named \"{name}\".")),
        }
    }

    pub fn save_preset(
        &mut self,
        name: &str,
        preset: &Preset,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !valid_preset_name(name) {
            return Err("Use letters, numbers, spaces, - and _ for the name.".into());
        }
        preset.options.validate().map_err(|e| e.to_string())?;
        let dir = self
            .presets_dir
            .as_ref()
            .ok_or("There is no config folder to save presets in.")?;
        write_atomic(
            &dir.join(format!("{name}.toml")),
            preset.to_toml().as_bytes(),
        )
        .map_err(|e| format!("The preset was not saved: {e}"))?;
        self.reload_presets();
        cx.notify();
        Ok(())
    }

    pub fn delete_preset(&mut self, name: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let dir = self
            .presets_dir
            .as_ref()
            .ok_or("There is no presets folder.")?;
        match std::fs::remove_file(dir.join(format!("{name}.toml"))) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("The preset was not deleted: {e}"));
            }
            _ => {}
        }
        self.reload_presets();
        cx.notify();
        Ok(())
    }

    /// Applies a settings change and saves it.
    pub fn update_settings(&mut self, change: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        change(&mut self.settings);
        self.runner.set_concurrency(self.settings.concurrency());
        if let Some(path) = &self.settings_path
            && let Err(e) = self.settings.save(path)
        {
            self.errors.push(format!("Settings were not saved: {e}"));
        }
        cx.notify();
    }

    /// [`Self::update_settings`] for a change that must reach the disk, such
    /// as the update check's rollback guard. If saving fails, the change is
    /// undone and the error returned.
    pub fn save_settings_now(
        &mut self,
        change: impl FnOnce(&mut Settings),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let before = self.settings.clone();
        change(&mut self.settings);
        if let Some(path) = &self.settings_path
            && let Err(e) = self.settings.save(path)
        {
            self.settings = before;
            cx.notify();
            return Err(e.to_string());
        }
        cx.notify();
        Ok(())
    }
}

/// The target and options a request or a picker selection resolves to.
pub fn resolve(
    state: &AppState,
    to: Option<&str>,
    preset: Option<&str>,
) -> Result<(Option<&'static Format>, Options), String> {
    let preset = preset.map(|name| state.preset(name)).transpose()?;
    let to = to
        .or(preset.and_then(|p| p.to.as_deref()))
        .map(|id| format_by_id(id).ok_or_else(|| format!("Unknown format \"{id}\".")))
        .transpose()?;
    let options = preset.map(|p| p.options.clone()).unwrap_or_default();
    Ok((to, options))
}

pub fn shared(cx: &App) -> Entity<AppState> {
    cx.global::<Shared>().0.clone()
}

pub fn file_name(path: &Path) -> SharedString {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
        .into()
}

/// `output` with its folder made absolute and canonical. A folder that
/// doesn't exist yet is made absolute against the working directory.
fn absolute_output(output: Output) -> Output {
    fn absolute(dir: &Path) -> PathBuf {
        std::fs::canonicalize(dir)
            .or_else(|_| std::path::absolute(dir))
            .unwrap_or_else(|_| dir.to_path_buf())
    }
    match output {
        Output::Beside => Output::Beside,
        Output::Dir(dir) => Output::Dir(absolute(&dir)),
        Output::Exact(path) => match (path.parent(), path.file_name()) {
            (Some(dir), Some(name)) if !dir.as_os_str().is_empty() => {
                Output::Exact(absolute(dir).join(name))
            }
            _ => Output::Exact(absolute(Path::new(".")).join(&path)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries() {
        let b = |done, failed, cancelled| {
            Batch {
                done,
                failed,
                cancelled,
            }
            .summary()
        };
        assert_eq!(b(0, 0, 3), None);
        assert_eq!(b(1, 0, 0).unwrap().1, "Converted 1 file.");
        assert_eq!(b(2, 1, 0).unwrap().1, "Converted 2 files; 1 file failed.");
        assert_eq!(b(0, 2, 0).unwrap().0, "Conversion failed");
    }

    #[test]
    fn preset_names() {
        for good in ["web", "Web 2048", "a_b-c"] {
            assert!(valid_preset_name(good), "{good}");
        }
        for bad in ["", "../x", "a/b", " x", "x ", "-x", "é", &"x".repeat(65)] {
            assert!(!valid_preset_name(bad), "{bad}");
        }
    }

    #[test]
    fn targets_intersect_in_first_file_order() {
        let registry = convt_engines::default_registry();
        let png = PathBuf::from("/x/a.png");
        let jpg = PathBuf::from("/x/b.JPG");
        let odd = PathBuf::from("/x/c.unknown");
        let t = common_targets(&registry, &[png.clone(), jpg.clone(), odd.clone()]);
        assert_eq!(t.unsupported, [odd]);
        let alone = registry.targets(format_by_id("png").unwrap());
        // Every common target is offered for both, in png's order.
        let jpeg_targets = registry.targets(format_by_id("jpeg").unwrap());
        let expected: Vec<_> = alone
            .into_iter()
            .filter(|f| jpeg_targets.contains(f))
            .collect();
        assert_eq!(t.formats, expected);
        assert!(t.formats.iter().any(|f| f.id == "webp"));
        assert_eq!(common_targets(&registry, &[]), Targets::default());
    }

    #[test]
    fn default_target_splits_photos_from_other_images() {
        let registry = convt_engines::default_registry();
        let settings = Settings::default();
        let state_target = |file: &str| {
            let from = format_by_extension(Path::new(file))?;
            let to = settings
                .defaults
                .get(Kind::of_file(Path::new(file), from)?)?;
            (from.id != to.id && registry.targets(from).contains(&to)).then_some(to.id)
        };
        assert_eq!(state_target("share.webp"), Some("jpeg"));
        assert_eq!(state_target("Screenshot 1.webp"), Some("png"));
        assert_eq!(state_target("diagram.svg"), Some("png"));
        assert_eq!(state_target("icon.bmp"), Some("png"));
        assert_eq!(state_target("already.png"), None);
        assert_eq!(state_target("already.jpg"), None);
        let heic = format_by_id("heic").unwrap();
        if registry.targets(heic).iter().any(|f| f.id == "jpeg") {
            assert_eq!(state_target("IMG_2041.heic"), Some("jpeg"));
            assert_eq!(state_target("Screenshot 1.heic"), Some("png"));
        }
    }
}
