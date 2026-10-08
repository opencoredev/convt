//! Desktop sign-in and Pro renewal.
//!
//! Sign-in opens convt.app/device in the browser. The account supplies the
//! online Pro trial and renews paid Pro keys. Sign-in opens convt.app/device in the
//! browser with a fresh [`Pending`] flow; the site answers with a
//! `convt://auth` link, which counts only while that flow is pending (see
//! `convt_license::account`). The device token goes to the credential store
//! next to the license key.
//!
//! While signed in, the app asks convt.app for the account's current Pro key
//! at launch, at most once a UTC day, and when the user clicks Refresh
//! license. Apart from the update check (`crate::update`), that is the only
//! network call the app makes on its own. A key
//! that comes back is stored without asking, but only when it covers newer
//! builds than the stored one ([`Licensing::offer_key`]). Offline, or when
//! the subscription lapsed, the stored key stays.
//!
//! [`Licensing::offer_key`]: convt_license::client::Licensing::offer_key

use std::sync::Arc;
use std::time::Instant;

use convt_license::account::{
    Access as RemoteAccess, Api, ApiError, LicenseReply, Pending, Session,
};
use convt_license::client::{ActivateError, Renewed, today};
use convt_license::date;
use gpui_kit::{Context, Task};

use crate::model::AppState;
use crate::request::AuthReply;
use crate::settings::TrialCache;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn trial_poll_delay(elapsed_secs: u64, rate_limited: bool) -> std::time::Duration {
    if rate_limited {
        std::time::Duration::from_secs(120)
    } else if elapsed_secs < 120 {
        std::time::Duration::from_secs(10)
    } else {
        std::time::Duration::from_secs(60)
    }
}

#[cfg(test)]
mod poll_tests {
    use super::trial_poll_delay;
    use std::time::Duration;

    #[test]
    fn trial_poll_schedule_stays_below_hourly_limit() {
        assert_eq!(trial_poll_delay(0, false), Duration::from_secs(10));
        assert_eq!(trial_poll_delay(119, false), Duration::from_secs(10));
        assert_eq!(trial_poll_delay(120, false), Duration::from_secs(60));
        assert_eq!(trial_poll_delay(900, false), Duration::from_secs(60));
        assert_eq!(trial_poll_delay(120, true), Duration::from_secs(120));
    }
}

/// Where a sign-in stands. Whether the app is signed in is
/// [`Account::session`]; this is the flow on top of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignIn {
    Idle,
    /// The browser is open on convt.app/device.
    Waiting,
    /// The link came back; the app is trading the code for a token.
    Finishing,
    Failed(String),
}

/// Which sign-in button was pressed. convt.app/device goes straight to that
/// way of signing in instead of showing its chooser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Google,
    Email,
}

/// What the signed-in account allows on this computer, as of the last
/// license refresh. `None` on [`Account::access`] until one has answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    /// Paid Pro; its key is stored like any other.
    Pro,
    /// A Pro trial through Polar. Conversions work through this UTC day
    /// (`YYYY-MM-DD`). No key is signed for a trial.
    Trial { ends_on: String },
    /// No subscription, and the account can still start its one trial:
    /// [`AppState::start_trial`] opens this checkout.
    CanStartTrial { checkout_url: String },
    /// No Pro and no trial left: buying is the way on.
    Lapsed,
}

/// Where the last license refresh stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refresh {
    Idle,
    Running,
    Done(String),
    Failed(String),
}

pub struct Account {
    /// convt.app, or the dev server a build from source points at.
    url: String,
    api: Arc<dyn Api>,
    pending: Option<Pending>,
    /// The page the browser was sent to, for "Open the page again".
    page: Option<String>,
    pub session: Option<Session>,
    pub sign_in: SignIn,
    pub refresh: Refresh,
    /// Something to tell the user that isn't the state of a flow, such as a
    /// sign-in link the app ignored.
    pub notice: Option<String>,
    /// What the account allows, from the last refresh.
    pub access: Option<Access>,
    /// The trial checkout is open in the browser; refreshes repeat until it
    /// comes back as a trial or Pro.
    pub awaiting_trial: bool,
    trial_poll_started: Option<Instant>,
    _sign_in_task: Option<Task<()>>,
    _refresh_task: Option<Task<()>>,
}

impl Account {
    pub fn new(url: String, api: Arc<dyn Api>, session: Option<Session>) -> Self {
        Self {
            url,
            api,
            pending: None,
            page: None,
            session,
            sign_in: SignIn::Idle,
            refresh: Refresh::Idle,
            notice: None,
            access: None,
            awaiting_trial: false,
            trial_poll_started: None,
            _sign_in_task: None,
            _refresh_task: None,
        }
    }

