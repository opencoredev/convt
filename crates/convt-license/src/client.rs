//! The license check every client shares: the desktop app, the CLI and the OS
//! menu integrations. Nothing here touches the network; renewal fetches keys
//! through [`crate::account`] and hands them to [`Licensing::offer_key`].
//!
//! Without a license, the first conversion starts a 7-day trial whose start
//! date is a file in the data directory. Keys live in the OS credential store
//! (Keychain, Windows Credential Manager, Secret Service), or in a file in the
//! config directory when no credential store runs. The convt.app sign-in of
//! a desktop app, when there is one, is kept the same way next to the key.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::VerifyingKey;

use crate::account::Session;
use crate::{License, Plan, date};

pub const TRIAL_DAYS: i64 = 7;
/// Where Buy sends people.
pub const BUY_URL: &str = "https://convt.app/pricing";
/// Where older builds can be downloaded.
pub const DOWNLOAD_URL: &str = "https://convt.app/download";

const KEYRING_SERVICE: &str = "convt";
/// What the key file holds after a removal the credential store didn't
/// confirm, so a key still in the store can't come back.
const REMOVED: &str = "removed";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// A build from source, which needs no license.
    Unrestricted,
    /// No license yet. `started` stays `None` until the first conversion.
    Trial {
        days_left: i64,
        started: Option<String>,
    },
    TrialEnded,
    /// Pro access granted by the signed-in account's online trial.
    AccountTrial {
        ends_on: String,
        days_left: i64,
    },
    /// Packaged install needs a signed-in account to start its online trial.
    SignInNeeded,
    Licensed(License),
    /// A valid license whose update window ended before this build was made.
    NotCovered(License),
}

impl State {
    pub fn allows_conversion(&self) -> bool {
        matches!(
            self,
            State::Unrestricted
                | State::Trial { .. }
                | State::AccountTrial { .. }
                | State::Licensed(_)
        )
    }

    /// One line on where this machine stands, for the user.
    pub fn summary(&self) -> String {
        match self {
            State::Unrestricted => "This build doesn't need a license.".into(),
            State::Trial { started: None, .. } => {
                format!("Free trial: {TRIAL_DAYS} days, starting with your first conversion.")
            }
            State::Trial { days_left: 1, .. } => "Free trial: last day.".into(),
            State::Trial { days_left, .. } => format!("Free trial: {days_left} days left."),
            State::AccountTrial { days_left: 1, .. } => "Pro trial: last day.".into(),
            State::AccountTrial { days_left, .. } => format!("Pro trial: {days_left} days left."),
            State::SignInNeeded => "Sign in to start your free trial.".into(),
            State::Licensed(l) if l.plan == Plan::Desktop => {
                format!("Licensed to {} (Desktop), with lifetime updates.", l.email)
            }
            State::Licensed(l) => format!(
                "Licensed to {} ({}), with updates until {}.",
                l.email, l.plan.name(), l.updates_until
            ),
            State::TrialEnded | State::NotCovered(_) => self.blocked_reason().unwrap_or_default(),
        }
    }

    /// Why conversions are stopped, for the user, or `None` if they aren't.
    pub fn blocked_reason(&self) -> Option<String> {
        match self {
            State::TrialEnded => Some(format!(
                "Your convt trial has ended. Buy a license at {BUY_URL} to keep converting."
            )),
            State::SignInNeeded => Some("Sign in to start your free trial.".into()),
            State::NotCovered(license) => Some(format!(
                "This build is newer than your license covers. Your license covers builds \
                 released up to {}; download one from {DOWNLOAD_URL}, or renew to use this one.",
                license.updates_until
            )),
            _ => None,
        }
    }
}

