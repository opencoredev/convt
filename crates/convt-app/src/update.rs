//! The update check: at most once a UTC day at launch, and from "Check now"
//! in Settings, while update checks are on (the default). It downloads the
//! signed manifest, verifies it with `convt_update` against the key this
//! build trusts, refuses anything older than the highest manifest sequence it
//! accepted before, and picks the newest build this machine's license covers.
//!
//! A covered update downloads in the background, while update checks are on,
//! when this install can replace itself ([`install::supported`]: the disk
//! image on macOS, the MSI on Windows, an AppImage on Linux). The download
//! must match the manifest's size and SHA-256 ([`download`]); then the app
//! offers "Restart to update", which installs it and starts the new version
//! ([`install`]). Other installs (deb, rpm, the tarball, an app convt can't
//! replace) open the download page instead. A newer build the license
//! doesn't cover offers the purchase page. A failed check (offline, a bad
//! signature, a rollback) is silent except for a line in Settings.

pub mod download;
pub mod install;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use convt_license::client::{State, today};
use convt_license::date;
use convt_update::{Artifact, Error as ManifestError, MAX_MANIFEST_BYTES};
use ed25519_dalek::VerifyingKey;
use futures::StreamExt as _;
use gpui_kit::{Context, Task};

use crate::account::{VERSION, background};
use crate::model::AppState;

/// Why the manifest couldn't be fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    Offline,
    Status(u16),
    TooLarge,
}

/// Downloads the manifest. Tests script their own.
pub trait Fetch: Send + Sync {
    fn fetch(&self) -> Result<Vec<u8>, FetchError>;
}

/// [`Fetch`] over HTTPS. Plain HTTP only to a loopback host, which only a
/// build from source can be pointed at.
pub struct Http {
    url: String,
    agent: ureq::Agent,
}

impl Http {
    pub fn new(url: &str) -> Self {
        let local = loopback_http(url);
        let agent = ureq::Agent::config_builder()
            .https_only(!local)
            // convt.app redirects to GitHub, which redirects twice more.
            .max_redirects(if local { 0 } else { 5 })
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(format!("convt/{VERSION}"))
            .build()
            .new_agent();
        Self {
            url: url.to_string(),
            agent,
        }
    }
}

impl Fetch for Http {
    fn fetch(&self) -> Result<Vec<u8>, FetchError> {
        let mut response = self.agent.get(&self.url).call().map_err(|e| {
            tracing::debug!(error = %e, "update check request failed");
            FetchError::Offline
        })?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(FetchError::Status(status));
        }
        let limit = MAX_MANIFEST_BYTES as u64 + 1;
        let bytes = response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|_| FetchError::TooLarge)?;
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(FetchError::TooLarge);
        }
        Ok(bytes)
    }
}

/// Whether `url` is plain HTTP to this machine, with no user info that could
/// hide another host (`http://localhost:80@example.com/`).
fn loopback_http(url: &str) -> bool {
    let Ok(uri) = url.parse::<ureq::http::Uri>() else {
        return false;
    };
    uri.scheme_str() == Some("http")
        && uri.authority().is_some_and(|a| !a.as_str().contains('@'))
        && matches!(
            uri.host(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        )
}

/// How this build checks for updates.
pub struct UpdateConfig {
    /// The key manifests must be signed with. `None` in a build from source
    /// without `CONVT_UPDATE_PUBKEY`: it has nothing to check against.
    pub key: Option<VerifyingKey>,
    pub fetch: Arc<dyn Fetch>,
    /// The release this install would update to: platform and artifact kind.
    pub target: (&'static str, &'static str),
    /// How this install downloads and installs updates itself. `None` when
    /// it can't, and a covered update opens the download page.
    pub install: Option<SelfInstall>,
}

/// What an install that updates itself needs.
pub struct SelfInstall {
    pub source: Arc<dyn download::Source>,
    /// Where verified downloads go, one folder per version.
    pub dir: PathBuf,
    pub installer: Arc<dyn install::Installer>,
}

impl UpdateConfig {
    pub fn from_env() -> Self {
        // Builds from source may point at a local server and key; packaged
        // builds use only what they embed.
        let source_build = !convt_license::ENFORCED;
        let env = |name| std::env::var(name).ok().filter(|_| source_build);
        let url = env("CONVT_UPDATE_URL")
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| crate::placeholder::UPDATE_MANIFEST_URL.to_string());
        let key = env("CONVT_UPDATE_PUBKEY")
            .and_then(|k| convt_license::parse_public_key(&k))
            .or_else(convt_update::public_key);
        let target = install_target();
        let install = match (
            install::supported(target.1),
            convt_engines::paths::data_dir(),
        ) {
            (Ok(()), Some(data)) => Some(SelfInstall {
                source: Arc::new(download::Http::new()),
                dir: data.join("updates"),
                installer: Arc::new(install::System),
            }),
            (Err(why), _) => {
                tracing::info!(%why, "updates open the download page");
                None
            }
            (_, None) => None,
        };
        Self {
            key,
            fetch: Arc::new(Http::new(url.trim())),
            target,
            install,
        }
    }
}

