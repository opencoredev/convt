//! Where convt keeps per-user files. The CLI and the desktop app share these,
//! so a preset saved in one shows up in the other.

use std::path::{Path, PathBuf};

/// `$CONVT_CONFIG_DIR`, else the platform config directory plus `convt`.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("CONVT_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|d| d.join("convt")))
}

/// Named presets, one `<name>.toml` each.
pub fn presets_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("presets"))
}

/// `$CONVT_DATA_DIR`, else the platform data directory plus `convt`.
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os("CONVT_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("convt")))
}

static SANDBOX_PACKAGE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// An explicit package location for the proc-free cloud executor. Desktop and
/// CLI discovery keep using the actual executable location.
pub fn set_sandbox_package(path: &Path) -> std::result::Result<(), &'static str> {
    if !path.is_absolute() {
        return Err("sandbox package must be absolute");
    }
    SANDBOX_PACKAGE
        .set(path.to_path_buf())
        .map_err(|_| "sandbox package already configured")
}

/// The running executable's directory. On macOS a symlinked CLI (such as
/// `/usr/local/bin/convt` pointing into `convt.app`) resolves to the bundle, so
/// it finds the tools and libraries that ship next to it.
pub(crate) fn exe_dir() -> Option<PathBuf> {
    if let Some(path) = SANDBOX_PACKAGE.get() {
        return Some(path.clone());
    }
    let exe = std::env::current_exe().ok()?;
    #[cfg(target_os = "macos")]
    let exe = exe.canonicalize().unwrap_or(exe);
    exe.parent().map(Path::to_path_buf)
}

/// Fixed package layout: executables live at the root; native libraries in lib.
/// A macOS app keeps executables in Contents/MacOS and libraries in Frameworks.
/// Never walk arbitrary ancestors (including for test binaries).
pub(crate) fn bundle_dirs() -> Vec<PathBuf> {
    exe_dir()
        .map(|p| {
            let mut dirs = vec![p.join("lib")];
            if p.file_name().is_some_and(|n| n == "MacOS")
                && p.parent()
                    .is_some_and(|p| p.file_name().is_some_and(|n| n == "Contents"))
            {
                dirs.push(p.parent().unwrap().join("Frameworks"));
            }
            dirs
        })
        .unwrap_or_default()
}

/// LibreOffice installed as a Mac app, in `/Applications` or the user's own
/// `~/Applications`. Finder-launched apps get a minimal `PATH`, so it would
/// not be found there.
#[cfg(target_os = "macos")]
pub(crate) fn libreoffice_app() -> Option<PathBuf> {
    let app = Path::new("LibreOffice.app/Contents/MacOS/soffice");
    [
        Some(PathBuf::from("/Applications")),
        dirs::home_dir().map(|h| h.join("Applications")),
    ]
    .into_iter()
    .flatten()
    .map(|d| d.join(app))
    .find(|p| p.is_file())
}
