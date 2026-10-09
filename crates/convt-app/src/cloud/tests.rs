//! Cloud jobs against a scripted API: no network.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use convt_core::{Cancel, Job, Output, format_by_id};
use convt_license::account::{self, ApiError, CloudCredential, LicenseReply, Session};
use convt_license::{License, Plan, client::State, date};
use futures::StreamExt;

use super::*;
use crate::jobs::{Queue, Runner, Status, Update};

/// convt.app's side: hands out numbered credentials, or refuses.
struct Site {
    refuse: Option<ApiError>,
    issued: Mutex<u32>,
}

impl account::Api for Site {
    fn exchange(&self, _: &str, _: &str) -> Result<Session, ApiError> {
        unreachable!()
    }
    fn current_key(&self, _: &str, _: &str) -> Result<LicenseReply, ApiError> {
        unreachable!()
    }
    fn sign_out(&self, _: &str) -> Result<(), ApiError> {
        unreachable!()
    }
    fn cloud_credential(&self, token: &str) -> Result<CloudCredential, ApiError> {
        assert_eq!(token, "cvd_device");
        if let Some(e) = &self.refuse {
            return Err(e.clone());
        }
        let mut n = self.issued.lock().unwrap();
        *n += 1;
        Ok(CloudCredential {
            base_url: "https://api.test".into(),
            token: format!("cvt_web_{n}"),
        })
    }
}

/// The jobs API, scripted: statuses to report in turn, outputs to serve,
/// and refusals per call. Records every call.
#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<String>>,
    create: Mutex<VecDeque<Result<(), CloudError>>>,
    statuses: Mutex<VecDeque<Result<RemoteJob, CloudError>>>,
    outputs: Vec<(&'static str, &'static [u8])>,
    /// Blocks the upload until the job is cancelled.
    hang_upload: bool,
    /// Presses Stop as the last download finishes.
    stop_on_last_download: bool,
}

fn remote(status: RemoteStatus, error: Option<&str>) -> RemoteJob {
    RemoteJob {
        id: "job_1".into(),
        status,
        error_code: error.map(str::to_string),
    }
}

impl Fake {
    fn log(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl CloudApi for Fake {
    fn create(
        &self,
        c: &CloudCredential,
        from: &str,
        to: &str,
        bytes: u64,
    ) -> Result<Created, CloudError> {
        self.log(format!("create {from} {to} {bytes} {}", c.token));
        self.create.lock().unwrap().pop_front().unwrap_or(Ok(()))?;
        Ok(Created {
            job: remote(RemoteStatus::Created, None),
            upload_url: "https://storage.test/up".into(),
        })
    }
    fn upload(
        &self,
        url: &str,
        file: &Path,
        bytes: u64,
        sent: &dyn Fn(u64),
        cancel: &Cancel,
    ) -> Result<(), CloudError> {
        assert_eq!(std::fs::metadata(file).unwrap().len(), bytes);
        self.log(format!("upload {url} {bytes}"));
        if self.hang_upload {
            while !cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(5));
            }
            return Err(CloudError::Cancelled);
        }
        sent(bytes / 2);
        sent(bytes);
        Ok(())
    }
    fn start(&self, c: &CloudCredential, id: &str) -> Result<RemoteJob, CloudError> {
        self.log(format!("start {id} {}", c.token));
        Ok(remote(RemoteStatus::Queued, None))
    }
    fn status(&self, c: &CloudCredential, id: &str) -> Result<RemoteJob, CloudError> {
        self.log(format!("status {id} {}", c.token));
        self.statuses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(remote(RemoteStatus::Running, None)))
    }
    fn outputs(&self, _: &CloudCredential, id: &str) -> Result<Vec<RemoteOutput>, CloudError> {
        self.log(format!("outputs {id}"));
        // Each listing signs new links, as the server does.
        let listing = self
            .calls()
            .iter()
            .filter(|c| c.starts_with("outputs"))
            .count();
        Ok(self
            .outputs
            .iter()
            .map(|(name, _)| RemoteOutput {
                name: name.to_string(),
                url: if listing == 1 {
                    format!("https://storage.test/{name}")
                } else {
                    format!("https://storage.test/{name}?listing={listing}")
                },
            })
            .collect())
    }
    fn download(&self, url: &str, to: &Path, cancel: &Cancel) -> Result<(), CloudError> {
        self.log(format!("download {url}"));
        let name = url.rsplit('/').next().unwrap();
        let name = name.split('?').next().unwrap();
        let (_, bytes) = self.outputs.iter().find(|(n, _)| *n == name).unwrap();
        std::fs::write(to, bytes).map_err(|e| CloudError::Io(e.to_string()))?;
        if self.stop_on_last_download && Some(name) == self.outputs.last().map(|(n, _)| *n) {
            cancel.cancel();
        }
        Ok(())
    }
    fn cancel(&self, _: &CloudCredential, id: &str) -> Result<(), CloudError> {
        self.log(format!("cancel {id}"));
        Ok(())
    }
}

fn cloud(fake: Arc<Fake>, refuse: Option<ApiError>) -> Cloud {
    Cloud {
        api: fake,
        credentials: Credentials::new(
            Arc::new(Site {
                refuse,
                issued: Mutex::new(0),
            }),
            "cvd_device".into(),
        ),
        poll: Duration::from_millis(1),
        link_reuse: LINK_REUSE,
    }
}