    pub fn email(&self) -> Option<&str> {
        self.session.as_ref().map(|s| s.email.as_str())
    }

    /// The site this build signs in to.
    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// The client for convt.app, which cloud jobs ask for credentials.
    pub(crate) fn api(&self) -> Arc<dyn Api> {
        self.api.clone()
    }
}

/// Runs `work` on its own thread, then `done` on the app state with its
/// result. Network calls never run on the UI thread.
pub(crate) fn background<T: Send + 'static>(
    cx: &mut Context<AppState>,
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(&mut AppState, T, &mut Context<AppState>) + 'static,
) -> Task<()> {
    let (tx, rx) = futures::channel::oneshot::channel();
    std::thread::Builder::new()
        .name("convt-account".into())
        .spawn(move || drop(tx.send(work())))
        .expect("spawn the account thread");
    cx.spawn(async move |this, cx| {
        if let Ok(value) = rx.await {
            let _ = this.update(cx, |state, cx| done(state, value, cx));
        }
    })
}

/// This computer's name, as the dashboard lists it.
fn device_name() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        // SAFETY: the buffer is valid for its length, and gethostname writes
        // at most that many bytes.
        let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0;
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let name = String::from_utf8_lossy(&buf[..end]);
        let name = name.trim().trim_end_matches(".local");
        if ok && !name.is_empty() {
            return name.to_string();
        }
    }
    #[cfg(windows)]
    if let Ok(name) = std::env::var("COMPUTERNAME")
        && !name.trim().is_empty()
    {
        return name.trim().to_string();
    }
    "This computer".into()
}

fn os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    }
}

impl AppState {
    /// Opens convt.app/device in the browser with a new sign-in flow. A flow
    /// already waiting is replaced, so only the newest page's link counts.
    pub fn start_sign_in(&mut self, cx: &mut Context<Self>) {
        self.start_sign_in_page(None, cx);
    }

    fn start_sign_in_page(&mut self, provider: Option<Provider>, cx: &mut Context<Self>) {
        let account = &mut self.account;
        // A link already came back and its code is being traded; a second
        // flow now would race the first one's result.
        if account.sign_in == SignIn::Finishing {
            return;
        }
        account.notice = None;
        let pending = match Pending::new() {
            Ok(pending) => pending,
            Err(e) => {
                account.sign_in = SignIn::Failed(format!("Sign-in couldn't start: {e}"));
                cx.notify();
                return;
            }
        };
        let mut page = pending.url(&account.url, &device_name(), os_name(), VERSION);
        if let Some(provider) = provider {
            page.push_str("&provider=");
            page.push_str(match provider {
                Provider::Google => "google",
                Provider::Email => "email",
            });
        }
        account.pending = Some(pending);
        account.page = Some(page.clone());
        account.sign_in = SignIn::Waiting;
        cx.open_url(&page);
        cx.notify();
    }

    /// [`Self::start_sign_in`], with the page told which button was pressed.
    // CNV-70 adds `&provider=` to the page.
    pub fn start_sign_in_with(&mut self, provider: Provider, cx: &mut Context<Self>) {
        self.start_sign_in_page(Some(provider), cx);
    }

    /// Opens the account's trial checkout and keeps refreshing until the
    /// trial shows up. Does nothing unless [`Access::CanStartTrial`].
    // CNV-70 adds the polling.
    pub fn start_trial(&mut self, cx: &mut Context<Self>) {
        if let Some(Access::CanStartTrial { checkout_url }) = &self.account.access {
            cx.open_url(checkout_url);
            self.account.awaiting_trial = true;
            self.account.trial_poll_started = Some(Instant::now());
            self.refresh_license(cx);
            cx.notify();
        }
    }

    /// Opens the waiting flow's page again, for a browser tab that was closed.
    pub fn reopen_sign_in(&mut self, cx: &mut Context<Self>) {
        if let (SignIn::Waiting, Some(page)) = (&self.account.sign_in, &self.account.page) {
            cx.open_url(page);
        }
    }

    /// Makes the waiting sign-in look `by` older, for tests of the timeout.
    #[cfg(test)]
    pub fn age_sign_in(&mut self, by: std::time::Duration) {
        self.account.pending = self.account.pending.take().map(|p| p.started_earlier(by));
    }

