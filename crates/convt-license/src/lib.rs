//! License keys are a base64 JSON payload plus an Ed25519 signature, joined by
//! a dot. The app checks them offline against a public key baked into the
//! build (see `build.rs`). A license unlocks every build dated on or before
//! its `updates_until`; those builds keep working forever.
//!
//! The [`client`] module, behind the `client` feature, holds what the app,
//! the CLI and the OS menus share: the trial, the stored key and the check
//! before a conversion. [`account`], behind the same feature, is desktop
//! sign-in and Pro renewal, the only code here that uses the network.

#[cfg(feature = "client")]
pub mod account;
#[cfg(feature = "client")]
pub mod client;
pub mod date;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct License {
    pub id: String,
    pub email: String,
    pub plan: Plan,
    /// Issue date, `YYYY-MM-DD`.
    pub issued: String,
    /// Builds dated on or before this day are covered, `YYYY-MM-DD`. For Pro
    /// keys this is the paid-through date.
    pub updates_until: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    Desktop,
    Pro,
}

/// Paid Desktop licenses do not expire. The date stays in the signed payload
/// for compatibility with older clients and update manifests.
pub const LIFETIME_UPDATES_UNTIL: &str = "9999-12-31";

impl Plan {
    pub fn name(self) -> &'static str {
        match self {
            Plan::Desktop => "Desktop",
            Plan::Pro => "Pro",
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("malformed license key")]
    Malformed,
    #[error("invalid signature")]
    BadSignature,
    #[error("this build is newer than your license's update window (ended {0})")]
    UpdatesExpired(String),
}

/// Whether this build checks licenses. Only packaged builds do.
pub const ENFORCED: bool = matches!(env!("CONVT_EMBEDDED_ENFORCE").as_bytes(), b"1");

/// The day this build was made, `YYYY-MM-DD`.
pub const BUILD_DATE: &str = env!("CONVT_EMBEDDED_BUILD_DATE");

/// The key this build accepts licenses from, if it has one.
pub fn public_key() -> Option<VerifyingKey> {
    parse_public_key(env!("CONVT_EMBEDDED_PUBKEY"))
}

/// A base64url Ed25519 public key, as `CONVT_LICENSE_PUBKEY` holds it.
pub fn parse_public_key(text: &str) -> Option<VerifyingKey> {
    let bytes: [u8; 32] = B64.decode(text.trim()).ok()?.try_into().ok()?;
    VerifyingKey::from_bytes(&bytes).ok()
}

/// The text form of a public key, for `CONVT_LICENSE_PUBKEY`.
pub fn encode_public_key(key: &VerifyingKey) -> String {
    B64.encode(key.as_bytes())
}

pub fn sign(license: &License, key: &SigningKey) -> String {
    let payload = B64.encode(serde_json::to_vec(license).expect("license serializes"));
    let sig = key.sign(payload.as_bytes());
    format!("{payload}.{}", B64.encode(sig.to_bytes()))
}

pub fn verify(token: &str, key: &VerifyingKey) -> Result<License, Error> {
    let (payload, sig) = token.trim().split_once('.').ok_or(Error::Malformed)?;
    let sig: [u8; 64] = B64
        .decode(sig)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or(Error::Malformed)?;
    key.verify(payload.as_bytes(), &Signature::from_bytes(&sig))
        .map_err(|_| Error::BadSignature)?;
    let json = B64.decode(payload).map_err(|_| Error::Malformed)?;
    let license: License = serde_json::from_slice(&json).map_err(|_| Error::Malformed)?;
    // Coverage compares dates as strings, which only works for real dates.
    if date::to_days(&license.issued).is_none() || date::to_days(&license.updates_until).is_none() {
        return Err(Error::Malformed);
    }
    Ok(license)
}

