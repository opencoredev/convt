//! Downloads the installer a verified manifest names, into convt's own
//! folder, and checks it against the manifest's size and SHA-256 before
//! anything uses it. The signed manifest is the trust root: the server and
//! the URL only carry bytes, so a redirect or a swapped file fails the check.
//!
//! Each attempt writes to its own `.part` file, created fresh, which is
//! renamed only once both checks pass, so an attempt that was stopped but
//! is still reading can never write into another attempt's file. A file
//! already downloaded for the same version is checked again and reused, and
//! the installer checks it once more right before it installs ([`check`]).
//! A download that receives nothing for [`IDLE`] fails instead of hanging.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use convt_update::Artifact;
use sha2::{Digest, Sha256};

use crate::account::VERSION;

/// Why the download didn't produce a verified file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Offline,
    /// Nothing arrived for [`IDLE`].
    Stalled,
    Status(u16),
    /// The server sent more or fewer bytes than the manifest says.
    Size,
    Checksum,
    Disk(String),
    Cancelled,
}

impl Error {
    /// A short line for the sidebar and Settings.
    pub fn plain(&self) -> String {
        match self {
            Error::Offline => "The download stopped. Check your internet connection.".into(),
            Error::Stalled => {
                "The download stalled. Check your internet connection and try again.".into()
            }
            Error::Status(s) => format!("The download server answered with HTTP {s}."),
            Error::Size | Error::Checksum => {
                "The download didn't match the signed list of releases, so it was deleted.".into()
            }
            Error::Disk(e) => format!("The download couldn't be saved: {e}"),
            Error::Cancelled => "The download was stopped.".into(),
        }
    }
}

/// Opens an artifact URL for reading. Tests script their own.
pub trait Source: Send + Sync {
    fn open(&self, url: &str) -> Result<Box<dyn Read + Send>, Error>;
}

/// [`Source`] over HTTPS only, with the update check's user agent.
pub struct Http {
    agent: ureq::Agent,
}

impl Http {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            // GitHub release downloads redirect twice; leave some room.
            .max_redirects(5)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            // An installer is tens of megabytes; allow a slow line.
            .timeout_global(Some(Duration::from_secs(60 * 60)))
            .user_agent(format!("convt/{VERSION}"))
            .build()
            .new_agent();
        Self { agent }
    }
}

impl Source for Http {
    fn open(&self, url: &str) -> Result<Box<dyn Read + Send>, Error> {
        let response = self.agent.get(url).call().map_err(|e| {
            tracing::debug!(error = %e, "update download request failed");
            Error::Offline
        })?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(Error::Status(status));
        }
        // The size check below stops a longer stream; ureq's own limit
        // would only get in the way.
        Ok(Box::new(
            response
                .into_body()
                .into_with_config()
                .limit(u64::MAX)
                .reader(),
        ))
    }
}

/// The file name the download gets: the URL's last segment when it is a
/// plain name, else `convt.<kind>`.
fn file_name(artifact: &Artifact) -> String {
    let last = url::Url::parse(&artifact.url).ok().and_then(|u| {
        u.path_segments()
            .and_then(|mut s| s.next_back().map(str::to_string))
    });
    match last {
        Some(name)
            if !name.is_empty()
                && !name.starts_with('.')
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) =>
        {
            name
        }
        _ => format!("convt.{}", artifact.kind),
    }
}

/// Where the verified file for `version` goes inside `dir`.
pub fn destination(dir: &Path, version: &str, artifact: &Artifact) -> PathBuf {
    dir.join(version).join(file_name(artifact))
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn disk(e: io::Error) -> Error {
    Error::Disk(e.to_string())
}

/// Whether `path` already holds exactly this artifact.
fn verified(path: &Path, artifact: &Artifact) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == artifact.size)
        && sha256_file(path).is_ok_and(|h| h == artifact.sha256)
}

