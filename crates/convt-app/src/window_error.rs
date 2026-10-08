//! When GPUI cannot open a window, tell the user and exit.
//!
//! On Linux, retry once with Mesa's lavapipe software Vulkan driver if it is
//! installed. GPUI's Linux renderer is Vulkan-only; there is no GL path.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

const SOFTWARE_ENV: &str = "CONVT_SOFTWARE_RENDER";

static FAILED: AtomicBool = AtomicBool::new(false);

/// True after a window failed to open and we gave up.
pub fn failed() -> bool {
    FAILED.load(Ordering::SeqCst)
}

/// Log the error, try a software-rendering restart, then show a dialog and
/// mark the process as failed so `main` exits non-zero.
pub fn report(error: &dyn std::fmt::Display, title: &str) {
    let message = format_message(error, title);
    eprintln!("convt-app: {message}");
    if let Some(path) = write_log(&message) {
        eprintln!("convt-app: details written to {}", path.display());
    }
    if try_software_reexec() {
        return;
    }
    show_dialog(&message);
    FAILED.store(true, Ordering::SeqCst);
}

pub fn format_message(error: &dyn std::fmt::Display, title: &str) -> String {
    format!(
        "convt could not open a window ({title}): {error}\n\n\
         This usually means there is no working graphics driver. \
         Install a Vulkan driver for your GPU, or Mesa's lavapipe package \
         for software rendering."
    )
}

pub fn log_path() -> Option<PathBuf> {
    convt_engines::paths::config_dir().map(|d| d.join("window-error.log"))
}

fn write_log(message: &str) -> Option<PathBuf> {
    let path = log_path()?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    std::fs::write(&path, format!("{message}\n")).ok()?;
    Some(path)
}

/// Known lavapipe ICD locations on Debian, Fedora and Flatpak-style Mesa.
pub const SOFTWARE_ICD_CANDIDATES: &[&str] = &[
    "/usr/share/vulkan/icd.d/lvp_icd.x86_64.json",
    "/usr/share/vulkan/icd.d/lvp_icd.json",
    "/usr/lib/x86_64-linux-gnu/GL/vulkan/icd.d/lvp_icd.json",
    "/usr/lib64/GL/vulkan/icd.d/lvp_icd.json",
];

pub fn software_icd() -> Option<PathBuf> {
    software_icd_in(SOFTWARE_ICD_CANDIDATES)
}

pub fn software_icd_in(candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(Path::new)
        .find(|path| path.is_file())
        .map(Path::to_path_buf)
}

pub fn already_software() -> bool {
    std::env::var_os(SOFTWARE_ENV).is_some()
}

/// Replaces this process with the same command, forcing lavapipe. Returns
/// false when there is nothing to try or exec failed.
pub fn try_software_reexec() -> bool {
    if !cfg!(target_os = "linux") || already_software() {
        return false;
    }
    let Some(icd) = software_icd() else {
        return false;
    };
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .env(SOFTWARE_ENV, "1")
        .env("VK_ICD_FILENAMES", &icd)
        .env("VK_DRIVER_FILES", &icd);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        eprintln!("convt-app: software-rendering restart failed: {err}");
        false
    }
    #[cfg(not(unix))]
    {
        let _ = icd;
        false
    }
}

pub fn show_dialog(message: &str) {
    if let Some(mut cmd) = dialog_command(message) {
        let _ = cmd.status();
    }
}

/// `zenity` first, then `kdialog`. None when neither is on PATH.
pub fn dialog_command(message: &str) -> Option<Command> {
    dialog_command_in(message, &std::env::var_os("PATH").unwrap_or_default())
}

pub fn dialog_command_in(message: &str, path: &std::ffi::OsStr) -> Option<Command> {
    if let Some(bin) = look_up("zenity", path) {
        let mut cmd = Command::new(bin);
        cmd.args(["--error", "--title=convt", "--no-wrap", "--text", message]);
        return Some(cmd);
    }
    if let Some(bin) = look_up("kdialog", path) {
        let mut cmd = Command::new(bin);
        cmd.args(["--title", "convt", "--error", message]);
        return Some(cmd);
    }
    None
}

fn look_up(name: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_names_the_window_and_the_error() {
        let message = format_message(&"Failed to create surface", "convt");
        assert!(message.contains("Failed to create surface"), "{message}");
        assert!(message.contains("could not open a window"), "{message}");
        assert!(
            message.contains("Vulkan") || message.contains("graphics"),
            "{message}"
        );
    }

    #[test]
    fn software_icd_picks_the_first_file() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.json");
        let found = dir.path().join("lvp.json");
        std::fs::write(&found, "{}").unwrap();
        let missing_s = missing.to_string_lossy();
        let found_s = found.to_string_lossy();
        assert_eq!(software_icd_in(&[&missing_s, &found_s]), Some(found));
        assert!(software_icd_in(&[&missing_s]).is_none());
    }

    #[test]
    fn zenity_is_preferred_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let zenity = dir.path().join("zenity");
        let kdialog = dir.path().join("kdialog");
        std::fs::write(&zenity, "").unwrap();
        std::fs::write(&kdialog, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&zenity, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::set_permissions(&kdialog, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let cmd = dialog_command_in("hello", dir.path().as_os_str()).expect("zenity");
        assert_eq!(cmd.get_program(), zenity);
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"--error".into()));
        assert!(args.contains(&"hello".into()));
    }

    #[test]
    fn kdialog_is_used_when_zenity_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let kdialog = dir.path().join("kdialog");
        std::fs::write(&kdialog, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&kdialog, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let cmd = dialog_command_in("no window", dir.path().as_os_str()).expect("kdialog");
        assert_eq!(cmd.get_program(), kdialog);
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"--error".into()));
        assert!(args.contains(&"no window".into()));
    }

    #[test]
    fn no_dialog_when_path_is_empty() {
        assert!(dialog_command_in("x", std::ffi::OsStr::new("")).is_none());
    }
}
