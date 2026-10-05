//! The license check as the `convt` binary applies it. Every run gets its own
//! config and data folders and the file key store, so the user's keyring and
//! trial are never touched.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use convt_license::{License, Plan, encode_public_key, sign};
use ed25519_dalek::SigningKey;

/// A throwaway key for these tests only.
fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

fn key(until: &str) -> String {
    let license = License {
        id: "lic_test".into(),
        email: "ada@example.com".into(),
        plan: Plan::Desktop,
        issued: "2026-01-01".into(),
        updates_until: until.into(),
    };
    sign(&license, &signing_key())
}

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let f = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        image::RgbImage::from_pixel(4, 4, image::Rgb([200, 20, 20]))
            .save(f.png())
            .unwrap();
        f
    }

    fn png(&self) -> PathBuf {
        self.dir.path().join("in.png")
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn run(&self, enforce: bool, args: &[&str], stdin: Option<&str>) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_convt"));
        cmd.args(args)
            .env("CONVT_CONFIG_DIR", self.path("config"))
            .env("CONVT_DATA_DIR", self.path("data"))
            .env("CONVT_LICENSE_STORE", "file")
            .env(
                "CONVT_LICENSE_PUBKEY",
                encode_public_key(&signing_key().verifying_key()),
            )
            .env("CONVT_LICENSE_ENFORCE", if enforce { "1" } else { "0" });
        match stdin {
            None => cmd.output().unwrap(),
            Some(text) => {
                use std::io::Write;
                cmd.stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());
                let mut child = cmd.spawn().unwrap();
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(text.as_bytes())
                    .unwrap();
                child.wait_with_output().unwrap()
            }
        }
    }

    fn convert(&self, enforce: bool) -> Output {
        self.run(
            enforce,
            &[self.png().to_str().unwrap(), "--to", "jpeg"],
            None,
        )
    }

    fn end_trial(&self) {
        std::fs::create_dir_all(self.path("data")).unwrap();
        std::fs::write(self.path("data/trial"), "2000-01-01\n").unwrap();
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn source_builds_convert_without_a_license() {
    let f = Fixture::new();
    f.end_trial();
    let out = f.convert(false);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(f.path("in.jpg").exists());
    let status = f.run(false, &["license"], None);
    assert_eq!(stdout(&status), "This build doesn't need a license.\n");
}

#[test]
fn the_first_conversion_starts_the_trial() {
    let f = Fixture::new();
    let status = f.run(true, &["license", "status"], None);
    assert!(stdout(&status).contains("starting with your first conversion"));
    assert!(!Path::new(&f.path("data/trial")).exists());

    let out = f.convert(true);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(f.path("data/trial").exists());
    assert_eq!(
        stdout(&f.run(true, &["license"], None)),
        "Free trial: 7 days left.\n"
    );
}

#[test]
fn an_ended_trial_stops_conversions_until_activation() {
    let f = Fixture::new();
    f.end_trial();
    let out = f.convert(true);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("trial has ended"), "{}", stderr(&out));
    assert!(!f.path("in.jpg").exists());
    // Listing formats and targets stays free.
    let targets = f.run(true, &["targets", "x.png"], None);
    assert!(targets.status.success());
    assert!(stdout(&targets).lines().any(|l| l == "jpeg"));

    let bad = f.run(true, &["license", "activate", "not-a-key"], None);
    assert!(!bad.status.success());
    assert!(stderr(&bad).contains("isn't valid"), "{}", stderr(&bad));

    // From standard input, so the key stays out of the process list.
    let good = f.run(
        true,
        &["license", "activate"],
        Some(&format!("{}\n", key("2099-01-01"))),
    );
    assert!(good.status.success(), "{}", stderr(&good));
    assert_eq!(
        stdout(&good),
        "Licensed to ada@example.com (Desktop), with updates until 2099-01-01.\n"
    );
    let out = f.convert(true);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(f.path("in.jpg").exists());

    let removed = f.run(true, &["license", "remove"], None);
    assert!(removed.status.success(), "{}", stderr(&removed));
    assert!(!f.path("config/license.key").exists());
    assert!(!f.convert(true).status.success());
}

#[test]
fn a_license_that_ended_before_this_build() {
    let f = Fixture::new();
    f.end_trial();
    let out = f.run(true, &["license", "activate", &key("2000-01-01")], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("covers builds released up to 2000-01-01"));
    let out = f.convert(true);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("2000-01-01"), "{}", stderr(&out));
}
