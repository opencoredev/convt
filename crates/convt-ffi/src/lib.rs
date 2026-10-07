//! A small API for the OS integrations. The macOS Finder Sync extension calls
//! `targets_for` to build its menu, then hands the actual conversion to the app.
//! Swift and Kotlin bindings come from `uniffi-bindgen`.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use convt_core::{Cancel, Job, Registry};

uniffi::setup_scaffolding!();

static REGISTRY: LazyLock<Registry> = LazyLock::new(convt_engines::default_registry);

#[derive(Debug, uniffi::Record)]
pub struct Target {
    pub id: String,
    pub name: String,
    pub category: String,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ConvtError {
    #[error("{message}")]
    Failed { message: String },
}

impl From<convt_core::Error> for ConvtError {
    fn from(e: convt_core::Error) -> Self {
        Self::Failed {
            message: e.to_string(),
        }
    }
}

/// Formats the file at `path` can be converted to, for the context menu.
#[uniffi::export]
pub fn targets_for(path: String) -> Vec<Target> {
    let Some(from) = convt_core::format_by_extension(Path::new(&path)) else {
        return Vec::new();
    };
    REGISTRY
        .targets(from)
        .into_iter()
        .map(|f| Target {
            id: f.id.into(),
            name: f.name.into(),
            category: format!("{:?}", f.category),
        })
        .collect()
}

/// Converts `input` to `to`, writing next to the input without replacing
/// any existing file. Returns every file written (one per page for PDFs).
/// Fails like the app does when the license stops conversions: no trial
/// yet (it starts in the app, after signing in), a trial that ended, or a
/// clock that needs checking.
#[uniffi::export]
pub fn convert(input: String, to: String) -> Result<Vec<String>, ConvtError> {
    use convt_engines::paths;
    use convt_license::client::{Config, Licensing};
    let mut licensing = Licensing::new(Config::from_env(paths::config_dir(), paths::data_dir()));
    if let Err(blocked) = licensing.begin_conversion() {
        return Err(ConvtError::Failed {
            message: blocked.to_string(),
        });
    }
    let to = convt_core::format_by_id(&to).ok_or(convt_core::Error::UnknownFormat(to))?;
    let job = Job::new(PathBuf::from(input), to);
    let outputs = REGISTRY.run(&job, &|_| {}, &Cancel::new())?;
    Ok(outputs
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}
