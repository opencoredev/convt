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
    fn install_menus(&self) -> Result<Status, String>;
    fn remove_menus(&self) -> Result<Status, String>;
}

/// Runs the shipped `install.py`.
pub struct Installer;

impl Backend for Installer {
    fn status(&self) -> Status {
        probe()
    }

    fn install_menus(&self) -> Result<Status, String> {
        run_installer(&["--user", "--dolphin", "--nemo", "--thunar", "--nautilus"])?;
        Ok(probe())
    }

    fn remove_menus(&self) -> Result<Status, String> {
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
    let mut user = kinds_in(&user_data(), true);
    if thunar_user_actions() {
        user.push("Thunar".into());
    }
    if !user.is_empty() {
        return Status::Installed(user);
    }
    // Nautilus scripts under /usr/share never appear in GNOME Files.
    let system = kinds_in(Path::new("/usr/share"), false);
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

fn kinds_in(root: &Path, user_scripts: bool) -> Vec<String> {
    let mut out = Vec::new();
    if dir_has_owned(&root.join("kio/servicemenus"), "convt-") {
        out.push("Dolphin".into());
    }
    if dir_has_owned(&root.join("nemo/actions"), "convt-") {
        out.push("Nemo".into());
    }
    if owned_menu_file(&root.join("nautilus-python/extensions/convt_nautilus.py"))
        || (user_scripts && dir_has_owned(&root.join("nautilus/scripts/Convert with convt"), ""))
    {
        out.push("GNOME Files".into());
    }
    out
}

fn user_config() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Thunar custom actions live in the user config tree, not /usr/share.
fn thunar_user_actions() -> bool {
    thunar_actions_in(&user_config().join("Thunar/uca.xml"))
}

const MENU_MARKER: &str = "convt-generated: linux-integration";

fn thunar_actions_in(uca: &Path) -> bool {
    if uca.is_symlink() || !uca.is_file() {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(uca) else {
        return false;
    };
    thunar_has_owned_action(&text)
}

fn thunar_has_owned_action(text: &str) -> bool {
    if text.contains(MENU_MARKER) {
        return true;
    }
    // Unmarked older actions: unique-id convt-webp / convt-more-options and a
    // convt-app command. A leftover convt-custom with `echo keep` is not ours.
    for chunk in text.split("<unique-id>") {
        let Some(end) = chunk.find("</unique-id>") else {
            continue;
        };
        if !legacy_thunar_id(&chunk[..end]) {
            continue;
        }
        if let Some(command) = thunar_command_near(&chunk[end..])
            && looks_like_convt_open(&command)
        {
            return true;
        }
    }
    false
}

fn legacy_thunar_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("convt-") else {
        return false;
    };
    rest == "more-options"
        || (!rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

fn thunar_command_near(after_id: &str) -> Option<String> {
    let start = after_id.find("<command>")? + "<command>".len();
    let end = after_id[start..].find("</command>")?;
    Some(after_id[start..start + end].replace("%%", "%"))
}

fn looks_like_convt_open(command: &str) -> bool {
    (command.contains("convt-app") || command.contains(".AppImage")) && command.contains(" open ")
}

fn owned_menu_file(path: &Path) -> bool {
    if path
        .symlink_metadata()
        .map(|m| !m.file_type().is_file() || m.file_type().is_symlink())
        .unwrap_or(true)
    {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    looks_like_owned_menu(name, &text)
}

/// Marked files, or unmarked templates from older install.py runs. A leftover
/// `convt-personal` file with a custom Name= is not ours.
fn looks_like_owned_menu(name: &str, text: &str) -> bool {
    if text.contains(MENU_MARKER) {
        return true;
    }
    if name.ends_with(".desktop") {
        return text.contains("X-KDE-Submenu=Convert with convt") && looks_like_convt_open(text);
    }
    if name.ends_with(".nemo_action") {
        let convert = text.contains("Name=Convert to ") || text.contains("Name=More options");
        let comment = text.contains("Comment=Convert with convt")
            || text.contains("Comment=Open Quick convert");
        return text.contains("[Nemo Action]") && convert && comment;
    }
    if name == "convt_nautilus.py" {
        return text.contains("class ConvtMenu") && text.contains("Convert with convt");
    }
    text.starts_with("#!/bin/sh\n") && text.contains("exec ") && looks_like_convt_open(text)
}

fn dir_has_owned(dir: &Path, prefix: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        (prefix.is_empty() || name.starts_with(prefix)) && owned_menu_file(&entry.path())
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
        assert!(kinds_in(dir.path(), true).is_empty());
    }

    #[test]
    fn probe_reads_dolphin_nemo_and_gnome_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let marker = "# convt-generated: linux-integration-v1\n";
        std::fs::create_dir_all(root.join("kio/servicemenus")).unwrap();
        std::fs::write(root.join("kio/servicemenus/convt-0.desktop"), marker).unwrap();
        std::fs::create_dir_all(root.join("nemo/actions")).unwrap();
        std::fs::write(root.join("nemo/actions/convt-jpeg.nemo_action"), marker).unwrap();
        std::fs::create_dir_all(root.join("nautilus/scripts/Convert with convt")).unwrap();
        std::fs::write(
            root.join("nautilus/scripts/Convert with convt/JPEG"),
            marker,
        )
        .unwrap();
        assert_eq!(kinds_in(root, true), ["Dolphin", "Nemo", "GNOME Files"]);
        assert_eq!(kinds_in(root, false), ["Dolphin", "Nemo"]);
    }

    #[test]
    fn probe_ignores_unrelated_convt_named_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("nemo/actions")).unwrap();
        std::fs::write(
            root.join("nemo/actions/convt-personal.nemo_action"),
            "[Nemo Action]\nName=Mine\nIcon-Name=convt\nExec=echo keep\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("kio/servicemenus")).unwrap();
        std::fs::write(root.join("kio/servicemenus/convt-0.desktop"), "mine").unwrap();
        assert!(kinds_in(root, true).is_empty());
    }

    #[test]
    fn probe_reads_unmarked_legacy_templates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("kio/servicemenus")).unwrap();
        std::fs::write(
            root.join("kio/servicemenus/convt-0.desktop"),
            "[Desktop Entry]\nX-KDE-Submenu=Convert with convt\nExec=/usr/bin/convt-app open --to webp -- %F\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("nemo/actions")).unwrap();
        std::fs::write(
            root.join("nemo/actions/convt-webp.nemo_action"),
            "[Nemo Action]\nName=Convert to WEBP\nComment=Convert with convt\nExec=/usr/bin/convt-app open --to webp -- %F\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("nautilus/scripts/Convert with convt")).unwrap();
        std::fs::write(
            root.join("nautilus/scripts/Convert with convt/WEBP"),
            "#!/bin/sh\nexec /usr/bin/convt-app open --to webp -- \"$@\"\n",
        )
        .unwrap();
        assert_eq!(kinds_in(root, true), ["Dolphin", "Nemo", "GNOME Files"]);
    }

    #[test]
    fn thunar_probe_reads_owned_actions() {
        let dir = tempfile::tempdir().unwrap();
        let uca = dir.path().join("Thunar/uca.xml");
        std::fs::create_dir_all(uca.parent().unwrap()).unwrap();
        std::fs::write(
            &uca,
            "<!-- convt-generated: linux-integration-v1 --><unique-id>convt-webp</unique-id>",
        )
        .unwrap();
        assert!(thunar_actions_in(&uca));
        assert!(!thunar_actions_in(&dir.path().join("missing.xml")));
        std::fs::write(
            &uca,
            "<action><unique-id>convt-custom</unique-id><command>echo keep</command></action>",
        )
        .unwrap();
        assert!(!thunar_actions_in(&uca));
        std::fs::write(
            &uca,
            "<action><unique-id>convt-webp</unique-id>\
             <command>/usr/bin/convt-app open --to webp -- %F</command></action>",
        )
        .unwrap();
        assert!(thunar_actions_in(&uca));
    }
}
