//! Per-user Linux file-manager menus. Settings runs `install.py`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What Settings shows for the right-click menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Not Linux, or a test that hid the row.
    Unavailable,
    MissingInstaller,
    NotInstalled,
    Installing,
    Removing,
    /// User-owned menus are present.
    Installed(Vec<String>),
    /// The package already placed menus under `/usr/share`.
    System(Vec<String>),
    Failed(String),
}

impl Status {
    pub fn is_installed(&self) -> bool {
        matches!(self, Status::Installed(_) | Status::System(_))
    }
}

/// The installer the app talks to. Tests use their own so they never write
/// into a real home directory.
pub trait Backend: Send + Sync {
    fn status(&self) -> Status;
    fn install(&self) -> Result<Status, String>;
    fn remove(&self) -> Result<Status, String>;
}

/// Runs the shipped `install.py`.
pub struct Installer;

impl Backend for Installer {
    fn status(&self) -> Status {
        probe()
    }

    fn install(&self) -> Result<Status, String> {
        run_installer(&["--user", "--dolphin", "--nemo", "--thunar", "--nautilus"])?;
        Ok(probe())
    }

    fn remove(&self) -> Result<Status, String> {
        run_installer(&["--uninstall"])?;
        Ok(probe())
    }
}

pub fn default_backend() -> std::sync::Arc<dyn Backend> {
    std::sync::Arc::new(Installer)
}

/// Looks at user and system integration directories. Never runs the installer.
pub fn probe() -> Status {
    if !cfg!(target_os = "linux") {
        return Status::Unavailable;
    }
    let user = kinds_in(&user_data());
    if !user.is_empty() {
        return Status::Installed(user);
    }
    let system = kinds_in(Path::new("/usr/share"));
    if !system.is_empty() {
        return Status::System(system);
    }
    if installer_path().is_none() {
        return Status::MissingInstaller;
    }
    Status::NotInstalled
}

fn user_data() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn kinds_in(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if dir_has(&root.join("kio/servicemenus"), "convt-") {
        out.push("Dolphin".into());
    }
    if dir_has(&root.join("nemo/actions"), "convt-") {
        out.push("Nemo".into());
    }
    if root
        .join("nautilus-python/extensions/convt_nautilus.py")
        .is_file()
        || dir_has(&root.join("nautilus/scripts/Convert with convt"), "")
    {
        out.push("GNOME Files".into());
    }
    out
}

fn dir_has(dir: &Path, prefix: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        prefix.is_empty() || name.starts_with(prefix)
    })
}

pub fn installer_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CONVT_LINUX_INSTALLER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        for candidate in [
            dir.join("share/integrations/install.py"),
            dir.parent()
                .unwrap_or(dir)
                .join("share/integrations/install.py"),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    #[cfg(debug_assertions)]
    {
        let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../integrations/linux/install.py");
        if dev.is_file() {
            return Some(dev);
        }
    }
    let system = PathBuf::from("/usr/share/convt/integrations/install.py");
    system.is_file().then_some(system)
}

fn run_installer(args: &[&str]) -> Result<(), String> {
    let installer = installer_path()
        .ok_or_else(|| "The menu installer is not in this build. Reinstall convt.".to_string())?;
    let python = look_up("python3").ok_or_else(|| "python3 is not on PATH.".to_string())?;
    let out = Command::new(python)
        .arg(&installer)
        .args(args)
        .env("PATH", installer_path_env())
        .output()
        .map_err(|e| format!("Could not run the menu installer: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    Err([err.trim(), stdout.trim()]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or("the installer failed")
        .to_string())
}

fn installer_path_env() -> OsString {
    let mut parts = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        parts.push(dir.to_path_buf());
    }
    if let Some(path) = std::env::var_os("PATH") {
        parts.extend(std::env::split_paths(&path));
    }
    std::env::join_paths(parts).unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

fn look_up(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_data_dir_is_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        assert!(kinds_in(dir.path()).is_empty());
    }

    #[test]
    fn probe_reads_dolphin_nemo_and_gnome_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("kio/servicemenus")).unwrap();
        std::fs::write(root.join("kio/servicemenus/convt-0.desktop"), "x").unwrap();
        std::fs::create_dir_all(root.join("nemo/actions")).unwrap();
        std::fs::write(root.join("nemo/actions/convt-jpeg.nemo_action"), "x").unwrap();
        std::fs::create_dir_all(root.join("nautilus/scripts/Convert with convt")).unwrap();
        std::fs::write(root.join("nautilus/scripts/Convert with convt/JPEG"), "x").unwrap();
        assert_eq!(kinds_in(root), ["Dolphin", "Nemo", "GNOME Files"]);
    }
}
