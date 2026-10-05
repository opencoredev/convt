//! Development license keys, for trying the licensing flow on a local build.
//!
//! ```sh
//! cargo run -p convt-license --example dev-keys -- keygen
//! cargo run -p convt-license --example dev-keys -- issue you@example.com 2027-10-02 [desktop|pro]
//! ```
//!
//! `keygen` writes `.convt-dev/license.key` (the signing key, readable only by
//! you) and `.convt-dev/license.pub` in the workspace root. Builds made after
//! that accept keys `issue` prints. `.convt-dev` is gitignored: never commit it.
//! Release keys are made and kept elsewhere.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use convt_license::{License, Plan, date, encode_public_key, sign};
use ed25519_dalek::SigningKey;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.convt-dev")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["keygen"] => keygen(),
        ["issue", email, until] => issue(email, until, Plan::Desktop),
        ["issue", email, until, "desktop"] => issue(email, until, Plan::Desktop),
        ["issue", email, until, "pro"] => issue(email, until, Plan::Pro),
        _ => Err("usage: dev-keys keygen | issue <email> <updates-until> [desktop|pro]".into()),
    };
    if let Err(e) = result {
        eprintln!("dev-keys: {e}");
        std::process::exit(1);
    }
}

fn keygen() -> Result<(), String> {
    let dir = dir();
    let secret = dir.join("license.key");
    if secret.exists() {
        return Err(format!(
            "{} exists; delete it to make a new key",
            secret.display()
        ));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| e.to_string())?;
    let key = SigningKey::from_bytes(&seed);

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&secret).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut file, format!("{}\n", B64.encode(seed)).as_bytes())
        .map_err(|e| e.to_string())?;

    let public = encode_public_key(&key.verifying_key());
    std::fs::write(dir.join("license.pub"), format!("{public}\n")).map_err(|e| e.to_string())?;
    println!("wrote {}", dir.display());
    println!("public key: {public}");
    Ok(())
}

fn issue(email: &str, until: &str, plan: Plan) -> Result<(), String> {
    if date::to_days(until).is_none() {
        return Err(format!("{until:?} is not YYYY-MM-DD"));
    }
    let text = std::fs::read_to_string(dir().join("license.key"))
        .map_err(|e| format!("no signing key ({e}); run keygen first"))?;
    let seed: [u8; 32] = B64
        .decode(text.trim())
        .ok()
        .and_then(|s| s.try_into().ok())
        .ok_or("the signing key is malformed")?;
    let mut id = [0u8; 8];
    getrandom::fill(&mut id).map_err(|e| e.to_string())?;
    let license = License {
        id: format!("dev_{}", B64.encode(id)),
        email: email.to_string(),
        plan,
        issued: date::from_days(today()),
        updates_until: until.to_string(),
    };
    println!("{}", sign(&license, &SigningKey::from_bytes(&seed)));
    Ok(())
}

fn today() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    (secs / 86_400) as i64
}
