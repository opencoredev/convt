//! Sending feedback from the Feedback window to convt.app.
//!
//! The app posts the message, the email if the user typed one, the app
//! version and the platform to convt.app's `/api/feedback`, which files it in
//! PostHog as a survey response. Nothing is sent until the user clicks Send:
//! this is not telemetry, and the app makes no other call for it.

use std::time::Duration;

use crate::account::{VERSION, os_name};

/// Characters in a message; convt.app refuses longer ones.
pub const MAX_MESSAGE: usize = 4000;

/// Why a submission didn't go through, worded for the Feedback window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    Offline,
    /// convt.app has no survey configured yet.
    Unavailable,
    RateLimited,
    InvalidEmail,
    InvalidMessage,
    Server(u16),
}

impl SendError {
    pub fn message(&self) -> String {
        match self {
            Self::Offline => "convt.app couldn't be reached. Your message is still here.".into(),
            Self::Unavailable => {
                "Feedback isn't set up yet. Email support@convt.app instead.".into()
            }
            Self::RateLimited => "That's a lot of feedback at once. Try again in an hour.".into(),
            Self::InvalidEmail => "That email address doesn't look right.".into(),
            Self::InvalidMessage => "Write a message first.".into(),
            Self::Server(status) => {
                format!("convt.app couldn't take your feedback (error {status}). Try again later.")
            }
        }
    }
}

/// Posts a feedback body. Tests script their own.
pub trait Sender: Send + Sync {
    fn send(&self, body: &serde_json::Value) -> Result<(), SendError>;
}

/// "Linux x86_64", "macOS aarch64".
pub fn platform() -> String {
    format!("{} {}", os_name(), std::env::consts::ARCH)
}

/// The JSON convt.app expects. `email` is dropped when blank.
pub fn body(message: &str, email: &str) -> serde_json::Value {
    let email = email.trim();
    serde_json::json!({
        "source": "desktop",
        "message": message.trim(),
        "email": (!email.is_empty()).then_some(email),
        "version": VERSION,
        "platform": platform(),
    })
}

/// Checks what the server would refuse, so the window can say so offline.
pub fn check(message: &str, email: &str) -> Result<(), SendError> {
    let message = message.trim();
    if message.is_empty() || message.chars().count() > MAX_MESSAGE {
        return Err(SendError::InvalidMessage);
    }
    let email = email.trim();
    if !email.is_empty() {
        let valid = email.len() <= 254
            && !email.chars().any(char::is_whitespace)
            && email.split_once('@').is_some_and(|(user, host)| {
                !user.is_empty()
                    && host.contains('.')
                    && !host.contains('@')
                    && !host.starts_with('.')
                    && !host.ends_with('.')
            });
        if !valid {
            return Err(SendError::InvalidEmail);
        }
    }
    Ok(())
}

/// [`Sender`] over HTTPS to `base/api/feedback`.
pub struct Http {
    base: String,
    agent: ureq::Agent,
}

impl Http {
    /// Plain HTTP is allowed only for a loopback host, which only a build
    /// from source can be pointed at (`CONVT_ACCOUNT_URL`).
    pub fn new(base: &str) -> Self {
        let local = ["http://localhost:", "http://127.0.0.1:", "http://[::1]:"]
            .iter()
            .any(|p| base.starts_with(p));
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
}

impl Sender for Http {
    fn send(&self, body: &serde_json::Value) -> Result<(), SendError> {
        let mut response = self
            .agent
            .post(format!("{}/api/feedback", self.base))
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .send(body.to_string())
            .map_err(|e| {
                tracing::debug!(error = %e, "feedback request failed");
                SendError::Offline
            })?;
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .with_config()
            .limit(16 * 1024)
            .read_to_string()
            .unwrap_or_default();
        let error = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|json| json.get("error")?.as_str().map(str::to_string));
        match (status, error.as_deref()) {
            (200..=299, _) => Ok(()),
            (400, Some("invalid_email")) => Err(SendError::InvalidEmail),
            (400, Some("invalid_message")) => Err(SendError::InvalidMessage),
            (429, _) => Err(SendError::RateLimited),
            (503, Some("unavailable")) => Err(SendError::Unavailable),
            (s, _) => Err(SendError::Server(s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_carries_version_and_platform() {
        let b = body("  PDF lost a page \n", "  ");
        assert_eq!(b["source"], "desktop");
        assert_eq!(b["message"], "PDF lost a page");
        assert!(b["email"].is_null());
        assert_eq!(b["version"], VERSION);
        assert_eq!(b["platform"], platform());
        assert_eq!(body("hi", " a@b.co ")["email"], "a@b.co");
    }

    #[test]
    fn check_matches_the_server() {
        assert_eq!(check("  ", ""), Err(SendError::InvalidMessage));
        assert_eq!(
            check(&"a".repeat(MAX_MESSAGE + 1), ""),
            Err(SendError::InvalidMessage)
        );
        assert_eq!(check(&"é".repeat(MAX_MESSAGE), ""), Ok(()));
        for bad in ["nope", "a@b", "a b@c.de", "@c.de", "a@.de", "a@c.de."] {
            assert_eq!(check("hi", bad), Err(SendError::InvalidEmail), "{bad}");
        }
        assert_eq!(check("hi", "ada@example.com"), Ok(()));
    }
}
