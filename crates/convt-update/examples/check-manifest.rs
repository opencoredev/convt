//! Tiny transport-free consumer, also checks the Bun signer against Rust.
use std::{env, fs};
fn main() {
    let args: Vec<_> = env::args().collect();
    let bytes = fs::read(&args[1]).unwrap();
    let key = convt_license::parse_public_key(&args[2]).expect("public key");
    let now: u64 = args[3].parse().unwrap();
    let verified = convt_update::verify(&bytes, &key, now, 0).expect("verified manifest");
    let selected = verified
        .select(
            "0.0.0",
            "1970-01-01",
            "2099-01-01",
            "linux-x86_64",
            "AppImage",
        )
        .unwrap();
    println!(
        "sequence={} selected={:?}",
        verified.manifest().sequence,
        selected.covered.map(|b| &b.version)
    );
}
