//! Short-lived web credentials. HMAC key stays in the web Worker and API only.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
#[derive(Deserialize, Serialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub aud: String,
}
pub fn verify(secret: &str, token: &str) -> Option<String> {
    let (body, signature) = token.strip_prefix("cvt_web_")?.split_once('.')?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(body.as_bytes());
    mac.verify_slice(&hex::decode(signature).ok()?).ok()?;
    let claims: Claims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).ok()?).ok()?;
    let now = chrono::Utc::now().timestamp();
    (claims.aud == "convt-cloud-web"
        && claims.exp > now
        && claims.exp <= now + 300
        && !claims.sub.is_empty())
    .then_some(claims.sub)
}
