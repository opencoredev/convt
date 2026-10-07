//! `convert` checks the license like the app does. This file runs as its own
//! process, so setting the environment here touches no other test.

use convt_license::{License, Plan, encode_public_key, sign};
use ed25519_dalek::SigningKey;

#[test]
fn convert_needs_a_trial_or_a_license() {
    let dir = tempfile::tempdir().unwrap();
    let key = SigningKey::from_bytes(&[9; 32]);
    // SAFETY: the only test in this process, set before any thread reads them.
    unsafe {
        std::env::set_var("CONVT_CONFIG_DIR", dir.path().join("config"));
        std::env::set_var("CONVT_DATA_DIR", dir.path().join("data"));
        std::env::set_var("CONVT_LICENSE_STORE", "file");
        std::env::set_var("CONVT_LICENSE_ENFORCE", "1");
        std::env::set_var(
            "CONVT_LICENSE_PUBKEY",
            encode_public_key(&key.verifying_key()),
        );
    }
    let png = dir.path().join("in.png");
    image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]))
        .save(&png)
        .unwrap();
    let input = png.to_string_lossy().into_owned();

    // Converting starts no trial: that takes a convt.app sign-in in the app.
    let err = convt_ffi::convert(input.clone(), "jpeg".into()).unwrap_err();
    assert!(err.to_string().contains("signing in to convt.app"), "{err}");
    assert!(!dir.path().join("data/trial").exists());
    assert!(!dir.path().join("in.jpg").exists());

    let trial = License {
        id: "trial_ffi".into(),
        email: mailbox("a"),
        plan: Plan::Trial,
        issued: "2000-01-01".into(),
        updates_until: "2000-01-07".into(),
    };
    std::fs::create_dir_all(dir.path().join("config")).unwrap();
    std::fs::write(dir.path().join("config/trial.key"), sign(&trial, &key)).unwrap();
    let err = convt_ffi::convert(input.clone(), "webp".into()).unwrap_err();
    assert!(err.to_string().contains("trial has ended"), "{err}");

    let license = License {
        id: "lic_ffi".into(),
        email: mailbox("a"),
        plan: Plan::Pro,
        issued: "2026-01-01".into(),
        updates_until: "2099-01-01".into(),
    };
    std::fs::write(dir.path().join("config/license.key"), sign(&license, &key)).unwrap();
    assert!(convt_ffi::convert(input, "webp".into()).is_ok());
}

/// A test mailbox, put together at run time so no address sits in the source.
fn mailbox(name: &str) -> String {
    [name, "convt.test"].join("@")
}