/// The platform and artifact kind of this install, as the manifest names them.
fn install_target() -> (&'static str, &'static str) {
    if cfg!(target_os = "macos") {
        ("macos-arm64", "dmg")
    } else if cfg!(windows) {
        ("windows-x86_64", "msi")
    } else if std::env::var_os("APPIMAGE").is_some() {
        ("linux-x86_64", "AppImage")
    } else if std::env::current_exe().is_ok_and(|e| e.starts_with("/opt/convt")) {
        if std::path::Path::new("/var/lib/dpkg/info/convt.list").exists() {
            ("linux-x86_64", "deb")
        } else {
            ("linux-x86_64", "rpm")
        }
    } else {
        ("linux-x86_64", "tar.gz")
    }
}

/// What the last update check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// Not checked this session.
    Idle,
    Checking,
    UpToDate,
    /// A newer build the license covers. `uncovered` is a still newer one
    /// it doesn't.
    Available {
        version: String,
        date: String,
        uncovered: Option<String>,
    },
    /// Downloading the covered build in the background.
    Downloading {
        version: String,
        percent: u8,
    },
    /// Downloaded and verified; "Restart to update" installs it.
    Ready {
        version: String,
        path: PathBuf,
    },
    /// Installing after "Restart to update"; the app quits when it's done.
    Installing {
        version: String,
    },
    /// The download or the install failed. The download page still works.
    InstallFailed {
        version: String,
        why: String,
    },
    /// Only builds the license doesn't cover are newer.
    NotCovered {
        version: String,
        date: String,
        purchase_url: String,
    },
    /// Offline, refused or not verifiable. Shown only in Settings.
    Failed(String),
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Fetches and verifies the manifest off the UI thread. Returns its bytes and
/// sequence, or the quiet note to show in Settings.
fn fetch_verified(
    fetch: &dyn Fetch,
    key: &VerifyingKey,
    minimum_sequence: u64,
    now: u64,
) -> Result<(Vec<u8>, u64), String> {
    let bytes = match fetch.fetch() {
        Ok(bytes) => bytes,
        Err(FetchError::Offline) => return Err("convt.app couldn't be reached.".into()),
        Err(FetchError::Status(s)) => return Err(format!("convt.app answered with HTTP {s}.")),
        Err(FetchError::TooLarge) => return Err("The list of releases was too large.".into()),
    };
    let verified = convt_update::verify(&bytes, key, now, minimum_sequence).map_err(|e| {
        tracing::warn!(error = %e, "refused an update manifest");
        refusal(&e).to_string()
    })?;
    let sequence = verified.manifest().sequence;
    Ok((bytes, sequence))
}

fn refusal(e: &ManifestError) -> &'static str {
    match e {
        ManifestError::BadSignature | ManifestError::Malformed => {
            "The list of releases didn't check out, so it was ignored."
        }
        ManifestError::Rollback => {
            "The list of releases was older than one seen before, so it was ignored."
        }
        ManifestError::Stale => {
            "The list of releases was out of date, or this computer's clock is off."
        }
        ManifestError::NotDistributable => "No release is published yet.",
    }
}