/// Why a conversion can't start.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Blocked {
    #[error("{}", .0.blocked_reason().unwrap_or_default())]
    State(State),
    /// The trial can't start without somewhere to record when it started.
    #[error("The free trial couldn't start because its start date couldn't be saved: {0}")]
    TrialNotRecorded(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ActivateError {
    #[error("That license key isn't valid. Check that you pasted all of it.")]
    Invalid,
    #[error("This build can't check license keys.")]
    NoPublicKey,
    #[error("The license couldn't be saved: {0}")]
    Store(String),
}

/// One secret in a [`KeyStore`]: its credential store entry, and its file
/// next to the license file.
#[derive(Debug, Clone, Copy)]
struct Slot {
    user: &'static str,
    /// `None` is the store's own file.
    file: Option<&'static str>,
}

const LICENSE: Slot = Slot {
    user: "license",
    file: None,
};
/// The device token from desktop sign-in, with the account's email.
const ACCOUNT: Slot = Slot {
    user: "account",
    file: Some("account.json"),
};

/// Where the license key is kept.
#[derive(Debug, Clone)]
pub enum KeyStore {
    /// The OS credential store, falling back to this file when there is none.
    Keyring { fallback: Option<PathBuf> },
    /// Only this file. Tests use it, and `CONVT_LICENSE_STORE=file` picks it.
    File(PathBuf),
    /// Nowhere: activation fails.
    None,
}

impl KeyStore {
    fn file(&self, slot: Slot) -> Option<PathBuf> {
        let own = match self {
            KeyStore::Keyring { fallback } => fallback.as_deref(),
            KeyStore::File(path) => Some(path.as_path()),
            KeyStore::None => None,
        }?;
        Some(match slot.file {
            Some(name) => own.with_file_name(name),
            None => own.to_path_buf(),
        })
    }

    /// The file is read first: it holds either the key saved last, while no
    /// credential store answered, or a removal marker.
    fn load(&self, slot: Slot) -> Option<String> {
        if let Some(file) = self.file(slot) {
            match std::fs::read_to_string(file) {
                Ok(text) if text.trim() == REMOVED => return None,
                Ok(text) if !text.trim().is_empty() => return Some(text.trim().to_string()),
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!(error = %e, slot = slot.user, "could not read the file"),
            }
        }
        if let KeyStore::Keyring { .. } = self {
            match keyring_entry(slot).and_then(|e| e.get_password()) {
                Ok(key) => return Some(key),
                Err(keyring::Error::NoEntry) => {}
                Err(e) => tracing::debug!(error = %e, "no credential store"),
            }
        }
        None
    }

    fn save(&self, slot: Slot, key: &str) -> Result<(), String> {
        if let KeyStore::Keyring { .. } = self {
            match keyring_entry(slot).and_then(|e| e.set_password(key)) {
                // The file would win over the credential store, so it has to go.
                Ok(()) => return self.remove_file(slot),
                Err(e) => tracing::debug!(error = %e, "no credential store, using the file"),
            }
        }
        let file = self.file(slot).ok_or("there is nowhere to store it")?;
        write_private(&file, key).map_err(|e| format!("{}: {e}", file.display()))
    }

    fn delete(&self, slot: Slot) -> Result<(), String> {
        let mut confirmed = true;
        if let KeyStore::Keyring { .. } = self {
            match keyring_entry(slot).and_then(|e| e.delete_credential()) {
                Ok(()) | Err(keyring::Error::NoEntry) | Err(keyring::Error::NoDefaultStore) => {}
                Err(e) => {
                    tracing::debug!(error = %e, "the credential store didn't confirm the removal");
                    confirmed = false;
                }
            }
        }
        match self.file(slot) {
            Some(_) if confirmed => self.remove_file(slot),
            Some(file) => {
                write_private(&file, REMOVED).map_err(|e| format!("{}: {e}", file.display()))
            }
            None if confirmed => Ok(()),
            None => Err("the credential store couldn't be reached".into()),
        }
    }

    fn remove_file(&self, slot: Slot) -> Result<(), String> {
        match self.file(slot).map(|f| {
            let removed = std::fs::remove_file(&f);
            (f, removed)
        }) {
            Some((file, Err(e))) if e.kind() != io::ErrorKind::NotFound => {
                Err(format!("{}: {e}", file.display()))
            }
            _ => Ok(()),
        }
    }
}

fn keyring_entry(slot: Slot) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, slot.user)
}

/// Replaces `path` with a new file holding `text` that only the user can
/// read, whatever the permissions of the file it replaces.
fn write_private(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    match std::fs::remove_file(&tmp) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options
        .open(&tmp)
        .and_then(|mut f| io::Write::write_all(&mut f, format!("{text}\n").as_bytes()))
        .and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// Today in UTC, as a day number.
pub fn today() -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    (secs / 86_400) as i64
}