/// Checks `path` against the artifact's size and SHA-256 once more, right
/// before it is installed, and deletes it if it changed since the download.
pub fn check(path: &Path, artifact: &Artifact) -> Result<(), Error> {
    if verified(path, artifact) {
        return Ok(());
    }
    let _ = fs::remove_file(path);
    Err(Error::Checksum)
}

/// How long a download may receive nothing before it fails.
pub const IDLE: Duration = Duration::from_secs(60);

/// Deletes what launches before this one left in `dir`: the downloads of
/// `running` and every older version, and unfinished `.part` files. Runs at
/// launch, before any download starts.
pub fn prune(dir: &Path, running: &str) {
    let Ok(running) = semver::Version::parse(running) else {
        return;
    };
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let old = name
            .to_str()
            .and_then(|n| semver::Version::parse(n).ok())
            .is_some_and(|v| v <= running);
        if old && path.is_dir() {
            if let Err(e) = fs::remove_dir_all(&path) {
                tracing::warn!(error = %e, path = %path.display(), "couldn't delete an old update");
            }
        } else if path.is_dir() {
            for part in fs::read_dir(&path).into_iter().flatten().flatten() {
                if part.path().extension().is_some_and(|e| e == "part") {
                    let _ = fs::remove_file(part.path());
                }
            }
        }
    }
}

/// Creates a `.part` file next to `path` that no other attempt uses.
fn create_part(path: &Path) -> Result<(PathBuf, File), Error> {
    static ATTEMPT: AtomicU64 = AtomicU64::new(0);
    create_part_counting(path, &ATTEMPT)
}

fn part_name(path: &Path, n: u64) -> PathBuf {
    let name = path.file_name().expect("a file name").to_string_lossy();
    path.with_file_name(format!("{name}.{}-{n}.part", std::process::id()))
}

