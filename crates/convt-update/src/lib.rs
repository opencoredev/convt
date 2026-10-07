//! Offline update authentication and license coverage. Callers own transport,
//! the check schedule, persisted sequence and all downloads.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use convt_license::date;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Separates update signatures from license signatures, even if keys are misconfigured.
pub const SIGNING_DOMAIN: &str = "convt-update-v1\n";
/// Trust root compiled into packaged clients. Source builds can have no key.
pub fn public_key() -> Option<VerifyingKey> {
    convt_license::parse_public_key(env!("CONVT_EMBEDDED_UPDATE_PUBKEY"))
}

pub const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub platform: String,
    pub kind: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Build {
    pub version: String,
    pub build_date: String,
    pub artifacts: Vec<Artifact>,
    pub source: Artifact,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    /// Monotonic release metadata revision, independent of software version.
    pub sequence: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    /// Verification artifacts can never be offered as updates.
    pub distribution_ready: bool,
    pub purchase_url: String,
    pub builds: Vec<Build>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedManifest {
    /// Base64url, without padding, of the exact UTF-8 JSON bytes.
    pub payload: String,
    /// Ed25519 over SIGNING_DOMAIN followed by payload's ASCII bytes.
    pub signature: String,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("malformed update manifest")]
    Malformed,
    #[error("invalid update signature")]
    BadSignature,
    #[error("update manifest is expired or issued in the future")]
    Stale,
    #[error("update manifest rolls back previously accepted metadata")]
    Rollback,
    #[error("verification artifacts are not distributable")]
    NotDistributable,
}

/// The only manifest type that permits update selection.
#[derive(Debug)]
pub struct VerifiedManifest(Manifest);

fn https(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
            && u.fragment().is_none()
    })
}
fn version(value: &str) -> Result<semver::Version, Error> {
    let v = semver::Version::parse(value).map_err(|_| Error::Malformed)?;
    if !v.pre.is_empty() || !v.build.is_empty() {
        return Err(Error::Malformed);
    }
    Ok(v)
}
fn artifact(a: &Artifact, source: bool) -> bool {
    https(&a.url)
        && a.size > 0
        && a.sha256.len() == 64
        && a.sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && if source {
            a.platform == "source" && a.kind == "tar.gz"
        } else {
            matches!(
                (a.platform.as_str(), a.kind.as_str()),
                ("linux-x86_64", "tar.gz" | "AppImage" | "deb" | "rpm")
                    | ("macos-arm64", "dmg" | "zip")
                    | ("windows-x86_64", "msi" | "exe" | "zip")
            )
        }
}
impl Manifest {
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema_version != 1
            || self.sequence == 0
            || self.issued_at >= self.expires_at
            || !https(&self.purchase_url)
            || self.builds.is_empty()
            || self.builds.len() > 256
        {
            return Err(Error::Malformed);
        }
        let mut identities = std::collections::BTreeSet::new();
        for b in &self.builds {
            version(&b.version)?;
            if date::to_days(&b.build_date).is_none()
                || b.artifacts.is_empty()
                || b.artifacts.len() > 32
                || !artifact(&b.source, true)
                || date::to_days(&b.build_date).unwrap() > (self.issued_at / 86400) as i64
                || !identities.insert((&b.build_date, &b.version))
            {
                return Err(Error::Malformed);
            }
            let mut targets = std::collections::BTreeSet::new();
            for a in &b.artifacts {
                if !artifact(a, false) || !targets.insert((&a.platform, &a.kind)) {
                    return Err(Error::Malformed);
                }
            }
        }
        Ok(())
    }
}