pub struct Config {
    pub enforce: bool,
    pub public_key: Option<VerifyingKey>,
    pub build_date: String,
    pub trial_file: Option<PathBuf>,
    pub store: KeyStore,
}

impl Config {
    /// What this build and its environment say. Packaged builds use only what
    /// was embedded. Builds from source skip the check unless
    /// `CONVT_LICENSE_ENFORCE=1`, and take `CONVT_LICENSE_PUBKEY` at run time,
    /// so the licensing flow can be tried without packaging.
    pub fn from_env(config_dir: Option<PathBuf>, data_dir: Option<PathBuf>) -> Self {
        let source_build = !crate::ENFORCED;
        let env = |name| std::env::var(name).ok().filter(|_| source_build);
        let public_key = env("CONVT_LICENSE_PUBKEY")
            .and_then(|k| crate::parse_public_key(&k))
            .or_else(crate::public_key);
        let fallback = config_dir.map(|d| d.join("license.key"));
        let store = match (std::env::var("CONVT_LICENSE_STORE").as_deref(), fallback) {
            (Ok("file"), Some(file)) => KeyStore::File(file),
            (_, fallback) => KeyStore::Keyring { fallback },
        };
        Self {
            enforce: crate::ENFORCED || env("CONVT_LICENSE_ENFORCE").is_some_and(|v| v == "1"),
            public_key,
            build_date: crate::BUILD_DATE.to_string(),
            trial_file: data_dir.map(|d| d.join("trial")),
            store,
        }
    }
}

/// The license state of this machine.
pub struct Licensing {
    config: Config,
    /// The stored key, read again before each conversion so a key removed
    /// by another client stops counting.
    key: Option<String>,
    local_trial_enabled: bool,
    account_trial_ends_on: Option<String>,
    account_trial_ends_at: Option<SystemTime>,
}

impl Licensing {
    pub fn new(config: Config) -> Self {
        let key = if config.enforce {
            config.store.load(LICENSE)
        } else {
            None
        };
        Self {
            config,
            key,
            local_trial_enabled: true,
            account_trial_ends_on: None,
            account_trial_ends_at: None,
        }
    }

    /// Reads the stored key again.
    pub fn reload(&mut self) {
        if self.config.enforce {
            self.key = self.config.store.load(LICENSE);
        }
    }

    pub fn enforced(&self) -> bool {
        self.config.enforce
    }

    /// Packaged desktop builds use the account trial; the CLI keeps its local trial.
    pub fn disable_local_trial(&mut self) {
        self.local_trial_enabled = false;
    }

    /// Sets the current account trial returned by convt.app. No key is stored.
    pub fn set_account_trial(&mut self, ends_on: Option<String>) {
        self.account_trial_ends_on = ends_on.filter(|date| crate::date::to_days(date).is_some());
        self.account_trial_ends_at = None;
    }

    /// Sets the exact online trial end. Older servers may omit it; those are
    /// treated conservatively as ending at the start of the display day.
    pub fn set_account_trial_exact(&mut self, ends_on: Option<String>, ends_at: Option<String>) {
        if ends_at.is_some() && ends_at.as_deref().and_then(parse_utc_timestamp).is_none() {
            self.account_trial_ends_on = None;
            self.account_trial_ends_at = None;
            return;
        }
        self.account_trial_ends_on = ends_on.filter(|date| crate::date::to_days(date).is_some());
        self.account_trial_ends_at = ends_at.as_deref().and_then(parse_utc_timestamp);
    }

    pub fn build_date(&self) -> &str {
        &self.config.build_date
    }

    /// Where the trial start date is kept.
    pub fn trial_file(&self) -> Option<&Path> {
        self.config.trial_file.as_deref()
    }

    pub fn state(&self) -> State {
        if self.config.enforce
            && let Some(end) = self.account_trial_ends_at
            && let Ok(remaining) = end.duration_since(SystemTime::now())
            && remaining > Duration::ZERO
            && let Some(ends_on) = &self.account_trial_ends_on
        {
            return State::AccountTrial {
                ends_on: ends_on.clone(),
                days_left: remaining.as_secs().div_ceil(86_400) as i64,
            };
        }
        self.state_on(today())
    }

