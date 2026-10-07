//! The license check every client shares: the desktop app, the CLI and the OS
//! menu integrations. Nothing here touches the network; the app fetches keys
//! and trials through [`crate::account`] and hands them to
//! [`Licensing::offer_key`].
//!
//! A license key, or a trial key, lives in the OS credential store (Keychain,
//! Windows Credential Manager, Secret Service), or in a file in the config
//! directory when no credential store runs. The convt.app sign-in of a
//! desktop app, when there is one, is kept the same way next to the key.
//!
//! The free trial needs a convt.app account: the desktop app signs in and
//! asks convt.app for a trial key, which works for 7 days by today's date.
//! Nothing starts a trial offline, and converting never starts one. Builds
//! from before that kept a local trial as a start date in the data
//! directory; that file is read once, its last day carried over to the
//! credential store, and the file removed.
//!
//! The clock can only be trusted so far. The latest time this machine has
//! seen is kept too (`last_seen`), and when the clock is more than an hour
//! behind it, a trial stops counting until convt.app vouches for the time
//! ([`Licensing::record_server_time`]). Paid keys never depend on the clock.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use ed25519_dalek::VerifyingKey;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::account::Session;
use crate::{License, date};

/// How long a trial lasts, counting the day it starts.
pub const TRIAL_DAYS: i64 = 7;
/// Where Buy sends people.
pub const BUY_URL: &str = "https://convt.app/pricing";
/// Where older builds can be downloaded.
pub const DOWNLOAD_URL: &str = "https://convt.app/download";
/// Where people reach convt when the trial check gets them wrong.
pub const CONTACT_URL: &str = "https://convt.app/contact";
/// How far the clock may fall behind the latest time this machine has seen
/// before a trial needs an online check, in seconds.
pub const CLOCK_TOLERANCE: i64 = 3600;

const KEYRING_SERVICE: &str = "convt";
/// What the key file holds after a removal the credential store didn't
/// confirm, so a key still in the store can't come back.
const REMOVED: &str = "removed";
/// What the legacy trial slot holds once there was nothing to carry over.
const NO_LEGACY_TRIAL: &str = "none";
/// The salt of the device hash. Changing it gives every computer a new trial.
const DEVICE_HASH_SALT: &[u8] = b"convt-device-hash-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// A build from source, which needs no license.
    Unrestricted,
    /// A trial from convt.app, or one an older build started here.
    /// `last_day` is the last day it works, `YYYY-MM-DD`.
    Trial {
        days_left: i64,
        last_day: String,
    },
    /// No license and no trial: the trial starts from the desktop app, after
    /// signing in to convt.app.
    NoTrial,
    TrialEnded,
    /// The clock went back since convt last ran, so the trial can't be
    /// judged until convt.app confirms the time.
    NeedsCheck,
    Licensed(License),
    /// A valid license whose update window ended before this build was made.
    NotCovered(License),
}

impl State {
    pub fn allows_conversion(&self) -> bool {
        matches!(
            self,
            State::Unrestricted | State::Trial { .. } | State::Licensed(_)
        )
    }

    /// One line on where this machine stands, for the user.
    pub fn summary(&self) -> String {
        match self {
            State::Unrestricted => "This build doesn't need a license.".into(),
            State::Trial { days_left: 1, .. } => "Free trial: last day.".into(),
            State::Trial { days_left, .. } => format!("Free trial: {days_left} days left."),
            State::Licensed(l) => format!(
                "Licensed to {} ({}), with updates until {}.",
                l.email,
                l.plan.name(),
                l.updates_until
            ),
            State::NoTrial | State::TrialEnded | State::NeedsCheck | State::NotCovered(_) => {
                self.blocked_reason().unwrap_or_default()
            }
        }
    }

