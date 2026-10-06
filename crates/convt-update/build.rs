fn main() {
    for name in ["CONVT_UPDATE_PUBKEY", "CONVT_LICENSE_PUBKEY"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let update = std::env::var("CONVT_UPDATE_PUBKEY").unwrap_or_default();
    let license = std::env::var("CONVT_LICENSE_PUBKEY").unwrap_or_default();
    assert!(
        update.is_empty() || update.trim() != license.trim(),
        "update and license public keys must be separate"
    );
    assert!(
        update.is_empty()
            || (update.len() == 43
                && update
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')),
        "CONVT_UPDATE_PUBKEY must be base64url Ed25519 (32 bytes)"
    );
    println!(
        "cargo:rustc-env=CONVT_EMBEDDED_UPDATE_PUBKEY={}",
        update.trim()
    );
}