    pub fn state_on(&self, today: i64) -> State {
        if !self.config.enforce {
            return State::Unrestricted;
        }
        if let Some(license) = self.license() {
            return match license.covers_build(&self.config.build_date) {
                Ok(()) => State::Licensed(license),
                Err(_) => State::NotCovered(license),
            };
        }
        if self.account_trial_ends_at.is_none()
            && let Some(ends_on) = &self.account_trial_ends_on
            && let Some(end) = date::to_days(ends_on)
            && today < end
        {
            return State::AccountTrial {
                ends_on: ends_on.clone(),
                days_left: end - today,
            };
        }
        let started = self.trial_started();
        if !self.local_trial_enabled && started.is_none() {
            return State::SignInNeeded;
        }
        let elapsed = started
            .as_deref()
            .and_then(date::to_days)
            .map_or(0, |start| (today - start).max(0));
        if elapsed >= TRIAL_DAYS {
            State::TrialEnded
        } else {
            State::Trial {
                days_left: TRIAL_DAYS - elapsed,
                started,
            }
        }
    }

    /// The stored license, if it verifies.
    fn license(&self) -> Option<License> {
        let key = self.key.as_deref()?;
        crate::verify(key, self.config.public_key.as_ref()?).ok()
    }

    fn trial_started(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.config.trial_file.as_ref()?).ok()?;
        let date = text.trim().to_string();
        date::to_days(&date).map(|_| date)
    }

    /// Checks that a conversion may run, starting the trial on the first one.
    /// Returns the state that blocks it otherwise.
    pub fn begin_conversion(&mut self) -> Result<(), Blocked> {
        self.reload();
        let state = self.state();
        if !state.allows_conversion() {
            return Err(Blocked::State(state));
        }
        if let State::Trial { started: None, .. } = state {
            let file =
                self.config.trial_file.as_deref().ok_or_else(|| {
                    Blocked::TrialNotRecorded("there is no data directory".into())
                })?;
            write_private(file, &date::from_days(today()))
                .map_err(|e| Blocked::TrialNotRecorded(format!("{}: {e}", file.display())))?;
        }
        Ok(())
    }

    /// Verifies and stores `key`. A valid key whose update window ended
    /// before this build is stored too; [`state`](Self::state) then says so.
    pub fn activate(&mut self, key: &str) -> Result<License, ActivateError> {
        let key = key.trim();
        let public_key = self
            .config
            .public_key
            .as_ref()
            .ok_or(ActivateError::NoPublicKey)?;
        let license = crate::verify(key, public_key).map_err(|_| ActivateError::Invalid)?;
        self.config
            .store
            .save(LICENSE, key)
            .map_err(ActivateError::Store)?;
        self.key = Some(key.to_string());
        Ok(license)
    }

    /// Removes the stored license from this machine.
    pub fn deactivate(&mut self) -> Result<(), String> {
        self.config.store.delete(LICENSE)?;
        self.key = None;
        Ok(())
    }

    /// Stores a key that renewal fetched, without asking, when it is a valid
    /// Pro key that covers builds the stored key doesn't. A stored key that
    /// covers as much or more stays, so renewal never shortens what this
    /// machine may run, and a key that doesn't verify changes nothing.
    pub fn offer_key(&mut self, key: &str) -> Result<Renewed, ActivateError> {
        let key = key.trim();
        let public_key = self
            .config
            .public_key
            .as_ref()
            .ok_or(ActivateError::NoPublicKey)?;
        let offered = crate::verify(key, public_key).map_err(|_| ActivateError::Invalid)?;
        if offered.plan != Plan::Pro {
            return Err(ActivateError::Invalid);
        }
        self.key = self.config.store.load(LICENSE);
        if let Some(current) = self.license()
            && (current.plan == Plan::Desktop || current.updates_until >= offered.updates_until)
        {
            return Ok(Renewed::Kept(current));
        }
        self.config
            .store
            .save(LICENSE, key)
            .map_err(ActivateError::Store)?;
        self.key = Some(key.to_string());
        Ok(Renewed::Stored(offered))
    }

    /// The convt.app sign-in kept on this machine, if any. Read whether or
    /// not this build checks licenses: sign-in is for Pro renewal and the
    /// cloud, not for converting.
    pub fn session(&self) -> Option<Session> {
        let text = self.config.store.load(ACCOUNT)?;
        serde_json::from_str(&text)
            .inspect_err(|_| tracing::warn!("the stored sign-in is unreadable; ignoring it"))
            .ok()
    }

    pub fn save_session(&self, session: &Session) -> Result<(), String> {
        let text = serde_json::to_string(session).map_err(|e| e.to_string())?;
        self.config.store.save(ACCOUNT, &text)
    }

    /// Forgets the sign-in on this machine. The license key stays.
    pub fn clear_session(&self) -> Result<(), String> {
        self.config.store.delete(ACCOUNT)
    }
}