/// A folder with `photo.png` in it.
fn input() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("photo.png");
    std::fs::write(&file, b"png bytes!").unwrap();
    (dir, file)
}

fn webp(file: &Path) -> Job {
    Job::new(file, format_by_id("webp").unwrap())
}

fn run_quietly(cloud: &Cloud, job: &Job, cancel: &Cancel) -> JobResult {
    run(cloud, job, &|_| {}, cancel)
}

/// Files left in `dir`, by name, staging folders included.
fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn uploads_polls_downloads_and_lands_beside_the_input() {
    let fake = Arc::new(Fake {
        outputs: vec![("1.webp", b"webp bytes")],
        ..Fake::default()
    });
    fake.statuses.lock().unwrap().extend([
        Ok(remote(RemoteStatus::Running, None)),
        Ok(remote(RemoteStatus::Succeeded, None)),
    ]);
    let (dir, file) = input();
    let seen = Mutex::new(Vec::new());
    let cloud = cloud(fake.clone(), None);
    let out = run(
        &cloud,
        &webp(&file),
        &|p| seen.lock().unwrap().push(p),
        &Cancel::new(),
    )
    .unwrap();
    assert_eq!(out, [dir.path().join("photo.webp")]);
    assert_eq!(std::fs::read(&out[0]).unwrap(), b"webp bytes");
    assert_eq!(listing(dir.path()), ["photo.png", "photo.webp"]);
    assert_eq!(
        fake.calls(),
        [
            "create png webp 10 cvt_web_1",
            "upload https://storage.test/up 10",
            "start job_1 cvt_web_1",
            "status job_1 cvt_web_1",
            "status job_1 cvt_web_1",
            "outputs job_1",
            "download https://storage.test/1.webp",
        ]
    );
    // The upload is the first half; the cloud's work has no progress to show.
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.first(), Some(&Some(0.0)));
    assert!(seen.contains(&Some(0.25)) && seen.contains(&Some(0.5)));
    assert!(seen.contains(&None));
    assert_eq!(seen.last(), Some(&Some(1.0)));
}

#[test]
fn outputs_follow_the_local_naming_rules() {
    let fake = Arc::new(Fake {
        outputs: vec![("2.webp", b"two"), ("1.webp", b"one")],
        ..Fake::default()
    });
    fake.statuses
        .lock()
        .unwrap()
        .push_back(Ok(remote(RemoteStatus::Succeeded, None)));
    let (dir, file) = input();
    // Taken already: the results are renamed, never overwritten.
    std::fs::write(dir.path().join("photo.webp"), b"mine").unwrap();
    let out_dir = dir.path().join("out");
    std::fs::create_dir(&out_dir).unwrap();
    let mut job = webp(&file);
    job.output = Output::Dir(out_dir.clone());
    let out = run_quietly(&cloud(fake.clone(), None), &job, &Cancel::new()).unwrap();
    assert_eq!(
        out,
        [out_dir.join("photo-2.webp"), out_dir.join("photo.webp")]
    );
    assert_eq!(std::fs::read(&out[1]).unwrap(), b"one");

    let fake = Arc::new(Fake {
        outputs: vec![("1.webp", b"again")],
        ..Fake::default()
    });
    fake.statuses
        .lock()
        .unwrap()
        .push_back(Ok(remote(RemoteStatus::Succeeded, None)));
    let out = run_quietly(&cloud(fake, None), &webp(&file), &Cancel::new()).unwrap();
    assert_eq!(out, [dir.path().join("photo (1).webp")]);
    assert_eq!(
        std::fs::read(dir.path().join("photo.webp")).unwrap(),
        b"mine"
    );
}

#[test]
fn a_refused_credential_stops_the_job_before_anything_is_sent() {
    let (dir, file) = input();
    for (refusal, kind, words) in [
        (ApiError::NeedsPro, "cloud_pro", "paid Pro"),
        (ApiError::SignedOut, "cloud_signed_out", "Sign in again"),
        (ApiError::Offline, "cloud_offline", "internet connection"),
        (ApiError::CloudOff, "cloud_unavailable", "isn't available"),
    ] {
        let fake = Arc::new(Fake::default());
        let e = run_quietly(
            &cloud(fake.clone(), Some(refusal)),
            &webp(&file),
            &Cancel::new(),
        )
        .unwrap_err();
        assert_eq!(e.kind, kind);
        assert!(e.message.contains(words), "{}", e.message);
        assert!(fake.calls().is_empty());
    }
    assert_eq!(listing(dir.path()), ["photo.png"]);
}