    pub fn cancel_sign_in(&mut self, cx: &mut Context<Self>) {
        if self.account.sign_in == SignIn::Waiting {
            self.account.pending = None;
            self.account.page = None;
            self.account.sign_in = SignIn::Idle;
            cx.notify();
        }
    }

    /// Handles a `convt://auth` link. It counts only while this app waits
    /// for the browser with a flow whose state matches; the flow is used up
    /// either way, so the same link can't sign in twice. Anything else is
    /// dropped without a network call, and the user is told why.
    pub fn finish_sign_in(
        &mut self,
        reply: AuthReply,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let now = Instant::now();
        let account = &mut self.account;
        let waiting = account.sign_in == SignIn::Waiting;
        let pending = account
            .pending
            .take_if(|p| waiting && (p.accepts(&reply.state, now) || p.expired(now)));
        let Some(pending) = pending else {
            let message = "Ignored a sign-in link that convt didn't start.";
            tracing::warn!("ignored a convt://auth link with no matching sign-in");
            account.notice = Some(message.into());
            cx.notify();
            return Err(message.into());
        };
        account.page = None;
        account.notice = None;
        if pending.expired(now) {
            account.sign_in =
                SignIn::Failed("The sign-in took too long. Start it again from convt.".into());
            cx.notify();
            return Err("expired".into());
        }
        let code = match reply.code {
            Ok(code) => code,
            Err(_) => {
                account.sign_in = SignIn::Failed("Sign-in was cancelled in the browser.".into());
                cx.notify();
                return Err("cancelled".into());
            }
        };
        account.sign_in = SignIn::Finishing;
        let api = account.api.clone();
        let verifier = pending.verifier().to_string();
        account._sign_in_task = Some(background(
            cx,
            move || api.exchange(&code, &verifier),
            |state, result, cx| state.signed_in(result, cx),
        ));
        cx.notify();
        Ok(())
    }

    fn signed_in(&mut self, result: Result<Session, ApiError>, cx: &mut Context<Self>) {
        match result.map_err(|e| e.to_string()).and_then(|session| {
            self.licensing
                .save_session(&session)
                .map(|()| session)
                .map_err(|e| format!("Signed in, but the sign-in couldn't be saved: {e}"))
        }) {
            Ok(session) => {
                self.account.session = Some(session);
                self.account.sign_in = SignIn::Idle;
                // Signing in was the user's action; fetch the Pro key now.
                self.refresh_license(cx);
            }
            Err(e) => self.account.sign_in = SignIn::Failed(e),
        }
        cx.notify();
    }

    /// Signs this computer out: forgets the token here and revokes it on
    /// convt.app. The license key stays.
    pub fn sign_out(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.account.session.take() else {
            return;
        };
        if let Err(e) = self.licensing.clear_session() {
            self.account.notice = Some(format!("The sign-in couldn't be removed: {e}"));
            self.account.session = Some(session);
            cx.notify();
            return;
        }
        self.account.sign_in = SignIn::Idle;
        self.account.refresh = Refresh::Idle;
        self.account.notice = Some("Signed out. The license on this computer stays.".into());
        self.account._refresh_task = None;
        self.account.awaiting_trial = false;
        self.account.trial_poll_started = None;
        self.update_settings(|s| s.trial_cache = None, cx);
        self.licensing.set_account_trial(None);
        // Best effort: the dashboard can sign this computer out too.
        let api = self.account.api.clone();
        self.account._sign_in_task = Some(background(
            cx,
            move || api.sign_out(&session.token),
            |_, result, _| {
                if let Err(e) = result {
                    tracing::info!(error = %e, "convt.app didn't confirm the sign-out");
                }
            },
        ));
        cx.notify();
    }

    /// The launch check: asks for the current Pro key if signed in and not
    /// yet asked today (UTC).
    pub fn renew_on_launch(&mut self, cx: &mut Context<Self>) {
        let today = date::from_days(today());
        if self.account.session.is_some()
            && self.settings.license_checked.as_deref() != Some(today.as_str())
        {
            self.refresh_license(cx);
        }
    }

    /// Asks convt.app for the current Pro key. Does nothing while signed
    /// out or while a refresh runs.
    pub fn refresh_license(&mut self, cx: &mut Context<Self>) {
        let Some(token) = self.account.session.as_ref().map(|s| s.token.clone()) else {
            return;
        };
        if self.account.refresh == Refresh::Running {
            return;
        }
        let today = date::from_days(today());
        self.update_settings(|s| s.license_checked = Some(today), cx);
        self.account.notice = None;
        self.account.refresh = Refresh::Running;
        let api = self.account.api.clone();
        self.account._refresh_task = Some(background(
            cx,
            move || api.current_key(&token, VERSION),
            |state, result, cx| state.renewed(result, cx),
        ));
        cx.notify();
    }

