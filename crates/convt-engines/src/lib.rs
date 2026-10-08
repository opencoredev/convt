//! The engines convt ships with. Each one wraps a native tool or library and
//! declares the conversions it supports; `convt-core` handles routing.

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use convt_core::{Ctx, Error, Registry, Result};

pub mod ffmpeg;
pub mod heic;
pub mod image;
pub mod office;
mod orientation;
#[cfg(test)]
mod orientation_tests;
pub mod packs;
pub mod paths;
#[cfg(feature = "pdfium")]
pub mod pdfium;
pub mod svg;
#[cfg(windows)]
mod windows_acl;

static CLOUD_SUPERVISED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Cloud tools stay in the executor's group. The parent worker owns tree cleanup;
/// seccomp forbids engines from changing the process group or session.
pub fn use_cloud_supervisor(package: &std::path::Path) -> std::result::Result<(), &'static str> {
    paths::set_sandbox_package(package)?;
    CLOUD_SUPERVISED.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// A registry with every engine that can run on this machine.
pub fn default_registry() -> Registry {
    registry(true)
}

/// [`default_registry`] without LibreOffice, for while the document pack is
/// being removed: nothing may start it from a folder being deleted.
pub fn registry_without_documents() -> Registry {
    registry(false)
}

fn registry(documents: bool) -> Registry {
    let mut r = Registry::new();
    r.register(Arc::new(image::ImageEngine));
    r.register(Arc::new(svg::SvgEngine));
    #[cfg(target_os = "macos")]
    r.register(Arc::new(heic::ImageIoEngine::new()));
    r.register(Arc::new(heic::LibheifEngine::new()));
    r.register(Arc::new(ffmpeg::FfmpegEngine::new()));
    #[cfg(feature = "pdfium")]
    r.register(Arc::new(pdfium::PdfiumEngine::new()));
    if documents {
        r.register(Arc::new(office::OfficeEngine::new()));
    }
    r
}

/// Finds a bundled or installed tool. Checks `$env_var`, then the directory
/// next to the running executable (where release builds bundle tools, and
/// `convt.app/Contents/MacOS` on macOS), then `PATH`.
pub(crate) fn find_tool(names: &[&str], env_var: &str) -> Option<PathBuf> {
    find_tool_with_pack(names, env_var, None)
}

/// An installed document pack follows the executable's own bundle and precedes PATH.
pub(crate) fn find_tool_with_pack(
    names: &[&str],
    env_var: &str,
    pack: Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(env_var)
        .map(PathBuf::from)
        .filter(|p| p.exists())
    {
        return Some(p);
    }
    let exe_dir = paths::exe_dir()?;
    for name in names {
        let bundled = exe_dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        if bundled.exists() {
            return if CLOUD_SUPERVISED.load(std::sync::atomic::Ordering::SeqCst) {
                bundled.canonicalize().ok()
            } else {
                Some(bundled)
            };
        }
    }
    pack.filter(|p| p.is_file())
        .or_else(|| names.iter().find_map(|n| which::which(n).ok()))
}

pub(crate) fn steps(from: &[&str], to: &[&str]) -> impl Iterator<Item = convt_core::Step> {
    let lookup =
        |id: &str| convt_core::format_by_id(id).unwrap_or_else(|| panic!("unknown format {id}"));
    let from: Vec<_> = from.iter().map(|id| lookup(id)).collect();
    let to: Vec<_> = to.iter().map(|id| lookup(id)).collect();
    from.into_iter().flat_map(move |a| {
        to.clone()
            .into_iter()
            .filter(move |b| *b != a)
            .map(move |b| convt_core::Step { from: a, to: b })
    })
}

/// CREATE_NO_WINDOW. GUI conversions must not flash a console for ffmpeg,
/// LibreOffice, sips, or any other child the engines start.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// On Windows, start the child without a console window. No-op elsewhere.
#[cfg_attr(
    not(windows),
    allow(clippy::needless_pass_by_ref_mut, unused_variables)
)]
pub fn hide_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

/// Runs a tool to completion, feeding each stdout line to `on_line`. Kills
/// the tool (and anything it spawned, on Unix) when the job is cancelled.
/// A non-zero exit becomes `EngineFailed` with the tail of stderr.
///
/// Once the tool exits, anything it left running in its process group gets
/// [`DRAIN_GRACE`] to finish writing and is then killed, so a stray background
/// process holding the pipes open cannot hang the job.
pub(crate) fn run_tool(
    engine: &'static str,
    cmd: Command,
    ctx: &Ctx,
    on_line: impl FnMut(&str),
) -> Result<()> {
    run_tool_attempt(engine, cmd, ctx, on_line, 0)
}

