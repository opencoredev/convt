//! The license check as the `convt` binary applies it. Every run gets its own
//! config and data folders and the file key store, so the user's keyring and
//! trial are never touched.

use std::path::PathBuf;
use std::process::{Command, Output};

use convt_license::{License, Plan, date, encode_public_key, sign};
use ed25519_dalek::SigningKey;

/// A throwaway key for these tests only.
fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

fn key(until: &str) -> String {
    let license = License {
        id: "lic_test".into(),
        email: mailbox("ada"),
        plan: Plan::Desktop,
        issued: "2026-01-01".into(),
        updates_until: until.into(),
    };
    sign(&license, &signing_key())
}

/// Today in UTC, as a day number.
fn today() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    (secs / 86_400) as i64
}

/// A trial key from convt.app that started today.
fn trial_key() -> String {
    let license = License {
        id: "trial_test".into(),
        email: mailbox("ada"),
        plan: Plan::Trial,
        issued: date::from_days(today()),
        updates_until: date::from_days(today() + 6),
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

    /// A trial key as the app keeps it, apart from bought keys.
    fn store_trial_key(&self, key: &str) {
        std::fs::create_dir_all(self.path("config")).unwrap();
        std::fs::write(self.path("config/trial.key"), key).unwrap();
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
fn converting_starts_no_trial() {
    let f = Fixture::new();
    let status = f.run(true, &["license", "status"], None);
    assert_eq!(
        stdout(&status),
        "No license or trial yet. Start your free trial from the convt app (it needs a \
         convt.app sign-in), or run `convt license activate <key>`.\n"
    );
    let out = f.convert(true);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("Start your free trial from the convt app"),
        "{}",
        stderr(&out)
    );
    assert!(!f.path("in.jpg").exists());
    assert!(!f.path("data/trial").exists());
}

#[test]
fn a_trial_from_the_app_lets_the_cli_convert() {
    let f = Fixture::new();
    f.store_trial_key(&trial_key());
    assert_eq!(
        stdout(&f.run(true, &["license"], None)),
        "Free trial: 7 days left.\n"
    );
    let out = f.convert(true);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(f.path("in.jpg").exists());
    // Pasting a trial key isn't how a trial starts.
    let pasted = f.run(true, &["license", "activate", &trial_key()], None);
    assert!(!pasted.status.success());
    assert!(stderr(&pasted).contains("trial key"), "{}", stderr(&pasted));
}

#[test]
fn a_clock_set_back_needs_a_check() {
    let f = Fixture::new();
    f.store_trial_key(&trial_key());
    // convt last ran three days from now.
    let later = (today() + 3) * 86_400;
    std::fs::write(f.path("config/last-seen"), format!("{later}\n")).unwrap();
    let status = stdout(&f.run(true, &["license"], None));
    assert!(status.contains("clock seems to have gone back"), "{status}");
    let out = f.convert(true);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("clock"), "{}", stderr(&out));
}

#[test]
fn an_ended_trial_stops_conversions_until_activation() {
    let f = Fixture::new();
    f.end_trial();
    let out = f.convert(true);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("trial has ended"), "{}", stderr(&out));
    assert!(!f.path("in.jpg").exists());
    // An older build's trial file is carried over once, then removed.
    assert!(!f.path("data/trial").exists());
    assert_eq!(
        std::fs::read_to_string(f.path("config/legacy-trial")).unwrap(),
        "2000-01-07\n"
    );
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
        format!(
            "Licensed to {} (Desktop), with updates until 2099-01-01.\n",
            mailbox("ada")
        )
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

/// A test mailbox, put together at run time so no address sits in the source.
fn mailbox(name: &str) -> String {
    [name, "convt.test"].join("@")
}