#[test]
fn server_refusals_and_failures_read_in_plain_words() {
    let (dir, file) = input();
    let refused = |status, code: &str| CloudError::Refused {
        status,
        code: code.into(),
    };
    for (error, kind, words) in [
        (refused(403, "limit_reached"), "cloud_limit", "50 GB"),
        (
            refused(403, "storage_limit_reached"),
            "cloud_limit",
            "24 hours",
        ),
        (refused(400, "file_too_large"), "cloud_too_large", "2 GB"),
        (refused(403, "not_enrolled"), "cloud_pro", "paid Pro"),
        (
            refused(400, "unsupported_format"),
            "cloud_unsupported",
            "WEBP",
        ),
        (
            refused(502, "storage_unavailable"),
            "cloud_server",
            "HTTP 502",
        ),
        (CloudError::Offline, "cloud_offline", "internet connection"),
    ] {
        let fake = Arc::new(Fake::default());
        fake.create.lock().unwrap().push_back(Err(error));
        let e = run_quietly(&cloud(fake, None), &webp(&file), &Cancel::new()).unwrap_err();
        assert_eq!(e.kind, kind);
        assert!(e.message.contains(words), "{}", e.message);
    }
    for (code, words) in [
        (Some("conversion_failed"), "couldn't convert this file"),
        (Some("expired"), "expired"),
        (Some("worker_shutdown"), "stopped during"),
    ] {
        let fake = Arc::new(Fake::default());
        fake.statuses
            .lock()
            .unwrap()
            .push_back(Ok(remote(RemoteStatus::Failed, code)));
        let e = run_quietly(&cloud(fake.clone(), None), &webp(&file), &Cancel::new()).unwrap_err();
        assert!(e.message.contains(words), "{}", e.message);
        assert!(!fake.calls().iter().any(|c| c.starts_with("download")));
    }
    // Nothing is left behind: no output, no staging folder.
    assert_eq!(listing(dir.path()), ["photo.png"]);
}

#[test]
fn an_expired_credential_is_renewed_once() {
    let fake = Arc::new(Fake {
        outputs: vec![("1.webp", b"x")],
        ..Fake::default()
    });
    fake.statuses.lock().unwrap().extend([
        Err(CloudError::Refused {
            status: 403,
            code: "unauthorized".into(),
        }),
        Ok(remote(RemoteStatus::Succeeded, None)),
    ]);
    let (_dir, file) = input();
    run_quietly(&cloud(fake.clone(), None), &webp(&file), &Cancel::new()).unwrap();
    let calls = fake.calls();
    assert!(calls.contains(&"status job_1 cvt_web_1".to_string()));
    assert!(calls.contains(&"status job_1 cvt_web_2".to_string()));
}

/// convt.app's side, issuing one credential and then answering `then`.
struct OnceThen {
    then: ApiError,
    issued: Mutex<bool>,
}

impl account::Api for OnceThen {
    fn exchange(&self, _: &str, _: &str) -> Result<Session, ApiError> {
        unreachable!()
    }
    fn current_key(&self, _: &str, _: &str) -> Result<LicenseReply, ApiError> {
        unreachable!()
    }
    fn sign_out(&self, _: &str) -> Result<(), ApiError> {
        unreachable!()
    }
    fn cloud_credential(&self, _: &str) -> Result<CloudCredential, ApiError> {
        let mut issued = self.issued.lock().unwrap();
        if *issued {
            return Err(self.then.clone());
        }
        *issued = true;
        Ok(CloudCredential {
            base_url: "https://api.test".into(),
            token: "cvt_web_1".into(),
        })
    }
}

#[test]
fn only_a_confirmed_revocation_reads_as_signed_out() {
    let (_dir, file) = input();
    for (then, kind) in [
        (ApiError::Offline, "cloud_unauthorized"),
        (ApiError::SignedOut, "cloud_signed_out"),
    ] {
        let fake = Arc::new(Fake::default());
        fake.create
            .lock()
            .unwrap()
            .push_back(Err(CloudError::Refused {
                status: 401,
                code: "unauthorized".into(),
            }));
        let cloud = Cloud {
            credentials: Credentials::new(
                Arc::new(OnceThen {
                    then,
                    issued: Mutex::new(false),
                }),
                "cvd_device".into(),
            ),
            ..cloud(fake, None)
        };
        let e = run_quietly(&cloud, &webp(&file), &Cancel::new()).unwrap_err();
        assert_eq!(e.kind, kind);
    }
}

#[test]
fn a_blip_while_polling_is_ridden_out() {
    let fake = Arc::new(Fake {
        outputs: vec![("1.webp", b"x")],
        ..Fake::default()
    });
    fake.statuses.lock().unwrap().extend([
        Err(CloudError::Offline),
        Err(CloudError::Refused {
            status: 429,
            code: "rate_limited".into(),
        }),
        Ok(remote(RemoteStatus::Succeeded, None)),
    ]);
    let (_dir, file) = input();
    assert!(run_quietly(&cloud(fake, None), &webp(&file), &Cancel::new()).is_ok());
}

#[test]
fn a_job_that_fails_while_polling_is_cancelled_on_the_server() {
    // An answer the app gives up on leaves the job queued there, holding
    // storage, unless the app cancels it.
    let fake = Arc::new(Fake::default());
    fake.statuses
        .lock()
        .unwrap()
        .push_back(Err(CloudError::Refused {
            status: 409,
            code: "conflict".into(),
        }));
    let (_dir, file) = input();
    assert!(run_quietly(&cloud(fake.clone(), None), &webp(&file), &Cancel::new()).is_err());
    assert_eq!(fake.calls().last().unwrap(), "cancel job_1");
}