/// Verify before parsing untrusted JSON. `now` is Unix seconds; persist the
/// largest accepted `sequence` and pass it as `minimum_sequence` on each call.
pub fn verify(
    bytes: &[u8],
    key: &VerifyingKey,
    now: u64,
    minimum_sequence: u64,
) -> Result<VerifiedManifest, Error> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(Error::Malformed);
    }
    let envelope: SignedManifest = serde_json::from_slice(bytes).map_err(|_| Error::Malformed)?;
    let signature: [u8; 64] = B64
        .decode(&envelope.signature)
        .map_err(|_| Error::Malformed)?
        .try_into()
        .map_err(|_| Error::Malformed)?;
    let message = format!("{SIGNING_DOMAIN}{}", envelope.payload);
    key.verify_strict(message.as_bytes(), &Signature::from_bytes(&signature))
        .map_err(|_| Error::BadSignature)?;
    let payload = B64.decode(envelope.payload).map_err(|_| Error::Malformed)?;
    let manifest: Manifest = serde_json::from_slice(&payload).map_err(|_| Error::Malformed)?;
    manifest.validate()?;
    if !manifest.distribution_ready {
        return Err(Error::NotDistributable);
    }
    if now < manifest.issued_at || now >= manifest.expires_at {
        return Err(Error::Stale);
    }
    if manifest.sequence < minimum_sequence {
        return Err(Error::Rollback);
    }
    Ok(VerifiedManifest(manifest))
}

