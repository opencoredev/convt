//! Persistent app logs and the diagnostics bundle "Copy logs" puts on the
//! clipboard.
//!
//! The rotating file lives in the same folder as PR #82's crash files
//! ([`crate::crash_report::logs_dir`]). The last few hundred lines are also
//! kept in memory. [`init`] writes them through the existing `tracing`
//! subscriber and still prints to stderr.
//!
//! Paths, credentials and personal data are scrubbed before a line is stored
//! or written to disk. The clipboard bundle uses those already-clean lines.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use convt_license::client::State;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

use crate::account::VERSION;
use crate::crash_report;

/// Lines kept in memory for the diagnostics bundle.
pub const MEMORY_LINES: usize = 200;
/// Rotate `convt.log` once it grows past this.
const ROTATE_BYTES: u64 = 2 * 1024 * 1024;

static STORE: OnceLock<Arc<LogStore>> = OnceLock::new();

pub fn log_file(dir: &Path) -> PathBuf {
    dir.join("convt.log")
}

/// Starts file + memory logging alongside stderr. Safe to call once.
pub fn init() {
    let store = LogStore::open(crash_report::logs_dir().as_deref());
    let _ = STORE.set(store.clone());
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_writer(LogWriter::new(store)),
        )
        .try_init();
}

pub fn recent_lines() -> Vec<String> {
    STORE.get().map(|s| s.recent()).unwrap_or_default()
}

/// One rotating file plus a ring of recent lines.
pub struct LogStore {
    lines: Mutex<VecDeque<String>>,
    file: Mutex<Option<File>>,
    path: Option<PathBuf>,
    rotate_bytes: u64,
}

impl LogStore {
    pub fn open(dir: Option<&Path>) -> Arc<Self> {
        Self::open_with(dir, ROTATE_BYTES)
    }

    pub fn open_with(dir: Option<&Path>, rotate_bytes: u64) -> Arc<Self> {
        let path = dir.map(log_file);
        let file = path.as_ref().and_then(|p| open_log(p).ok());
        Arc::new(Self {
            lines: Mutex::new(VecDeque::new()),
            file: Mutex::new(file),
            path,
            rotate_bytes,
        })
    }

    pub fn recent(&self) -> Vec<String> {
        self.lines.lock().unwrap().iter().cloned().collect()
    }

    pub fn push(&self, line: impl Into<String>) {
        let line = line.into();
        if line.is_empty() {
            return;
        }
        let line = sanitize(&line);
        if line.is_empty() {
            return;
        }
        {
            let mut lines = self.lines.lock().unwrap();
            if lines.len() == MEMORY_LINES {
                lines.pop_front();
            }
            lines.push_back(line.clone());
        }
        if let Some(path) = &self.path {
            let mut file = self.file.lock().unwrap();
            if write_rotated(&mut file, path, &line, self.rotate_bytes).is_err() {
                *file = open_log(path).ok();
            }
        }
    }
}

fn open_log(path: &Path) -> io::Result<File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

fn rotated_file(path: &Path) -> PathBuf {
    path.with_extension("log.1")
}

fn write_rotated(
    file: &mut Option<File>,
    path: &Path,
    line: &str,
    rotate_bytes: u64,
) -> io::Result<()> {
    let needs_rotate = match file.as_ref() {
        Some(handle) => handle.metadata()?.len() >= rotate_bytes,
        None => return Ok(()),
    };
    if needs_rotate {
        if let Some(handle) = file.as_mut() {
            handle.flush()?;
        }
        // Close first: Windows cannot rename a file that still has an open handle.
        *file = None;
        let rotated = rotated_file(path);
        let _ = std::fs::remove_file(&rotated);
        std::fs::rename(path, &rotated)?;
        *file = Some(open_log(path)?);
    }
    let handle = file
        .as_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "log file closed"))?;
    writeln!(handle, "{line}")?;
    handle.flush()
}

/// Paths via [`crash_report::scrub`], then emails, tokens and license keys.
fn sanitize(line: &str) -> String {
    redact_secrets(&crash_report::scrub(line))
}