#[test]
fn files_the_cloud_cant_take_never_leave() {
    let (dir, file) = input();
    let fake = Arc::new(Fake::default());
    let big = dir.path().join("big.png");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(MAX_INPUT_BYTES + 1)
        .unwrap();
    let e = run_quietly(&cloud(fake.clone(), None), &webp(&big), &Cancel::new()).unwrap_err();
    assert_eq!(e.kind, "cloud_too_large");
    let empty = dir.path().join("empty.png");
    std::fs::write(&empty, b"").unwrap();
    assert_eq!(
        run_quietly(&cloud(fake.clone(), None), &webp(&empty), &Cancel::new())
            .unwrap_err()
            .kind,
        "cloud_empty"
    );
    // PNG to MP4 runs nowhere, and the cloud's list says so.
    let mp4 = Job::new(&file, format_by_id("mp4").unwrap());
    let e = run_quietly(&cloud(fake.clone(), None), &mp4, &Cancel::new()).unwrap_err();
    assert_eq!(e.kind, "cloud_unsupported");
    assert!(fake.calls().is_empty());
}

#[test]
fn access_needs_a_sign_in_and_current_pro() {
    let today = date::to_days("2026-10-08").unwrap();
    let license = |plan, until: &str| {
        State::Licensed(License {
            id: "lic_1".into(),
            email: String::new(),
            plan,
            issued: "2026-01-01".into(),
            updates_until: until.into(),
        })
    };
    let url = "https://convt.app";
    let pro = license(Plan::Pro, "2026-11-01");
    assert_eq!(access(url, true, &pro, today), CloudAccess::Ready);
    assert_eq!(access(url, false, &pro, today), CloudAccess::SignedOut);
    assert_eq!(
        access(url, true, &license(Plan::Pro, "2026-10-07"), today),
        CloudAccess::NeedsPro
    );
    assert_eq!(
        access(url, true, &license(Plan::Pro, "2026-10-08"), today),
        CloudAccess::Ready
    );
    assert_eq!(
        access(url, true, &license(Plan::Desktop, "2030-01-01"), today),
        CloudAccess::NeedsPro
    );
    for state in [
        State::TrialEnded,
        State::Trial {
            days_left: 3,
            started: None,
        },
    ] {
        assert_eq!(access(url, true, &state, today), CloudAccess::NeedsPro);
    }
    // Builds from source check no license; convt.app still checks Pro.
    assert_eq!(
        access(url, true, &State::Unrestricted, today),
        CloudAccess::Ready
    );
    assert!(matches!(
        access("", true, &pro, today),
        CloudAccess::Unavailable(_)
    ));
    assert!(CloudAccess::NeedsPro.reason().unwrap().contains("Pro"));
    assert_eq!(CloudAccess::Ready.reason(), None);
}

/// Through the runner and queue, as the Activity list sees it.
#[test]
fn stop_cancels_the_cloud_job_on_the_server() {
    let fake = Arc::new(Fake {
        hang_upload: true,
        ..Fake::default()
    });
    let (dir, file) = input();
    let (runner, mut rx) = Runner::new(Arc::new(convt_core::Registry::new()), 1);
    let mut queue = Queue::default();
    let job = webp(&file);
    let id = queue.add_cloud(&job);
    assert!(queue.get(id).unwrap().setup.cloud);
    runner.submit_cloud(id, job, Arc::new(cloud(fake.clone(), None)));
    futures::executor::block_on(async {
        while let Some(update) = rx.next().await {
            let started = update == Update::Started(id);
            queue.apply(update);
            if started {
                break;
            }
        }
        // Wait for the upload to begin, then press Stop.
        while !fake.calls().iter().any(|c| c.starts_with("upload")) {
            std::thread::sleep(Duration::from_millis(5));
        }
        runner.cancel(id);
        while let Some(update) = rx.next().await {
            if queue.apply(update).is_some() {
                break;
            }
        }
    });
    assert_eq!(queue.get(id).unwrap().status, Status::Cancelled);
    assert_eq!(fake.calls().last().unwrap(), "cancel job_1");
    assert_eq!(listing(dir.path()), ["photo.png"]);
}

#[test]
fn cloud_and_local_jobs_share_the_queue() {
    let fake = Arc::new(Fake {
        outputs: vec![("1.webp", b"x")],
        ..Fake::default()
    });
    fake.statuses
        .lock()
        .unwrap()
        .push_back(Ok(remote(RemoteStatus::Succeeded, None)));
    let (_dir, file) = input();
    let (runner, mut rx) = Runner::new(Arc::new(convt_core::Registry::new()), 1);
    let mut queue = Queue::default();
    // The registry is empty, so the local job fails at once.
    let blocked = queue.add(&webp(&file));
    let cloud_id = queue.add_cloud(&webp(&file));
    runner.submit_cloud(cloud_id, webp(&file), Arc::new(cloud(fake, None)));
    runner.submit(blocked, webp(&file));
    futures::executor::block_on(async {
        let mut done = 0;
        while done < 2 {
            if queue.apply(rx.next().await.unwrap()).is_some() {
                done += 1;
            }
        }
    });
    assert!(matches!(
        queue.get(cloud_id).unwrap().status,
        Status::Done(_)
    ));
    assert!(matches!(
        queue.get(blocked).unwrap().status,
        Status::Failed(_)
    ));
}

/// Test stand-ins for the parts of the app cloud jobs don't use.
mod app_support {
    use std::path::PathBuf;

    use convt_core::Registry;

    use crate::pack;
    use crate::update::{Fetch, FetchError};

    pub struct NoPacks;

