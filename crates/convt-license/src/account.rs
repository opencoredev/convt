//! Desktop sign-in and Pro renewal: the only part of this crate that talks to
//! the network, and only when a client asks it to.
//!
//! Sign-in runs in the browser. The app makes a [`Pending`] flow (a random
//! `state` and a PKCE verifier), opens convt.app/device with the state and the
//! verifier's hash, and the user approves there. The site answers with
//! `convt://auth?state=...&code=...`. The app accepts the link only while it
//! holds a pending flow with that state, consumes the flow, and trades the
//! one-time code plus the verifier for a device token ([`Api::exchange`]). A
//! link the app didn't ask for, or one it already used, has no pending flow to
//! match and is dropped. Someone who intercepts the link gets a code that is
//! useless without the verifier, which never leaves this machine.
//!
//! Renewal ([`Api::current_key`]) sends the device token and gets back the
//! account's current Pro key, if it has one. The token can be revoked from the
//! dashboard; the server then answers 401 and the app signs out.
//!
//! A cloud conversion ([`Api::cloud_credential`]) sends the device token and
//! gets back the cloud API's address and a five-minute credential for it,
//! which convt.app issues only while the account has paid Pro.

use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Where packaged builds sign in and renew.
pub const ACCOUNT_URL: &str = "https://convt.app";

/// How long the app waits for the browser before a reply no longer counts.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// The site this build signs in to. Builds from source take
/// `CONVT_ACCOUNT_URL`, so the flow can run against a local dev server;
/// packaged builds always use [`ACCOUNT_URL`].
pub fn account_url() -> String {
    if !crate::ENFORCED
        && let Ok(url) = std::env::var("CONVT_ACCOUNT_URL")
        && !url.trim().is_empty()
    {
        return url.trim().trim_end_matches('/').to_string();
    }
    ACCOUNT_URL.to_string()
}

/// A sign-in the app started and hasn't finished.
pub struct Pending {
    state: String,
    verifier: String,
    /// When the flow expires. Kept instead of the start so tests can age a
    /// flow by up to [`SIGN_IN_TIMEOUT`]: on Windows an `Instant` counts from
    /// boot, and a runner may have been up for less than that.
    deadline: Instant,
}

impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pending").finish_non_exhaustive()
    }
}

impl Pending {
    /// A new flow with a fresh random state and verifier.
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            state: random_text()?,
            verifier: random_text()?,
            deadline: Instant::now() + SIGN_IN_TIMEOUT,
        })
    }

    /// The SHA-256 of the verifier, which the site stores with the code.
    pub fn challenge(&self) -> String {
        challenge_of(&self.verifier)
    }

    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// The page that asks the user to approve this computer.
    pub fn url(&self, base: &str, name: &str, os: &str, version: &str) -> String {
        format!(
            "{base}/device?state={}&challenge={}&name={}&os={}&version={}",
            self.state,
            self.challenge(),
            encode(name),
            encode(os),
            encode(version)
        )
    }

    /// Whether a reply carrying `state` belongs to this flow and came in time.
    /// The comparison takes the same time wherever the strings differ.
    pub fn accepts(&self, state: &str, now: Instant) -> bool {
        let same = state.len() == self.state.len()
            && state
                .bytes()
                .zip(self.state.bytes())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0;
        same && !self.expired(now)
    }

    /// Whether the user took longer than [`SIGN_IN_TIMEOUT`] to come back.
    pub fn expired(&self, now: Instant) -> bool {
        now >= self.deadline
    }

    /// The state, which travels in the page URL and comes back in the link.
    pub fn state(&self) -> &str {
        &self.state
    }

    /// The flow as if it had started `ago` earlier, for tests of the timeout.
    /// `ago` may be at most [`SIGN_IN_TIMEOUT`].
    pub fn started_earlier(mut self, ago: Duration) -> Self {
        assert!(
            ago <= SIGN_IN_TIMEOUT,
            "a flow can age by SIGN_IN_TIMEOUT at most"
        );
        self.deadline -= ago;
        self
    }
}

/// 32 random bytes as base64url, 43 characters.
fn random_text() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("no random numbers: {e}"))?;
    Ok(B64.encode(bytes))
}

pub fn challenge_of(verifier: &str) -> String {
    B64.encode(Sha256::digest(verifier.as_bytes()))
}