fn create_part_counting(path: &Path, attempt: &AtomicU64) -> Result<(PathBuf, File), Error> {
    loop {
        let part = part_name(path, attempt.fetch_add(1, Ordering::Relaxed));
        match OpenOptions::new().write(true).create_new(true).open(&part) {
            Ok(file) => return Ok((part, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(disk(e)),
        }
    }
}

/// Downloads `artifact` for `version` into `dir` and returns the verified
/// file. Other versions' downloads in `dir` are deleted first. `progress`
/// gets the bytes so far; `cancelled` is polled between reads and while
/// waiting for them.
pub fn fetch(
    source: &dyn Source,
    artifact: &Artifact,
    version: &str,
    dir: &Path,
    progress: &dyn Fn(u64),
    cancelled: &dyn Fn() -> bool,
) -> Result<PathBuf, Error> {
    fetch_within(source, artifact, version, dir, progress, cancelled, IDLE)
}

fn fetch_within(
    source: &dyn Source,
    artifact: &Artifact,
    version: &str,
    dir: &Path,
    progress: &dyn Fn(u64),
    cancelled: &dyn Fn() -> bool,
    idle: Duration,
) -> Result<PathBuf, Error> {
    let path = destination(dir, version, artifact);
    if verified(&path, artifact) {
        progress(artifact.size);
        return Ok(path);
    }
    // One update at a time: drop older downloads.
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_name() != version {
                let p = entry.path();
                let _ = if p.is_dir() {
                    fs::remove_dir_all(&p)
                } else {
                    fs::remove_file(&p)
                };
            }
        }
    }
    let parent = path.parent().expect("a version folder");
    fs::create_dir_all(parent).map_err(disk)?;
    let reader = source.open(&artifact.url)?;
    let (part, file) = create_part(&path)?;
    let result = stream(reader, file, artifact, progress, cancelled, idle)
        // A stopped attempt never replaces the file a newer one may own.
        .and_then(|()| match cancelled() {
            true => Err(Error::Cancelled),
            false => fs::rename(&part, &path).map_err(disk),
        });
    match result {
        Ok(()) => Ok(path),
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

/// How often a download waiting for bytes looks at `cancelled`.
const POLL: Duration = Duration::from_millis(100);

/// Reads `reader` on its own thread, so a read that blocks can't hold up a
/// cancel or the idle limit. Only this function writes to `file`.
fn stream(
    mut reader: Box<dyn Read + Send>,
    mut file: File,
    artifact: &Artifact,
    progress: &dyn Fn(u64),
    cancelled: &dyn Fn() -> bool,
    idle: Duration,
) -> Result<(), Error> {
    let (tx, rx) = mpsc::sync_channel::<io::Result<Vec<u8>>>(4);
    std::thread::Builder::new()
        .name("convt-update-read".into())
        .spawn(move || {
            let mut buf = vec![0; 1 << 16];
            loop {
                let chunk = match reader.read(&mut buf) {
                    Ok(0) => return,
                    Ok(n) => Ok(buf[..n].to_vec()),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => Err(e),
                };
                let failed = chunk.is_err();
                if tx.send(chunk).is_err() || failed {
                    return;
                }
            }
        })
        .map_err(disk)?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut last = Instant::now();
    loop {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let chunk = match rx.recv_timeout(POLL.min(idle)) {
            Ok(Ok(chunk)) => chunk,
            Ok(Err(e)) => {
                tracing::debug!(error = %e, "update download broke off");
                return Err(Error::Offline);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) if last.elapsed() >= idle => {
                return Err(Error::Stalled);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
        };
        last = Instant::now();
        total += chunk.len() as u64;
        // Never write a byte past the signed size.
        if total > artifact.size {
            return Err(Error::Size);
        }
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(disk)?;
        progress(total);
    }
    if total != artifact.size {
        return Err(Error::Size);
    }
    if hex(&hasher.finalize()) != artifact.sha256 {
        return Err(Error::Checksum);
    }
    file.sync_all().map_err(disk)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Serves fixed bytes for any URL and counts the requests.
    struct Fake {
        body: Mutex<Result<Vec<u8>, Error>>,
        opened: AtomicUsize,
    }

    impl Fake {
        fn new(body: Result<Vec<u8>, Error>) -> Self {
            Self {
                body: Mutex::new(body),
                opened: AtomicUsize::new(0),
            }
        }
    }

    impl Source for Fake {
        fn open(&self, _: &str) -> Result<Box<dyn Read + Send>, Error> {
            self.opened.fetch_add(1, Ordering::SeqCst);
            let body = self.body.lock().unwrap().clone()?;
            Ok(Box::new(io::Cursor::new(body)))
        }
    }

    fn artifact(bytes: &[u8]) -> Artifact {
        Artifact {
            platform: "linux-x86_64".into(),
            kind: "AppImage".into(),
            url: "https://downloads.convt.test/v9/convt-linux-x86_64.AppImage".into(),
            size: bytes.len() as u64,
            sha256: hex(&Sha256::digest(bytes)),
        }
    }

    fn run(source: &Fake, a: &Artifact, dir: &Path) -> Result<PathBuf, Error> {
        fetch(source, a, "9.2.0", dir, &|_| {}, &|| false)
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        walk(dir)
            .into_iter()
            // Joined with `/` on every platform, so Windows compares the same.
            .map(|p| {
                let rel = p.strip_prefix(dir).unwrap();
                let parts: Vec<_> = rel.iter().map(|c| c.to_string_lossy()).collect();
                parts.join("/")
            })
            .collect()
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for e in fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
        out
    }

    #[test]
    fn a_matching_download_is_kept_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let body = b"a new convt".to_vec();
        let a = artifact(&body);
        let source = Fake::new(Ok(body.clone()));
        let seen = Mutex::new(Vec::new());
        let path = fetch(
            &source,
            &a,
            "9.2.0",
            dir.path(),
            &|n| seen.lock().unwrap().push(n),
            &|| false,
        )
        .unwrap();
        assert_eq!(path, dir.path().join("9.2.0/convt-linux-x86_64.AppImage"));
        assert_eq!(fs::read(&path).unwrap(), body);
        assert_eq!(seen.lock().unwrap().last(), Some(&(body.len() as u64)));
        assert_eq!(leftovers(dir.path()), ["9.2.0/convt-linux-x86_64.AppImage"]);
        // The same version again: checked, not fetched.
        assert_eq!(run(&source, &a, dir.path()).unwrap(), path);
        assert_eq!(source.opened.load(Ordering::SeqCst), 1);
        // A file changed on disk is fetched again.
        fs::write(&path, b"a new convX").unwrap();
        assert_eq!(run(&source, &a, dir.path()).unwrap(), path);
        assert_eq!(source.opened.load(Ordering::SeqCst), 2);
        assert_eq!(fs::read(&path).unwrap(), body);
    }

    #[test]
    fn wrong_bytes_leave_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let body = b"a new convt".to_vec();
        let a = artifact(&body);
        type Case = (&'static str, Result<Vec<u8>, Error>, Error);
        let cases: Vec<Case> = vec![
            ("short", Ok(body[..5].to_vec()), Error::Size),
            ("long", Ok([&body[..], b"!"].concat()), Error::Size),
            ("much too long", Ok(vec![b'x'; 1 << 20]), Error::Size),
            (
                "same size, other bytes",
                Ok(b"a new convX".to_vec()),
                Error::Checksum,
            ),
            ("offline", Err(Error::Offline), Error::Offline),
            ("not found", Err(Error::Status(404)), Error::Status(404)),
        ];
        for (what, body, expected) in cases {
            let source = Fake::new(body);
            assert_eq!(run(&source, &a, dir.path()), Err(expected), "{what}");
            assert!(leftovers(dir.path()).is_empty(), "{what}");
        }
    }

    #[test]
    fn an_oversized_stream_stops_at_the_signed_size() {
        // A server that never ends: the download stops right after the
        // signed size instead of filling the disk.
        struct Endless;
        impl Read for Endless {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                buf.fill(b'x');
                Ok(buf.len())
            }
        }
        struct EndlessSource;
        impl Source for EndlessSource {
            fn open(&self, _: &str) -> Result<Box<dyn Read + Send>, Error> {
                Ok(Box::new(Endless))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let a = artifact(&[b'x'; 100_000]);
        let most = Mutex::new(0);
        let result = fetch(
            &EndlessSource,
            &a,
            "9.2.0",
            dir.path(),
            &|n| *most.lock().unwrap() = n,
            &|| false,
        );
        assert_eq!(result, Err(Error::Size));
        assert!(*most.lock().unwrap() <= a.size);
        assert!(leftovers(dir.path()).is_empty());
    }

    #[test]
    fn a_new_version_replaces_older_downloads_and_cancel_stops() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("9.1.0")).unwrap();
        fs::write(dir.path().join("9.1.0/convt.AppImage"), b"old").unwrap();
        let body = b"a new convt".to_vec();
        let a = artifact(&body);
        let source = Fake::new(Ok(body));
        assert_eq!(
            fetch(&source, &a, "9.2.0", dir.path(), &|_| {}, &|| true),
            Err(Error::Cancelled)
        );
        assert!(leftovers(dir.path()).is_empty());
        run(&source, &a, dir.path()).unwrap();
        assert_eq!(leftovers(dir.path()), ["9.2.0/convt-linux-x86_64.AppImage"]);
    }

    /// Reads nothing until `release` is set, then serves `body`.
    struct Blocked {
        body: io::Cursor<Vec<u8>>,
        release: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl Read for Blocked {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            while !self.release.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
            self.body.read(buf)
        }
    }

    struct BlockedSource(Vec<u8>, std::sync::Arc<std::sync::atomic::AtomicBool>);

    impl Source for BlockedSource {
        fn open(&self, _: &str) -> Result<Box<dyn Read + Send>, Error> {
            Ok(Box::new(Blocked {
                body: io::Cursor::new(self.0.clone()),
                release: self.1.clone(),
            }))
        }
    }

    #[test]
    fn a_stalled_download_fails_and_a_cancel_does_not_wait_for_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let body = b"a new convt".to_vec();
        let a = artifact(&body);
        let release = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let source = BlockedSource(body.clone(), release.clone());
        let started = Instant::now();
        let result = fetch_within(
            &source,
            &a,
            "9.2.0",
            dir.path(),
            &|_| {},
            &|| false,
            Duration::from_millis(200),
        );
        assert_eq!(result, Err(Error::Stalled));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(leftovers(dir.path()).is_empty());

        // A cancel stops a download whose read is blocked.
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let started = Instant::now();
        let result = std::thread::scope(|s| {
            s.spawn(|| {
                std::thread::sleep(Duration::from_millis(50));
                cancel.store(true, Ordering::SeqCst);
            });
            fetch(&source, &a, "9.2.0", dir.path(), &|_| {}, &|| {
                cancel.load(Ordering::SeqCst)
            })
        });
        assert_eq!(result, Err(Error::Cancelled));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(leftovers(dir.path()).is_empty());
        release.store(true, Ordering::SeqCst);
    }

    #[test]
    fn each_attempt_writes_its_own_part_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("convt.AppImage");
        let (a, _fa) = create_part(&path).unwrap();
        let (b, _fb) = create_part(&path).unwrap();
        assert_ne!(a, b);
        // A file that already has the next name is skipped, never opened.
        let counter = AtomicU64::new(0);
        let taken = part_name(&path, 0);
        fs::write(&taken, b"stale").unwrap();
        let (c, _fc) = create_part_counting(&path, &counter).unwrap();
        assert_eq!(c, part_name(&path, 1));
        assert_eq!(fs::read(&taken).unwrap(), b"stale");
        for p in [&a, &b, &c] {
            assert!(p.to_string_lossy().ends_with(".part"), "{}", p.display());
            assert_eq!(p.parent(), Some(dir.path()));
        }
    }

    #[test]
    fn a_file_changed_after_the_download_is_refused_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let body = b"a new convt".to_vec();
        let a = artifact(&body);
        let path = run(&Fake::new(Ok(body)), &a, dir.path()).unwrap();
        assert_eq!(check(&path, &a), Ok(()));
        fs::write(&path, b"a new convX").unwrap();
        assert_eq!(check(&path, &a), Err(Error::Checksum));
        assert!(!path.exists());
        assert_eq!(check(&path, &a), Err(Error::Checksum));
    }

    #[test]
    fn launch_prunes_this_and_older_versions_and_part_files() {
        let dir = tempfile::tempdir().unwrap();
        for (folder, file) in [
            ("0.1.0", "convt.AppImage"),
            ("0.2.0", "convt.AppImage"),
            ("0.3.0", "convt.AppImage"),
            ("0.3.0", "convt.AppImage.1-0.part"),
            ("notes", "keep.txt"),
        ] {
            fs::create_dir_all(dir.path().join(folder)).unwrap();
            fs::write(dir.path().join(folder).join(file), b"x").unwrap();
        }
        prune(dir.path(), "0.2.0");
        assert_eq!(
            {
                let mut v = leftovers(dir.path());
                v.sort();
                v
            },
            ["0.3.0/convt.AppImage", "notes/keep.txt"]
        );
        // A missing folder is fine.
        prune(&dir.path().join("missing"), "0.2.0");
    }

    #[test]
    fn odd_url_names_fall_back_to_a_plain_one() {
        let mut a = artifact(b"x");
        for (url, name) in [
            (
                "https://h.test/a/convt-0.3.0-windows-x86_64.msi",
                "convt-0.3.0-windows-x86_64.msi",
            ),
            ("https://h.test/a/", "convt.AppImage"),
            ("https://h.test/a/..", "convt.AppImage"),
            ("https://h.test/a/%2e%2e%2fx", "convt.AppImage"),
            ("https://h.test/a/b%20c", "convt.AppImage"),
        ] {
            a.url = url.into();
            assert_eq!(file_name(&a), name, "{url}");
        }
    }
}
