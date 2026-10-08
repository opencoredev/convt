//! Installs a downloaded, verified update and starts the new version once
//! this process has quit.
//!
//! The installer re-checks the file's SHA-256 against the signed manifest
//! first (the caller does, in `update.rs`); that hash is the trust root.
//!
//! - macOS: the disk image is mounted read-only, and its `convt.app` must
//!   pass `codesign --verify --deep --strict`. When the running app is
//!   signed with a Team ID, the check also requires an Apple-issued
//!   certificate chain whose leaf carries that Team ID. A running app with
//!   no Team ID (ad-hoc signed, as a build from source is) has nothing to
//!   compare against, so it relies on the hash and the signature check
//!   alone. The bundle replaces the running one in a single exchange
//!   (`renamex_np` with `RENAME_SWAP`), so `convt.app` is whole at every
//!   moment, even if convt exits mid-update; a file system that can't swap
//!   gets two renames, rolled back if the second fails. A shell waits for
//!   this process to exit, then `open`s the bundle.
//! - Windows: a hidden PowerShell waits, with no time limit, for this
//!   process to exit, runs the MSI with `/passive` (the MSI is per-user and
//!   upgrades in place), records msiexec's exit code in
//!   [`RESULT_FILE`] in the updates folder, then starts convt again, updated
//!   or not. The next launch reads that file ([`take_result`]) and says when
//!   the install failed. If it can't tell that convt exited, it installs
//!   nothing.
//! - Linux: the AppImage named by `$APPIMAGE` is replaced with a rename in
//!   its own folder, and a shell starts it once this process has quit.
//!
//! The shells on macOS and Linux wait by reading a pipe whose only write end
//! this process holds; it closes when the process exits, however it exits.
//! The staging and backup copies have fixed hidden names next to the
//! target, and leftovers from an interrupted update are swept first. Quit
//! waits while an install runs (`menu::quit`), but a quit from the Dock or
//! the system can't be held back, which is why the swap has no moment
//! without a convt to start.
//!
//! Anything else (the deb, rpm and tarball, an app running from its disk
//! image or a folder convt can't write to) keeps the download page.

// Each platform uses some of these helpers; the tests run all of them everywhere.
#![allow(dead_code)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Puts a verified update in place, or hands it to a helper that will, and
/// arranges for the new version to start after this process exits. Runs off
/// the UI thread; the caller quits on `Ok`. Tests use their own.
pub trait Installer: Send + Sync {
    fn install(&self, file: &Path) -> Result<(), String>;
}

/// The installer for this platform.
pub struct System;

impl Installer for System {
    fn install(&self, file: &Path) -> Result<(), String> {
        platform::install(file)
    }
}

/// Whether this install can update itself with an artifact of `kind`, and
/// if not, why. Checked before downloading and again before installing.
pub fn supported(kind: &str) -> Result<(), String> {
    platform::supported(kind)
}

/// The `.app` bundle an executable runs from: the nearest ancestor named
/// `*.app`.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .skip(1)
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
}

/// Why a bundle at this path can't be replaced in place, judged from the
/// path alone: macOS runs a downloaded app that wasn't moved from a
/// randomized read-only copy. [`read_only`] catches a disk image.
pub fn path_refusal(bundle: &Path) -> Option<&'static str> {
    bundle
        .to_string_lossy()
        .contains("/AppTranslocation/")
        .then_some(
            "convt is running from a temporary copy macOS made. Move it to Applications first.",
        )
}

/// The Team ID `codesign -dv` printed, if the code is signed with one.
/// Only letters and digits count, so it can go into a code requirement.
pub fn team_id(codesign_output: &str) -> Option<String> {
    codesign_output
        .lines()
        .find_map(|l| l.trim().strip_prefix("TeamIdentifier="))
        .map(str::trim)
        .filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_alphanumeric()))
        .map(str::to_string)
}

/// The `codesign -R` requirement an update must meet: signed through
/// Apple's certificate chain, with `team` on the leaf certificate.
pub fn team_requirement(team: &str) -> String {
    format!("=anchor apple generic and certificate leaf[subject.OU] = \"{team}\"")
}

/// The hidden sibling of `path` with `tag` in its name. Fixed, so a later
/// update finds and removes what an interrupted one left.
fn sibling(path: &Path, tag: &str) -> PathBuf {
    let name = path
        .file_name()
        .map_or("convt".into(), |n| n.to_string_lossy().into_owned());
    path.with_file_name(format!(".{name}.{tag}"))
}