#[derive(Debug)]
pub struct Selection<'a> {
    /// Newest covered build newer than the running build for this target.
    pub covered: Option<&'a Build>,
    pub covered_artifact: Option<&'a Artifact>,
    /// Newest newer build outside the paid update window.
    pub uncovered: Option<&'a Build>,
    pub purchase_url: &'a str,
}
impl VerifiedManifest {
    pub fn manifest(&self) -> &Manifest {
        &self.0
    }
    /// Never downgrades either the version or the build date. This also blocks
    /// replay of old, still-valid manifests without relying on local state.
    pub fn select(
        &self,
        running_version: &str,
        running_date: &str,
        updates_until: &str,
        platform: &str,
        kind: &str,
    ) -> Result<Selection<'_>, Error> {
        let running = version(running_version)?;
        if date::to_days(running_date).is_none() || date::to_days(updates_until).is_none() {
            return Err(Error::Malformed);
        }
        let mut candidates = self
            .0
            .builds
            .iter()
            .filter(|b| {
                let v = version(&b.version).expect("verified schema");
                b.build_date.as_str() >= running_date
                    && v >= running
                    && (b.build_date.as_str() > running_date || v > running)
                    && b.artifacts
                        .iter()
                        .any(|a| a.platform == platform && a.kind == kind)
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| {
            a.build_date.cmp(&b.build_date).then_with(|| {
                version(&a.version)
                    .unwrap()
                    .cmp(&version(&b.version).unwrap())
            })
        });
        let covered = candidates
            .iter()
            .rev()
            .find(|b| b.build_date.as_str() <= updates_until)
            .copied();
        let uncovered = candidates
            .iter()
            .rev()
            .find(|b| b.build_date.as_str() > updates_until)
            .copied();
        Ok(Selection {
            covered,
            covered_artifact: covered.and_then(|b| {
                b.artifacts
                    .iter()
                    .find(|a| a.platform == platform && a.kind == kind)
            }),
            uncovered,
            purchase_url: &self.0.purchase_url,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    fn fixture() -> Manifest {
        let a = Artifact {
            platform: "linux-x86_64".into(),
            kind: "AppImage".into(),
            url: "https://downloads.convt.app/a".into(),
            size: 1,
            sha256: "a".repeat(64),
        };
        let source = Artifact {
            platform: "source".into(),
            kind: "tar.gz".into(),
            ..a.clone()
        };
        Manifest {
            schema_version: 1,
            sequence: 10,
            issued_at: 1791331200,
            expires_at: 1893456000,
            distribution_ready: true,
            purchase_url: "https://convt.app/pricing".into(),
            builds: [
                ("0.1.0", "2026-10-01"),
                ("0.2.0", "2026-10-02"),
                ("0.3.0", "2026-10-03"),
            ]
            .into_iter()
            .map(|(v, d)| Build {
                version: v.into(),
                build_date: d.into(),
                artifacts: vec![a.clone()],
                source: source.clone(),
            })
            .collect(),
        }
    }
    fn signed(m: &Manifest, key: &SigningKey) -> Vec<u8> {
        let payload = B64.encode(serde_json::to_vec(m).unwrap());
        let signature = B64.encode(
            key.sign(format!("{SIGNING_DOMAIN}{payload}").as_bytes())
                .to_bytes(),
        );
        serde_json::to_vec(&SignedManifest { payload, signature }).unwrap()
    }
    #[test]
    fn valid_and_coverage_selection() {
        let key = SigningKey::from_bytes(&[42; 32]);
        let m = verify(
            &signed(&fixture(), &key),
            &key.verifying_key(),
            1791331200,
            10,
        )
        .unwrap();
        let s = m
            .select(
                "0.1.0",
                "2026-10-01",
                "2026-10-02",
                "linux-x86_64",
                "AppImage",
            )
            .unwrap();
        assert_eq!(s.covered.unwrap().version, "0.2.0");
        assert_eq!(s.uncovered.unwrap().version, "0.3.0");
        assert!(s.covered_artifact.is_some());
        assert!(
            m.select("0.1.0", "2026-10-01", "2026-10-03", "macos-arm64", "dmg")
                .unwrap()
                .covered
                .is_none()
        );
    }
    #[test]
    fn tampered_and_wrong_key() {
        let k = SigningKey::from_bytes(&[42; 32]);
        let data = signed(&fixture(), &k);
        assert_eq!(
            verify(
                &data,
                &SigningKey::from_bytes(&[43; 32]).verifying_key(),
                1791331200,
                0
            )
            .unwrap_err(),
            Error::BadSignature
        );
        let mut e: SignedManifest = serde_json::from_slice(&data).unwrap();
        e.payload = B64.encode(b"{}");
        assert_eq!(
            verify(
                &serde_json::to_vec(&e).unwrap(),
                &k.verifying_key(),
                1791331200,
                0
            )
            .unwrap_err(),
            Error::BadSignature
        );
    }
    #[test]
    fn rollback_and_freshness() {
        let k = SigningKey::from_bytes(&[42; 32]);
        let data = signed(&fixture(), &k);
        assert_eq!(
            verify(&data, &k.verifying_key(), 1791331200, 11).unwrap_err(),
            Error::Rollback
        );
        for now in [1791331199, 1893456000] {
            assert_eq!(
                verify(&data, &k.verifying_key(), now, 0).unwrap_err(),
                Error::Stale
            );
        }
        let m = verify(&data, &k.verifying_key(), 1791331200, 0).unwrap();
        let s = m
            .select(
                "0.3.0",
                "2026-10-03",
                "2099-01-01",
                "linux-x86_64",
                "AppImage",
            )
            .unwrap();
        assert!(s.covered.is_none());
        assert!(
            m.select(
                "0.9.0",
                "2026-10-01",
                "2099-01-01",
                "linux-x86_64",
                "AppImage"
            )
            .unwrap()
            .covered
            .is_none()
        );
        assert!(
            m.select(
                "0.1.0",
                "2026-10-04",
                "2099-01-01",
                "linux-x86_64",
                "AppImage"
            )
            .unwrap()
            .covered
            .is_none()
        );
    }
    #[test]
    fn schema_and_verification_artifacts() {
        let k = SigningKey::from_bytes(&[42; 32]);
        let mut m = fixture();
        m.builds[0].build_date = "2026-02-29".into();
        assert_eq!(
            verify(&signed(&m, &k), &k.verifying_key(), 1791331200, 0).unwrap_err(),
            Error::Malformed
        );
        m = fixture();
        m.builds[0].artifacts[0].url = "file:///tmp/a".into();
        assert!(m.validate().is_err());
        m = fixture();
        m.distribution_ready = false;
        assert_eq!(
            verify(&signed(&m, &k), &k.verifying_key(), 1791331200, 0).unwrap_err(),
            Error::NotDistributable
        );
    }
}