/// Percent-encodes a query value: everything but unreserved characters.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The signed-in account, kept in the credential store.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub email: String,
    pub token: String,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("email", &self.email)
            .finish_non_exhaustive()
    }
}

/// Why a call to convt.app failed, in words for the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    #[error("convt.app couldn't be reached. Check your internet connection and try again.")]
    Offline,
    /// The device token is unknown or was revoked from the dashboard.
    #[error("This computer was signed out of convt.app.")]
    SignedOut,
    /// The one-time code was used, expired or doesn't match the verifier.
    #[error("convt.app didn't accept that sign-in. Start it again from convt.")]
    Rejected,
    #[error("Too many tries. Wait a few minutes, then try again.")]
    RateLimited,
    #[error("convt.app had a problem (HTTP {0}). Try again later.")]
    Server(u16),
    #[error("convt.app sent an answer this version of convt doesn't understand.")]
    BadResponse,
    /// The account has no paid Pro, which cloud conversion needs.
    #[error("Cloud conversion needs an active paid Pro subscription.")]
    NeedsPro,
    #[error("Cloud conversion isn't available on convt.app yet.")]
    CloudOff,
}

/// Where to send a cloud conversion, and the short-lived credential for it.
#[derive(Clone, PartialEq, Eq)]
pub struct CloudCredential {
    pub base_url: String,
    pub token: String,
}

impl std::fmt::Debug for CloudCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudCredential")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

/// Whether `url` may carry files and credentials: HTTPS, or plain HTTP to a
/// loopback host, which only a local stack serves.
pub fn secure_url(url: &str) -> bool {
    url.starts_with("https://")
        || ["http://localhost:", "http://127.0.0.1:", "http://[::1]:"]
            .iter()
            .any(|p| url.starts_with(p))
}

/// The calls the app makes to convt.app. Tests script their own.
pub trait Api: Send + Sync {
    /// Trades a one-time code and its verifier for a device token.
    fn exchange(&self, code: &str, verifier: &str) -> Result<Session, ApiError>;
    /// The account's current Pro key, or `None` if it has none.
    fn current_key(&self, token: &str, version: &str) -> Result<LicenseReply, ApiError>;
    /// Revokes this device's token on the server.
    fn sign_out(&self, token: &str) -> Result<(), ApiError>;
    /// A five-minute credential for the cloud API, if the account has paid
    /// Pro. Fakes that don't script it answer like a site without the route.
    fn cloud_credential(&self, token: &str) -> Result<CloudCredential, ApiError> {
        let _ = token;
        Err(ApiError::Server(404))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Access {
    Pro,
    Trial { ends_on: String },
    CanStartTrial { checkout_url: String },
    Lapsed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenseReply {
    pub key: Option<String>,
    pub access: Option<Access>,
}

/// [`Api`] over HTTPS to [`account_url`].
pub struct Http {
    base: String,
    agent: ureq::Agent,
}

impl Http {
    /// Plain HTTP is allowed only for a loopback host, which only a build
    /// from source can be pointed at.
    pub fn new(base: &str) -> Self {
        let local = !base.starts_with("https://") && secure_url(base);
        let agent = ureq::Agent::config_builder()
            .https_only(!local)
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(20)))
            .build()
            .new_agent();
        Self {
            base: base.trim_end_matches('/').to_string(),
            agent,
        }
    }

    fn post(
        &self,
        path: &str,
        token: Option<&str>,
        body: serde_json::Value,
    ) -> Result<(u16, serde_json::Value), ApiError> {
        let mut request = self
            .agent
            .post(format!("{}{path}", self.base))
            .header("content-type", "application/json")
            .header("accept", "application/json");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let mut response = request.send(body.to_string()).map_err(|e| {
            tracing::debug!(error = %e, path, "convt.app request failed");
            ApiError::Offline
        })?;
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .with_config()
            .limit(64 * 1024)
            .read_to_string()
            .map_err(|_| ApiError::Offline)?;
        let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        Ok((status, json))
    }
}

/// Maps the statuses every endpoint shares.
fn check(status: u16) -> Result<(), ApiError> {
    match status {
        200..=299 => Ok(()),
        401 => Err(ApiError::SignedOut),
        400 | 403 | 404 => Err(ApiError::Rejected),
        429 => Err(ApiError::RateLimited),
        s => Err(ApiError::Server(s)),
    }
}

impl Api for Http {
    fn exchange(&self, code: &str, verifier: &str) -> Result<Session, ApiError> {
        let (status, json) = self.post(
            "/api/device/token",
            None,
            serde_json::json!({ "code": code, "verifier": verifier }),
        )?;
        // A bad code is the user's flow failing, not a revoked device.
        if status == 401 {
            return Err(ApiError::Rejected);
        }
        check(status)?;
        serde_json::from_value(json).map_err(|_| ApiError::BadResponse)
    }