impl License {
    /// Whether a build dated `build_date` (`YYYY-MM-DD`) is covered.
    /// ISO dates compare correctly as strings.
    pub fn covers_build(&self, build_date: &str) -> Result<(), Error> {
        if self.plan == Plan::Desktop {
            return Ok(());
        }
        if build_date > self.updates_until.as_str() {
            Err(Error::UpdatesExpired(self.updates_until.clone()))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keypair() -> SigningKey {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).unwrap();
        SigningKey::from_bytes(&seed)
    }

    fn license() -> License {
        License {
            id: "lic_1".into(),
            email: "a@b.c".into(),
            plan: Plan::Desktop,
            issued: "2026-10-02".into(),
            updates_until: "2027-10-02".into(),
        }
    }

    #[test]
    fn round_trip() {
        let key = keypair();
        let token = sign(&license(), &key);
        assert_eq!(verify(&token, &key.verifying_key()), Ok(license()));
    }

    #[test]
    fn rejects_tampering_and_wrong_key() {
        let key = keypair();
        let token = sign(&license(), &key);
        assert_eq!(
            verify(&token, &keypair().verifying_key()),
            Err(Error::BadSignature)
        );
        let forged = License {
            plan: Plan::Pro,
            updates_until: "2099-01-01".into(),
            ..license()
        };
        let forged_payload = B64.encode(serde_json::to_vec(&forged).unwrap());
        let sig = token.split_once('.').unwrap().1;
        assert_eq!(
            verify(&format!("{forged_payload}.{sig}"), &key.verifying_key()),
            Err(Error::BadSignature)
        );
        assert_eq!(
            verify("garbage", &key.verifying_key()),
            Err(Error::Malformed)
        );
    }

    #[test]
    fn update_window() {
        let l = license();
        assert!(l.covers_build("2027-10-02").is_ok());
        assert!(matches!(
            l.covers_build("2027-10-03"),
            Err(Error::UpdatesExpired(_))
        ));
    }

    #[test]
    fn paid_desktop_license_is_lifetime_even_with_legacy_expiry() {
        let mut l = license();
        l.updates_until = "2027-10-02".into();
        assert!(l.covers_build("2099-01-01").is_ok());
    }

    #[test]
    fn rejects_signed_licenses_with_bad_dates() {
        let key = keypair();
        for (issued, until) in [
            ("2026-10-02", "2027-13-01"),
            ("soon", "2027-10-02"),
            ("2026-10-02", "9999-99-99"),
        ] {
            let token = sign(
                &License {
                    issued: issued.into(),
                    updates_until: until.into(),
                    ..license()
                },
                &key,
            );
            assert_eq!(
                verify(&token, &key.verifying_key()),
                Err(Error::Malformed),
                "{until}"
            );
        }
    }

    #[test]
    fn plans_have_no_lifetime() {
        let json = serde_json::to_string(&License {
            plan: Plan::Pro,
            ..license()
        })
        .unwrap();
        let lifetime = json.replace("\"pro\"", "\"lifetime\"");
        assert!(serde_json::from_str::<License>(&lifetime).is_err());
        let open_ended = json.replace("\"2027-10-02\"", "null");
        assert!(serde_json::from_str::<License>(&open_ended).is_err());
    }

    #[test]
    fn public_keys_round_trip() {
        let key = keypair().verifying_key();
        assert_eq!(parse_public_key(&encode_public_key(&key)), Some(key));
        assert_eq!(parse_public_key("short"), None);
        assert_eq!(parse_public_key(""), None);
    }

    #[test]
    fn dates() {
        for (date, days) in [
            ("1970-01-01", 0),
            ("2000-02-29", 11_016),
            ("2026-10-02", 20_728),
        ] {
            assert_eq!(date::to_days(date), Some(days), "{date}");
            assert_eq!(date::from_days(days), date);
        }
        for bad in [
            "2026-02-29",
            "2026-13-01",
            "2026-1-01",
            "26-01-01",
            "2026-01-00",
            "x",
        ] {
            assert_eq!(date::to_days(bad), None, "{bad}");
        }
        let mut day = date::to_days("1999-12-25").unwrap();
        while day < date::to_days("2001-01-05").unwrap() {
            assert_eq!(date::to_days(&date::from_days(day)), Some(day));
            day += 1;
        }
    }
}