impl Update {
    /// The covered version being downloaded, ready or installed.
    fn in_flight(&self) -> Option<&str> {
        match self {
            Update::Downloading { version, .. }
            | Update::Ready { version, .. }
            | Update::Installing { version }
            | Update::InstallFailed { version, .. } => Some(version),
            _ => None,
        }
    }
}

/// The self-update in progress: the artifact the last selection picked, and
/// the download running for it.
#[derive(Default)]
pub struct Updater {
    artifact: Option<Artifact>,
    cancel: Option<Arc<AtomicBool>>,
    task: Option<Task<()>>,
    /// Why "Restart to update" waited, such as running conversions.
    pub notice: Option<String>,
}

impl Updater {
    fn stop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.task = None;
        self.notice = None;
    }
}

enum Event {
    Progress(u8),
    Done(Result<PathBuf, download::Error>),
}

/// What a verified manifest offers this install with this license, and the
/// covered build's artifact for this install.
fn select(
    bytes: &[u8],
    key: &VerifyingKey,
    minimum_sequence: u64,
    build_date: &str,
    updates_until: &str,
    target: (&str, &str),
    now: u64,
) -> (Update, Option<Artifact>) {
    let verified = match convt_update::verify(bytes, key, now, minimum_sequence) {
        Ok(v) => v,
        Err(e) => return (Update::Failed(refusal(&e).into()), None),
    };
    match verified.select(VERSION, build_date, updates_until, target.0, target.1) {
        Err(_) => (
            Update::Failed("This build's version couldn't be compared.".into()),
            None,
        ),
        Ok(s) => (
            match (s.covered, s.uncovered) {
                (Some(b), uncovered) => Update::Available {
                    version: b.version.clone(),
                    date: b.build_date.clone(),
                    uncovered: uncovered.map(|u| u.version.clone()),
                },
                (None, Some(b)) => Update::NotCovered {
                    version: b.version.clone(),
                    date: b.build_date.clone(),
                    purchase_url: s.purchase_url.to_string(),
                },
                (None, None) => Update::UpToDate,
            },
            s.covered_artifact.cloned(),
        ),
    }
}

/// The last day of updates this machine's license covers. Without a license
/// (a trial, or a build that checks none) every build is fair game.
fn updates_until(state: &State) -> String {
    match state {
        State::Licensed(l) | State::NotCovered(l) => l.updates_until.clone(),
        _ => "9999-12-31".into(),
    }
}

impl AppState {
    /// The launch check: once a UTC day, only while update checks are on.
    pub fn check_updates_on_launch(&mut self, cx: &mut Context<Self>) {
        let today = date::from_days(today());
        if self.settings.update_checks
            && self.settings.update_checked.as_deref() != Some(today.as_str())
        {
            self.check_updates(cx);
        }
    }

    /// Checks now. Does nothing while update checks are off or a check runs.
    /// The day is recorded before the request and the accepted sequence
    /// before the result shows; if either can't be saved, nothing is accepted,
    /// so a restart can neither repeat the day's request nor replay an older
    /// manifest.
    pub fn check_updates(&mut self, cx: &mut Context<Self>) {
        if !self.settings.update_checks
            || matches!(
                self.update,
                Update::Checking | Update::Downloading { .. } | Update::Installing { .. }
            )
        {
            return;
        }
        let Some(key) = self.update_config.key else {
            self.update = Update::Failed("This build has no key to check updates with.".into());
            cx.notify();
            return;
        };
        let today = date::from_days(today());
        if let Err(e) = self.save_settings_now(|s| s.update_checked = Some(today), cx) {
            self.update = Update::Failed(format!("Settings couldn't be saved: {e}"));
            cx.notify();
            return;
        }
        self.update = Update::Checking;
        let fetch = self.update_config.fetch.clone();
        let minimum = self.settings.update_sequence;
        self._update_task = Some(background(
            cx,
            move || fetch_verified(&*fetch, &key, minimum, now_unix()),
            |state, result, cx| {
                state.update = match result {
                    Err(note) => Update::Failed(note),
                    Ok((bytes, sequence)) => {
                        let seq = sequence.max(state.settings.update_sequence);
                        match state.save_settings_now(|s| s.update_sequence = seq, cx) {
                            Err(e) => Update::Failed(format!(
                                "The list of releases was ignored because settings couldn't be saved: {e}"
                            )),
                            Ok(()) => {
                                state.update_manifest = Some(Arc::new(bytes));
                                state.reselect_update();
                                state.download_update(cx);
                                state.update.clone()
                            }
                        }
                    }
                };
                cx.notify();
            },
        ));
        cx.notify();
    }