/// Deletes the staging and backup copies earlier updates of `path` left
/// next to it: exactly the names [`sibling`] makes (`.<name>.new` and
/// `.<name>.old`), and those with a process id after a dash, which older
/// builds used. Anything else that starts the same way stays.
pub fn sweep(path: &Path) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return;
    };
    let name = name.to_string_lossy();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let file = entry.file_name();
        if is_leftover(&file.to_string_lossy(), &name)
            && let Err(e) = remove_any(&entry.path())
        {
            tracing::warn!(error = %e, path = %entry.path().display(), "couldn't delete an old update copy");
        }
    }
}

/// Whether `file` is a staging or backup copy of `name`.
fn is_leftover(file: &str, name: &str) -> bool {
    ["new", "old"].iter().any(|tag| {
        file.strip_prefix(&format!(".{name}.{tag}"))
            .is_some_and(|rest| {
                rest.is_empty()
                    || rest.strip_prefix('-').is_some_and(|pid| {
                        !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit())
                    })
            })
    })
}

fn remove_any(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Replaces `current` with `staged`, which must be in the same folder, and
/// deletes the old one. Where the file system can, the two trade places in
/// one step, so `current` is never missing even if this process dies part
/// way. Elsewhere the current one moves aside and the staged one takes its
/// name; if that second rename fails the first is undone.
pub fn swap_in(current: &Path, staged: &Path) -> io::Result<()> {
    swap_using(current, staged, exchange)
}

fn swap_using(
    current: &Path,
    staged: &Path,
    exchange: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    match exchange(staged, current) {
        Ok(()) => {
            // `staged` now holds the old copy. The running process keeps its
            // open files, so it can go; a later sweep gets it if this fails.
            if let Err(e) = remove_any(staged) {
                tracing::warn!(error = %e, "couldn't delete the old convt");
            }
            return Ok(());
        }
        Err(e) if !cannot_exchange(&e) => return Err(e),
        Err(_) => {}
    }
    let backup = sibling(current, "old");
    remove_any(&backup)?;
    fs::rename(current, &backup)?;
    if let Err(e) = fs::rename(staged, current) {
        if let Err(undo) = fs::rename(&backup, current) {
            tracing::error!(error = %undo, backup = %backup.display(), "couldn't restore convt after a failed update");
        }
        return Err(e);
    }
    if let Err(e) = remove_any(&backup) {
        tracing::warn!(error = %e, "couldn't delete the old convt");
    }
    Ok(())
}

/// Whether an [`exchange`] failed because the file system or the OS can't
/// swap, rather than because of the paths.
fn cannot_exchange(e: &io::Error) -> bool {
    if e.kind() == io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(unix)]
    {
        // ENOTSUP and EOPNOTSUPP are the same number on some systems.
        e.raw_os_error().is_some_and(|code| {
            [libc::ENOTSUP, libc::EOPNOTSUPP, libc::EINVAL, libc::ENOSYS].contains(&code)
        })
    }
    #[cfg(not(unix))]
    false
}

/// Swaps what `a` and `b` name in one atomic step: `renamex_np` with
/// `RENAME_SWAP` on macOS, `renameat2` with `RENAME_EXCHANGE` on Linux.
#[cfg(any(target_os = "macos", all(target_os = "linux", target_env = "gnu")))]
fn exchange(a: &Path, b: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let c = |p: &Path| {
        std::ffi::CString::new(p.as_os_str().as_bytes())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
    };
    let (a, b) = (c(a)?, c(b)?);
    // SAFETY: both are valid NUL-terminated paths for the duration of the call.
    #[cfg(target_os = "macos")]
    let r = unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_SWAP) };
    // SAFETY: as above; AT_FDCWD resolves relative paths as rename does.
    #[cfg(target_os = "linux")]
    let r = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            a.as_ptr(),
            libc::AT_FDCWD,
            b.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if r == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(target_os = "macos", all(target_os = "linux", target_env = "gnu"))))]
