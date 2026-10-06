//! Video thumbnails: one frame grabbed with the FFmpeg the engines run,
//! found through the same tool discovery, on a worker thread and kept in
//! memory for the session. Rendering only reads the cache and asks for a
//! frame; every filesystem call (metadata, the mount check) and FFmpeg run
//! on the worker, so a dead network mount can't freeze a window. Files on
//! network mounts get no frame, so FFmpeg never reads a video over the
//! network; local disks and USB drives do. Until a frame is ready, or when
//! there is none, windows show the extension badge.
//!
//! The cache and the request queue are bounded, and the FFmpeg the worker
//! runs is killed when the app quits ([`shutdown`]), and on Linux also by
//! the kernel if the app dies.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui_kit::{App, Image, ImageFormat};

/// Thumbnails are drawn at most 64 px wide; twice that stays sharp on HiDPI.
const WIDTH: u32 = 160;
/// How long one frame may take before FFmpeg is stopped.
const TIMEOUT: Duration = Duration::from_secs(10);
/// Files remembered, waiting ones included.
const KEEP: usize = 300;
/// Requests waiting for the worker. More are dropped and asked for again
/// on a later redraw.
const QUEUE: usize = 16;
/// How often a shown file is checked for changes, on the worker.
const RECHECK: Duration = Duration::from_secs(30);

/// A file's size and modification time: a changed file gets a new frame.
type Stamp = (u64, Option<SystemTime>);

struct Entry {
    frame: Option<Arc<Image>>,
    /// What the worker last saw, `None` until it has looked.
    stamp: Option<Stamp>,
    /// Waiting in the queue or on the worker.
    queued: bool,
    checked: Instant,
}

/// The frames by path, and the queue to the worker.
#[derive(Default)]
struct Cache {
    entries: HashMap<PathBuf, Entry>,
    worker: Option<SyncSender<PathBuf>>,
    /// Wakes the app to redraw once a frame is ready.
    redraw: Option<UnboundedSender<()>>,
}

impl Cache {
    /// The frame for `path` if there is one, asking the worker for it (or
    /// to check it again) through `queue`. Touches no files.
    fn frame(&mut self, path: &Path, queue: &SyncSender<PathBuf>) -> Option<Arc<Image>> {
        if let Some(entry) = self.entries.get_mut(path) {
            if !entry.queued && entry.checked.elapsed() >= RECHECK {
                entry.queued = queue.try_send(path.to_path_buf()).is_ok();
            }
            return entry.frame.clone();
        }
        if self.entries.len() >= KEEP {
            // Forget settled files; waiting ones count against the limit.
            self.entries.retain(|_, e| e.queued);
            if self.entries.len() >= KEEP {
                return None;
            }
        }
        // A full queue drops the request; a later redraw asks again.
        if queue.try_send(path.to_path_buf()).is_ok() {
            self.entries.insert(
                path.to_path_buf(),
                Entry {
                    frame: None,
                    stamp: None,
                    queued: true,
                    checked: Instant::now(),
                },
            );
        }
        None
    }
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Mutex::default);
/// The FFmpeg processes running, so quitting can stop them.
static CHILDREN: LazyLock<Mutex<Vec<Child>>> = LazyLock::new(Mutex::default);
static QUIT: AtomicBool = AtomicBool::new(false);

/// Redraws the windows whenever a frame becomes ready, and stops FFmpeg
/// when the app quits.
pub fn init(cx: &mut App) {
    let (tx, mut rx) = unbounded();
    CACHE.lock().unwrap().redraw = Some(tx);
    cx.spawn(async move |cx| {
        while rx.next().await.is_some() {
            cx.update(|cx| cx.refresh_windows());
        }
    })
    .detach();
    cx.on_app_quit(|_| {
        shutdown();
        async {}
    })
    .detach();
}

