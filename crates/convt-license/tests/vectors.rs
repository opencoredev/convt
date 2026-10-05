//! Fixed test vectors shared with the TypeScript signer on convt.app.
//! The seed below is a public test key, not a real signing key.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use convt_license::{Error, License, sign, verify};
use ed25519_dalek::SigningKey;
use serde::Deserialize;

#[derive(Deserialize)]
struct Vectors {
    seed_hex: String,
    public_key_b64url: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    license: License,
    token: String,
}

fn load() -> (SigningKey, Vectors) {
    let v: Vectors = serde_json::from_str(include_str!("vectors.json")).unwrap();
    let seed: Vec<u8> = (0..v.seed_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&v.seed_hex[i..i + 2], 16).unwrap())
        .collect();
    (SigningKey::from_bytes(&seed.try_into().unwrap()), v)
}

#[test]
fn public_key_matches_seed() {
    let (key, v) = load();
    assert_eq!(
        B64.encode(key.verifying_key().to_bytes()),
        v.public_key_b64url
    );
}

#[test]
fn sign_reproduces_and_verify_accepts() {
    let (key, v) = load();
    for case in &v.cases {
        assert_eq!(sign(&case.license, &key), case.token, "{}", case.name);
        assert_eq!(
            verify(&case.token, &key.verifying_key()).as_ref(),
            Ok(&case.license),
            "{}",
            case.name
        );
    }
}

#[test]
fn flipped_signature_byte_is_rejected() {
    let (key, v) = load();
    let token = &v.cases[0].token;
    let (payload, sig) = token.split_once('.').unwrap();
    let mut sig = B64.decode(sig).unwrap();
    sig[0] ^= 1;
    let bad = format!("{payload}.{}", B64.encode(sig));
    assert_eq!(verify(&bad, &key.verifying_key()), Err(Error::BadSignature));
}
