//! The update check: at every launch and then every [`CHECK_INTERVAL`] while
//! the app runs, as long as automatic checks are on (the default), and
//! whenever the user clicks Check now or Check for Updates…. It downloads the
//! signed manifest, verifies it with `convt_update` against the key this
//! build trusts, refuses anything older than the highest manifest sequence it
//! accepted before, and picks the newest build this machine's license covers.
//!
//! Nothing is downloaded or installed: a covered update opens the download
//! page, and a newer build the license doesn't cover offers the purchase page.
//! A failure (offline, a bad signature, a rollback) is silent except for a
//! line in Settings.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use convt_license::client::State;
use convt_update::{Error as ManifestError, MAX_MANIFEST_BYTES};
use ed25519_dalek::VerifyingKey;
use gpui_kit::Context;

use crate::account::{VERSION, background};
use crate::model::AppState;

/// How long a running app waits between automatic checks.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60 * 60);

/// How often the schedule looks at the clock. Timers can stop while the
/// computer sleeps, so the schedule compares wall-clock times instead of
/// waiting out one long timer, and a check that came due during sleep runs
/// soon after waking.
pub const SCHEDULE_TICK: Duration = Duration::from_secs(15 * 60);

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
            .max_redirects(if local { 0 } else { 3 })
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
        Self {
            key,
            fetch: Arc::new(Http::new(url.trim())),
            target: install_target(),
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
    /// Only builds the license doesn't cover are newer.
    NotCovered {
        version: String,
        date: String,
        purchase_url: String,
    },
    /// Offline, refused or not verifiable. Shown only in Settings.
    Failed(String),
}

pub(crate) fn now_unix() -> u64 {
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

/// What a verified manifest offers this install with this license.
fn select(
    bytes: &[u8],
    key: &VerifyingKey,
    minimum_sequence: u64,
    build_date: &str,
    updates_until: &str,
    target: (&str, &str),
    now: u64,
) -> Update {
    let verified = match convt_update::verify(bytes, key, now, minimum_sequence) {
        Ok(v) => v,
        Err(e) => return Update::Failed(refusal(&e).into()),
    };
    match verified.select(VERSION, build_date, updates_until, target.0, target.1) {
        Err(_) => Update::Failed("This build's version couldn't be compared.".into()),
        Ok(s) => match (s.covered, s.uncovered) {
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
    /// Checks now if automatic checks are on, then keeps checking every
    /// [`CHECK_INTERVAL`] while the app runs. Call once, at launch.
    pub fn start_update_checks(&mut self, cx: &mut Context<Self>) {
        if self.settings.update_checks {
            self.check_updates(cx);
        }
        self._update_schedule = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SCHEDULE_TICK).await;
                if this.update(cx, |s, cx| s.check_updates_if_due(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    /// The scheduled check: only while automatic checks are on, and only
    /// once [`CHECK_INTERVAL`] has passed since the last check started.
    pub fn check_updates_if_due(&mut self, cx: &mut Context<Self>) {
        let due = self
            .update_attempted
            .is_none_or(|at| now_unix().saturating_sub(at) >= CHECK_INTERVAL.as_secs());
        if self.settings.update_checks && due {
            self.check_updates(cx);
        }
    }

    /// Checks now: the user asked, so this runs whether or not automatic
    /// checks are on. Does nothing while a check runs. The accepted sequence
    /// and the time are saved before the result shows; if they can't be,
    /// nothing is accepted, so a restart can't replay an older manifest.
    pub fn check_updates(&mut self, cx: &mut Context<Self>) {
        if self.update == Update::Checking {
            return;
        }
        self.update_attempted = Some(now_unix());
        let Some(key) = self.update_config.key else {
            self.update = Update::Failed("This build has no key to check updates with.".into());
            cx.notify();
            return;
        };
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
                        let saved = state.save_settings_now(
                            |s| {
                                s.update_sequence = seq;
                                s.update_checked_at = Some(now_unix());
                            },
                            cx,
                        );
                        match saved {
                            Err(e) => Update::Failed(format!(
                                "The list of releases was ignored because settings couldn't be saved: {e}"
                            )),
                            Ok(()) => {
                                state.update_manifest = Some(Arc::new(bytes));
                                state.reselect_update();
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
        self.update = select(
            bytes,
            &key,
            self.settings.update_sequence,
            self.licensing.build_date(),
            &updates_until(&self.license),
            self.update_config.target,
            now_unix(),
        );
    }

    /// Turns automatic checks on or off. Turning them on checks right away;
    /// turning them off keeps what the last check found.
    pub fn set_update_checks(&mut self, on: bool, cx: &mut Context<Self>) {
        self.update_settings(|s| s.update_checks = on, cx);
        if on {
            self.check_updates(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::loopback_http;

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