/// Stops grabbing frames and kills any FFmpeg still running.
pub fn shutdown() {
    QUIT.store(true, Ordering::SeqCst);
    for mut child in std::mem::take(&mut *CHILDREN.lock().unwrap()) {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// The frame for a video, if it is ready. Called while rendering, so it
/// only reads the cache: the first call for a file queues the work.
pub fn video_frame(path: &Path) -> Option<Arc<Image>> {
    if QUIT.load(Ordering::Relaxed) {
        return None;
    }
    let mut cache = CACHE.lock().unwrap();
    let queue = cache.worker.get_or_insert_with(spawn_worker).clone();
    cache.frame(path, &queue)
}

/// One thread looks at files and grabs frames in order, so a folder of
/// videos doesn't start FFmpeg dozens of times at once. It lives as long as
/// the app, which the Linux death signal on its FFmpeg relies on.
fn spawn_worker() -> SyncSender<PathBuf> {
    let (tx, rx) = mpsc::sync_channel::<PathBuf>(QUEUE);
    std::thread::Builder::new()
        .name("convt-thumbnails".into())
        .spawn(move || {
            for path in rx {
                if QUIT.load(Ordering::Relaxed) {
                    return;
                }
                look_at(&path);
            }
        })
        .expect("spawn the thumbnail thread");
    tx
}

/// Updates the frame for `path` if the file changed since the last look.
fn look_at(path: &Path) {
    let known = match CACHE.lock().unwrap().entries.get(path) {
        // Forgotten since it was asked for: no longer worth the work.
        None => return,
        Some(entry) => entry.stamp,
    };
    // A file that isn't there, or isn't on this computer, gets the badge.
    let stamp = std::fs::metadata(path)
        .ok()
        .filter(|m| m.is_file() && is_local(path))
        .map_or((0, None), |m| (m.len(), m.modified().ok()));
    let changed = known != Some(stamp);
    let frame = (changed && stamp != (0, None))
        .then(|| grab(path))
        .flatten()
        .map(|png| Arc::new(Image::from_bytes(ImageFormat::Png, png)));
    let mut cache = CACHE.lock().unwrap();
    if let Some(entry) = cache.entries.get_mut(path) {
        if changed {
            entry.frame = frame;
            entry.stamp = Some(stamp);
        }
        entry.queued = false;
        entry.checked = Instant::now();
    }
    if changed && let Some(redraw) = &cache.redraw {
        let _ = redraw.unbounded_send(());
    }
}

fn ffmpeg() -> Option<&'static Path> {
    static FFMPEG: OnceLock<Option<PathBuf>> = OnceLock::new();
    FFMPEG
        .get_or_init(convt_engines::ffmpeg::ffmpeg_path)
        .as_deref()
}

/// A PNG of the frame a second in, past most fade-ins, or of the first
/// frame for clips shorter than that.
fn grab(path: &Path) -> Option<Vec<u8>> {
    let ffmpeg = ffmpeg()?;
    ["1", "0"]
        .into_iter()
        .find_map(|at| run(ffmpeg, path, at).filter(|png| !png.is_empty()))
}

fn run(ffmpeg: &Path, path: &Path, at: &str) -> Option<Vec<u8>> {
    if QUIT.load(Ordering::SeqCst) {
        return None;
    }
    let mut command = convt_engines::ffmpeg::thumbnail_command(ffmpeg, path, at, WIDTH);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    die_with_parent(&mut command);
    let mut child = command.spawn().ok()?;
    let pid = child.id();
    let mut stdout = child.stdout.take()?;
    {
        let mut children = CHILDREN.lock().unwrap();
        if QUIT.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        children.push(child);
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut png = Vec::new();
        let _ = tx.send(stdout.read_to_end(&mut png).map(|_| png));
    });
    let png = rx.recv_timeout(TIMEOUT);
    // Reaped only here or in `shutdown`, under the lock, so a pid is never
    // signalled after it could have been reused. Gone means quit killed it.
    let mut child = {
        let mut children = CHILDREN.lock().unwrap();
        let at = children.iter().position(|c| c.id() == pid)?;
        children.swap_remove(at)
    };
    if png.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().ok()?;
    match png {
        Ok(Ok(png)) if status.success() && png.starts_with(b"\x89PNG") => Some(png),
        _ => None,
    }
}

/// On Linux, the kernel kills FFmpeg if the thread that started it (the
/// thumbnail worker, which lives as long as the app) goes away, even when
/// the app is killed outright.
#[cfg(target_os = "linux")]
fn die_with_parent(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    let parent = std::process::id();
    // SAFETY: the closure runs between fork and exec and only makes
    // async-signal-safe system calls.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            // The app may have died before the death signal was set.
            if libc::getppid() as u32 != parent {
                return Err(std::io::Error::other("convt has quit"));
            }
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn die_with_parent(_: &mut Command) {}

/// FUSE file systems that reach over the network.
#[cfg(target_os = "linux")]
const REMOTE_FUSE: [&str; 10] = [
    "fuse.sshfs",
    "fuse.gvfsd-fuse",
    "fuse.rclone",
    "fuse.s3fs",
    "fuse.curlftpfs",
    "fuse.glusterfs",
    "fuse.davfs",
    "fuse.smbnetfs",
    "fuse.gcsfuse",
    "fuse.goofys",
];

/// Whether `path` is on this computer: a local disk or removable media such
/// as a USB drive, not a network share (NFS, SMB, sshfs, GVfs and the like).
/// Unknown counts as not local. Runs on the worker only.
#[cfg(target_os = "linux")]
pub fn is_local(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    const FUSE: u64 = 0x65735546;
    const REMOTE: [u64; 10] = [
        0x6969,     // NFS
        0x517B,     // SMB
        0xFF534D42, // CIFS
        0xFE534D42, // SMB2
        0x73757245, // Coda
        0x5346414F, // AFS
        0x00C36400, // Ceph
        0x01021997, // 9P
        0x564C,     // NCP
        0x6B414653, // kAFS
    ];
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `c_path` is NUL-terminated and statfs only writes into `stat`,
    // a zeroed struct we own.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c_path.as_ptr(), &mut stat) } != 0 {
        return false;
    }
    let kind = (stat.f_type as u64) & 0xFFFF_FFFF;
    if REMOTE.contains(&kind) {
        return false;
    }
    if kind != FUSE {
        return true;
    }
    // FUSE covers both USB drives (ntfs-3g, exFAT) and network mounts; the
    // mount table says which.
    let Ok(real) = std::fs::canonicalize(path) else {
        return false;
    };
    let table = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    mount_type(&table, &real).is_some_and(|t| !REMOTE_FUSE.contains(&t.as_str()))
}