fn redact_secrets(input: &str) -> String {
    let mut redact_next = false;
    input
        .split_whitespace()
        .map(|word| {
            if redact_next {
                redact_next = false;
                return "<redacted>".into();
            }
            if word.eq_ignore_ascii_case("Bearer")
                || word.eq_ignore_ascii_case("Token")
                || word.eq_ignore_ascii_case("Basic")
            {
                redact_next = true;
                return word.to_string();
            }
            let trimmed = word.trim_matches(|c: char| matches!(c, ',' | ';' | ')' | ']'));
            let lower = trimmed.to_ascii_lowercase();
            if trimmed.contains('@')
                && trimmed
                    .rsplit_once('@')
                    .is_some_and(|(_, domain)| domain.contains('.'))
            {
                word.replacen(trimmed, "<email>", 1)
            } else if lower.starts_with("token=")
                || lower.starts_with("api_key=")
                || lower.starts_with("x-api-key=")
                || lower.starts_with("access_token=")
                || lower.starts_with("refresh_token=")
                || lower.starts_with("license_key=")
                || lower.starts_with("secret=")
                || lower.starts_with("authorization=")
            {
                "<credential>=<redacted>".into()
            } else if lower.starts_with("cvt_")
                || lower.starts_with("convt_")
                || (trimmed.len() >= 19
                    && trimmed.matches('-').count() >= 3
                    && trimmed
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-'))
            {
                "<license-key>".into()
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Newtype so we can implement `MakeWriter` (orphan rules block `Arc<LogStore>`).
#[derive(Clone)]
pub struct LogWriter(Arc<LogStore>);

impl LogWriter {
    pub fn new(store: Arc<LogStore>) -> Self {
        Self(store)
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogWriter {
    type Writer = StoreWriter;

    fn make_writer(&'a self) -> Self::Writer {
        StoreWriter {
            store: self.0.clone(),
        }
    }
}

pub struct StoreWriter {
    store: Arc<LogStore>,
}

impl Write for StoreWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let line = String::from_utf8_lossy(buf)
            .trim_end_matches(['\n', '\r'])
            .to_string();
        self.store.push(line);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.store.file.lock().unwrap().as_mut() {
            file.flush()?;
        }
        Ok(())
    }
}

/// Fields the clipboard bundle is built from. Tests pass these directly.
#[derive(Debug, Clone)]
pub struct BundleInput<'a> {
    pub version: &'a str,
    pub os: &'a str,
    pub os_version: &'a str,
    pub arch: &'a str,
    pub license: &'a str,
    pub lines: &'a [String],
}

/// App version, OS, arch, license state (no keys) and recent log lines.
/// Lines from the store are already sanitized; this still sanitizes so
/// tests can pass raw lines.
pub fn format_bundle(input: &BundleInput<'_>) -> String {
    let mut out = String::new();
    out.push_str(&format!("convt-app {}\n", input.version));
    out.push_str(&format!("os: {}\n", input.os));
    out.push_str(&format!("os_version: {}\n", input.os_version));
    out.push_str(&format!("arch: {}\n", input.arch));
    out.push_str(&format!("license: {}\n", input.license));
    out.push_str("\n--- logs ---\n");
    if input.lines.is_empty() {
        out.push_str("(no log lines yet)\n");
    } else {
        for line in input.lines {
            out.push_str(&sanitize(line));
            out.push('\n');
        }
    }
    out
}

pub fn current_bundle(license: &str) -> String {
    format_bundle(&BundleInput {
        version: VERSION,
        os: std::env::consts::OS,
        os_version: &os_version(),
        arch: std::env::consts::ARCH,
        license,
        lines: &recent_lines(),
    })
}

/// License state for the bundle: no email, no key.
pub fn license_label(state: &State) -> String {
    match state {
        State::Unrestricted => "unrestricted".into(),
        State::Trial { .. } => "trial".into(),
        State::TrialEnded => "trial_ended".into(),
        State::Licensed(l) => format!("licensed ({})", l.plan.name().to_lowercase()),
        State::NotCovered(l) => format!("not_covered ({})", l.plan.name().to_lowercase()),
    }
}

fn os_version() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(text) = std::fs::read_to_string("/etc/os-release") {
            for line in text.lines() {
                if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
                    return value.trim().trim_matches('"').to_string();
                }
            }
        }
        "linux".into()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| format!("macOS {}", s.trim()))
            .filter(|s| s.len() > "macOS ".len())
            .unwrap_or_else(|| "macOS".into())
    }
    #[cfg(target_os = "windows")]
    {
        "Windows".into()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        std::env::consts::OS.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use convt_license::{License, Plan};

    #[test]
    fn bundle_lists_version_os_arch_license_and_scrubbed_logs() {
        let lines = vec!["opened /Users/someone/secret.png".into(), "ready".into()];
        let text = format_bundle(&BundleInput {
            version: "0.2.0",
            os: "linux",
            os_version: "Ubuntu 24.04",
            arch: "x86_64",
            license: "trial",
            lines: &lines,
        });
        assert!(text.starts_with("convt-app 0.2.0\n"), "{text}");
        assert!(text.contains("os: linux\n"), "{text}");
        assert!(text.contains("os_version: Ubuntu 24.04\n"), "{text}");
        assert!(text.contains("arch: x86_64\n"), "{text}");
        assert!(text.contains("license: trial\n"), "{text}");
        assert!(text.contains("--- logs ---"), "{text}");
        assert!(text.contains("<PATH>") || text.contains("<HOME>"), "{text}");
        assert!(!text.contains("someone"), "{text}");
        assert!(!text.contains("secret"), "{text}");
        assert!(text.contains("ready"), "{text}");
    }

    #[test]
    fn empty_logs_are_noted() {
        let text = format_bundle(&BundleInput {
            version: "1.0.0",
            os: "macos",
            os_version: "macOS 15.0",
            arch: "aarch64",
            license: "unrestricted",
            lines: &[],
        });
        assert!(text.contains("(no log lines yet)"), "{text}");
        assert!(text.contains("license: unrestricted\n"), "{text}");
    }

    #[test]
    fn license_label_never_includes_an_address_or_key() {
        let addr = format!("{}@{}", "someone", "convt.test");
        let license = License {
            id: "lic".into(),
            email: addr.clone(),
            plan: Plan::Desktop,
            issued: "2026-01-01".into(),
            updates_until: "2027-01-01".into(),
        };
        let cases: [(State, &str); 5] = [
            (State::Unrestricted, "unrestricted"),
            (
                State::Trial {
                    days_left: 3,
                    started: Some("2026-10-01".into()),
                },
                "trial",
            ),
            (State::TrialEnded, "trial_ended"),
            (State::Licensed(license.clone()), "licensed (desktop)"),
            (State::NotCovered(license), "not_covered (desktop)"),
        ];
        for (state, want) in cases {
            let label = license_label(&state);
            assert_eq!(label, want);
            assert!(!label.contains(&addr), "{label}");
            assert!(!label.contains('@'), "{label}");
        }
    }

    #[test]
    fn memory_ring_keeps_the_last_n_lines() {
        let dir = tempfile::tempdir().unwrap();
        let store = LogStore::open(Some(dir.path()));
        for i in 0..(MEMORY_LINES + 5) {
            store.push(format!("line {i}"));
        }
        let recent = store.recent();
        assert_eq!(recent.len(), MEMORY_LINES);
        assert_eq!(recent[0], format!("line {}", 5));
        assert_eq!(
            recent.last().unwrap(),
            &format!("line {}", MEMORY_LINES + 4)
        );
        let file = std::fs::read_to_string(log_file(dir.path())).unwrap();
        assert!(file.contains("line 0"));
        assert!(file.contains(&format!("line {}", MEMORY_LINES + 4)));
    }

    #[test]
    fn persist_scrubs_paths_and_secrets_before_store_or_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = LogStore::open(Some(dir.path()));
        let addr = format!("{}@{}", "someone", "convt.test");
        let secret = format!("{}={}", "token", "api-secret");
        store.push(format!(
            "opened /Users/someone/holiday.png: {addr} {secret} Bearer hunter2"
        ));
        let recent = store.recent().join("\n");
        let file = std::fs::read_to_string(log_file(dir.path())).unwrap();
        for text in [&recent, &file] {
            assert!(!text.contains("someone"), "{text}");
            assert!(!text.contains("holiday"), "{text}");
            assert!(!text.contains("api-secret"), "{text}");
            assert!(!text.contains("hunter2"), "{text}");
            assert!(!text.contains(&addr), "{text}");
            assert!(
                text.contains("<PATH>") || text.contains("<HOME>") || text.contains("<email>"),
                "{text}"
            );
            assert!(
                text.contains("<credential>=<redacted>") || text.contains("<redacted>"),
                "{text}"
            );
        }
    }

    #[test]
    fn rotation_closes_the_handle_then_renames() {
        let dir = tempfile::tempdir().unwrap();
        let store = LogStore::open_with(Some(dir.path()), 40);
        store.push("aaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        store.push("bbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        store.push("cccccccc");
        let current = log_file(dir.path());
        let rotated = rotated_file(&current);
        assert!(rotated.is_file(), "expected {}", rotated.display());
        let old = std::fs::read_to_string(&rotated).unwrap();
        let new = std::fs::read_to_string(&current).unwrap();
        assert!(old.contains('a') || old.contains('b'), "{old}");
        assert!(new.contains('c') || new.contains('b'), "{new}");
        assert_ne!(old, new);
    }
}