    impl pack::Backend for NoPacks {
        fn offer(&self) -> pack::Offer {
            pack::Offer::default()
        }
        fn status(&self) -> pack::Status {
            pack::Status::NotInstalled
        }
        fn install(
            &self,
            _: &dyn Fn(pack::Progress),
            _: &dyn Fn() -> bool,
        ) -> Result<PathBuf, pack::Failure> {
            panic!("tests never download")
        }
        fn remove(&self) -> Result<(), String> {
            panic!("tests never remove a pack")
        }
        fn registry(&self) -> Registry {
            Registry::new()
        }
        fn registry_without_documents(&self) -> Registry {
            Registry::new()
        }
    }

    pub struct NoUpdates;

    impl Fetch for NoUpdates {
        fn fetch(&self) -> Result<Vec<u8>, FetchError> {
            Err(FetchError::Offline)
        }
    }
}

/// The whole path through [`AppState`]: access, consent, the Activity list,
/// history and Retry.
mod app {
    use std::time::Instant;

    use convt_core::Options;
    use convt_license::client::{self, KeyStore};
    use ed25519_dalek::SigningKey;
    use gpui_kit::{AppContext as _, Entity, TestAppContext};

    use super::app_support::*;
    use super::*;
    use crate::history::Outcome;
    use crate::model::{AppState, Paths};
    use crate::update::UpdateConfig;

    fn key(plan: Plan) -> String {
        let license = License {
            id: "lic_test".into(),
            email: String::new(),
            plan,
            issued: "2026-01-01".into(),
            updates_until: "2099-01-01".into(),
        };
        convt_license::sign(&license, &SigningKey::from_bytes(&[7; 32]))
    }

    /// An app with `key` stored, signed in if `signed_in`, and `fake` as
    /// the cloud.
    fn app(
        cx: &mut TestAppContext,
        key: Option<String>,
        signed_in: bool,
        fake: Arc<Fake>,
    ) -> (tempfile::TempDir, Entity<AppState>) {
        app_with_site(cx, key, signed_in, fake, None)
    }

    /// [`app`], with convt.app refusing cloud credentials with `refuse`.
    fn app_with_site(
        cx: &mut TestAppContext,
        key: Option<String>,
        signed_in: bool,
        fake: Arc<Fake>,
        refuse: Option<ApiError>,
    ) -> (tempfile::TempDir, Entity<AppState>) {
        let dir = tempfile::tempdir().unwrap();
        if let Some(key) = key {
            std::fs::write(dir.path().join("license.key"), key).unwrap();
        }
        if signed_in {
            let session = Session {
                email: String::new(),
                token: "cvd_device".into(),
            };
            std::fs::write(
                dir.path().join("account.json"),
                serde_json::to_string(&session).unwrap(),
            )
            .unwrap();
        }
        let paths = Paths {
            settings: None,
            history: Some(dir.path().join("history.db")),
            presets: None,
            license: client::Config {
                enforce: true,
                public_key: Some(SigningKey::from_bytes(&[7; 32]).verifying_key()),
                build_date: "2026-10-01".into(),
                trial_file: Some(dir.path().join("trial")),
                store: KeyStore::File(dir.path().join("license.key")),
            },
            account_url: "https://convt.test".into(),
            account_api: Arc::new(Site {
                refuse,
                issued: Mutex::new(0),
            }),
            update: UpdateConfig {
                key: None,
                fetch: Arc::new(NoUpdates),
                target: ("linux-x86_64", "AppImage"),
                install: None,
            },
        };
        cx.executor().allow_parking();
        let app = cx.update(|cx| {
            let app = cx.new(|cx| AppState::new(Arc::new(NoPacks), paths, cx));
            app.update(cx, |s, _| s.set_cloud_api(fake, Duration::from_millis(1)));
            app
        });
        (dir, app)
    }