    /// Picks from the last accepted manifest again for the current license,
    /// after a check or when activation, removal or renewal changed it.
    pub fn reselect_update(&mut self) {
        let (Some(bytes), Some(key)) = (&self.update_manifest, self.update_config.key) else {
            return;
        };
        // Past the point of no return: the install finishes and convt quits.
        if matches!(self.update, Update::Installing { .. }) {
            return;
        }
        let (update, artifact) = select(
            bytes,
            &key,
            self.settings.update_sequence,
            self.licensing.build_date(),
            &updates_until(&self.license),
            self.update_config.target,
            now_unix(),
        );
        // A license change that still covers the version being downloaded
        // or ready leaves it alone.
        if let Update::Available { version, .. } = &update
            && self.update.in_flight() == Some(version.as_str())
            && self.updater.artifact == artifact
        {
            return;
        }
        self.updater.stop();
        self.updater.artifact = artifact;
        self.update = update;
    }

    /// Downloads the covered update in the background, if this install
    /// updates itself and update checks are on. Also "Try again" after a
    /// failure. A verified download of the same version is reused.
    pub fn download_update(&mut self, cx: &mut Context<Self>) {
        let version = match &self.update {
            Update::Available { version, .. } | Update::InstallFailed { version, .. } => {
                version.clone()
            }
            _ => return,
        };
        let (true, Some(install), Some(artifact)) = (
            self.settings.update_checks,
            &self.update_config.install,
            self.updater.artifact.clone(),
        ) else {
            return;
        };
        self.updater.stop();
        let cancel = Arc::new(AtomicBool::new(false));
        self.updater.cancel = Some(cancel.clone());
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let source = install.source.clone();
        let dir = install.dir.clone();
        let thread_version = version.clone();
        std::thread::Builder::new()
            .name("convt-update".into())
            .spawn(move || {
                let last = std::cell::Cell::new(0u8);
                let size = artifact.size.max(1);
                let result = download::fetch(
                    &*source,
                    &artifact,
                    &thread_version,
                    &dir,
                    &|bytes| {
                        let percent = (bytes.saturating_mul(100) / size).min(100) as u8;
                        if percent != last.replace(percent) {
                            let _ = tx.unbounded_send(Event::Progress(percent));
                        }
                    },
                    &|| cancel.load(Ordering::SeqCst),
                );
                let _ = tx.unbounded_send(Event::Done(result));
            })
            .expect("spawn the update thread");
        self.updater.task = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                let Ok(()) = this.update(cx, |state, cx| state.download_event(event, cx)) else {
                    break;
                };
            }
        }));
        self.update = Update::Downloading {
            version,
            percent: 0,
        };
        cx.notify();
    }

    fn download_event(&mut self, event: Event, cx: &mut Context<Self>) {
        let Update::Downloading { version, percent } = &mut self.update else {
            return;
        };
        match event {
            Event::Progress(p) => *percent = p,
            Event::Done(Ok(path)) => {
                self.update = Update::Ready {
                    version: version.clone(),
                    path,
                };
            }
            Event::Done(Err(download::Error::Cancelled)) => return,
            Event::Done(Err(e)) => {
                tracing::warn!(error = ?e, "update download failed");
                self.update = Update::InstallFailed {
                    version: version.clone(),
                    why: e.plain(),
                };
            }
        }
        cx.notify();
    }

    /// "Restart to update": installs the ready update off the UI thread,
    /// then quits so the helper can start the new version. Waits while
    /// conversions run, since quitting would stop them.
    pub fn restart_to_update(&mut self, cx: &mut Context<Self>) {
        let Update::Ready { version, path } = self.update.clone() else {
            return;
        };
        let Some(installer) = self
            .update_config
            .install
            .as_ref()
            .map(|i| i.installer.clone())
        else {
            return;
        };
        if self.queue.active() > 0 {
            self.updater.notice =
                Some("Wait for the conversions to finish; restarting would stop them.".into());
            cx.notify();
            return;
        }
        self.updater.notice = None;
        self.update = Update::Installing {
            version: version.clone(),
        };
        self.updater.task = Some(background(
            cx,
            move || installer.install(&path),
            move |state, result, cx| {
                match result {
                    Ok(()) => cx.quit(),
                    Err(why) => {
                        tracing::warn!(%why, "update install failed");
                        state.update = Update::InstallFailed { version, why };
                    }
                }
                cx.notify();
            },
        ));
        cx.notify();
    }

    /// Turns update checks on or off. Turning them on checks right away.
    pub fn set_update_checks(&mut self, on: bool, cx: &mut Context<Self>) {
        self.update_settings(|s| s.update_checks = on, cx);
        if on {
            self.check_updates(cx);
        } else {
            self._update_task = None;
            self.updater.stop();
            self.update = Update::Idle;
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::loopback_http;

    /// The real thing, on demand: fetches the live manifest through the
    /// app's own transport, verifies it, and downloads and checks the newest
    /// Linux AppImage. Needs the network and the production update key,
    /// embedded or as `CONVT_LIVE_UPDATE_PUBKEY`:
    /// `cargo test -p convt-app live_appimage -- --ignored --nocapture`
    #[test]
    #[ignore = "downloads the real release from GitHub"]
    fn live_appimage_downloads_and_verifies() {
        use super::{Fetch, Http, download};
        let key = convt_update::public_key()
            .or_else(|| {
                std::env::var("CONVT_LIVE_UPDATE_PUBKEY")
                    .ok()
                    .and_then(|k| convt_license::parse_public_key(&k))
            })
            .expect("an update public key");
        let bytes = Http::new(crate::placeholder::UPDATE_MANIFEST_URL)
            .fetch()
            .expect("the live manifest");
        let verified = convt_update::verify(&bytes, &key, super::now_unix(), 0).unwrap();
        let pick = verified
            .select(
                "0.1.0",
                "2026-01-01",
                "9999-12-31",
                "linux-x86_64",
                "AppImage",
            )
            .unwrap();
        let (build, artifact) = (pick.covered.unwrap(), pick.covered_artifact.unwrap());
        eprintln!("{} {} {} bytes", build.version, artifact.url, artifact.size);
        let dir = tempfile::tempdir().unwrap();
        let path = download::fetch(
            &download::Http::new(),
            artifact,
            &build.version,
            dir.path(),
            &|_| {},
            &|| false,
        )
        .unwrap();
        let data = std::fs::read(&path).unwrap();
        assert_eq!(data.len() as u64, artifact.size);
        use sha2::Digest as _;
        let sha: String = sha2::Sha256::digest(&data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(sha, artifact.sha256);
        assert!(data.starts_with(b"\x7fELF"));
        eprintln!("verified {} sha256 {sha}", path.display());
    }

    #[test]
    fn plain_http_only_to_this_machine() {
        for ok in [
            "http://127.0.0.1:8000/manifest.json",
            "http://localhost:8000/m",
            "http://[::1]:8000/m",
        ] {
            assert!(loopback_http(ok), "{ok}");
        }
        for bad in [
            "http://localhost:80@updates.example.com/manifest.json",
            "http://user@127.0.0.1:8000/m",
            "http://localhost.example.com/m",
            "http://127.0.0.1.example.com/m",
            "https://127.0.0.1:8000/m",
            "http://example.com/m",
            "not a url",
        ] {
            assert!(!loopback_http(bad), "{bad}");
        }
    }
}
