//! Downloads the installer a verified manifest names, into convt's own
//! folder, and checks it against the manifest's size and SHA-256 before
//! anything uses it. The signed manifest is the trust root: the server and
//! the URL only carry bytes, so a redirect or a swapped file fails the check.
//!
//! The bytes go to a `.part` file that is renamed only once both checks
//! pass. A file already downloaded for the same version is checked again and
//! reused.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use convt_update::Artifact;
use sha2::{Digest, Sha256};

use crate::account::VERSION;

/// Why the download didn't produce a verified file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Offline,
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

/// Downloads `artifact` for `version` into `dir` and returns the verified
/// file. Other versions' downloads in `dir` are deleted first. `progress`
/// gets the bytes so far; `cancelled` is polled between reads.
pub fn fetch(
    source: &dyn Source,
    artifact: &Artifact,
    version: &str,
    dir: &Path,
    progress: &dyn Fn(u64),
    cancelled: &dyn Fn() -> bool,
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
    let part = parent.join(format!(
        "{}.part",
        path.file_name().unwrap().to_string_lossy()
    ));
    let result = stream(source, artifact, &part, progress, cancelled);
    match result {
        Ok(()) => {
            fs::rename(&part, &path).map_err(disk)?;
            Ok(path)
        }
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

fn stream(
    source: &dyn Source,
    artifact: &Artifact,
    part: &Path,
    progress: &dyn Fn(u64),
    cancelled: &dyn Fn() -> bool,
) -> Result<(), Error> {
    let mut reader = source.open(&artifact.url)?;
    let mut file = File::create(part).map_err(disk)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 1 << 16];
    let mut total = 0u64;
    loop {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                tracing::debug!(error = %e, "update download broke off");
                return Err(Error::Offline);
            }
        };
        total += n as u64;
        // Never write a byte past the signed size.
        if total > artifact.size {
            return Err(Error::Size);
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(disk)?;
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
            .map(|p| p.strip_prefix(dir).unwrap().display().to_string())
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