    fn renewed(&mut self, result: Result<LicenseReply, ApiError>, cx: &mut Context<Self>) {
        let kept = "The license on this computer stays as it is.";
        let rate_limited = matches!(&result, Err(ApiError::RateLimited));
        self.account.refresh = match result {
            Ok(reply) => {
                let remote_access = reply.access.clone();
                self.account.access = remote_access.clone().map(|access| match access {
                    RemoteAccess::Pro => Access::Pro,
                    RemoteAccess::Trial { ends_on, .. } => Access::Trial { ends_on },
                    RemoteAccess::CanStartTrial { checkout_url } => {
                        Access::CanStartTrial { checkout_url }
                    }
                    RemoteAccess::Lapsed => Access::Lapsed,
                });
                match &self.account.access {
                    Some(Access::Trial { ends_on }) => {
                        let ends_on = ends_on.clone();
                        let ends_at = match &remote_access {
                            Some(RemoteAccess::Trial { ends_at, .. }) => ends_at.clone(),
                            _ => None,
                        };
                        self.update_settings(
                            |s| {
                                s.trial_cache = ends_at.clone().map(|ends_at| TrialCache {
                                    ends_at,
                                    fetched_on: date::from_days(today()),
                                })
                            },
                            cx,
                        );
                        self.licensing
                            .set_account_trial_exact(Some(ends_on), ends_at);
                    }
                    Some(Access::Pro | Access::CanStartTrial { .. } | Access::Lapsed) | None => {
                        self.update_settings(|s| s.trial_cache = None, cx);
                        self.licensing.set_account_trial(None);
                    }
                }
                if self
                    .account
                    .access
                    .as_ref()
                    .is_some_and(|a| matches!(a, Access::Trial { .. } | Access::Pro))
                {
                    self.account.awaiting_trial = false;
                    self.account.trial_poll_started = None;
                }
                match reply.key {
                    Some(key) => match self.licensing.offer_key(&key) {
                        Ok(Renewed::Stored(l)) => Refresh::Done(format!(
                            "Got your Pro key, with updates until {}.",
                            l.updates_until
                        )),
                        Ok(Renewed::Kept(l)) => Refresh::Done(format!(
                            "Your license is up to date, with updates until {}.",
                            l.updates_until
                        )),
                        Err(ActivateError::Store(e)) => {
                            Refresh::Failed(format!("The new key couldn't be saved: {e}"))
                        }
                        Err(_) => Refresh::Failed(format!(
                            "convt.app sent a key this build doesn't accept. {kept}"
                        )),
                    },
                    None => {
                        Refresh::Done(format!("This account has no Pro key on convt.app. {kept}"))
                    }
                }
            }
            Err(ApiError::SignedOut) => {
                // Revoked from the dashboard: forget the token here too.
                if let Err(e) = self.licensing.clear_session() {
                    tracing::warn!(error = %e, "could not forget a revoked sign-in");
                }
                self.account.session = None;
                self.account.access = None;
                self.update_settings(|s| s.trial_cache = None, cx);
                self.licensing.set_account_trial(None);
                Refresh::Failed(format!(
                    "This computer was signed out of convt.app. Sign in again to keep Pro renewing. {kept}"
                ))
            }
            Err(e) => Refresh::Failed(format!("Couldn't refresh the license. {e} {kept}")),
        };
        if self.account.awaiting_trial
            && self
                .account
                .trial_poll_started
                .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(15 * 60))
            && matches!(self.account.access, Some(Access::CanStartTrial { .. }))
        {
            if self.account.session.is_some() {
                let elapsed = self
                    .account
                    .trial_poll_started
                    .map_or(0, |t| t.elapsed().as_secs());
                let delay = trial_poll_delay(elapsed, rate_limited);
                self.account._refresh_task = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = this.update(cx, |state, cx| {
                        if state.account.awaiting_trial && state.account.refresh != Refresh::Running
                        {
                            state.refresh_license(cx);
                        }
                    });
                }));
            }
        } else if self
            .account
            .trial_poll_started
            .is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(15 * 60))
        {
            self.account.awaiting_trial = false;
            self.account.trial_poll_started = None;
        }
        self.license = self.licensing.state();
        // A renewed key may cover an update that needed renewing.
        self.reselect_update();
        cx.notify();
    }
}