    /// Why conversions are stopped, for the user, or `None` if they aren't.
    pub fn blocked_reason(&self) -> Option<String> {
        match self {
            State::NoTrial => Some(format!(
                "Start your free {TRIAL_DAYS}-day trial by signing in to convt.app from the convt \
                 app, or buy a license at {BUY_URL}."
            )),
            State::TrialEnded => Some(format!(
                "Your convt trial has ended. Buy a license at {BUY_URL} to keep converting."
            )),
            State::NeedsCheck => Some(
                "Your computer's clock seems to have gone back. Connect to the internet and \
                 open convt to check your trial."
                    .into(),
            ),
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
}

#[derive(Debug, thiserror::Error)]
pub enum ActivateError {
    #[error("That license key isn't valid. Check that you pasted all of it.")]
    Invalid,
    /// Trial keys come from signing in, for one computer; pasting one
    /// elsewhere doesn't count.
    #[error("That's a trial key. Start the trial from the convt app by signing in to convt.app.")]
    TrialKey,
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

/// The license key, paid or trial.
const LICENSE: Slot = Slot {
    user: "license",
    file: None,
};
/// A trial key from convt.app, kept apart from [`LICENSE`] so a trial never
/// replaces a key the user bought.
const TRIAL_KEY: Slot = Slot {
    user: "trial_key",
    file: Some("trial.key"),
};
/// The device token from desktop sign-in, with the account's email.
const ACCOUNT: Slot = Slot {
    user: "account",
    file: Some("account.json"),
};
/// The last day of a trial an older build kept as a file, or
/// [`NO_LEGACY_TRIAL`]. Any value means the file was dealt with.
const LEGACY_TRIAL: Slot = Slot {
    user: "legacy_trial",
    file: Some("legacy-trial"),
};
/// The latest time this machine has seen, in Unix seconds.
const LAST_SEEN: Slot = Slot {
    user: "last_seen",
    file: Some("last-seen"),
};
/// A random id for this installation, for the device hash when the OS has
/// no machine id.
const INSTALL: Slot = Slot {
    user: "install",
    file: Some("install-id"),
};

/// Where the license key is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Now, in Unix seconds.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Today in UTC, as a day number.
pub fn today() -> i64 {
    now().div_euclid(86_400)
}

/// The device hash convt.app keeps one trial per: the lowercase hex
/// HMAC-SHA256 of `input` under the app's salt. `input` is `machine:` plus
/// the OS machine id, or `install:` plus this installation's random id. Only
/// the hash leaves the machine.
pub fn device_hash_of(input: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(DEVICE_HASH_SALT).expect("HMAC takes any key");
    mac.update(input.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub struct Config {
    pub enforce: bool,
    pub public_key: Option<VerifyingKey>,
    pub build_date: String,
    /// Where builds before sign-in trials kept the trial's start date. Read
    /// once, then removed.
    pub trial_file: Option<PathBuf>,
    pub store: KeyStore,
}

impl Config {
    /// What this build and its environment say. Packaged builds use only what
    /// was embedded, and keep licensing state in the platform's own folders
    /// whatever `CONVT_CONFIG_DIR`, `CONVT_DATA_DIR` or `CONVT_LICENSE_STORE`
    /// say. Builds from source skip the check unless `CONVT_LICENSE_ENFORCE=1`,
    /// and take `CONVT_LICENSE_PUBKEY`, `CONVT_LICENSE_STORE` and the
    /// `config_dir` and `data_dir` given here, so the licensing flow can be
    /// tried without packaging.
    pub fn from_env(config_dir: Option<PathBuf>, data_dir: Option<PathBuf>) -> Self {
        Self::resolve(
            crate::ENFORCED,
            &|name| std::env::var(name).ok(),
            (config_dir, data_dir),
            (
                dirs::config_dir().map(|d| d.join("convt")),
                dirs::data_dir().map(|d| d.join("convt")),
            ),
        )
    }

    /// [`Self::from_env`] with the build kind, the environment and both pairs
    /// of folders passed in, so tests can check a packaged build's choices.
    fn resolve(
        packaged: bool,
        env: &dyn Fn(&str) -> Option<String>,
        given: (Option<PathBuf>, Option<PathBuf>),
        platform: (Option<PathBuf>, Option<PathBuf>),
    ) -> Self {
        let env = |name| env(name).filter(|_| !packaged);
        let (config_dir, data_dir) = if packaged { platform } else { given };
        let public_key = env("CONVT_LICENSE_PUBKEY")
            .and_then(|k| crate::parse_public_key(&k))
            .or_else(crate::public_key);
        let fallback = config_dir.map(|d| d.join("license.key"));
        let store = match (env("CONVT_LICENSE_STORE").as_deref(), fallback) {
            (Some("file"), Some(file)) => KeyStore::File(file),
            (_, fallback) => KeyStore::Keyring { fallback },
        };
        Self {
            enforce: packaged || env("CONVT_LICENSE_ENFORCE").is_some_and(|v| v == "1"),
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
    /// The stored paid key, read again before each conversion so a key
    /// removed by another client stops counting.
    key: Option<String>,
    /// The stored trial key, read again like `key`.
    trial_key: Option<String>,
    /// The last day of a trial an older build started here, `YYYY-MM-DD`.
    legacy_trial: Option<String>,
    /// The latest time this machine has seen, in Unix seconds.
    last_seen: Option<i64>,
}

impl Licensing {
    pub fn new(config: Config) -> Self {
        let mut licensing = Self {
            config,
            key: None,
            trial_key: None,
            legacy_trial: None,
            last_seen: None,
        };
        if licensing.config.enforce {
            licensing.legacy_trial = licensing.carry_over_legacy_trial();
            licensing.reload();
            licensing.note_time(now());
        }
        licensing
    }

    /// Reads the stored key and the latest time seen again.
    pub fn reload(&mut self) {
        if self.config.enforce {
            self.key = self.config.store.load(LICENSE);
            self.trial_key = self.config.store.load(TRIAL_KEY);
            self.last_seen = self
                .config
                .store
                .load(LAST_SEEN)
                .and_then(|t| t.parse().ok());
        }
    }

    pub fn enforced(&self) -> bool {
        self.config.enforce
    }

    pub fn build_date(&self) -> &str {
        &self.config.build_date
    }

    /// Reads the trial file of an older build once. A start date no later
    /// than today counts, whichever build is running (an older release may
    /// have started it after this one was built): its last day goes to the
    /// credential store and the file is removed. Whatever the file held, the
    /// store then says it was dealt with, so the file is never read again,
    /// and deleting or rewriting it later gives nothing. With no file there
    /// is nothing to record, so a store that can't be read for now never
    /// gets a marker that would hide the days it holds.
    fn carry_over_legacy_trial(&self) -> Option<String> {
        if let Some(value) = self.config.store.load(LEGACY_TRIAL) {
            return date::to_days(&value).map(|_| value);
        }
        let file = self.config.trial_file.as_deref();
        let text = match file.map(std::fs::read_to_string) {
            Some(Ok(text)) => Some(text),
            Some(Err(e)) if e.kind() != io::ErrorKind::NotFound => {
                // Unreadable for now: leave it for the next run rather than
                // recording that there was no trial.
                tracing::warn!(error = %e, "could not read the old trial file");
                return None;
            }
            Some(Err(_)) | None => return None,
        };
        let last_day = text
            .and_then(|text| date::to_days(text.trim()))
            .filter(|&start| start <= today())
            .map(|start| date::from_days(start + TRIAL_DAYS - 1));
        let value = last_day.as_deref().unwrap_or(NO_LEGACY_TRIAL);
        match self.config.store.save(LEGACY_TRIAL, value) {
            Ok(()) => {
                if let Some(file) = file
                    && let Err(e) = std::fs::remove_file(file)
                    && e.kind() != io::ErrorKind::NotFound
                {
                    tracing::warn!(error = %e, "could not remove the old trial file");
                }
            }
            // The file stays for the next run to try again.
            Err(e) => tracing::warn!(error = %e, "could not carry over the old trial"),
        }
        last_day
    }

    /// Remembers `now` as the latest time seen, if it is, at most once an
    /// hour so the credential store isn't written on every conversion.
    fn note_time(&mut self, now: i64) {
        if self.last_seen.is_none_or(|seen| now >= seen + 3600) {
            self.set_last_seen(now);
        }
    }

    fn set_last_seen(&mut self, time: i64) {
        self.last_seen = Some(time);
        if let Err(e) = self.config.store.save(LAST_SEEN, &time.to_string()) {
            tracing::debug!(error = %e, "could not record the time");
        }
    }

    /// Takes convt.app's clock, from a successful call, as the latest time
    /// seen. A trial blocked by a clock that went back counts again once the
    /// clock here is within [`CLOCK_TOLERANCE`] of it.
    pub fn record_server_time(&mut self, server_now: i64) {
        if self.config.enforce {
            self.set_last_seen(server_now);
        }
    }

    pub fn state(&self) -> State {
        self.state_at(now())
    }

    /// The state at `now`, in Unix seconds.
    pub fn state_at(&self, now: i64) -> State {
        if !self.config.enforce {
            return State::Unrestricted;
        }
        let paid = self.license();
        if let Some(paid) = &paid
            && paid.covers_build(&self.config.build_date).is_ok()
        {
            return State::Licensed(paid.clone());
        }
        // A paid key this build has outgrown still says so, unless a trial
        // runs this build (or is waiting on a clock check).
        match (self.trial_state(now), paid) {
            (state @ (State::Trial { .. } | State::NeedsCheck), _) => state,
            (_, Some(paid)) => State::NotCovered(paid),
            (state, None) => state,
        }
    }

    /// Where the trial stands at `now`: the trial key, or a trial an older
    /// build started here, whichever runs longer.
    fn trial_state(&self, now: i64) -> State {
        let token = self
            .trial()
            .and_then(|l| Some((date::to_days(&l.updates_until)?, date::to_days(&l.issued)?)));
        let legacy = self.legacy_trial.as_deref().and_then(date::to_days);
        let Some(last_day) = token.map(|(until, _)| until).max(legacy) else {
            return State::NoTrial;
        };
        let today = now.div_euclid(86_400);
        let seen = self.last_seen.unwrap_or(now);
        // Behind the latest time seen, or before the day convt.app issued
        // the trial key: the clock was set back. The issue day allows one
        // day of slack so a clock a little behind around midnight UTC isn't
        // paused; it can't add days, since the last day is fixed.
        let behind =
            now < seen - CLOCK_TOLERANCE || token.is_some_and(|(_, issued)| today < issued - 1);
        if behind {
            return if last_day < seen.div_euclid(86_400) {
                State::TrialEnded
            } else {
                State::NeedsCheck
            };
        }
        // Count from the issue day at the earliest, so the slack above never
        // makes a trial longer than convt.app signed it for.
        let counted_from = token.map_or(today, |(_, issued)| today.max(issued));
        let days_left = last_day - counted_from + 1;
        if days_left <= 0 {
            State::TrialEnded
        } else {
            State::Trial {
                days_left,
                last_day: date::from_days(last_day),
            }
        }
    }

    /// The stored paid key, if it verifies.
    fn license(&self) -> Option<License> {
        self.verified(self.key.as_deref()?)
            .filter(|l| l.plan.is_paid())
    }

    /// The stored trial key, if it verifies.
    fn trial(&self) -> Option<License> {
        self.verified(self.trial_key.as_deref()?)
            .filter(|l| !l.plan.is_paid())
    }

    fn verified(&self, key: &str) -> Option<License> {
        crate::verify(key, self.config.public_key.as_ref()?).ok()
    }

    /// Checks that a conversion may run. Returns the state that blocks it
    /// otherwise. Nothing here starts a trial.
    pub fn begin_conversion(&mut self) -> Result<(), Blocked> {
        self.reload();
        let now = now();
        if self.config.enforce {
            self.note_time(now);
        }
        let state = self.state_at(now);
        if !state.allows_conversion() {
            return Err(Blocked::State(state));
        }
        Ok(())
    }

    /// Verifies and stores a pasted Desktop or Pro key. A valid key whose
    /// update window ended before this build is stored too;
    /// [`state`](Self::state) then says so. Trial keys only come from
    /// signing in ([`Self::offer_key`]).
    pub fn activate(&mut self, key: &str) -> Result<License, ActivateError> {
        let key = key.trim();
        let public_key = self
            .config
            .public_key
            .as_ref()
            .ok_or(ActivateError::NoPublicKey)?;
        let license = crate::verify(key, public_key).map_err(|_| ActivateError::Invalid)?;
        if !license.plan.is_paid() {
            return Err(ActivateError::TrialKey);
        }
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

    /// Stores a key that convt.app sent, without asking, when it runs longer
    /// than the stored key of its kind. Paid and trial keys are kept apart,
    /// so a trial never replaces a key the user bought (which still runs the
    /// older builds it covers, offline), and a paid key leaves the trial
    /// alone. Renewal never shortens what this machine may run, and a key
    /// that doesn't verify changes nothing.
    pub fn offer_key(&mut self, key: &str) -> Result<Renewed, ActivateError> {
        let key = key.trim();
        let public_key = self
            .config
            .public_key
            .as_ref()
            .ok_or(ActivateError::NoPublicKey)?;
        let offered = crate::verify(key, public_key).map_err(|_| ActivateError::Invalid)?;
        let paid = offered.plan.is_paid();
        let slot = if paid { LICENSE } else { TRIAL_KEY };
        let stored = self.config.store.load(slot);
        let current = stored
            .as_deref()
            .and_then(|k| self.verified(k))
            .filter(|l| l.plan.is_paid() == paid);
        if let Some(current) = current
            && current.updates_until >= offered.updates_until
        {
            // Another client may have stored it since this one last read.
            if paid {
                self.key = stored;
            } else {
                self.trial_key = stored;
            }
            return Ok(Renewed::Kept(current));
        }
        self.config
            .store
            .save(slot, key)
            .map_err(ActivateError::Store)?;
        if paid {
            self.key = Some(key.to_string());
        } else {
            self.trial_key = Some(key.to_string());
        }
        Ok(Renewed::Stored(offered))
    }

    /// This computer's device hash for starting a trial ([`device_hash_of`]):
    /// from the OS machine id, or else from a random id made once for this
    /// installation and kept in the credential store.
    pub fn device_hash(&self) -> Result<String, String> {
        let machine = machine_uid::get()
            .inspect_err(|e| tracing::debug!(error = %e, "no machine id"))
            .ok();
        self.device_hash_from(machine)
    }

    fn device_hash_from(&self, machine: Option<String>) -> Result<String, String> {
        if let Some(id) = machine
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            return Ok(device_hash_of(&format!("machine:{id}")));
        }
        let install = match self.config.store.load(INSTALL) {
            Some(id) => id,
            None => {
                let mut bytes = [0u8; 32];
                getrandom::fill(&mut bytes).map_err(|e| format!("no random numbers: {e}"))?;
                let id = B64.encode(bytes);
                self.config
                    .store
                    .save(INSTALL, &id)
                    .map_err(|e| format!("this computer's id couldn't be saved: {e}"))?;
                id
            }
        };
        Ok(device_hash_of(&format!("install:{install}")))
    }

    /// The convt.app sign-in kept on this machine, if any. Read whether or
    /// not this build checks licenses: sign-in is for the trial, renewal and
    /// the cloud, not for converting.
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

/// What [`Licensing::offer_key`] did with a key convt.app sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Renewed {
    /// The key gives more than the stored one, and replaced it.
    Stored(License),
    /// The stored key gives as much or more, and stays.
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

    const DAY: i64 = 86_400;

    impl Fixture {
        fn new() -> Self {
            let mut seed = [0u8; 32];
            getrandom::fill(&mut seed).unwrap();
            Self {
                dir: tempfile::tempdir().unwrap(),
                signing: SigningKey::from_bytes(&seed),
            }
        }

        fn config(&self, enforce: bool) -> Config {
            Config {
                enforce,
                public_key: Some(self.signing.verifying_key()),
                build_date: "2026-10-02".into(),
                trial_file: Some(self.dir.path().join("data/trial")),
                store: KeyStore::File(self.dir.path().join("config/license.key")),
            }
        }

        fn licensing(&self, enforce: bool) -> Licensing {
            Licensing::new(self.config(enforce))
        }

        /// A licensed build made today, for the old trial file, whose start
        /// may not be after the build.
        fn built_today(&self) -> Licensing {
            Licensing::new(Config {
                build_date: date::from_days(today()),
                ..self.config(true)
            })
        }

        fn path(&self, rel: &str) -> PathBuf {
            self.dir.path().join(rel)
        }

        fn signed(&self, plan: Plan, issued: &str, until: &str) -> String {
            sign(
                &License {
                    id: format!("lic_{}_{until}", plan.name()),
                    email: "a@b.c".into(),
                    plan,
                    issued: issued.into(),
                    updates_until: until.into(),
                },
                &self.signing,
            )
        }

        fn key(&self, until: &str) -> String {
            self.signed(Plan::Desktop, "2026-01-01", until)
        }

        /// A trial key from convt.app started `start` days from today.
        fn trial(&self, start: i64) -> String {
            self.signed(
                Plan::Trial,
                &date::from_days(today() + start),
                &date::from_days(today() + start + TRIAL_DAYS - 1),
            )
        }

        fn old_trial_file(&self, start: &str) {
            write_private(&self.path("data/trial"), start).unwrap();
        }
    }

    #[test]
    fn source_builds_are_unrestricted() {
        let f = Fixture::new();
        let mut l = f.licensing(false);
        assert_eq!(l.state(), State::Unrestricted);
        assert!(l.begin_conversion().is_ok());
        assert!(!f.path("data/trial").exists());
        assert!(!f.path("config/last-seen").exists());
    }

    #[test]
    fn converting_never_starts_a_trial() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        assert_eq!(l.state(), State::NoTrial);
        assert_eq!(l.begin_conversion(), Err(Blocked::State(State::NoTrial)));
        assert!(!f.path("data/trial").exists());
        let reason = State::NoTrial.blocked_reason().unwrap();
        assert!(reason.contains("signing in to convt.app"), "{reason}");
        assert!(reason.contains(BUY_URL));
    }

    #[test]
    fn a_trial_key_counts_by_todays_date_not_the_build() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        let started = today();
        // The fixture's build is older than the trial; that doesn't matter.
        assert!(matches!(
            l.offer_key(&f.trial(0)).unwrap(),
            Renewed::Stored(k) if k.plan == Plan::Trial
        ));
        // Whole days from now, so the latest time seen is never ahead.
        let at = |day: i64| now() + (day - started) * DAY;
        assert_eq!(
            l.state_at(at(started)),
            State::Trial {
                days_left: 7,
                last_day: date::from_days(started + 6)
            }
        );
        assert!(l.begin_conversion().is_ok());
        assert!(matches!(
            l.state_at(at(started + 6)),
            State::Trial { days_left: 1, .. }
        ));
        assert_eq!(l.state_at(at(started + 7)), State::TrialEnded);
        assert_eq!(
            State::Trial {
                days_left: 1,
                last_day: String::new()
            }
            .summary(),
            "Free trial: last day."
        );
        // A new process reads the key back; the trial is stored like a key.
        assert!(matches!(
            f.licensing(true).state(),
            State::Trial { days_left: 7, .. }
        ));
        // A trial that ended yesterday blocks, whatever the build date.
        let mut ended = Licensing::new(Config {
            store: KeyStore::File(f.path("other/license.key")),
            ..f.config(true)
        });
        ended.offer_key(&f.trial(-7)).unwrap();
        assert_eq!(
            ended.begin_conversion(),
            Err(Blocked::State(State::TrialEnded))
        );
    }

    #[test]
    fn a_trial_key_cannot_be_pasted() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        assert!(matches!(
            l.activate(&f.trial(0)),
            Err(ActivateError::TrialKey)
        ));
        assert_eq!(l.state(), State::NoTrial);
    }

    #[test]
    fn an_old_trial_file_is_carried_over_once() {
        let f = Fixture::new();
        // Started three days ago by an older build.
        f.old_trial_file(&date::from_days(today() - 3));
        let l = f.built_today();
        assert_eq!(
            l.state(),
            State::Trial {
                days_left: 4,
                last_day: date::from_days(today() + 3)
            }
        );
        assert!(!f.path("data/trial").exists(), "the file is gone");
        assert_eq!(
            std::fs::read_to_string(f.path("config/legacy-trial")).unwrap(),
            format!("{}\n", date::from_days(today() + 3))
        );
        // A new file, even one with a later start, is never read again.
        f.old_trial_file(&date::from_days(today()));
        let again = f.built_today();
        assert!(matches!(again.state(), State::Trial { days_left: 4, .. }));
        assert!(f.path("data/trial").exists(), "not even read");
    }

    #[test]
    fn an_old_trial_file_counts_by_today_not_the_build_date() {
        let f = Fixture::new();
        // The fixture's build is from 2026-10-02; an older release may have
        // started the trial after that, and it still counts.
        let start = date::from_days(today() - 1);
        f.old_trial_file(&start);
        assert!(matches!(
            f.licensing(true).state(),
            State::Trial { days_left: 6, .. }
        ));
        // A start in the future counts for nothing, and is never read again.
        let g = Fixture::new();
        g.old_trial_file(&date::from_days(today() + 1));
        assert_eq!(g.licensing(true).state(), State::NoTrial);
        assert!(!g.path("data/trial").exists());
        assert_eq!(
            std::fs::read_to_string(g.path("config/legacy-trial")).unwrap(),
            "none\n"
        );
        // An ended old trial carries over as ended.
        let h = Fixture::new();
        h.old_trial_file("2000-01-01");
        assert_eq!(h.licensing(true).state(), State::TrialEnded);
        // No file: nothing is recorded, so nothing can hide a later read.
        let n = Fixture::new();
        assert_eq!(n.licensing(true).state(), State::NoTrial);
        assert!(!n.path("config/legacy-trial").exists());
    }

    #[test]
    fn a_kept_key_stored_by_another_client_counts_here_too() {
        let f = Fixture::new();
        let mut app = f.licensing(true);
        assert_eq!(app.state(), State::NoTrial);
        // The CLI stores a key while the app is open.
        f.licensing(true).activate(&f.key("2027-10-02")).unwrap();
        // Renewal offers the same key: kept, and this client now sees it.
        assert!(matches!(
            app.offer_key(&f.key("2027-10-02")),
            Ok(Renewed::Kept(_))
        ));
        assert!(matches!(app.state(), State::Licensed(_)));
        // The same for a trial key.
        let g = Fixture::new();
        let mut app = g.licensing(true);
        g.licensing(true).offer_key(&g.trial(0)).unwrap();
        assert!(matches!(app.offer_key(&g.trial(0)), Ok(Renewed::Kept(_))));
        assert!(matches!(app.state(), State::Trial { .. }));
    }

    #[test]
    fn renewal_keeps_whatever_works_on_this_build() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        // A Desktop key this build (2026-10-02) has outgrown.
        l.activate(&f.key("2026-09-01")).unwrap();
        assert!(matches!(l.state(), State::NotCovered(_)));
        // A running trial runs this build, but the bought key stays stored
        // for the older builds it covers.
        assert!(matches!(l.offer_key(&f.trial(0)), Ok(Renewed::Stored(_))));
        assert!(matches!(l.state(), State::Trial { .. }));
        assert_eq!(
            std::fs::read_to_string(f.path("config/license.key"))
                .unwrap()
                .trim(),
            f.key("2026-09-01")
        );
        // When the trial ends, the bought key says what it still covers.
        assert!(matches!(l.state_at(now() + 30 * DAY), State::NotCovered(_)));
        assert!(matches!(
            l.offer_key(&f.key("2026-09-01")),
            Ok(Renewed::Kept(_))
        ));
        // A covering paid key beats the trial.
        assert!(matches!(
            l.offer_key(&f.key("2027-10-02")),
            Ok(Renewed::Stored(_))
        ));
        assert!(matches!(l.offer_key(&f.trial(0)), Ok(Renewed::Kept(_))));
        assert!(matches!(l.state(), State::Licensed(_)));
        // Once a trial has ended, an outgrown paid key is the better one.
        let g = Fixture::new();
        let mut m = g.licensing(true);
        m.offer_key(&g.trial(-10)).unwrap();
        assert!(matches!(
            m.offer_key(&g.key("2026-09-01")),
            Ok(Renewed::Stored(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_trial_file_is_carried_over_once_it_reads() {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new();
        let start = date::from_days(today() - 2);
        f.old_trial_file(&start);
        let file = f.path("data/trial");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read_to_string(&file).is_ok() {
            return; // Running as root: permissions don't stop the read.
        }
        assert_eq!(f.built_today().state(), State::NoTrial);
        assert!(
            !f.path("config/legacy-trial").exists(),
            "nothing recorded yet"
        );
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            f.built_today().state(),
            State::Trial { days_left: 5, .. }
        ));
    }

    #[test]
    fn deleting_the_trial_file_grants_nothing() {
        let f = Fixture::new();
        f.old_trial_file("2000-01-01");
        assert_eq!(f.licensing(true).state(), State::TrialEnded);
        let _ = std::fs::remove_file(f.path("data/trial"));
        assert_eq!(f.licensing(true).state(), State::TrialEnded);
        // Never having had one is no trial either.
        assert_eq!(Fixture::new().licensing(true).state(), State::NoTrial);
    }

    #[test]
    fn a_clock_set_back_stops_the_trial_until_convt_app_confirms_the_time() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        l.offer_key(&f.trial(0)).unwrap();
        let now = now();
        assert!(matches!(l.state_at(now), State::Trial { .. }));
        // Within the tolerance is fine.
        assert!(matches!(
            l.state_at(now - CLOCK_TOLERANCE + 60),
            State::Trial { .. }
        ));
        // Convt ran two days from now, then the clock went back to today.
        l.record_server_time(now + 2 * DAY);
        assert_eq!(l.state_at(now), State::NeedsCheck);
        assert_eq!(f.licensing(true).state(), State::NeedsCheck);
        assert_eq!(l.begin_conversion(), Err(Blocked::State(State::NeedsCheck)));
        assert!(
            State::NeedsCheck
                .blocked_reason()
                .unwrap()
                .contains("clock")
        );
        // convt.app says it's today after all: the trial counts again.
        l.record_server_time(now);
        assert!(matches!(l.state_at(now), State::Trial { days_left: 7, .. }));
        assert!(matches!(f.licensing(true).state(), State::Trial { .. }));
        // convt.app says it's three days later than this clock: still blocked.
        l.record_server_time(now + 3 * DAY);
        assert_eq!(l.state_at(now), State::NeedsCheck);
        // A trial that had already ended by the latest time seen says so.
        l.record_server_time(now + 30 * DAY);
        assert_eq!(l.state_at(now), State::TrialEnded);
        // A paid key never looks at the clock.
        l.offer_key(&f.key("2027-10-02")).unwrap();
        assert!(matches!(l.state_at(now), State::Licensed(_)));
    }

    #[test]
    fn a_trial_key_issued_after_today_means_the_clock_is_behind() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        l.offer_key(&f.trial(5)).unwrap();
        assert_eq!(l.state(), State::NeedsCheck);
    }

    #[test]
    fn a_clock_a_day_behind_gets_no_extra_trial_day() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        // convt.app issued the key tomorrow by this clock: within the slack,
        // but still seven days, not eight.
        l.offer_key(&f.trial(1)).unwrap();
        assert!(matches!(l.state(), State::Trial { days_left: 7, .. }));
        // The trial reply carries convt.app's time, a day ahead: paused.
        l.record_server_time(now() + DAY);
        assert_eq!(l.state(), State::NeedsCheck);
    }

    #[test]
    fn the_time_seen_is_written_at_most_hourly() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        let file = f.path("config/last-seen");
        let first: i64 = std::fs::read_to_string(&file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        l.note_time(first + 600);
        assert_eq!(l.last_seen, Some(first));
        l.note_time(first + 3600);
        assert_eq!(l.last_seen, Some(first + 3600));
        // An earlier time never lowers it.
        l.note_time(first - 2 * DAY);
        assert_eq!(l.last_seen, Some(first + 3600));
    }

    #[test]
    fn keys_from_convt_app_never_shorten_what_this_machine_runs() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        // A trial first.
        assert!(matches!(l.offer_key(&f.trial(0)), Ok(Renewed::Stored(_))));
        assert!(matches!(l.state(), State::Trial { .. }));
        // The same trial again is kept.
        assert!(matches!(l.offer_key(&f.trial(0)), Ok(Renewed::Kept(_))));
        // A paid key replaces the trial, even one that runs for less time.
        assert!(matches!(
            l.offer_key(&f.signed(Plan::Pro, "2026-09-01", "2026-10-05")),
            Ok(Renewed::Stored(k)) if k.plan == Plan::Pro
        ));
        assert!(matches!(l.state(), State::Licensed(_)));
        // A trial never replaces a paid key, even a longer one: it is kept
        // apart, and the paid key still decides.
        assert!(matches!(
            l.offer_key(&f.signed(Plan::Trial, "2026-10-01", "2099-01-01")),
            Ok(Renewed::Stored(k)) if k.plan == Plan::Trial
        ));
        assert!(matches!(l.state(), State::Licensed(k) if k.plan == Plan::Pro));
        assert!(
            std::fs::read_to_string(f.path("config/license.key"))
                .unwrap()
                .starts_with(&f.signed(Plan::Pro, "2026-09-01", "2026-10-05")[..20])
        );
        // A Desktop key with a later window replaces Pro, and stays ahead.
        assert!(matches!(
            l.offer_key(&f.key("2027-10-01")),
            Ok(Renewed::Stored(k)) if k.plan == Plan::Desktop
        ));
        assert!(matches!(
            l.offer_key(&f.signed(Plan::Pro, "2026-09-01", "2027-01-01")),
            Ok(Renewed::Kept(k)) if k.plan == Plan::Desktop
        ));
        assert!(matches!(l.state(), State::Licensed(k) if k.updates_until == "2027-10-01"));
    }

    #[test]
    fn config_from_a_packaged_build_ignores_the_overrides() {
        let env = |name: &str| match name {
            "CONVT_LICENSE_STORE" => Some("file".to_string()),
            "CONVT_LICENSE_ENFORCE" => Some("1".to_string()),
            _ => None,
        };
        let given = (
            Some(PathBuf::from("/override/config")),
            Some(PathBuf::from("/override/data")),
        );
        let platform = (
            Some(PathBuf::from("/home/u/.config/convt")),
            Some(PathBuf::from("/home/u/.local/share/convt")),
        );
        let packaged = Config::resolve(true, &env, given.clone(), platform.clone());
        assert!(packaged.enforce);
        assert_eq!(
            packaged.store,
            KeyStore::Keyring {
                fallback: Some(PathBuf::from("/home/u/.config/convt/license.key"))
            }
        );
        assert_eq!(
            packaged.trial_file,
            Some(PathBuf::from("/home/u/.local/share/convt/trial"))
        );
        // A build from source takes them, so tests and development can.
        let source = Config::resolve(false, &env, given, platform);
        assert!(source.enforce);
        assert_eq!(
            source.store,
            KeyStore::File(PathBuf::from("/override/config/license.key"))
        );
        assert_eq!(
            source.trial_file,
            Some(PathBuf::from("/override/data/trial"))
        );
        let off = Config::resolve(false, &|_| None, (None, None), (None, None));
        assert!(!off.enforce);
        assert_eq!(off.store, KeyStore::Keyring { fallback: None });
    }

    #[test]
    fn the_device_hash_is_a_stable_salted_hex_digest() {
        let a = device_hash_of("machine:abc");
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
        assert_eq!(a, device_hash_of("machine:abc"));
        assert_ne!(a, device_hash_of("machine:abd"));
        assert_ne!(a, device_hash_of("install:abc"));
        // HMAC-SHA256 under the salt, not a plain hash of the id.
        let mut mac = Hmac::<Sha256>::new_from_slice(b"convt-device-hash-v1").unwrap();
        mac.update(b"machine:abc");
        let expected: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(a, expected);

        let f = Fixture::new();
        let l = f.licensing(true);
        assert_eq!(
            l.device_hash_from(Some(" abc\n".into())).unwrap(),
            device_hash_of("machine:abc")
        );
        // Without a machine id, a random install id is made once and kept.
        let first = l.device_hash_from(None).unwrap();
        assert_eq!(
            first,
            f.licensing(true)
                .device_hash_from(Some(String::new()))
                .unwrap()
        );
        let id = std::fs::read_to_string(f.path("config/install-id")).unwrap();
        assert_eq!(first, device_hash_of(&format!("install:{}", id.trim())));
        assert_ne!(
            first,
            Fixture::new()
                .licensing(true)
                .device_hash_from(None)
                .unwrap()
        );
        // This machine's real hash has the same shape.
        let real = l.device_hash().unwrap();
        assert_eq!(real.len(), 64);
    }

    #[test]
    fn activation_stores_the_key_privately() {
        let f = Fixture::new();
        f.old_trial_file("2000-01-01");
        let mut l = f.licensing(true);
        assert!(matches!(l.activate("nope"), Err(ActivateError::Invalid)));
        let license = l.activate(&format!("  {}\n", f.key("2027-10-02"))).unwrap();
        assert_eq!(l.state(), State::Licensed(license.clone()));
        assert!(l.begin_conversion().is_ok());

        let file = f.path("config/license.key");
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
        write_private(&f.path("config/license.key"), &other.key("2099-01-01")).unwrap();
        assert_eq!(f.licensing(true).state(), State::NoTrial);
    }

    #[test]
    fn a_newer_build_than_the_license_covers() {
        let f = Fixture::new();
        let mut l = f.licensing(true);
        let license = l.activate(&f.key("2026-10-01")).unwrap();
        let state = l.state();
        assert_eq!(state, State::NotCovered(license));
        assert!(state.blocked_reason().unwrap().contains("2026-10-01"));
        assert!(l.begin_conversion().is_err());
        // The build made on the last covered day is fine.
        let mut l = f.licensing(true);
        l.activate(&f.key("2026-10-02")).unwrap();
        assert!(matches!(l.state(), State::Licensed(_)));
    }

    #[test]
    fn a_build_without_a_key_cannot_activate() {
        let f = Fixture::new();
        let mut l = Licensing::new(Config {
            public_key: None,
            ..f.config(true)
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
        let fallback = f.path("config/license.key");
        let keyring = || Config {
            store: KeyStore::Keyring {
                fallback: Some(fallback.clone()),
            },
            ..f.config(true)
        };
        let mut l = Licensing::new(keyring());
        let license = l.activate(&f.key("2027-10-02")).unwrap();
        assert!(!fallback.exists(), "the key went to the credential store");
        let fresh = Licensing::new(keyring());
        assert_eq!(fresh.state(), State::Licensed(license));
        // A key saved to the file while the store was away wins over the
        // older one still in the store.
        let newer = f.key("2028-10-02");
        write_private(&fallback, &newer).unwrap();
        assert!(matches!(fresh.config.store.load(LICENSE), Some(k) if k == newer));
        std::fs::remove_file(&fallback).unwrap();
        l.deactivate().unwrap();
        assert!(keyring_entry(LICENSE).unwrap().get_password().is_err());
        for slot in [LAST_SEEN, LEGACY_TRIAL, INSTALL] {
            let _ = keyring_entry(slot).and_then(|e| e.delete_credential());
        }
    }

    #[test]
    fn a_key_removed_elsewhere_stops_counting() {
        let f = Fixture::new();
        f.old_trial_file("2000-01-01");
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
        f.signed(Plan::Pro, "2026-09-01", until)
    }

    #[test]
    fn renewal_stores_only_a_key_that_covers_more() {
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
        // Desktop keys come through renewal too.
        assert!(matches!(
            l.offer_key(&f.key("2027-10-01")),
            Ok(Renewed::Stored(k)) if k.plan == Plan::Desktop
        ));
        // Keys that don't verify are refused and change nothing.
        let other = Fixture::new();
        assert!(matches!(
            l.offer_key(&pro_key(&other, "2099-01-01")),
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
        let file = f.path("config/account.json");
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
            ..f.config(true)
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
        write_private(&f.path("config/license.key"), REMOVED).unwrap();
        assert_eq!(f.licensing(true).state(), State::NoTrial);
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_open_key_file_becomes_private() {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new();
        let file = f.path("config/license.key");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "old").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        f.licensing(true).activate(&f.key("2027-10-02")).unwrap();
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(!file.with_extension("tmp").exists());
    }
}