    fn wait_until(cx: &mut TestAppContext, what: &str, done: impl Fn(&AppState) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            cx.run_until_parked();
            if cx.read(|cx| done(cx.global::<Probe>().0.read(cx))) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    struct Probe(Entity<AppState>);
    impl gpui_kit::Global for Probe {}

    fn convert(
        cx: &mut TestAppContext,
        app: &Entity<AppState>,
        file: &Path,
        options: Options,
    ) -> Result<Vec<crate::jobs::JobId>, String> {
        let file = file.to_path_buf();
        app.update(cx, |s, cx| {
            s.convert_in_cloud(
                &[file],
                format_by_id("webp").unwrap(),
                &options,
                Output::Beside,
                cx,
            )
        })
    }

    #[gpui_kit::test]
    fn access_follows_the_sign_in_and_the_stored_license(cx: &mut TestAppContext) {
        let fake = Arc::new(Fake::default());
        let (_d, app) = app(cx, Some(key(Plan::Pro)), false, fake.clone());
        cx.read(|cx| assert_eq!(app.read(cx).cloud_access(), CloudAccess::SignedOut));
        let (_d, app) = app_signed_in(cx, Some(key(Plan::Desktop)), fake.clone());
        cx.read(|cx| assert_eq!(app.read(cx).cloud_access(), CloudAccess::NeedsPro));
        let (dir, app) = app_signed_in(cx, None, fake.clone());
        cx.read(|cx| assert_eq!(app.read(cx).cloud_access(), CloudAccess::NeedsPro));
        let png = dir.path().join("a.png");
        std::fs::write(&png, b"png").unwrap();
        let refused = convert(cx, &app, &png, Options::default()).unwrap_err();
        assert!(refused.contains("Pro"));
        // Deciding sent nothing.
        assert!(fake.calls().is_empty());
    }

    fn app_signed_in(
        cx: &mut TestAppContext,
        key: Option<String>,
        fake: Arc<Fake>,
    ) -> (tempfile::TempDir, Entity<AppState>) {
        app(cx, key, true, fake)
    }

    #[gpui_kit::test]
    fn a_revoked_sign_in_found_by_a_cloud_job_signs_out(cx: &mut TestAppContext) {
        let fake = Arc::new(Fake::default());
        let (dir, app) = app_with_site(
            cx,
            Some(key(Plan::Pro)),
            true,
            fake.clone(),
            Some(ApiError::SignedOut),
        );
        cx.update(|cx| cx.set_global(Probe(app.clone())));
        app.update(cx, |s, cx| {
            s.update_settings(|s| s.cloud_consent = true, cx)
        });
        cx.read(|cx| assert_eq!(app.read(cx).cloud_access(), CloudAccess::Ready));
        let png = dir.path().join("photo.png");
        std::fs::write(&png, b"png bytes").unwrap();
        convert(cx, &app, &png, Options::default()).unwrap();
        wait_until(cx, "the job to fail", |s| s.queue.active() == 0);
        cx.read(|cx| {
            let s = app.read(cx);
            assert!(s.account.session.is_none());
            assert_eq!(s.cloud_access(), CloudAccess::SignedOut);
        });
        assert!(!dir.path().join("account.json").exists());
        assert!(fake.calls().is_empty());
    }

    #[gpui_kit::test]
    fn a_cloud_job_runs_through_activity_history_and_retry(cx: &mut TestAppContext) {
        let fake = Arc::new(Fake {
            outputs: vec![("1.webp", b"webp")],
            ..Fake::default()
        });
        for _ in 0..2 {
            fake.statuses
                .lock()
                .unwrap()
                .push_back(Ok(remote(RemoteStatus::Succeeded, None)));
        }
        let (dir, app) = app_signed_in(cx, Some(key(Plan::Pro)), fake.clone());
        cx.update(|cx| cx.set_global(Probe(app.clone())));
        cx.read(|cx| assert_eq!(app.read(cx).cloud_access(), CloudAccess::Ready));
        let png = dir.path().join("photo.png");
        std::fs::write(&png, b"png bytes").unwrap();

        // Not without consent, and not with options the cloud can't apply.
        let e = convert(cx, &app, &png, Options::default()).unwrap_err();
        assert!(e.contains("Agree"), "{e}");
        app.update(cx, |s, cx| {
            s.update_settings(|s| s.cloud_consent = true, cx)
        });
        let quality = Options {
            quality: Some(40),
            ..Options::default()
        };
        assert!(convert(cx, &app, &png, quality).is_err());
        assert!(fake.calls().is_empty());

        let ids = convert(cx, &app, &png, Options::default()).unwrap();
        wait_until(cx, "the cloud job", |s| {
            s.entry(ids[0]).is_some_and(|e| e.status.is_finished())
        });
        let out = dir.path().join("photo.webp");
        cx.read(|cx| {
            let s = app.read(cx);
            let entry = s.entry(ids[0]).unwrap();
            assert_eq!(entry.status, Status::Done(vec![out.clone()]));
            assert!(entry.setup.cloud);
            let record = &s.recent[0];
            assert_eq!(record.outcome, Outcome::Done(vec![out.clone()]));
            assert!(record.setup.as_ref().unwrap().cloud);
        });
        assert_eq!(std::fs::read(&out).unwrap(), b"webp");

        // Retry from history runs on the cloud again, and renames.
        let record = cx.read(|cx| app.read(cx).recent[0].clone());
        let retried = app
            .update(cx, |s, cx| {
                s.retry(
                    &record.input,
                    format_by_id("webp").unwrap(),
                    record.setup.as_ref(),
                    cx,
                )
            })
            .unwrap();
        wait_until(cx, "the retry", |s| {
            s.entry(retried[0]).is_some_and(|e| e.status.is_finished())
        });
        assert!(dir.path().join("photo (1).webp").exists());
        let creates = fake
            .calls()
            .iter()
            .filter(|c| c.starts_with("create"))
            .count();
        assert_eq!(creates, 2);
    }

    #[gpui_kit::test]
    fn no_cloud_job_starts_while_an_update_installs(cx: &mut TestAppContext) {
        let fake = Arc::new(Fake::default());
        let (dir, app) = app_signed_in(cx, Some(key(Plan::Pro)), fake.clone());
        app.update(cx, |s, cx| {
            s.update_settings(|s| s.cloud_consent = true, cx);
            s.update = crate::update::Update::Installing {
                version: "9.2.0".into(),
            };
        });
        let png = dir.path().join("photo.png");
        std::fs::write(&png, b"png bytes").unwrap();
        let e = convert(cx, &app, &png, Options::default()).unwrap_err();
        assert_eq!(e, crate::model::INSTALLING);
        cx.read(|cx| assert_eq!(app.read(cx).queue.active(), 0));
        assert!(fake.calls().is_empty());
    }

    #[gpui_kit::test]
    fn a_refusal_shows_as_the_jobs_error(cx: &mut TestAppContext) {
        let fake = Arc::new(Fake::default());
        fake.create
            .lock()
            .unwrap()
            .push_back(Err(CloudError::Refused {
                status: 403,
                code: "limit_reached".into(),
            }));
        let (dir, app) = app_signed_in(cx, Some(key(Plan::Pro)), fake);
        cx.update(|cx| cx.set_global(Probe(app.clone())));
        app.update(cx, |s, cx| {
            s.update_settings(|s| s.cloud_consent = true, cx)
        });
        let png = dir.path().join("photo.png");
        std::fs::write(&png, b"png bytes").unwrap();
        let ids = convert(cx, &app, &png, Options::default()).unwrap();
        wait_until(cx, "the cloud job", |s| {
            s.entry(ids[0]).is_some_and(|e| e.status.is_finished())
        });
        cx.read(|cx| {
            let s = app.read(cx);
            let Status::Failed(e) = &s.entry(ids[0]).unwrap().status else {
                panic!("not failed")
            };
            assert!(e.message.contains("50 GB"), "{}", e.message);
            assert!(matches!(&s.recent[0].outcome, Outcome::Failed(m) if m.contains("50 GB")));
        });
    }
}

/// Against a real local stack (test-convt-web and test-convt-server): the
/// site, convt-server, a worker and MinIO. Ignored unless asked for:
///
/// ```sh
/// CONVT_E2E_ACCOUNT_URL=http://localhost:3000 CONVT_E2E_DEVICE_TOKEN=cvd_... \
/// CONVT_E2E_DESKTOP_TOKEN=cvd_... CONVT_E2E_INPUT=/tmp/photo.png \
/// CONVT_E2E_SLOW_INPUT=/tmp/long.mp4 \
///   cargo test -p convt-app live -- --ignored --nocapture
/// ```
///
/// The Pro device's key comes from /api/device/license and must verify with
/// the key this build embeds (`.convt-dev/license.pub`).
mod live {
    use std::time::Instant;

