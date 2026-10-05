//! Where convt keeps per-user files. The CLI and the desktop app share these,
//! so a preset saved in one shows up in the other.

use std::path::PathBuf;

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

/// Fixed package layout: executables live at the root; native libraries in lib.
/// A macOS app keeps executables in Contents/MacOS and libraries in Frameworks.
/// Never walk arbitrary ancestors (including for test binaries).
pub(crate) fn bundle_dirs() -> Vec<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent().map(|p| {
                let mut dirs = vec![p.join("lib")];
                if p.file_name().is_some_and(|n| n == "MacOS")
                    && p.parent()
                        .is_some_and(|p| p.file_name().is_some_and(|n| n == "Contents"))
                {
                    dirs.push(p.parent().unwrap().join("Frameworks"));
                }
                dirs
            })
        })
        .unwrap_or_default()
}
