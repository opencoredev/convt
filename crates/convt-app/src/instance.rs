//! One app per user. The first launch holds a lock and listens on a local
//! socket; later launches send their request there and exit.
//!
//! The socket lives in a directory only this user can open. The protocol is
//! one JSON [`Request`] per connection, answered with `ok`.

use std::path::PathBuf;

#[cfg(not(unix))]
use crate::request::Request;

/// Requests larger than this are dropped. A request is a list of paths.
#[cfg(unix)]
const MAX_REQUEST: u64 = 4 << 20;

pub enum Role {
    /// This process is the app. Call [`Primary::listen`] once the UI can
    /// take requests.
    Primary(Primary),
    /// The running app took the request. Only the Unix socket path forwards today.
    #[cfg_attr(not(unix), allow(dead_code))]
    Forwarded,
}

/// `$CONVT_RUNTIME_DIR`, else `$XDG_RUNTIME_DIR/convt`, else a per-user
/// folder in the temp directory.
pub fn runtime_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("CONVT_RUNTIME_DIR") {
        return dir.into();
    }
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir).join("convt");
    }
    #[cfg(unix)]
    let name = format!("convt-{}", unsafe { libc::getuid() });
    #[cfg(not(unix))]
    let name = "convt".to_string();
    std::env::temp_dir().join(name)
}

#[cfg(unix)]
pub use unix::{Primary, claim};

#[cfg(unix)]
mod unix {
    use std::fs::{DirBuilder, File, OpenOptions};
    use std::io::{self, BufRead, BufReader, Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use super::{MAX_REQUEST, Role};
    use crate::request::Request;

    /// How long a second launch waits for a starting app to bind its socket.
    const CONNECT_WAIT: Duration = Duration::from_secs(5);

    pub struct Primary {
        listener: UnixListener,
        socket: PathBuf,
        /// Held for the life of the process; releasing it lets a new app start.
        _lock: File,
    }

    impl Primary {
        /// Accepts requests on a background thread and hands each to `on_request`.
        pub fn listen(self, on_request: impl Fn(Request) + Send + 'static) {
            std::thread::Builder::new()
                .name("convt-instance".into())
                .spawn(move || {
                    let Primary {
                        listener,
                        socket,
                        _lock,
                    } = self;
                    for stream in listener.incoming() {
                        match stream.and_then(read_request) {
                            Ok(req) => on_request(req),
                            Err(e) => tracing::warn!(error = %e, socket = %socket.display(), "bad request"),
                        }
                    }
                })
                .expect("spawn instance listener");
        }
    }

    fn read_request(stream: UnixStream) -> io::Result<Request> {
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut line = Vec::new();
        BufReader::new((&stream).take(MAX_REQUEST)).read_until(b'\n', &mut line)?;
        let req = serde_json::from_slice(&line).map_err(io::Error::other)?;
        (&stream).write_all(b"ok\n")?;
        Ok(req)
    }

    fn send(stream: UnixStream, req: &Request) -> io::Result<()> {
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let mut line = serde_json::to_vec(req).map_err(io::Error::other)?;
        line.push(b'\n');
        (&stream).write_all(&line)?;
        let mut reply = String::new();
        BufReader::new(&stream).read_line(&mut reply)?;
        if reply.trim() == "ok" {
            Ok(())
        } else {
            Err(io::Error::other(
                "the running app did not accept the request",
            ))
        }
    }

    /// Creates `dir` for this user only, and refuses one that someone else
    /// owns or can open.
    fn private_dir(dir: &Path) -> io::Result<()> {
        DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        let meta = std::fs::metadata(dir)?;
        let uid = unsafe { libc::getuid() };
        if meta.uid() != uid || meta.mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "{} must belong to you and be private (mode 700)",
                    dir.display()
                ),
            ));
        }
        Ok(())
    }

    /// Becomes the app, or hands `req` to the one already running.
    pub fn claim(dir: &Path, req: &Request) -> io::Result<Role> {
        private_dir(dir)?;
        let socket = dir.join("app.sock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(dir.join("app.lock"))?;
        // SAFETY: flock on a file descriptor we own.
        let locked = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
        if locked {
            // Whoever held the lock before is gone; its socket is stale.
            match std::fs::remove_file(&socket) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
            let listener = UnixListener::bind(&socket)?;
            return Ok(Role::Primary(Primary {
                listener,
                socket,
                _lock: lock,
            }));
        }
        // Another app holds the lock. It may still be starting, so retry
        // until its socket answers.
        let start = Instant::now();
        loop {
            match UnixStream::connect(&socket) {
                Ok(stream) => {
                    send(stream, req)?;
                    return Ok(Role::Forwarded);
                }
                Err(e) if start.elapsed() > CONNECT_WAIT => {
                    return Err(io::Error::new(
                        e.kind(),
                        format!(
                            "convt is running but not answering at {}: {e}",
                            socket.display()
                        ),
                    ));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }
}

/// Without a Unix socket every launch opens its own app. Windows gets a
/// named pipe later.
#[cfg(not(unix))]
pub struct Primary;

#[cfg(not(unix))]
impl Primary {
    pub fn listen(self, _on_request: impl Fn(Request) + Send + 'static) {}
}

#[cfg(not(unix))]
pub fn claim(_dir: &std::path::Path, _req: &Request) -> std::io::Result<Role> {
    Ok(Role::Primary(Primary))
}

#[cfg(all(test, unix))]
mod tests {
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Duration;

    use crate::request::Request;

    use super::*;

    #[test]
    fn second_launch_forwards_to_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let rt = dir.path().join("rt");
        let Role::Primary(primary) = claim(&rt, &Request::default()).unwrap() else {
            panic!("first launch should be primary");
        };
        let (tx, rx) = mpsc::channel();
        primary.listen(move |r| tx.send(r).unwrap());
        let req = Request {
            files: vec![PathBuf::from("/tmp/a b \"c\".png")],
            to: Some("jpeg".into()),
            ..Request::default()
        };
        assert!(matches!(claim(&rt, &req).unwrap(), Role::Forwarded));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), req);
    }

    #[test]
    fn a_stale_socket_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let rt = dir.path().join("rt");
        {
            let Role::Primary(_gone) = claim(&rt, &Request::default()).unwrap() else {
                panic!()
            };
        }
        // The socket file is left behind, but the lock is free again.
        assert!(rt.join("app.sock").exists());
        assert!(matches!(
            claim(&rt, &Request::default()).unwrap(),
            Role::Primary(_)
        ));
    }

    #[test]
    fn refuses_a_shared_directory() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(claim(dir.path(), &Request::default()).is_err());
    }
}