fn exchange(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

/// Replaces the file at `target` with a copy of `new`: copied next to it
/// first, made executable, then renamed over it, so `target` is always
/// either the old file or the whole new one.
pub fn replace_file(target: &Path, new: &Path) -> io::Result<()> {
    sweep(target);
    let staged = sibling(target, "new");
    remove_any(&staged)?;
    let result = (|| {
        fs::copy(new, &staged)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
        }
        fs::File::open(&staged)?.sync_all()?;
        fs::rename(&staged, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

/// Whether this user may create files in `dir`.
#[cfg(unix)]
pub fn writable(dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `c` is a valid NUL-terminated path for the duration of the call.
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

/// Whether `path` is on a read-only file system, such as a mounted disk image.
#[cfg(unix)]
pub fn read_only(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid path and `st` a writable statvfs.
    let ok = unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0;
    ok && (st.f_flag & libc::ST_RDONLY) != 0
}

/// The shell that waits for this process to exit and then runs `then` with
/// the given arguments. Arguments travel as positional parameters, never
/// through the script text.
///
/// The shell reads its standard input, a pipe, until it ends. The returned
/// writer is the pipe's only write end: keep it open until this process
/// exits (`std::mem::forget` after spawning), and the pipe ends exactly then.
/// Both ends are close-on-exec, so no other child ever holds the write end.
#[cfg(unix)]
pub fn after_exit(
    then: &str,
    args: &[&std::ffi::OsStr],
) -> io::Result<(std::process::Command, io::PipeWriter)> {
    let (reader, writer) = io::pipe()?;
    let script = format!("cat >/dev/null; exec {then} \"$@\" </dev/null");
    let mut cmd = std::process::Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(script)
        .arg("sh")
        .args(args)
        .stdin(reader)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    {
        use std::os::unix::process::CommandExt;
        // Its own process group, so a signal to convt's group can't take it too.
        cmd.process_group(0);
    }
    Ok((cmd, writer))
}

/// Spawns an [`after_exit`] shell and keeps its pipe open until this process
/// exits.
#[cfg(unix)]
fn spawn_after_exit(mut cmd: std::process::Command, writer: io::PipeWriter) -> io::Result<()> {
    cmd.spawn()?;
    // Drops this process's copy of the read end.
    drop(cmd);
    std::mem::forget(writer);
    Ok(())
}

/// A PowerShell literal: single quotes, with inner ones doubled.
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Where the Windows helper records how msiexec ended, in the updates folder.
pub const RESULT_FILE: &str = "install-result.json";

/// The exit codes msiexec ends a good install with: done, done and a
/// restart starts, done and a restart is needed.
const MSI_OK: [i64; 3] = [0, 1641, 3010];

/// The PowerShell script that installs `msi` (the MSI of `version`) once
/// `pid` has exited and then starts convt again: from the folder the MSI
/// records, else `exe`. It waits as long as convt runs, and installs nothing
/// unless convt has exited. msiexec's exit code goes into `result` (-1 if it
/// didn't start, nothing if Windows didn't say), which the next launch reads; a failed install still
/// starts the old convt, so it can say so.
pub fn windows_script(pid: u32, msi: &Path, exe: &Path, version: &str, result: &Path) -> String {
    // Windows paths can't contain double quotes, so quoting the MSI path for
    // msiexec's command line is safe.
    let args = format!("/i \"{}\" /passive /norestart", msi.display());
    format!(
        "$ErrorActionPreference = 'Stop'\n\
         $p = Get-Process -Id {pid} -ErrorAction SilentlyContinue\n\
         if ($p) {{ $p.WaitForExit(); if (-not $p.HasExited) {{ exit 1 }} }}\n\
         $ErrorActionPreference = 'SilentlyContinue'\n\
         $i = Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\\msiexec.exe') -ArgumentList {args} -Wait -PassThru\n\
         $code = if ($i) {{ $i.ExitCode }} else {{ -1 }}\n\
         if ($null -ne $code) {{ @{{ version = {version}; exit_code = $code }} | ConvertTo-Json -Compress | Set-Content -LiteralPath {result} -Encoding UTF8 }}\n\
         $exe = {exe}\n\
         $dir = (Get-ItemProperty -Path 'HKCU:\\Software\\Convt' -Name InstallFolder).InstallFolder\n\
         if ($dir) {{ $c = Join-Path $dir 'convt-app.exe'; if (Test-Path -LiteralPath $c) {{ $exe = $c }} }}\n\
         Start-Process -FilePath $exe\n",
        args = ps_quote(&args),
        version = ps_quote(version),
        result = ps_quote(&result.display().to_string()),
        exe = ps_quote(&exe.display().to_string()),
    )
}

/// What the Windows helper wrote after msiexec ran.
#[derive(serde::Deserialize)]
struct InstallResult {
    version: String,
    exit_code: i64,
}

/// Reads and deletes the result an earlier install left in `dir`. Returns
/// the version and why, in a short line, when that install failed; `None`
/// when it worked or there's nothing to read.
pub fn take_result(dir: &Path) -> Option<(String, String)> {
    let path = dir.join(RESULT_FILE);
    let text = fs::read_to_string(&path).ok()?;
    if let Err(e) = fs::remove_file(&path) {
        tracing::warn!(error = %e, "couldn't delete the install result");
    }
    // PowerShell 5 writes UTF-8 with a byte order mark.
    let result: InstallResult = match serde_json::from_str(text.trim_start_matches('\u{feff}')) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "couldn't read the install result");
            return None;
        }
    };
    if MSI_OK.contains(&result.exit_code) {
        return None;
    }
    let why = match result.exit_code {
        -1 => "The installer couldn't be started.".to_string(),
        1602 => "The install was cancelled.".to_string(),
        1618 => "Another install was running. Try again in a moment.".to_string(),
        code => format!("Windows Installer stopped with error {code}."),
    };
    Some((result.version, why))
}

/// `script` as `-EncodedCommand` takes it: base64 of UTF-16LE, so no
/// command-line quoting is involved.
pub fn encoded(script: &str) -> String {
    use base64::Engine as _;
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::process::Command;

    fn running_bundle() -> Result<PathBuf, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = fs::canonicalize(&exe).unwrap_or(exe);
        bundle_of(&exe).ok_or_else(|| "convt isn't running from an app bundle.".to_string())
    }

    fn check(bundle: &Path) -> Result<(), String> {
        if let Some(why) = path_refusal(bundle) {
            return Err(why.into());
        }
        if read_only(bundle) {
            return Err(
                "convt is running from its disk image or a read-only disk. Drag it to Applications first."
                    .into(),
            );
        }
        let parent = bundle.parent().ok_or("convt's folder couldn't be found.")?;
        if !writable(parent) {
            return Err(format!(
                "convt can't write to {}, where it's installed.",
                parent.display()
            ));
        }
        Ok(())
    }

    pub fn supported(kind: &str) -> Result<(), String> {
        if kind != "dmg" {
            return Err("This kind of install updates from the download page.".into());
        }
        check(&running_bundle()?)
    }

    fn run(cmd: &mut Command) -> Result<std::process::Output, String> {
        let out = cmd.output().map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(out)
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    fn team_of(bundle: &Path) -> Option<String> {
        let out = Command::new("/usr/bin/codesign")
            .arg("-dv")
            .arg(bundle)
            .output()
            .ok()?;
        team_id(&String::from_utf8_lossy(&out.stderr))
    }

    /// Mounted read-only at a private folder; detached when dropped.
    struct Mount(PathBuf);

    impl Drop for Mount {
        fn drop(&mut self) {
            let detach = |force: bool| {
                let mut cmd = Command::new("/usr/bin/hdiutil");
                cmd.arg("detach").arg(&self.0).arg("-quiet");
                if force {
                    cmd.arg("-force");
                }
                cmd.status().is_ok_and(|s| s.success())
            };
            if !detach(false) && !detach(true) {
                tracing::warn!(mount = %self.0.display(), "couldn't detach the update image");
            }
            let _ = fs::remove_dir(&self.0);
        }
    }

    pub fn install(dmg: &Path) -> Result<(), String> {
        install_bundle(dmg, &running_bundle()?, true)
    }

    /// Replaces `bundle` with the convt.app on `dmg`, and with `relaunch`
    /// opens it once this process exits. Separate from [`install`] so a test
    /// can update a copy instead of the running app.
    pub(super) fn install_bundle(dmg: &Path, bundle: &Path, relaunch: bool) -> Result<(), String> {
        let bundle = bundle.to_path_buf();
        check(&bundle)?;
        let point = std::env::temp_dir().join(format!("convt-update-{}", std::process::id()));
        let _ = fs::remove_dir(&point);
        fs::create_dir(&point).map_err(|e| e.to_string())?;
        run(Command::new("/usr/bin/hdiutil")
            .args([
                "attach",
                "-nobrowse",
                "-readonly",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(&point)
            .arg(dmg))
        .map_err(|e| format!("The update's disk image couldn't be opened. {e}"))?;
        let mount = Mount(point);
        let new = mount.0.join("convt.app");
        if !new.join("Contents/Info.plist").is_file() {
            return Err("The update's disk image has no convt.app.".into());
        }
        let team = team_of(&bundle);
        let mut verify = Command::new("/usr/bin/codesign");
        verify.args(["--verify", "--deep", "--strict"]);
        if let Some(team) = &team {
            // Apple's chain with this Team ID on the leaf, checked by
            // codesign itself rather than by reading its output.
            verify.arg(format!("-R{}", team_requirement(team)));
        }
        run(verify.arg(&new)).map_err(|e| {
            if team.is_some() && e.contains("requirement") {
                "The update is signed by someone else, so it wasn't installed.".to_string()
            } else {
                format!("The update's signature didn't check out. {e}")
            }
        })?;
        if let Some(team) = &team
            && team_of(&new).as_ref() != Some(team)
        {
            return Err("The update is signed by someone else, so it wasn't installed.".into());
        }
        sweep(&bundle);
        let staged = sibling(&bundle, "new");
        remove_any(&staged).map_err(|e| e.to_string())?;
        if let Err(e) = run(Command::new("/usr/bin/ditto").arg(&new).arg(&staged)) {
            let _ = remove_any(&staged);
            return Err(format!("The update couldn't be copied. {e}"));
        }
        drop(mount);
        if let Err(e) = swap_in(&bundle, &staged) {
            let _ = remove_any(&staged);
            return Err(format!("The update couldn't replace convt. {e}"));
        }
        if !relaunch {
            return Ok(());
        }
        after_exit("/usr/bin/open", &[bundle.as_os_str()])
            .and_then(|(cmd, writer)| spawn_after_exit(cmd, writer))
            .map_err(|e| format!("convt was updated but couldn't restart; open it again. {e}"))?;
        Ok(())
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn supported(kind: &str) -> Result<(), String> {
        if kind == "msi" {
            Ok(())
        } else {
            Err("This kind of install updates from the download page.".into())
        }
    }

    pub fn install(msi: &Path) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        // Downloads live in `<updates>/<version>/`.
        let folder = msi
            .parent()
            .ok_or("The update's folder couldn't be found.")?;
        let version = folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let result = folder.parent().unwrap_or(folder).join(RESULT_FILE);
        let script = windows_script(std::process::id(), msi, &exe, &version, &result);
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let powershell = Path::new(&root).join("System32\\WindowsPowerShell\\v1.0\\powershell.exe");
        Command::new(powershell)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
            ])
            .arg("-EncodedCommand")
            .arg(encoded(&script))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .map_err(|e| format!("The installer couldn't be started. {e}"))?;
        Ok(())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use super::*;

    fn appimage() -> Result<PathBuf, String> {
        let path = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_file())
            .ok_or("convt isn't running as an AppImage.")?;
        let dir = path
            .parent()
            .ok_or("The AppImage's folder couldn't be found.")?;
        if !writable(dir) {
            return Err(format!(
                "convt can't write to {}, where the AppImage is.",
                dir.display()
            ));
        }
        Ok(path)
    }

    pub fn supported(kind: &str) -> Result<(), String> {
        if kind != "AppImage" {
            return Err("This kind of install updates from the download page.".into());
        }
        appimage().map(drop)
    }

    pub fn install(file: &Path) -> Result<(), String> {
        let target = appimage()?;
        replace_file(&target, file)
            .map_err(|e| format!("The update couldn't replace convt. {e}"))?;
        let restart =
            |e: io::Error| format!("convt was updated but couldn't restart; open it again. {e}");
        let (mut cmd, writer) = after_exit("", &[target.as_os_str()]).map_err(restart)?;
        // The new AppImage's runtime sets its own; don't hand it ours.
        for var in ["APPIMAGE", "APPDIR", "ARGV0", "OWD", "LD_LIBRARY_PATH"] {
            cmd.env_remove(var);
        }
        spawn_after_exit(cmd, writer).map_err(restart)?;
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    use super::*;

    pub fn supported(_: &str) -> Result<(), String> {
        Err("This platform updates from the download page.".into())
    }

    pub fn install(_: &Path) -> Result<(), String> {
        Err("This platform updates from the download page.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn finds_the_bundle_and_refuses_temporary_copies() {
        assert_eq!(
            bundle_of(Path::new(
                "/Applications/convt.app/Contents/MacOS/convt-app"
            )),
            Some(PathBuf::from("/Applications/convt.app"))
        );
        assert_eq!(bundle_of(Path::new("/usr/bin/convt-app")), None);
        assert!(path_refusal(Path::new("/Applications/convt.app")).is_none());
        assert!(path_refusal(Path::new("/Users/a/Applications/convt.app")).is_none());
        assert!(
            path_refusal(Path::new(
                "/private/var/folders/x/T/AppTranslocation/1234/d/convt.app"
            ))
            .is_some()
        );
    }

    #[test]
    fn reads_the_team_id() {
        let signed = "Executable=/Applications/convt.app/Contents/MacOS/convt-app\n\
                      Identifier=app.convt.desktop\nTeamIdentifier=AB12CD34EF\nSealed Resources version=2";
        assert_eq!(team_id(signed).as_deref(), Some("AB12CD34EF"));
        assert_eq!(team_id("Identifier=x\nTeamIdentifier=not set\n"), None);
        assert_eq!(team_id("code object is not signed at all"), None);
        // Nothing that could change a code requirement gets through.
        assert_eq!(team_id("TeamIdentifier=AB\" or true or \"x"), None);
        assert_eq!(
            team_requirement("AB12CD34EF"),
            r#"=anchor apple generic and certificate leaf[subject.OU] = "AB12CD34EF""#
        );
    }

    fn bundle(dir: &Path, name: &str, version: &str) -> PathBuf {
        let b = dir.join(name);
        fs::create_dir_all(b.join("Contents/MacOS")).unwrap();
        fs::write(b.join("Contents/version"), version).unwrap();
        b
    }

    #[test]
    fn a_bundle_swap_replaces_the_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let current = bundle(dir.path(), "convt.app", "old");
        let staged = bundle(dir.path(), ".convt.app.new", "new");
        swap_in(&current, &staged).unwrap();
        assert_eq!(
            fs::read_to_string(current.join("Contents/version")).unwrap(),
            "new"
        );
        // An exchange leaves it for the next sweep, as the install does first.
        sweep(&current);
        assert_eq!(names(dir.path()), ["convt.app"]);
    }

    #[test]
    fn a_failed_bundle_swap_puts_the_old_one_back() {
        let dir = tempfile::tempdir().unwrap();
        let current = bundle(dir.path(), "convt.app", "old");
        // Nothing staged: the second rename fails.
        let missing = dir.path().join(".convt.app.new");
        assert!(swap_in(&current, &missing).is_err());
        assert_eq!(
            fs::read_to_string(current.join("Contents/version")).unwrap(),
            "old"
        );
        assert_eq!(names(dir.path()), ["convt.app"]);
        // A stale backup from an earlier try doesn't get in the way.
        let stale = sibling(&current, "old");
        fs::create_dir(&stale).unwrap();
        let staged = bundle(dir.path(), ".convt.app.new", "new");
        swap_in(&current, &staged).unwrap();
        assert_eq!(
            fs::read_to_string(current.join("Contents/version")).unwrap(),
            "new"
        );
        // An exchange leaves it for the next sweep, as the install does first.
        sweep(&current);
        assert_eq!(names(dir.path()), ["convt.app"]);
    }

    #[test]
    fn leftovers_from_interrupted_updates_are_swept() {
        let dir = tempfile::tempdir().unwrap();
        let current = bundle(dir.path(), "convt.app", "old");
        // Fixed names, and the per-process names older builds used.
        assert_eq!(sibling(&current, "new"), dir.path().join(".convt.app.new"));
        assert_eq!(sibling(&current, "old"), dir.path().join(".convt.app.old"));
        bundle(dir.path(), ".convt.app.new", "half copied");
        bundle(dir.path(), ".convt.app.old-4242", "older");
        fs::write(dir.path().join(".convt.app.new-17"), b"x").unwrap();
        // Other apps' files and similar names stay.
        bundle(dir.path(), "Other.app", "other");
        fs::write(dir.path().join(".convt.apples"), b"x").unwrap();
        for neighbor in [
            ".convt.app.old-notes",
            ".convt.app.newer",
            ".convt.app.new-",
            ".convt.app.old-12a",
            ".convt.app.new.bak",
        ] {
            fs::write(dir.path().join(neighbor), b"keep").unwrap();
        }
        sweep(&current);
        assert_eq!(
            names(dir.path()),
            [
                ".convt.app.new-",
                ".convt.app.new.bak",
                ".convt.app.newer",
                ".convt.app.old-12a",
                ".convt.app.old-notes",
                ".convt.apples",
                "Other.app",
                "convt.app"
            ]
        );
    }

    #[test]
    fn without_an_atomic_exchange_the_swap_falls_back_to_renames() {
        let unsupported = |_: &Path, _: &Path| Err(io::Error::from(io::ErrorKind::Unsupported));
        let dir = tempfile::tempdir().unwrap();
        let current = bundle(dir.path(), "convt.app", "old");
        let staged = bundle(dir.path(), ".convt.app.new", "new");
        swap_using(&current, &staged, unsupported).unwrap();
        assert_eq!(
            fs::read_to_string(current.join("Contents/version")).unwrap(),
            "new"
        );
        assert_eq!(names(dir.path()), ["convt.app"]);
        // A failed second rename puts the old one back.
        let missing = dir.path().join(".convt.app.new");
        assert!(swap_using(&current, &missing, unsupported).is_err());
        assert_eq!(
            fs::read_to_string(current.join("Contents/version")).unwrap(),
            "new"
        );
        assert_eq!(names(dir.path()), ["convt.app"]);
        // Any other exchange error stops before anything moves.
        let staged = bundle(dir.path(), ".convt.app.new", "newer");
        let denied = |_: &Path, _: &Path| Err(io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(swap_using(&current, &staged, denied).is_err());
        assert_eq!(names(dir.path()), [".convt.app.new", "convt.app"]);
    }

    /// The atomic exchange itself, where this OS has one and the temp
    /// folder's file system supports it.
    #[test]
    fn the_exchange_swaps_both_names_in_one_step() {
        let dir = tempfile::tempdir().unwrap();
        let current = bundle(dir.path(), "convt.app", "old");
        let staged = bundle(dir.path(), ".convt.app.new", "new");
        match exchange(&staged, &current) {
            Err(e) if cannot_exchange(&e) => {
                eprintln!("no atomic exchange here: {e}");
                return;
            }
            r => r.unwrap(),
        }
        // Both names exist at every point: each now holds the other copy.
        let version = |b: &Path| fs::read_to_string(b.join("Contents/version")).unwrap();
        assert_eq!(version(&current), "new");
        assert_eq!(version(&staged), "old");
        // What an exit right after the exchange leaves: the next update's
        // sweep removes the old copy.
        sweep(&current);
        assert_eq!(names(dir.path()), ["convt.app"]);
        assert_eq!(version(&current), "new");
    }

    #[cfg(unix)]
    #[test]
    fn an_appimage_is_replaced_whole_and_stays_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("convt.AppImage");
        fs::write(&target, b"old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
        let cache = tempfile::tempdir().unwrap();
        let new = cache.path().join("convt-linux-x86_64.AppImage");
        fs::write(&new, b"new appimage").unwrap();
        fs::set_permissions(&new, fs::Permissions::from_mode(0o644)).unwrap();
        replace_file(&target, &new).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new appimage");
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(names(dir.path()), ["convt.AppImage"]);
        // The verified download stays where it was.
        assert!(new.is_file());

        // A copy that fails leaves the old file and no stray copy.
        assert!(replace_file(&target, &cache.path().join("missing")).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"new appimage");
        assert_eq!(names(dir.path()), ["convt.AppImage"]);
        assert!(writable(dir.path()));
        assert!(!read_only(dir.path()));
    }

    #[cfg(unix)]
    #[test]
    fn the_relaunch_waits_until_the_pipe_closes() {
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("ran");
        let (mut relaunch, writer) = after_exit("touch", &[out.as_os_str()]).unwrap();
        let mut child = relaunch.spawn().unwrap();
        drop(relaunch);
        // A child started afterwards, like a conversion's FFmpeg, must not
        // hold the write end and keep the helper waiting.
        let mut other = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(!out.exists(), "ran before this process let go");
        assert!(child.try_wait().unwrap().is_none());
        // What exiting does: the last write end closes.
        let closed = Instant::now();
        drop(writer);
        let deadline = Instant::now() + Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "the helper kept waiting");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(out.is_file());
        assert!(closed.elapsed() < Duration::from_secs(10));
        let _ = other.kill();
        let _ = other.wait();
    }

    #[test]
    fn the_windows_script_quotes_paths() {
        let script = windows_script(
            4242,
            Path::new(r"C:\Users\O'Neil\AppData\Local\convt\updates\9.2.0\convt.msi"),
            Path::new(r"C:\Users\O'Neil\AppData\Local\convt\convt-app.exe"),
            "9.2.0",
            Path::new(r"C:\Users\O'Neil\AppData\Local\convt\updates\install-result.json"),
        );
        assert!(script.contains("Get-Process -Id 4242"));
        assert!(script.contains("WaitForExit()"));
        assert!(script.contains("if (-not $p.HasExited) { exit 1 }"));
        assert!(!script.contains("-Timeout"));
        // Stop on any error before msiexec runs.
        assert!(
            script.find("'Stop'").unwrap() < script.find("msiexec").unwrap()
                && script.find("HasExited").unwrap() < script.find("msiexec").unwrap()
        );
        assert!(script.contains(
            r#"-ArgumentList '/i "C:\Users\O''Neil\AppData\Local\convt\updates\9.2.0\convt.msi" /passive /norestart' -Wait -PassThru"#
        ));
        assert!(script.contains(r"$exe = 'C:\Users\O''Neil\AppData\Local\convt\convt-app.exe'"));
        // msiexec's exit code is kept for the next launch, before convt
        // starts again whatever it was.
        assert!(script.contains("$code = if ($i) { $i.ExitCode } else { -1 }"));
        assert!(script.contains(
            r"@{ version = '9.2.0'; exit_code = $code } | ConvertTo-Json -Compress | Set-Content -LiteralPath 'C:\Users\O''Neil\AppData\Local\convt\updates\install-result.json'"
        ));
        let recorded = script.find("Set-Content").unwrap();
        assert!(script.find("msiexec").unwrap() < recorded);
        assert!(recorded < script.rfind("Start-Process -FilePath $exe").unwrap());
        // Base64 of UTF-16LE, decodable back to the script.
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded(&script))
            .unwrap();
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(String::from_utf16(&units).unwrap(), script);
    }

    #[test]
    fn the_next_launch_reads_the_install_result_once() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(RESULT_FILE);
        assert_eq!(take_result(dir.path()), None, "nothing to read");
        // What ConvertTo-Json and Set-Content -Encoding UTF8 write.
        for (code, why) in [
            (1603, Some("Windows Installer stopped with error 1603.")),
            (1602, Some("The install was cancelled.")),
            (-1, Some("The installer couldn't be started.")),
            (0, None),
            (3010, None),
            (1641, None),
        ] {
            fs::write(
                &file,
                format!("\u{feff}{{\"version\":\"9.2.0\",\"exit_code\":{code}}}\r\n"),
            )
            .unwrap();
            assert_eq!(
                take_result(dir.path()),
                why.map(|w| ("9.2.0".to_string(), w.to_string())),
                "{code}"
            );
            assert!(!file.exists(), "read once, then deleted");
        }
        // Garbage is deleted too, and shows nothing.
        fs::write(&file, b"not json").unwrap();
        assert_eq!(take_result(dir.path()), None);
        assert!(!file.exists());
    }

    /// Updates a copy of an installed convt.app from a real release disk
    /// image, through the same hdiutil, codesign, ditto and swap steps as
    /// Restart to update, without relaunching. Run on a Mac:
    /// `CONVT_TEST_BUNDLE=/Applications/convt.app CONVT_TEST_DMG=<dmg> cargo test
    /// -p convt-app updates_a_copy_of_the_app -- --ignored`
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "needs an installed convt.app and a release disk image"]
    fn updates_a_copy_of_the_app() {
        let bundle = PathBuf::from(std::env::var("CONVT_TEST_BUNDLE").unwrap());
        let dmg = PathBuf::from(std::env::var("CONVT_TEST_DMG").unwrap());
        let dir = tempfile::tempdir_in(std::env::var("HOME").unwrap()).unwrap();
        let copy = dir.path().join("convt.app");
        let ditto = std::process::Command::new("/usr/bin/ditto")
            .arg(&bundle)
            .arg(&copy)
            .status()
            .unwrap();
        assert!(ditto.success());
        let version = |app: &Path| {
            let out = std::process::Command::new("/usr/bin/defaults")
                .arg("read")
                .arg(app.join("Contents/Info.plist"))
                .arg("CFBundleShortVersionString")
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        let before = version(&copy);
        platform::install_bundle(&dmg, &copy, false).unwrap();
        let after = version(&copy);
        eprintln!("updated a copy from {before} to {after}");
        assert_ne!(before, after);
        assert_eq!(
            names(dir.path()),
            ["convt.app"],
            "no staging or backup left"
        );
        let verify = std::process::Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&copy)
            .status()
            .unwrap();
        assert!(verify.success());
        let mounts = std::process::Command::new("/usr/bin/hdiutil")
            .arg("info")
            .output()
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&mounts.stdout).contains("convt-update-"),
            "the disk image is detached"
        );
    }
}