    fn current_key(&self, token: &str, version: &str) -> Result<LicenseReply, ApiError> {
        let (status, json) = self.post(
            "/api/device/license",
            Some(token),
            serde_json::json!({ "version": version }),
        )?;
        check(status)?;
        let key = match json.get("key") {
            Some(serde_json::Value::String(key)) => Ok(Some(key.clone())),
            Some(serde_json::Value::Null) => Ok(None),
            _ => Err(ApiError::BadResponse),
        }?;
        let access = json
            .get("access")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| ApiError::BadResponse)?;
        Ok(LicenseReply { key, access })
    }

    fn sign_out(&self, token: &str) -> Result<(), ApiError> {
        let (status, _) = self.post("/api/device/sign-out", Some(token), serde_json::json!({}))?;
        // Already revoked is signed out too.
        if status == 401 {
            return Ok(());
        }
        check(status)
    }

    fn cloud_credential(&self, token: &str) -> Result<CloudCredential, ApiError> {
        let (status, json) = self.post("/api/device/cloud", Some(token), serde_json::json!({}))?;
        let error = json.pointer("/error").and_then(|e| e.as_str());
        match (status, error) {
            (403, Some("not_pro")) => return Err(ApiError::NeedsPro),
            (503, Some("not_configured")) => return Err(ApiError::CloudOff),
            _ => check(status)?,
        }
        let text = |key| json.get(key).and_then(|v| v.as_str()).map(str::to_string);
        match (text("baseUrl"), text("token")) {
            (Some(base_url), Some(token)) if secure_url(&base_url) => Ok(CloudCredential {
                base_url: base_url.trim_end_matches('/').to_string(),
                token,
            }),
            _ => Err(ApiError::BadResponse),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn a_flow_accepts_only_its_own_state_in_time() {
        let before = Instant::now();
        let a = Pending::new().unwrap();
        let after = Instant::now();
        let b = Pending::new().unwrap();
        assert_ne!(a.state(), b.state());
        assert_eq!(a.state().len(), 43);
        let now = Instant::now();
        assert!(a.accepts(a.state(), now));
        assert!(!a.accepts(b.state(), now));
        assert!(!a.accepts("", now));
        assert!(!a.accepts(&a.state()[..42], now));
        // Later times are passed in, so this doesn't depend on how long the
        // machine has been up.
        let second = Duration::from_secs(1);
        assert!(a.accepts(a.state(), before + SIGN_IN_TIMEOUT - second));
        assert!(!a.accepts(a.state(), after + SIGN_IN_TIMEOUT));
        // The deadline itself is too late.
        assert!((before + SIGN_IN_TIMEOUT..=after + SIGN_IN_TIMEOUT).contains(&a.deadline));
        assert!(!a.accepts(a.state(), a.deadline));
        assert!(!a.accepts(a.state(), after + SIGN_IN_TIMEOUT + second));

        let aged = Pending::new().unwrap().started_earlier(SIGN_IN_TIMEOUT);
        assert!(!aged.accepts(aged.state(), Instant::now()));
        let aged = Pending::new()
            .unwrap()
            .started_earlier(SIGN_IN_TIMEOUT - Duration::from_secs(60));
        assert!(aged.accepts(aged.state(), Instant::now()));
    }

    #[test]
    fn the_page_gets_the_challenge_never_the_verifier() {
        let p = Pending::new().unwrap();
        let url = p.url("https://convt.app", "Leo's PC & co", "Linux", "0.1.0");
        assert!(url.starts_with("https://convt.app/device?state="));
        assert!(url.contains(&format!("challenge={}", p.challenge())));
        assert!(!url.contains(p.verifier()));
        assert!(url.contains("name=Leo%27s%20PC%20%26%20co"));
        // RFC 7636's example.
        assert_eq!(
            challenge_of("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_session_never_prints_its_token() {
        let s = Session {
            email: "a@b.c".into(),
            token: "cvd_secret".into(),
        };
        assert!(!format!("{s:?}").contains("secret"));
    }

    /// Answers one request with `status` and `body`, and reports what it got.
    fn serve_once(status: u16, body: &'static str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut body_in = vec![0; length];
            reader.read_exact(&mut body_in).unwrap();
            head.push_str(&String::from_utf8(body_in).unwrap());
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            tx.send(head).unwrap();
        });
        (base, rx)
    }

    #[test]
    fn exchange_and_renewal_over_http() {
        let (base, got) = serve_once(200, r#"{"token":"cvd_x","email":"a@b.c"}"#);
        let session = Http::new(&base).exchange("code1", "ver1").unwrap();
        assert_eq!(session.token, "cvd_x");
        let request = got.recv().unwrap();
        assert!(request.starts_with("POST /api/device/token "));
        assert!(request.contains(r#""code":"code1""#) && request.contains(r#""verifier":"ver1""#));

        let (base, got) = serve_once(200, r#"{"key":"k.s"}"#);
        let key = Http::new(&base).current_key("cvd_x", "0.1.0").unwrap();
        assert_eq!(key.key.as_deref(), Some("k.s"));
        let request = got.recv().unwrap().to_ascii_lowercase();
        assert!(request.starts_with("post /api/device/license "));
        assert!(request.contains("authorization: bearer cvd_x"));

        let (base, _) = serve_once(200, r#"{"key":null}"#);
        assert_eq!(
            Http::new(&base).current_key("t", "v"),
            Ok(LicenseReply {
                key: None,
                access: None
            })
        );
        let (base, _) = serve_once(401, r#"{"error":"signed_out"}"#);
        assert_eq!(
            Http::new(&base).current_key("t", "v"),
            Err(ApiError::SignedOut)
        );
        let (base, _) = serve_once(400, r#"{"error":"invalid_grant"}"#);
        assert_eq!(Http::new(&base).exchange("c", "v"), Err(ApiError::Rejected));
        let (base, _) = serve_once(429, "{}");
        assert_eq!(
            Http::new(&base).current_key("t", "v"),
            Err(ApiError::RateLimited)
        );
        let (base, _) = serve_once(200, "not json");
        assert_eq!(
            Http::new(&base).current_key("t", "v"),
            Err(ApiError::BadResponse)
        );
        let (base, _) = serve_once(401, "{}");
        assert_eq!(Http::new(&base).sign_out("t"), Ok(()));
    }

    #[test]
    fn cloud_credentials_over_http() {
        let (base, got) = serve_once(
            200,
            r#"{"baseUrl":"https://api.example/","token":"cvt_web_a.b","expiresIn":300}"#,
        );
        let credential = Http::new(&base).cloud_credential("cvd_x").unwrap();
        assert_eq!(credential.base_url, "https://api.example");
        assert_eq!(credential.token, "cvt_web_a.b");
        assert!(!format!("{credential:?}").contains("cvt_web"));
        let request = got.recv().unwrap().to_ascii_lowercase();
        assert!(request.starts_with("post /api/device/cloud "));
        assert!(request.contains("authorization: bearer cvd_x"));

        let (base, _) = serve_once(403, r#"{"error":"not_pro"}"#);
        assert_eq!(
            Http::new(&base).cloud_credential("t"),
            Err(ApiError::NeedsPro)
        );
        let (base, _) = serve_once(503, r#"{"error":"not_configured"}"#);
        assert_eq!(
            Http::new(&base).cloud_credential("t"),
            Err(ApiError::CloudOff)
        );
        let (base, _) = serve_once(401, r#"{"error":"signed_out"}"#);
        assert_eq!(
            Http::new(&base).cloud_credential("t"),
            Err(ApiError::SignedOut)
        );
        // A credential is never sent anywhere but HTTPS or this computer.
        let (base, _) = serve_once(200, r#"{"baseUrl":"http://api.example","token":"t"}"#);
        assert_eq!(
            Http::new(&base).cloud_credential("t"),
            Err(ApiError::BadResponse)
        );
    }

    #[test]
    fn nothing_listening_is_offline() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let api = Http::new(&format!("http://127.0.0.1:{port}"));
        assert_eq!(api.current_key("t", "v"), Err(ApiError::Offline));
    }

    #[test]
    fn plain_http_is_refused_for_anything_but_loopback() {
        let api = Http::new("http://convt.example:1");
        assert_eq!(api.current_key("t", "v"), Err(ApiError::Offline));
    }
}