fn run_tool_attempt(
    engine: &'static str,
    mut cmd: Command,
    ctx: &Ctx,
    mut on_line: impl FnMut(&str),
    restart: u8,
) -> Result<()> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    if !CLOUD_SUPERVISED.load(std::sync::atomic::Ordering::SeqCst) {
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    }
    hide_console(&mut cmd);
    let mut child = cmd.spawn()?;
    let mut stderr = child.stderr.take().expect("piped");
    let (err_tx, err_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 8192];
        while let Ok(n) = stderr.read(&mut chunk) {
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.len() > 65536 {
                buf.drain(..buf.len() - 65536);
            }
        }
        let _ = err_tx.send(buf);
    });
    let stdout = child.stdout.take().expect("piped");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout)
            .lines()
            .map_while(std::io::Result::ok)
        {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let cancelled = |child: &mut Child| {
        kill_tree(child);
        let _ = child.wait();
        Err(Error::Cancelled)
    };
    // Until the tool exits.
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => on_line(&line),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
        }
        if ctx.is_cancelled() {
            return cancelled(&mut child);
        }
        if has_exited(&mut child)? {
            break;
        }
    }
    // Then until its output closes, or the grace period runs out.
    let deadline = Instant::now() + DRAIN_GRACE;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left.min(Duration::from_millis(100))) {
            Ok(line) => on_line(&line),
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) if left.is_zero() => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if ctx.is_cancelled() {
            return cancelled(&mut child);
        }
    }
    // The tool is a zombie until `wait`, so its process group id cannot be
    // reused and this only reaches leftovers it spawned.
    kill_tree(&mut child);
    let status = child.wait()?;
    let stderr = err_rx.recv_timeout(DRAIN_GRACE).unwrap_or_default();
    if engine == "libreoffice"
        && status.code() == Some(81)
        && restart == 0
        && CLOUD_SUPERVISED.load(std::sync::atomic::Ordering::SeqCst)
    {
        // Native LibreOffice requests one restart after creating its private
        // profile. Its desktop launcher handles this; cloud avoids that proc-
        // dependent launcher and keeps the restart in the confined job group.
        return run_tool_attempt(engine, cmd, ctx, on_line, 1);
    }
    if status.success() {
        Ok(())
    } else {
        Err(Error::EngineFailed {
            engine,
            message: convt_core::stderr_tail(&stderr),
        })
    }
}

const DRAIN_GRACE: Duration = Duration::from_secs(1);

/// Whether the child has exited, without reaping it on Unix.
#[cfg(unix)]
fn has_exited(child: &mut Child) -> Result<bool> {
    loop {
        // SAFETY: waitid writes only into `info`, a zeroed siginfo_t we own.
        // WNOWAIT leaves the child waitable, so `Child::wait` still reaps it.
        let (r, pid) = unsafe {
            let mut info: libc::siginfo_t = std::mem::zeroed();
            let r = libc::waitid(
                libc::P_PID,
                child.id() as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            );
            (r, info.si_pid())
        };
        if r == 0 {
            return Ok(pid != 0);
        }
        let err = std::io::Error::last_os_error();
        if err.kind() != std::io::ErrorKind::Interrupted {
            return Err(err.into());
        }
    }
}

#[cfg(not(unix))]
fn has_exited(child: &mut Child) -> Result<bool> {
    Ok(child.try_wait()?.is_some())
}

fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    // SAFETY: kill(2) with a negative pid signals the process group that
    // `process_group(0)` created for this child; it touches no memory.
    if !CLOUD_SUPERVISED.load(std::sync::atomic::Ordering::SeqCst) {
        unsafe {
            libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
        }
    }
    // TODO(windows): put the child in a job object so LibreOffice's
    // soffice.bin dies with it.
    let _ = child.kill();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use convt_core::{Cancel, Options, Step, format_by_id};

    fn sh(script: &str, cancel: &Cancel) -> (Result<()>, Vec<String>, Duration) {
        let options = Options::default();
        let step = Step {
            from: format_by_id("png").unwrap(),
            to: format_by_id("jpeg").unwrap(),
        };
        let ctx = Ctx::new(step, &options, &|_| {}, cancel);
        let mut cmd = Command::new("sh");
        cmd.args(["-c", script]);
        let mut lines = Vec::new();
        let start = Instant::now();
        let r = run_tool("test", cmd, &ctx, |l| lines.push(l.to_string()));
        (r, lines, start.elapsed())
    }

    #[test]
    fn a_background_process_holding_the_pipes_does_not_hang() {
        let (r, lines, took) = sh("sleep 30 & echo done", &Cancel::new());
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(lines, ["done"]);
        assert!(took < Duration::from_secs(5), "{took:?}");
    }

    #[test]
    fn failures_carry_stderr() {
        let (r, _, _) = sh("echo broken >&2; exit 3", &Cancel::new());
        match r {
            Err(Error::EngineFailed { message, .. }) => assert!(message.contains("broken")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cancel_stops_a_running_tool() {
        let cancel = Cancel::new();
        let c = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            c.cancel();
        });
        let (r, _, took) = sh("sleep 30", &cancel);
        assert!(matches!(r, Err(Error::Cancelled)), "{r:?}");
        assert!(took < Duration::from_secs(5), "{took:?}");
    }
}

#[cfg(test)]
mod hide_console_tests {
    use super::*;

    #[test]
    fn hide_console_is_safe_to_call() {
        hide_console(&mut Command::new("true"));
    }

    #[cfg(windows)]
    #[test]
    fn create_no_window_matches_the_win32_flag() {
        assert_eq!(CREATE_NO_WINDOW, 0x0800_0000);
    }
}