/// The file system type of the mount `path` is on, from a mountinfo table.
#[cfg(target_os = "linux")]
fn mount_type(table: &str, path: &Path) -> Option<String> {
    // Mount points escape spaces and some other bytes as \ooo.
    fn unescape(field: &str) -> String {
        let bytes = field.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\'
                && let Some(code) = field
                    .get(i + 1..i + 4)
                    .and_then(|o| u8::from_str_radix(o, 8).ok())
            {
                out.push(code);
                i += 4;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }
    table
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(' ');
            let point = PathBuf::from(unescape(fields.nth(4)?));
            let kind = fields.skip_while(|f| *f != "-").nth(1)?;
            path.starts_with(&point)
                .then(|| (point.components().count(), kind.to_string()))
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, kind)| kind)
}

/// macOS marks local volumes, USB drives included. Not verified on a Mac.
#[cfg(target_os = "macos")]
pub fn is_local(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `path` is NUL-terminated and statfs only writes into `stat`,
    // a zeroed struct we own.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(path.as_ptr(), &mut stat) } != 0 {
        return false;
    }
    stat.f_flags & libc::MNT_LOCAL as u32 != 0
}

/// UNC paths and mapped network drives are remote; local and removable
/// drives are not. Not verified on Windows.
#[cfg(windows)]
pub fn is_local(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDriveTypeW(root: *const u16) -> u32;
    }
    const DRIVE_REMOTE: u32 = 4;
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return false;
    };
    match prefix.kind() {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
            let root: Vec<u16> = format!("{}:\\", letter as char)
                .encode_utf16()
                .chain([0])
                .collect();
            // SAFETY: `root` is a NUL-terminated UTF-16 string we own.
            unsafe { GetDriveTypeW(root.as_ptr()) != DRIVE_REMOTE }
        }
        _ => false,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub fn is_local(_: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Polls in real time; the worker is a real thread.
    fn wait_for_frame(path: &Path) -> Option<Arc<Image>> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(frame) = video_frame(path) {
                return Some(frame);
            }
            let settled = CACHE
                .lock()
                .unwrap()
                .entries
                .get(path)
                .is_some_and(|e| !e.queued && e.stamp.is_some());
            if settled || Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_video_gets_a_frame_that_is_not_black() {
        let Some(ffmpeg) = ffmpeg() else {
            eprintln!("skipped: no ffmpeg");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        // Two seconds that start black, like a fade-in.
        let clip = dir.path().join("fade in: clip.mp4");
        let made = Command::new(ffmpeg)
            .args(["-v", "error", "-f", "lavfi", "-i"])
            .arg("color=c=red:s=64x48:d=2,fade=in:st=0:d=0.5")
            .args(["-pix_fmt", "yuv420p"])
            .arg(&clip)
            .status()
            .unwrap();
        assert!(made.success());
        assert!(is_local(&clip));
        assert!(video_frame(&clip).is_none(), "the first call only queues");
        let frame = wait_for_frame(&clip).expect("a frame");
        let decoded = image::load_from_memory(&frame.bytes).unwrap().to_rgb8();
        assert_eq!(decoded.width(), WIDTH);
        let center = decoded.get_pixel(WIDTH / 2, decoded.height() / 2);
        assert!(center[0] > 150 && center[1] < 80, "{center:?}");
        // Cached: the same image comes back without another grab.
        assert!(Arc::ptr_eq(&frame, &video_frame(&clip).unwrap()));
    }

    #[test]
    fn missing_and_broken_videos_keep_the_badge() {
        let dir = tempfile::tempdir().unwrap();
        assert!(wait_for_frame(&dir.path().join("missing.mp4")).is_none());
        let broken = dir.path().join("broken.mp4");
        std::fs::write(&broken, b"not a video").unwrap();
        assert!(wait_for_frame(&broken).is_none());
    }

    #[test]
    fn the_cache_and_the_queue_stay_bounded() {
        // A worker that never catches up.
        let (queue, waiting) = mpsc::sync_channel(QUEUE);
        let mut cache = Cache::default();
        for i in 0..5 * KEEP {
            let path = PathBuf::from(format!("/v/{i}.mp4"));
            assert!(cache.frame(&path, &queue).is_none());
        }
        assert_eq!(cache.entries.len(), QUEUE, "only queued requests are kept");
        assert_eq!(waiting.try_iter().count(), QUEUE);

        // Settled entries make way for new ones; waiting ones still count.
        let (queue, _waiting) = mpsc::sync_channel(5 * KEEP);
        let mut cache = Cache::default();
        for i in 0..2 * KEEP {
            let path = PathBuf::from(format!("/v/{i}.mp4"));
            cache.frame(&path, &queue);
            if i % 2 == 0 {
                cache.entries.get_mut(&path).unwrap().queued = false;
            }
            assert!(cache.entries.len() <= KEEP);
        }
    }

    /// Rendering calls `video_frame`, so it must not touch the file system:
    /// a dead network mount would freeze the window.
    #[test]
    fn asking_for_a_frame_touches_no_files() {
        let source = include_str!("thumbs.rs");
        for (name, end) in [("pub fn video_frame", "\n}\n"), ("fn frame(", "\n    }\n")] {
            let body = &source[source.find(name).unwrap()..];
            let body = &body[..body.find(end).unwrap()];
            for call in [
                "fs::",
                "metadata",
                "is_local",
                "exists",
                "canonicalize",
                "grab",
            ] {
                assert!(!body.contains(call), "{name} calls {call}");
            }
        }
    }

    #[test]
    fn local_disks_are_local() {
        let dir = tempfile::tempdir().unwrap();
        assert!(is_local(dir.path()));
        assert!(!is_local(Path::new("/no/such/place")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn usb_drives_are_local_and_network_mounts_are_not() {
        let table = "\
22 1 8:2 / / rw - ext4 /dev/sda2 rw
40 22 8:17 / /media/leo/USB\\040STICK rw - fuseblk /dev/sdb1 rw
41 22 0:50 / /home/leo/remote rw - fuse.sshfs leo@nas:/ rw
42 22 0:51 / /run/user/1000/gvfs rw - fuse.gvfsd-fuse gvfsd-fuse rw";
        let kind = |p: &str| mount_type(table, Path::new(p));
        assert_eq!(
            kind("/media/leo/USB STICK/clip.mp4").as_deref(),
            Some("fuseblk")
        );
        assert_eq!(
            kind("/home/leo/remote/clip.mp4").as_deref(),
            Some("fuse.sshfs")
        );
        assert_eq!(kind("/home/leo/clip.mp4").as_deref(), Some("ext4"));
        for remote in ["fuse.sshfs", "fuse.gvfsd-fuse"] {
            assert!(REMOTE_FUSE.contains(&remote));
        }
        assert!(!REMOTE_FUSE.contains(&"fuseblk"));
    }

    /// FFmpeg reading a pipe nobody writes to never finishes on its own;
    /// quitting must still stop it.
    #[cfg(target_os = "linux")]
    #[test]
    fn quitting_kills_and_reaps_a_running_ffmpeg() {
        use std::os::unix::ffi::OsStrExt;
        let Some(ffmpeg) = ffmpeg() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("stuck.mp4");
        let c_fifo = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: a NUL-terminated path we own.
        assert_eq!(unsafe { libc::mkfifo(c_fifo.as_ptr(), 0o600) }, 0);
        let path = fifo.clone();
        let started = Instant::now();
        let grab = std::thread::spawn(move || run(ffmpeg, &path, "0"));
        // Wait for its FFmpeg, then stop it the way `shutdown` does. Other
        // tests' FFmpeg runs are left alone.
        let pid = loop {
            let found = CHILDREN.lock().unwrap().iter().map(Child::id).find(|pid| {
                std::fs::read(format!("/proc/{pid}/cmdline"))
                    .is_ok_and(|c| c.windows(9).any(|w| w == b"stuck.mp4"))
            });
            if let Some(pid) = found {
                break pid;
            }
            assert!(started.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        };
        {
            let mut children = CHILDREN.lock().unwrap();
            let at = children.iter().position(|c| c.id() == pid).unwrap();
            let mut child = children.swap_remove(at);
            child.kill().unwrap();
            child.wait().unwrap();
        }
        assert!(grab.join().unwrap().is_none());
        assert!(
            started.elapsed() < TIMEOUT,
            "stopped by the kill, not the timeout"
        );
        // Reaped: no zombie is left behind.
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
}