    use convt_core::Options;
    use convt_license::client::{self, KeyStore};
    use gpui_kit::{AppContext as _, Entity, TestAppContext};

    use super::app_support::*;
    use super::*;
    use crate::model::{AppState, Paths};
    use crate::update::UpdateConfig;

    fn env(name: &str) -> String {
        std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
    }

    fn live_app(
        cx: &mut TestAppContext,
        token: &str,
        key: Option<String>,
    ) -> (tempfile::TempDir, Entity<AppState>) {
        let url = env("CONVT_E2E_ACCOUNT_URL");
        let dir = tempfile::tempdir().unwrap();
        if let Some(key) = key {
            std::fs::write(dir.path().join("license.key"), key).unwrap();
        }
        let session = Session {
            email: String::new(),
            token: token.into(),
        };
        std::fs::write(
            dir.path().join("account.json"),
            serde_json::to_string(&session).unwrap(),
        )
        .unwrap();
        let paths = Paths {
            settings: None,
            history: Some(dir.path().join("history.db")),
            presets: None,
            license: client::Config {
                enforce: true,
                public_key: convt_license::public_key(),
                build_date: "2026-10-01".into(),
                trial_file: Some(dir.path().join("trial")),
                store: KeyStore::File(dir.path().join("license.key")),
            },
            account_url: url.clone(),
            account_api: Arc::new(account::Http::new(&url)),
            update: UpdateConfig {
                key: None,
                fetch: Arc::new(NoUpdates),
                target: ("linux-x86_64", "AppImage"),
                install: None,
            },
        };
        cx.executor().allow_parking();
        let app = cx.update(|cx| {
            let app = cx.new(|cx| AppState::new(Arc::new(NoPacks), paths, cx));
            app.update(cx, |s, cx| {
                s.set_cloud_api(Arc::new(Http::new()), Duration::from_millis(500));
                s.update_settings(|s| s.cloud_consent = true, cx);
            });
            app
        });
        (dir, app)
    }

