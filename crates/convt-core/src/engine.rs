use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{Error, Format, Options, Result};

/// Reports progress from 0.0 to 1.0, or `None` when the engine can't tell.
pub type Progress<'a> = &'a (dyn Fn(Option<f32>) + Send + Sync);

/// A single conversion an engine can perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub from: &'static Format,
    pub to: &'static Format,
}

/// Cancels a running job. Clones share the same flag.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Everything an engine needs for one step besides the input file.
pub struct Ctx<'a> {
    pub step: Step,
    pub options: &'a Options,
    pub(crate) progress: Progress<'a>,
    pub(crate) cancel: &'a Cancel,
}

impl<'a> Ctx<'a> {
    pub fn new(
        step: Step,
        options: &'a Options,
        progress: Progress<'a>,
        cancel: &'a Cancel,
    ) -> Self {
        Self {
            step,
            options,
            progress,
            cancel,
        }
    }

    pub fn progress(&self, fraction: f32) {
        (self.progress)(Some(fraction.clamp(0.0, 1.0)));
    }

    pub fn indeterminate(&self) {
        (self.progress)(None);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Returns `Err(Cancelled)` once the job is cancelled. Engines call this
    /// between units of work.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Where to write artifact `index` (0-based) inside `out_dir`.
    pub fn artifact(&self, out_dir: &Path, index: usize) -> PathBuf {
        out_dir.join(format!("{}.{}", index + 1, self.step.to.extension()))
    }
}

pub trait Engine: Send + Sync {
    /// Short identifier, e.g. `"ffmpeg"`.
    fn id(&self) -> &'static str;

    /// Higher wins when two engines can do the same conversion.
    fn priority(&self) -> i32 {
        0
    }

    /// Returns why the engine can't run on this machine, if it can't
    /// (missing binary, missing library, unsupported OS).
    fn unavailable_reason(&self) -> Option<String> {
        None
    }

    /// Every direct conversion this engine supports.
    fn steps(&self) -> Vec<Step>;

    /// Converts `input` and writes the results into `out_dir`, which is empty
    /// and private to this call. Returns every file written, in order (one
    /// per page for paged output). Use [`Ctx::artifact`] for the names.
    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>>;
}