fn parse_utc_timestamp(value: &str) -> Option<SystemTime> {
    let (date, time) = value.strip_suffix('Z')?.split_once('T')?;
    let day = crate::date::to_days(date)?;
    let mut parts = time.split(':');
    let hour: u64 = parts.next()?.parse().ok()?;
    let minute: u64 = parts.next()?.parse().ok()?;
    let second: u64 = parts.next()?.split('.').next()?.parse().ok()?;
    if hour >= 24 || minute >= 60 || second >= 60 {
        return None;
    }
    let seconds = (day as u64)
        .checked_mul(86_400)?
        .checked_add(hour * 3_600 + minute * 60 + second)?;
    Some(UNIX_EPOCH + Duration::from_secs(seconds))
}

/// What [`Licensing::offer_key`] did with a key renewal fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Renewed {
    /// The key covers newer builds than the stored one, and replaced it.
    Stored(License),
    /// The stored key covers as much or more, and stays.
    Kept(License),
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::{Plan, sign};

    struct Fixture {
        dir: tempfile::TempDir,
        signing: SigningKey,
    }

    impl Fixture {
        fn new() -> Self {
            let mut seed = [0u8; 32];
            getrandom::fill(&mut seed).unwrap();
            Self {
                dir: tempfile::tempdir().unwrap(),
                signing: SigningKey::from_bytes(&seed),
            }
        }

        fn licensing(&self, enforce: bool) -> Licensing {
            Licensing::new(Config {
                enforce,
                public_key: Some(self.signing.verifying_key()),
                build_date: "2026-10-02".into(),
                trial_file: Some(self.dir.path().join("data/trial")),
                store: KeyStore::File(self.dir.path().join("config/license.key")),
            })
        }

        fn key(&self, until: &str) -> String {
            sign(
                &License {
                    id: "lic_1".into(),
                    email: "a@b.c".into(),
                    plan: Plan::Desktop,
                    issued: "2026-01-01".into(),
                    updates_until: until.into(),
                },
                &self.signing,
            )
        }
    }

    #[test]
    fn source_builds_are_unrestricted() {
        let f = Fixture::new();
        let mut l = f.licensing(false);
        assert_eq!(l.state(), State::Unrestricted);
        assert!(l.begin_conversion().is_ok());
        assert!(!f.dir.path().join("data/trial").exists());
    }

    #[test]
    fn the_first_conversion_starts_the_trial() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        let start = today();
        assert_eq!(
            l.state(),
            State::Trial {
                days_left: 7,
                started: None
            }
        );
        l.begin_conversion().unwrap();
        let started = date::from_days(start);
        assert_eq!(
            std::fs::read_to_string(f.dir.path().join("data/trial")).unwrap(),
            format!("{started}\n")
        );
        assert_eq!(
            l.state_on(start + 6),
            State::Trial {
                days_left: 1,
                started: Some(started.clone())
            }
        );
        assert_eq!(l.state_on(start + 7), State::TrialEnded);
        // Another conversion doesn't move the start.
        l.begin_conversion().unwrap();
        assert_eq!(l.state_on(start + 7), State::TrialEnded);
        // A clock set back doesn't add days.
        assert!(matches!(
            l.state_on(start - 30),
            State::Trial { days_left: 7, .. }
        ));
    }

    #[test]
    fn an_account_trial_ends_at_the_exact_timestamp() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        let ends_on = date::from_days(today() + 2);
        l.set_account_trial_exact(Some(ends_on.clone()), Some(format!("{ends_on}T23:59:59Z")));
        assert!(
            matches!(l.state(), State::AccountTrial { ends_on: ref got, days_left, .. } if got == &ends_on && days_left >= 2)
        );
        assert!(l.begin_conversion().is_ok());
    }

    #[test]
    fn a_source_build_ignores_an_account_trial() {
        let f = Fixture::new();
        let mut l = f.licensing(false);
        let ends_on = date::from_days(today() + 2);
        l.set_account_trial_exact(Some(ends_on.clone()), Some(format!("{ends_on}T23:59:59Z")));
        assert_eq!(l.state(), State::Unrestricted);
    }

    #[test]
    fn an_old_date_only_trial_is_conservative_and_exclusive() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        let ends_on = date::from_days(today() + 2);
        l.set_account_trial(Some(ends_on.clone()));
        assert!(matches!(
            l.state_on(today() + 1),
            State::AccountTrial { days_left: 1, .. }
        ));
        assert!(!matches!(
            l.state_on(today() + 2),
            State::AccountTrial { .. }
        ));
    }

    #[test]
    fn a_packaged_install_without_an_account_needs_sign_in() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        l.disable_local_trial();
        assert_eq!(l.state(), State::SignInNeeded);
        assert!(!l.state().allows_conversion());
    }

    #[test]
    fn an_ended_trial_blocks_conversions() {
        let f = Fixture::new();
        let trial = f.dir.path().join("data/trial");
        write_private(&trial, &date::from_days(today() - 7)).unwrap();
        let mut l = f.licensing(true);
        assert_eq!(l.begin_conversion(), Err(Blocked::State(State::TrialEnded)));
        assert!(
            State::TrialEnded
                .blocked_reason()
                .unwrap()
                .contains(BUY_URL)
        );
    }

    #[test]
    fn activation_stores_the_key_privately() {
        let f = Fixture::new();
        write_private(
            &f.dir.path().join("data/trial"),
            &date::from_days(today() - 30),
        )
        .unwrap();
        let mut l = f.licensing(true);
        assert!(matches!(l.activate("nope"), Err(ActivateError::Invalid)));
        let license = l.activate(&format!("  {}\n", f.key("2027-10-02"))).unwrap();
        assert_eq!(l.state(), State::Licensed(license.clone()));
        assert!(l.begin_conversion().is_ok());

        let file = f.dir.path().join("config/license.key");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // A new process reads it back.
        assert_eq!(f.licensing(true).state(), State::Licensed(license));

        l.deactivate().unwrap();
        assert!(!file.exists());
        assert_eq!(l.state(), State::TrialEnded);
        assert_eq!(f.licensing(true).state(), State::TrialEnded);
    }

    #[test]
    fn a_key_from_another_signer_is_rejected() {
        let f = Fixture::new();
        let other = Fixture::new();
        let mut l = f.licensing(true);
        assert!(matches!(
            l.activate(&other.key("2027-10-02")),
            Err(ActivateError::Invalid)
        ));
        // A tampered file is ignored rather than trusted.
        write_private(
            &f.dir.path().join("config/license.key"),
            &other.key("2099-01-01"),
        )
        .unwrap();
        assert!(matches!(f.licensing(true).state(), State::Trial { .. }));
    }

    #[test]
    fn a_newer_build_than_the_license_covers() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        let license = l.activate(&pro_key(&f, "2026-10-01")).unwrap();
        let state = l.state();
        assert_eq!(state, State::NotCovered(license));
        assert!(state.blocked_reason().unwrap().contains("2026-10-01"));
        assert!(l.begin_conversion().is_err());
        // The build made on the last covered day is fine.
        let mut l = f.licensing(true);
        l.activate(&pro_key(&f, "2026-10-02")).unwrap();
        assert!(matches!(l.state(), State::Licensed(_)));
    }

    #[test]
    fn a_build_without_a_key_cannot_activate() {
        let f = Fixture::new();
        let mut l = Licensing::new(Config {
            public_key: None,
            ..f.licensing(true).config
        });
        assert!(matches!(
            l.activate(&f.key("2027-10-02")),
            Err(ActivateError::NoPublicKey)
        ));
    }

    /// Runs against a real Secret Service. Start one on a private session bus
    /// with its own data directory so the user's keyring is never touched:
    ///
    /// ```sh
    /// T=$(mktemp -d); mkdir -m 700 $T/run; mkdir $T/data
    /// env -u DBUS_SESSION_BUS_ADDRESS XDG_RUNTIME_DIR=$T/run XDG_DATA_HOME=$T/data \
    ///   dbus-run-session -- sh -c 'echo -n test | gnome-keyring-daemon --unlock --components=secrets >/dev/null;
    ///   cargo test -p convt-license --features client -- --ignored secret_service'
    /// rm -rf $T
    /// ```
    #[test]
    #[ignore = "needs a disposable Secret Service; see the doc comment"]
    fn secret_service_round_trip() {
        assert!(
            std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok_and(|a| !a.contains("/run/user/"))
                && std::env::var("XDG_DATA_HOME").is_ok(),
            "refusing to touch the login session's keyring"
        );
        let f = Fixture::new();
        let fallback = f.dir.path().join("config/license.key");
        let mut l = Licensing::new(Config {
            store: KeyStore::Keyring {
                fallback: Some(fallback.clone()),
            },
            ..f.licensing(true).config
        });
        let license = l.activate(&f.key("2027-10-02")).unwrap();
        assert!(!fallback.exists(), "the key went to the credential store");
        let fresh = Licensing::new(Config {
            store: KeyStore::Keyring {
                fallback: Some(fallback.clone()),
            },
            ..f.licensing(true).config
        });
        assert_eq!(fresh.state(), State::Licensed(license));
        // A key saved to the file while the store was away wins over the
        // older one still in the store.
        let newer = f.key("2028-10-02");
        write_private(&fallback, &newer).unwrap();
        assert!(matches!(fresh.config.store.load(LICENSE), Some(k) if k == newer));
        std::fs::remove_file(&fallback).unwrap();
        l.deactivate().unwrap();
        assert!(keyring_entry(LICENSE).unwrap().get_password().is_err());
    }

    #[test]
    fn a_trial_that_cannot_be_recorded_does_not_start() {
        let f = Fixture::new();
        // The data "directory" is a file, so the trial file can't be written.
        std::fs::write(f.dir.path().join("data"), "").unwrap();
        let mut l = f.licensing(true);
        assert!(matches!(
            l.begin_conversion(),
            Err(Blocked::TrialNotRecorded(_))
        ));
        let mut l = Licensing::new(Config {
            trial_file: None,
            ..f.licensing(true).config
        });
        assert!(matches!(
            l.begin_conversion(),
            Err(Blocked::TrialNotRecorded(_))
        ));
        // Without enforcement there is nothing to record.
        assert!(f.licensing(false).begin_conversion().is_ok());
    }

    #[test]
    fn a_key_removed_elsewhere_stops_counting() {
        let f = Fixture::new();
        write_private(
            &f.dir.path().join("data/trial"),
            &date::from_days(today() - 30),
        )
        .unwrap();
        let mut app = f.licensing(true);
        app.activate(&f.key("2027-10-02")).unwrap();
        let mut cli = f.licensing(true);
        assert!(cli.begin_conversion().is_ok());
        app.deactivate().unwrap();
        assert_eq!(
            cli.begin_conversion(),
            Err(Blocked::State(State::TrialEnded))
        );
    }

    fn pro_key(f: &Fixture, until: &str) -> String {
        sign(
            &License {
                id: format!("lic_pro_{until}"),
                email: "a@b.c".into(),
                plan: Plan::Pro,
                issued: "2026-09-01".into(),
                updates_until: until.into(),
            },
            &f.signing,
        )
    }

    #[test]
    fn renewal_stores_only_a_pro_key_that_covers_more() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        // Nothing stored: a valid Pro key goes in without asking.
        let first = l.offer_key(&pro_key(&f, "2026-11-01")).unwrap();
        assert!(matches!(&first, Renewed::Stored(k) if k.updates_until == "2026-11-01"));
        assert!(matches!(l.state(), State::Licensed(_)));
        // The same key again, or an older one, changes nothing.
        assert!(matches!(
            l.offer_key(&pro_key(&f, "2026-11-01")),
            Ok(Renewed::Kept(_))
        ));
        assert!(matches!(
            l.offer_key(&pro_key(&f, "2026-10-15")),
            Ok(Renewed::Kept(k)) if k.updates_until == "2026-11-01"
        ));
        // The next period's key replaces it.
        assert!(matches!(
            l.offer_key(&pro_key(&f, "2026-12-01")),
            Ok(Renewed::Stored(k)) if k.updates_until == "2026-12-01"
        ));
        assert!(
            matches!(f.licensing(true).state(), State::Licensed(k) if k.updates_until == "2026-12-01")
        );
        // A Desktop key with a later window stays ahead of a Pro key.
        l.activate(&f.key("2027-10-01")).unwrap();
        assert!(matches!(
            l.offer_key(&pro_key(&f, "2027-01-01")),
            Ok(Renewed::Kept(k)) if k.plan == Plan::Desktop
        ));
        // Keys that don't verify, or aren't Pro, are refused and change nothing.
        let other = Fixture::new();
        assert!(matches!(
            l.offer_key(&pro_key(&other, "2099-01-01")),
            Err(ActivateError::Invalid)
        ));
        assert!(matches!(
            l.offer_key(&f.key("2099-01-01")),
            Err(ActivateError::Invalid)
        ));
        assert!(matches!(
            l.offer_key("garbage"),
            Err(ActivateError::Invalid)
        ));
        assert!(matches!(l.state(), State::Licensed(k) if k.updates_until == "2027-10-01"));
    }

    #[test]
    fn renewal_replaces_a_key_this_build_outgrew() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        l.activate(&pro_key(&f, "2026-09-01")).unwrap();
        assert!(matches!(l.state(), State::NotCovered(_)));
        l.offer_key(&pro_key(&f, "2026-11-01")).unwrap();
        assert!(matches!(l.state(), State::Licensed(_)));
    }

    #[test]
    fn renewal_never_replaces_a_legacy_desktop_key() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        l.activate(&f.key("2026-09-01")).unwrap();
        assert!(matches!(
            l.offer_key(&pro_key(&f, "2028-01-01")),
            Ok(Renewed::Kept(k)) if k.plan == Plan::Desktop
        ));
        assert!(matches!(l.state(), State::Licensed(k) if k.plan == Plan::Desktop));
    }

    #[test]
    fn the_sign_in_is_stored_privately_next_to_the_key() {
        let f = Fixture::new();
        // A source build keeps a sign-in too: it is for renewal and the cloud.
        let l = f.licensing(false);
        assert_eq!(l.session(), None);
        let session = Session {
            email: "a@b.c".into(),
            token: "cvd_token".into(),
        };
        l.save_session(&session).unwrap();
        let file = f.dir.path().join("config/account.json");
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("cvd_token")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert_eq!(f.licensing(true).session(), Some(session));
        // Signing out keeps the license.
        let mut l = f.licensing(true);
        l.activate(&f.key("2027-10-02")).unwrap();
        l.clear_session().unwrap();
        assert!(!file.exists());
        assert_eq!(l.session(), None);
        assert!(matches!(l.state(), State::Licensed(_)));
        // A broken file is no sign-in, not a crash.
        write_private(&file, "{").unwrap();
        assert_eq!(l.session(), None);
        // Nowhere to keep it.
        let none = Licensing::new(Config {
            store: KeyStore::None,
            ..f.licensing(true).config
        });
        assert!(
            none.save_session(&Session {
                email: "a@b.c".into(),
                token: "t".into()
            })
            .is_err()
        );
    }

    #[test]
    fn a_removal_marker_means_no_key() {
        let f = Fixture::new();
        let file = f.dir.path().join("config/license.key");
        write_private(&file, REMOVED).unwrap();
        assert!(matches!(f.licensing(true).state(), State::Trial { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_open_key_file_becomes_private() {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new();
        let file = f.dir.path().join("config/license.key");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "old").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        f.licensing(true).activate(&f.key("2027-10-02")).unwrap();
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(!file.with_extension("tmp").exists());
    }
}