    fn finish(cx: &mut TestAppContext, app: &Entity<AppState>, id: crate::jobs::JobId) -> Status {
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            cx.run_until_parked();
            let status = cx.read(|cx| app.read(cx).entry(id).unwrap().status.clone());
            if status.is_finished() {
                return status;
            }
            assert!(Instant::now() < deadline, "timed out; last {status:?}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    #[gpui_kit::test]
    #[ignore = "needs the local web, API, worker and storage stack"]
    fn converts_on_the_local_cloud(cx: &mut TestAppContext) {
        let url = env("CONVT_E2E_ACCOUNT_URL");
        let token = env("CONVT_E2E_DEVICE_TOKEN");
        let input = PathBuf::from(env("CONVT_E2E_INPUT"));
        // The app's own renewal call fetches the Pro key it then checks.
        let key = account::Api::current_key(&account::Http::new(&url), &token, "0.0.0")
            .expect("renewal")
            .key
            .expect("a Pro key");
        let (dir, app) = live_app(cx, &token, Some(key));
        let access = cx.read(|cx| app.read(cx).cloud_access());
        eprintln!("access with the Pro device: {access:?}");
        assert_eq!(access, CloudAccess::Ready);
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        let webp = format_by_id("webp").unwrap();
        let started = Instant::now();
        let ids = app
            .update(cx, |s, cx| {
                s.convert_in_cloud(
                    std::slice::from_ref(&input),
                    webp,
                    &Options::default(),
                    Output::Dir(out.clone()),
                    cx,
                )
            })
            .unwrap();
        let status = finish(cx, &app, ids[0]);
        eprintln!("converted in {:?}: {status:?}", started.elapsed());
        let Status::Done(files) = status else {
            panic!("not converted")
        };
        let kept = std::env::temp_dir().join("convt-cloud-e2e-result.webp");
        std::fs::copy(&files[0], &kept).unwrap();
        eprintln!("copied the result to {}", kept.display());
        assert_eq!(files[0].file_name().unwrap(), "photo.webp");
        assert!(std::fs::read(&files[0]).unwrap().starts_with(b"RIFF"));

        // Stop while the cloud converts a long video: cancelled here and on
        // the server.
        let slow = PathBuf::from(env("CONVT_E2E_SLOW_INPUT"));
        let ids = app
            .update(cx, |s, cx| {
                s.convert_in_cloud(
                    &[slow],
                    format_by_id("webm").unwrap(),
                    &Options::default(),
                    Output::Dir(out.clone()),
                    cx,
                )
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        // No progress means uploaded and started: the cloud is converting.
        while cx.read(|cx| app.read(cx).entry(ids[0]).unwrap().status.clone())
            != Status::Running(None)
        {
            assert!(Instant::now() < deadline, "never started");
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_secs(2));
        app.update(cx, |s, _| s.cancel(ids[0]));
        let status = finish(cx, &app, ids[0]);
        eprintln!("after Stop: {status:?}");
        assert_eq!(status, Status::Cancelled);
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);

        // A signed-in account without Pro is refused by convt.app.
        let desktop = env("CONVT_E2E_DESKTOP_TOKEN");
        let (_d, app) = live_app(cx, &desktop, None);
        let access = cx.read(|cx| app.read(cx).cloud_access());
        eprintln!("access with the Desktop-only device: {access:?}");
        assert_eq!(access, CloudAccess::NeedsPro);
        let job = Job {
            output: Output::Dir(out.clone()),
            ..Job::new(&input, webp)
        };
        let cloud = Cloud {
            api: Arc::new(Http::new()),
            credentials: Credentials::new(Arc::new(account::Http::new(&url)), desktop),
            poll: Duration::from_millis(500),
            link_reuse: LINK_REUSE,
        };
        let e = run(&cloud, &job, &|_| {}, &Cancel::new()).unwrap_err();
        eprintln!("Desktop-only device: {} ({})", e.message, e.kind);
        assert_eq!(e.kind, "cloud_pro");
    }
}

#[test]
fn pages_follow_the_names_workers_publish() {
    let files = |names: &[&str]| -> Vec<(PathBuf, String)> {
        names
            .iter()
            .map(|n| (PathBuf::from(n), n.to_string()))
            .collect()
    };
    // Listed sorted, as the server returns them: page 2 before page 1.
    assert_eq!(
        pages_of(&files(&["input-10.png", "input-2.png", "input.png"])),
        [9, 1, 0]
    );
    assert_eq!(pages_of(&files(&["2.png", "1.png"])), [1, 0]);
    assert_eq!(pages_of(&files(&["input.webp"])), [0]);
    // Names that don't tell pages apart keep their order.
    assert_eq!(pages_of(&files(&["a.png", "b.png"])), [0, 1]);
}

#[test]
fn later_downloads_ask_for_fresh_links() {
    let fake = Arc::new(Fake {
        outputs: vec![("input-2.webp", b"two"), ("input.webp", b"one")],
        ..Fake::default()
    });
    fake.statuses
        .lock()
        .unwrap()
        .push_back(Ok(remote(RemoteStatus::Succeeded, None)));
    let (_dir, file) = input();
    let mut cloud = cloud(fake.clone(), None);
    // Every link counts as stale, as after a long first download.
    cloud.link_reuse = Duration::ZERO;
    run_quietly(&cloud, &webp(&file), &Cancel::new()).unwrap();
    let tail: Vec<String> = fake
        .calls()
        .into_iter()
        .skip_while(|c| !c.starts_with("outputs"))
        .collect();
    assert_eq!(
        tail,
        [
            "outputs job_1",
            "outputs job_1",
            "download https://storage.test/input-2.webp?listing=2",
            "outputs job_1",
            "download https://storage.test/input.webp?listing=3",
        ]
    );
}

#[test]
fn stop_after_the_last_download_publishes_nothing() {
    let fake = Arc::new(Fake {
        outputs: vec![("input.webp", b"webp")],
        stop_on_last_download: true,
        ..Fake::default()
    });
    fake.statuses
        .lock()
        .unwrap()
        .push_back(Ok(remote(RemoteStatus::Succeeded, None)));
    let (dir, file) = input();
    let e = run_quietly(&cloud(fake.clone(), None), &webp(&file), &Cancel::new()).unwrap_err();
    assert_eq!(e.kind, "cancelled");
    assert_eq!(listing(dir.path()), ["photo.png"]);
    assert_eq!(
        fake.calls().last().map(String::as_str),
        Some("cancel job_1")
    );
}
