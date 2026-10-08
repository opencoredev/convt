//! Headless window tests. They open real windows on GPUI's test platform,
//! click the same elements a user would and run real conversions with the
//! default registry, so they need no display.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use convt_core::{Background, Ctx, Engine, Options, Preset, Registry, VideoCodec, format_by_id};
use convt_license::account::{Api, ApiError, Session, challenge_of};
use convt_license::client::{self, KeyStore};
use convt_license::{License, Plan};
use ed25519_dalek::SigningKey;
use gpui_kit::component::input::InputState;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, ClipboardEntry, Decorations, ElementId, Entity,
    ImageFormat, Render, SharedString, TestAppContext, Tiling, Window, WindowBounds, WindowOptions,
    point, px, size,
};
use tempfile::TempDir;

use super::first_run::{FirstRunView, Question, Screen, Stage};
use super::main_window::{MainView, Page};
use super::quick::QuickView;
use super::settings_window::{SettingsTab, SettingsView};
use super::{AboutView, Open, PopoverView, menus, theme};
use crate::account::{Access, Provider, Refresh, SignIn};
use crate::cloud::CloudAccess;
use crate::history::Outcome;
use crate::jobs::JobId;
use crate::model::{AppState, PackPhase, Paths, Shared};
use crate::pack::{self, Failure, FailureKind};
use crate::request::{Request, Source};
use crate::tray::{self, Indicator};
use crate::update::{Fetch, FetchError, Update, UpdateConfig};
use convt_license::client::BUY_URL;

struct Fixture {
    dir: TempDir,
    app: Entity<AppState>,
    /// What the app asks convt.app; scripted, never the network.
    api: Arc<TestApi>,
    /// What the update check downloads; scripted, never the network.
    releases: Arc<TestReleases>,
}

/// A scripted update server. It counts every fetch, so tests can prove a
/// launch or a switched-off setting made no request.
#[derive(Default)]
struct TestReleases {
    fetches: AtomicUsize,
    answer: Mutex<Option<Result<Vec<u8>, FetchError>>>,
    /// Runs during the next fetch, such as breaking the settings file.
    during: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl TestReleases {
    fn fetches(&self) -> usize {
        self.fetches.load(Ordering::SeqCst)
    }
    fn serve(&self, answer: Result<Vec<u8>, FetchError>) {
        *self.answer.lock().unwrap() = Some(answer);
    }
}

impl Fetch for TestReleases {
    fn fetch(&self) -> Result<Vec<u8>, FetchError> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        if let Some(during) = self.during.lock().unwrap().take() {
            during();
        }
        self.answer
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(Err(FetchError::Offline))
    }
}

/// The base URL the fixtures sign in to. Nothing listens there.
const ACCOUNT_URL: &str = "https://convt.test";

/// A scripted convt.app. It counts every call, so tests can prove that a
/// link or a launch reached the network or didn't.
struct TestApi {
    exchanges: AtomicUsize,
    renewals: AtomicUsize,
    sign_outs: AtomicUsize,
    exchange: Mutex<Result<Session, ApiError>>,
    key: Mutex<Result<Option<String>, ApiError>>,
    /// The last code and verifier the app traded.
    traded: Mutex<Option<(String, String)>>,
    /// Keeps `exchange` from answering while set, to test what happens meanwhile.
    hold_exchange: AtomicBool,
}

impl Default for TestApi {
    fn default() -> Self {
        Self {
            exchanges: AtomicUsize::new(0),
            renewals: AtomicUsize::new(0),
            sign_outs: AtomicUsize::new(0),
            exchange: Mutex::new(Err(ApiError::Rejected)),
            key: Mutex::new(Err(ApiError::Offline)),
            traded: Mutex::new(None),
            hold_exchange: AtomicBool::new(false),
        }
    }
}

impl TestApi {
    fn calls(&self) -> (usize, usize, usize) {
        (
            self.exchanges.load(Ordering::SeqCst),
            self.renewals.load(Ordering::SeqCst),
            self.sign_outs.load(Ordering::SeqCst),
        )
    }
    fn answer_key(&self, key: Result<Option<String>, ApiError>) {
        *self.key.lock().unwrap() = key;
    }
}

impl Api for TestApi {
    fn exchange(&self, code: &str, verifier: &str) -> Result<Session, ApiError> {
        self.exchanges.fetch_add(1, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.hold_exchange.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        *self.traded.lock().unwrap() = Some((code.into(), verifier.into()));
        self.exchange.lock().unwrap().clone()
    }
    fn current_key(
        &self,
        token: &str,
        version: &str,
    ) -> Result<convt_license::account::LicenseReply, ApiError> {
        assert_eq!(version, crate::account::VERSION);
        assert!(!token.is_empty());
        self.renewals.fetch_add(1, Ordering::SeqCst);
        Ok(convt_license::account::LicenseReply {
            key: self.key.lock().unwrap().clone()?,
            access: None,
        })
    }
    fn sign_out(&self, _: &str) -> Result<(), ApiError> {
        self.sign_outs.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// The pack backend for tests that aren't about the pack: this machine's
/// registry, no pack, and nothing to download.
struct SystemPacks;

impl pack::Backend for SystemPacks {
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
    ) -> Result<PathBuf, Failure> {
        panic!("tests never download")
    }
    fn remove(&self) -> Result<(), String> {
        panic!("tests never remove a real pack")
    }
    fn registry(&self) -> Registry {
        convt_engines::default_registry()
    }
    fn registry_without_documents(&self) -> Registry {
        convt_engines::registry_without_documents()
    }
}

/// "Converts" documents to PDF and text, standing in for LibreOffice so the
/// tests don't depend on what this machine has installed.
struct TestOffice;

impl Engine for TestOffice {
    fn id(&self) -> &'static str {
        "test-office"
    }
    fn steps(&self) -> Vec<convt_core::Step> {
        let f = |id| format_by_id(id).unwrap();
        ["docx", "odt", "xlsx"]
            .into_iter()
            .flat_map(|from| {
                ["pdf", "txt"].into_iter().map(move |to| convt_core::Step {
                    from: f(from),
                    to: f(to),
                })
            })
            .filter(|s| s.from != s.to)
            .collect()
    }
    fn convert(&self, ctx: &Ctx, _: &Path, out_dir: &Path) -> convt_core::Result<Vec<PathBuf>> {
        for _ in 0..20 {
            ctx.check()?;
            std::thread::sleep(Duration::from_millis(10));
        }
        let out = ctx.artifact(out_dir, 0);
        std::fs::write(&out, b"%PDF-1.4 test")?;
        Ok(vec![out])
    }
}

/// A scripted document pack: it downloads 150 MB in 10 MB steps, can hold
/// mid-download, fail as told, and counts every call.
#[derive(Default)]
struct TestPacks {
    unconfigured: bool,
    installed: AtomicBool,
    rejected: Mutex<Option<String>>,
    /// How the next install ends; `None` installs.
    next: Mutex<Option<Failure>>,
    /// Holds the download after its first step until cancelled or released.
    hold: AtomicBool,
    /// Holds a removal until released.
    hold_remove: AtomicBool,
    installs: AtomicUsize,
    removes: AtomicUsize,
}

const PACK_SIZE: u64 = 150_000_000;

impl TestPacks {
    fn installs(&self) -> usize {
        self.installs.load(Ordering::SeqCst)
    }

    fn fail_next(&self, kind: FailureKind, message: &str) {
        *self.next.lock().unwrap() = Some(Failure {
            kind,
            message: message.into(),
        });
    }
}

impl pack::Backend for TestPacks {
    fn offer(&self) -> pack::Offer {
        pack::Offer {
            configured: !self.unconfigured,
            download: Some(PACK_SIZE),
            installed: Some(410_000_000),
            destination: Some(home().join(".local/share/convt/packs/documents")),
        }
    }
    fn status(&self) -> pack::Status {
        if self.installed.load(Ordering::SeqCst) {
            pack::Status::Installed("/data/convt/packs/documents/soffice".into())
        } else if let Some(reason) = self.rejected.lock().unwrap().clone() {
            pack::Status::Rejected(reason)
        } else {
            pack::Status::NotInstalled
        }
    }
    fn install(
        &self,
        progress: &dyn Fn(pack::Progress),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<PathBuf, Failure> {
        self.installs.fetch_add(1, Ordering::SeqCst);
        for step in 1..=15 {
            if cancelled() {
                return Err(Failure {
                    kind: FailureKind::Cancelled,
                    message: "cancelled".into(),
                });
            }
            progress(pack::Progress::Download {
                bytes: step * 10_000_000,
                total: Some(PACK_SIZE),
            });
            while self.hold.load(Ordering::SeqCst) && !cancelled() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        if let Some(failure) = self.next.lock().unwrap().take() {
            return Err(failure);
        }
        progress(pack::Progress::Verifying);
        progress(pack::Progress::Installing);
        self.installed.store(true, Ordering::SeqCst);
        *self.rejected.lock().unwrap() = None;
        Ok("/data/convt/packs/documents/soffice".into())
    }
    fn remove(&self) -> Result<(), String> {
        self.removes.fetch_add(1, Ordering::SeqCst);
        while self.hold_remove.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }
        self.installed.store(false, Ordering::SeqCst);
        *self.rejected.lock().unwrap() = None;
        Ok(())
    }
    fn registry(&self) -> Registry {
        let mut registry = Registry::new();
        registry.register(Arc::new(convt_engines::image::ImageEngine));
        registry.register(Arc::new(convt_engines::svg::SvgEngine));
        if self.installed.load(Ordering::SeqCst) {
            registry.register(Arc::new(TestOffice));
        }
        registry
    }
    fn registry_without_documents(&self) -> Registry {
        let mut registry = Registry::new();
        registry.register(Arc::new(convt_engines::image::ImageEngine));
        registry.register(Arc::new(convt_engines::svg::SvgEngine));
        registry
    }
}

/// The build date licensed fixtures pretend to have.
const BUILD_DATE: &str = "2026-10-01";

impl Fixture {
    /// A build from source, which needs no license.
    fn new(cx: &mut TestAppContext) -> Self {
        Self::build(cx, |_| client::Config {
            enforce: false,
            public_key: None,
            build_date: BUILD_DATE.into(),
            trial_file: None,
            store: KeyStore::None,
        })
    }

    /// A build that checks licenses against [`test_key`], keeping the trial
    /// and the key in the fixture directory. `trial` is the trial start date
    /// and `key` a license key stored before the app starts.
    fn licensed(cx: &mut TestAppContext, trial: Option<&str>, key: Option<&str>) -> Self {
        Self::build(cx, |dir| {
            if let Some(started) = trial {
                std::fs::write(dir.join("trial"), started).unwrap();
            }
            if let Some(key) = key {
                std::fs::write(dir.join("license.key"), key).unwrap();
            }
            client::Config {
                enforce: true,
                public_key: Some(test_key().verifying_key()),
                build_date: BUILD_DATE.into(),
                trial_file: Some(dir.join("trial")),
                store: KeyStore::File(dir.join("license.key")),
            }
        })
    }

    fn build(cx: &mut TestAppContext, license: impl FnOnce(&Path) -> client::Config) -> Self {
        Self::build_with(cx, Arc::new(SystemPacks), license)
    }

    /// A licensed build already signed in to convt.app as `email`.
    fn signed_in(cx: &mut TestAppContext, key: Option<&str>, email: &str) -> Self {
        let session = Session {
            email: email.into(),
            token: "cvd_test_token".into(),
        };
        Self::build(cx, |dir| {
            std::fs::write(dir.join("trial"), "2026-09-30").unwrap();
            if let Some(key) = key {
                std::fs::write(dir.join("license.key"), key).unwrap();
            }
            std::fs::write(
                dir.join("account.json"),
                serde_json::to_string(&session).unwrap(),
            )
            .unwrap();
            client::Config {
                enforce: true,
                public_key: Some(test_key().verifying_key()),
                build_date: BUILD_DATE.into(),
                trial_file: Some(dir.join("trial")),
                store: KeyStore::File(dir.join("license.key")),
            }
        })
    }

    /// A licensed build whose document pack is `packs`, with `key` stored.
    fn licensed_with_packs(
        cx: &mut TestAppContext,
        packs: Arc<TestPacks>,
        key: Option<&str>,
    ) -> Self {
        Self::build_with(cx, packs, |dir| {
            if let Some(key) = key {
                std::fs::write(dir.join("license.key"), key).unwrap();
            }
            client::Config {
                enforce: true,
                public_key: Some(test_key().verifying_key()),
                build_date: BUILD_DATE.into(),
                trial_file: Some(dir.join("trial")),
                store: KeyStore::File(dir.join("license.key")),
            }
        })
    }

    /// A build from source whose document pack is `packs`.
    fn with_packs(cx: &mut TestAppContext, packs: Arc<TestPacks>) -> Self {
        Self::build_with(cx, packs, |_| client::Config {
            enforce: false,
            public_key: None,
            build_date: BUILD_DATE.into(),
            trial_file: None,
            store: KeyStore::None,
        })
    }

    fn build_with(
        cx: &mut TestAppContext,
        packs: Arc<dyn pack::Backend>,
        license: impl FnOnce(&Path) -> client::Config,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let api = Arc::new(TestApi::default());
        let releases = Arc::new(TestReleases::default());
        let paths = Paths {
            settings: Some(dir.path().join("settings.toml")),
            history: Some(dir.path().join("history.db")),
            presets: Some(dir.path().join("presets")),
            license: license(dir.path()),
            account_url: ACCOUNT_URL.into(),
            account_api: api.clone(),
            update: UpdateConfig {
                key: Some(update_key().verifying_key()),
                fetch: releases.clone(),
                target: ("linux-x86_64", "AppImage"),
            },
        };
        // Conversions run on real job threads that wake the UI.
        cx.executor().allow_parking();
        let app = cx.update(|cx| {
            cx.set_app_identity("app.convt.desktop", "convt");
            gpui_kit::init(cx);
            theme::init(cx);
            let app = cx.new(|cx| AppState::new(packs, paths, cx));
            cx.set_global(Shared(app.clone()));
            app
        });
        Self {
            dir,
            app,
            api,
            releases,
        }
    }

    /// A 4x4 still in the fixture directory. The name's extension picks the format.
    fn png(&self, name: &str) -> PathBuf {
        self.rgb(name)
    }

    fn bmp(&self, name: &str) -> PathBuf {
        self.rgb(name)
    }

    fn rgb(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        image::RgbImage::from_pixel(4, 4, image::Rgb([200, 40, 40]))
            .save(&path)
            .unwrap();
        path
    }

    /// A small Word document in the fixture directory.
    fn docx(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, b"PK\x03\x04 not really a document").unwrap();
        path
    }

    fn quick(
        &self,
        request: Request,
        cx: &mut TestAppContext,
    ) -> (AnyWindowHandle, Entity<QuickView>) {
        let app = self.app.clone();
        open(cx, move |window, cx| {
            cx.new(|cx| QuickView::new(app, request, window, cx))
        })
    }

    fn main(&self, cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<MainView>) {
        let app = self.app.clone();
        open(cx, move |window, cx| {
            cx.new(|cx| MainView::new(app, window, cx))
        })
    }

    fn settings(
        &self,
        tab: SettingsTab,
        cx: &mut TestAppContext,
    ) -> (AnyWindowHandle, Entity<SettingsView>) {
        let app = self.app.clone();
        let (window, view) = open(cx, move |window, cx| {
            cx.new(|cx| SettingsView::new(app, window, cx))
        });
        view.update(cx, |v, cx| v.set_tab(tab, cx));
        (window, view)
    }

    fn last_job(&self, cx: &mut TestAppContext) -> JobId {
        cx.read(|cx| {
            self.app
                .read(cx)
                .queue
                .entries
                .last()
                .expect("a job was queued")
                .id
        })
    }

    fn jobs(&self, cx: &mut TestAppContext) -> usize {
        cx.read(|cx| self.app.read(cx).queue.entries.len())
    }

    fn settings_file(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("settings.toml")).unwrap_or_default()
    }
}

fn open<V: Render>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static,
) -> (AnyWindowHandle, Entity<V>) {
    cx.update(|cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.), px(0.)),
                size: size(px(1040.), px(900.)),
            })),
            ..Default::default()
        };
        let (handle, view) = super::open_window(options, cx, build).expect("open a test window");
        (handle, view)
    })
}

fn id(name: &str) -> ElementId {
    ElementId::Name(SharedString::from(name.to_string()))
}

fn click(cx: &mut TestAppContext, handle: AnyWindowHandle, name: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(id(name), cx);
    })
    .unwrap();
    cx.run_until_parked();
}

/// The label of an element, or `None` if the window doesn't show it.
fn label(cx: &mut TestAppContext, handle: AnyWindowHandle, name: &str) -> Option<String> {
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id(name))
            .map(|e| e.label().unwrap_or_default().to_string())
    })
    .unwrap()
}

/// A switch's or checkbox's state, as screen readers get it.
fn toggled(cx: &mut TestAppContext, handle: AnyWindowHandle, name: &str) -> Option<bool> {
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id(name)).and_then(|e| e.checked())
    })
    .unwrap()
}

fn shown(cx: &mut TestAppContext, handle: AnyWindowHandle, name: &str) -> bool {
    label(cx, handle, name).is_some()
}

/// Jobs run on real threads, so this polls in real time rather than on the
/// test clock.
fn wait_for_label(
    cx: &mut TestAppContext,
    handle: AnyWindowHandle,
    name: &str,
    done: impl Fn(&str) -> bool,
) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(text) = label(cx, handle, name)
            && done(&text)
        {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "{name} never matched; last label {:?}",
            label(cx, handle, name)
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Polls the app state in real time until `done` holds.
fn wait_until(cx: &mut TestAppContext, what: &str, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        cx.run_until_parked();
        if cx.read(|cx| done(cx)) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn is_jpeg(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|bytes| bytes.starts_with(&[0xFF, 0xD8, 0xFF]))
}

fn is_png(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|bytes| bytes.starts_with(b"\x89PNG"))
}

fn is_webp(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|bytes| bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP")
}

fn cli(files: Vec<PathBuf>, to: Option<&str>, preset: Option<&str>) -> Request {
    Request {
        files,
        to: to.map(Into::into),
        preset: preset.map(Into::into),
        source: Some(Source::Cli),
        ..Request::default()
    }
}

/// The signing key licensed fixtures trust. Made up for tests.
fn test_key() -> SigningKey {
    SigningKey::from_bytes(&[11; 32])
}

fn license_key(email: &str, updates_until: &str) -> String {
    let license = License {
        id: "test".into(),
        email: email.into(),
        plan: Plan::Desktop,
        issued: "2026-09-01".into(),
        updates_until: updates_until.into(),
    };
    convt_license::sign(&license, &test_key())
}

/// The open window of kind `V` and its view.
fn window_of<V: 'static>(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<V>) {
    cx.read(|cx| Open::<V>::get(cx).expect("the window is open"))
}

fn set_input(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    input: &Entity<InputState>,
    value: &str,
) {
    let value = value.to_string();
    let input = input.clone();
    cx.update_window(window, |_, window, cx| {
        input.update(cx, |s, cx| s.set_value(value, window, cx))
    })
    .unwrap();
}

fn type_key(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<SettingsView>,
    key: &str,
) {
    let input = cx.read(|cx| view.read(cx).license_key.clone());
    set_input(cx, window, &input, key);
}

#[gpui_kit::test]
fn explorer_request_opens_activity_and_converts_beside_the_input(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let png = f.png("explorer selection.png");
    let mut request = cli(vec![png], Some("jpeg"), None);
    request.show_progress = true;
    cx.update(|cx| super::route(request, cx));
    window_of::<MainView>(cx);
    cx.read(|cx| assert!(!f.app.read(cx).quit_when_idle));
    let job = f.last_job(cx);
    wait_until(cx, "Explorer job", |cx| {
        f.app
            .read(cx)
            .entry(job)
            .is_some_and(|e| e.status.is_finished())
    });
    assert!(is_jpeg(&f.dir.path().join("explorer selection.jpg")));
}

#[gpui_kit::test]
fn a_cli_request_with_a_target_converts_in_place_without_a_window(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let png = f.png("red dot.png");
    cx.update(|cx| super::route(cli(vec![png.clone()], Some("jpeg"), None), cx));
    assert!(
        cx.update(|cx| cx.windows().is_empty()),
        "a silent conversion opens no window"
    );
    let job = f.last_job(cx);
    cx.read(|cx| assert!(f.app.read(cx).quit_when_idle, "nothing keeps the app open"));
    wait_until(cx, "the job to finish", |cx| {
        f.app
            .read(cx)
            .entry(job)
            .is_some_and(|e| e.status.is_finished())
    });
    assert!(is_jpeg(&f.dir.path().join("red dot.jpg")));
    cx.read(|cx| {
        let recent = &f.app.read(cx).recent;
        assert_eq!(recent.len(), 1);
        assert!(
            matches!(&recent[0].outcome, Outcome::Done(out) if out[0].ends_with("red dot.jpg"))
        );
    });
    assert!(cx.update(|cx| cx.windows().is_empty()));

    // While it ran, the tray would have shown a spinner and nothing more.
    cx.read(|cx| assert_eq!(tray::indicator(f.app.read(cx)), Some(Indicator::Idle)));
}

#[gpui_kit::test]
fn the_tray_shows_a_spinner_only_while_jobs_run(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    cx.read(|cx| assert_eq!(tray::indicator(f.app.read(cx)), Some(Indicator::Idle)));
    // A slow-to-start batch: queue more jobs than run at once.
    let files: Vec<PathBuf> = (0..4).map(|i| f.png(&format!("{i}.png"))).collect();
    let app = f.app.clone();
    cx.update(|cx| {
        app.update(cx, |s, cx| {
            s.update_settings(|s| s.concurrency = Some(1), cx);
            let to = convt_core::format_by_id("jpeg").unwrap();
            s.convert(&files, to, &Options::default(), cx).unwrap();
            assert_eq!(tray::indicator(s), Some(Indicator::Busy));
            s.update_settings(|s| s.menu_bar_icon = false, cx);
            assert_eq!(tray::indicator(s), None, "no icon when it's turned off");
        })
    });
    wait_until(cx, "the batch to finish", |cx| {
        app.read(cx).queue.active() == 0
    });
}

#[gpui_kit::test]
fn links_and_requests_that_need_a_choice_open_quick_convert(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let png = f.png("a.png");
    let odd = f.dir.path().join("notes.unknown");
    std::fs::write(&odd, "x").unwrap();
    let windows = |cx: &mut TestAppContext| cx.update(|cx| cx.windows().len());

    // A link never converts by itself, even with a target.
    let link =
        crate::request::parse_url(&format!("convt://convert?file={}&to=jpeg", png.display()))
            .unwrap();
    cx.update(|cx| super::route(link, cx));
    assert_eq!(windows(cx), 1);
    assert_eq!(f.jobs(cx), 0, "a link must not start a conversion");

    // No target: "More options…" in the Finder menu sends this.
    cx.update(|cx| super::route(cli(vec![png.clone()], None, None), cx));
    assert_eq!(windows(cx), 2);

    // A file that can't be converted needs a confirmation.
    cx.update(|cx| super::route(cli(vec![png, odd], Some("jpeg"), None), cx));
    assert_eq!(windows(cx), 3);
    assert_eq!(f.jobs(cx), 0);
}

#[gpui_kit::test]
fn quick_convert_sums_up_its_options_in_one_row(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let (window, view) = f.quick(cli(vec![f.png("beach.png")], None, None), cx);
    click(cx, window, "to-webp");
    // Collapsed: one row that says what the options are.
    assert_eq!(
        label(cx, window, "options-summary").as_deref(),
        Some("Balanced · Original size · Transparent")
    );
    assert!(!shown(cx, window, "quality-best"));
    click(cx, window, "options-toggle");
    click(cx, window, "quality-best");
    click(cx, window, "options-toggle");
    assert!(!shown(cx, window, "quality-best"));
    assert_eq!(
        label(cx, window, "options-summary").as_deref(),
        Some("Best · Original size · Transparent")
    );

    // The name keeps the file's stem, as text, until it's clicked.
    assert_eq!(
        label(cx, window, "file-name-edit").as_deref(),
        Some("beach.webp")
    );
    assert!(!shown(cx, window, "file-name"));
    click(cx, window, "file-name-edit");
    assert!(shown(cx, window, "file-name"));
    cx.read(|cx| assert!(view.read(cx).editing_name));
}

#[gpui_kit::test]
fn quick_convert_waits_for_a_click_and_applies_options(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let png = f.png("a.png");
    let request = Request {
        source: Some(Source::Url),
        ..cli(vec![png], Some("jpeg"), None)
    };
    let (window, view) = f.quick(request, cx);
    cx.read(|cx| {
        let view = view.read(cx);
        assert!(view.jobs.is_empty(), "a link must not start a conversion");
        assert_eq!(view.to.map(|f| f.id), Some("jpeg"));
        assert_eq!(view.file_name.read(cx).value(), "a.jpg");
    });
    assert_eq!(label(cx, window, "save-to").as_deref(), Some("Same folder"));

    // Picking another target renames the output and offers its options.
    click(cx, window, "to-webp");
    click(cx, window, "options-toggle");
    cx.read(|cx| {
        let view = view.read(cx);
        assert_eq!(view.to.map(|f| f.id), Some("webp"));
        assert_eq!(view.file_name.read(cx).value(), "a.webp");
    });
    click(cx, window, "quality-best");
    click(cx, window, "size");
    click(cx, window, "size-2048");
    cx.read(|cx| {
        let options = view.read(cx).conversion_options();
        assert_eq!((options.quality, options.max_size), (Some(95), Some(2048)));
    });

    // A new file name is used as given.
    let input = cx.read(|cx| view.read(cx).file_name.clone());
    set_input(cx, window, &input, "renamed.webp");
    click(cx, window, "convert");
    let job = f.last_job(cx);
    wait_for_label(cx, window, &format!("status-{job}"), |s| {
        s.starts_with("Saved")
    });
    assert!(is_webp(&f.dir.path().join("renamed.webp")));
    // The heading follows the job, as the footer does.
    assert_eq!(
        label(cx, window, "jobs-title").as_deref(),
        Some("Converted")
    );
    for done in ["show-in-folder", "open", "close"] {
        assert!(shown(cx, window, done), "{done} is missing");
    }
    assert!(!shown(cx, window, "convert"), "the picker should be gone");
}

#[test]
fn a_typed_output_name_stays_one_file_in_the_folder() {
    use super::quick::output_name;
    for (typed, wanted) in [
        ("renamed.webp", "renamed.webp"),
        ("  renamed.webp  ", "renamed.webp"),
        ("renamed.WEBP", "renamed.WEBP"),
        // A missing or different extension gets the target's.
        ("renamed", "renamed.webp"),
        ("renamed.png", "renamed.png.webp"),
        ("my holiday", "my holiday.webp"),
        ("..hidden", "..hidden.webp"),
    ] {
        assert_eq!(output_name(typed, "webp").as_deref(), Ok(wanted), "{typed}");
    }
    for typed in [
        "",
        "   ",
        ".",
        "..",
        "...",
        "/tmp/x.webp",
        "../x.webp",
        "sub/x.webp",
        "..\\x.webp",
        "C:\\x.webp",
        "x\0.webp",
        "x\n.webp",
    ] {
        assert!(
            output_name(typed, "webp").is_err(),
            "{typed:?} was accepted"
        );
    }
    assert_eq!(
        output_name("/tmp/x.webp", "webp").unwrap_err(),
        "Use a file name, not a folder path."
    );
    assert_eq!(
        output_name("", "webp").unwrap_err(),
        "Type a name for the file."
    );
    if cfg!(windows) {
        assert!(output_name("C:x.webp", "webp").is_err());
        assert!(output_name("a:b.webp", "webp").is_err());
    }
}

#[gpui_kit::test]
fn quick_convert_refuses_a_file_name_that_leaves_the_folder(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let png = f.png("a.png");
    let (window, view) = f.quick(cli(vec![png], None, None), cx);
    click(cx, window, "to-webp");
    let outside = tempfile::tempdir().unwrap();
    let escape = outside.path().join("x.webp");
    let input = cx.read(|cx| view.read(cx).file_name.clone());
    click(cx, window, "file-name-edit");
    for typed in [
        escape.to_string_lossy().to_string(),
        "../x.webp".to_string(),
        "sub/x.webp".to_string(),
        "..".to_string(),
        " ".to_string(),
    ] {
        set_input(cx, window, &input, &typed);
        click(cx, window, "convert");
        assert!(
            shown(cx, window, "file-name-error"),
            "{typed:?} wasn't refused"
        );
        assert_eq!(f.jobs(cx), 0, "{typed:?} started a conversion");
        cx.read(|cx| assert!(view.read(cx).editing_name));
    }
    assert!(!escape.exists());
    assert!(!f.dir.path().parent().unwrap().join("x.webp").exists());
    assert_eq!(
        label(cx, window, "file-name-error").as_deref(),
        Some("Type a name for the file.")
    );

    // Typing a new name clears the error; one without its extension gets it.
    set_input(cx, window, &input, "");
    cx.update_window(window, |_, window, cx| {
        use gpui_kit::Focusable as _;
        let focus = input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    })
    .unwrap();
    cx.simulate_input(window, "renamed");
    assert!(!shown(cx, window, "file-name-error"));
    cx.read(|cx| assert_eq!(input.read(cx).value(), "renamed"));
    click(cx, window, "convert");
    assert!(!shown(cx, window, "file-name-error"));
    let job = f.last_job(cx);
    wait_for_label(cx, window, &format!("status-{job}"), |s| {
        s.starts_with("Saved")
    });
    assert!(is_webp(&f.dir.path().join("renamed.webp")));
}

#[gpui_kit::test]
fn quick_convert_explains_targets_it_cannot_reach(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let png = f.png("a.png");
    let odd = f.dir.path().join("notes.unknown");
    std::fs::write(&odd, "x").unwrap();

    let (window, view) = f.quick(cli(vec![png.clone()], Some("mp3"), None), cx);
    assert_eq!(
        label(cx, window, "error").as_deref(),
        Some("This file can't become MP3.")
    );
    cx.read(|cx| assert!(view.read(cx).jobs.is_empty()));

    // An unsupported file is skipped on convert.
    let (window, view) = f.quick(cli(vec![png, odd], Some("jpeg"), None), cx);
    assert_eq!(
        label(cx, window, "skipped").as_deref(),
        Some("1 file can't be converted and will be skipped.")
    );
    click(cx, window, "convert");
    cx.read(|cx| assert_eq!(view.read(cx).jobs.len(), 1));
    let job = f.last_job(cx);
    wait_for_label(cx, window, &format!("status-{job}"), |s| {
        s.starts_with("Saved")
    });

    let empty = f.dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    let (window, _) = f.quick(cli(vec![empty], None, None), cx);
    assert_eq!(
        label(cx, window, "error").as_deref(),
        Some("There are no files to convert here.")
    );
}

#[gpui_kit::test]
fn add_files_opens_quick_convert_and_lists_the_results(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let a = f.bmp("a.bmp");
    let b = f.bmp("b.bmp");
    let (window, view) = f.main(cx);
    assert!(shown(cx, window, "empty"));
    assert_eq!(
        label(cx, window, "empty-add-files").as_deref(),
        Some("Choose files…")
    );

    // Add files converts nothing by itself: Quick convert asks for the
    // format, and the same file twice is listed once.
    view.update(cx, |v, cx| v.add(&[a.clone(), b.clone(), a], cx));
    assert_eq!(f.jobs(cx), 0);
    let (quick, quick_view) = last_quick(cx);
    cx.read(|cx| {
        assert_eq!(
            quick_view.read(cx).files,
            [f.dir.path().join("a.bmp"), b.clone()]
        )
    });
    cx.read(|cx| assert!(quick_view.read(cx).to.is_none()));
    click(cx, quick, "to-png");
    click(cx, quick, "convert");
    wait_until(cx, "both files", |cx| f.app.read(cx).recent.len() == 2);
    assert!(is_png(&f.dir.path().join("a.png")) && is_png(&f.dir.path().join("b.png")));
    let records = cx.read(|cx| f.app.read(cx).recent.clone());
    for record in &records {
        let status = label(cx, window, &format!("record-status-{}", record.id)).unwrap();
        assert!(status.starts_with("Done · "), "{status}");
        assert!(shown(cx, window, &format!("show-{}", record.id)));
    }
    assert!(!shown(cx, window, "empty"));

    // Each day clears on its own.
    assert!(!shown(cx, window, "clear-finished"));
    click(cx, window, "clear-today");
    cx.read(|cx| {
        let state = f.app.read(cx);
        assert!(state.recent.is_empty() && state.queue.entries.is_empty());
    });
    assert!(shown(cx, window, "empty"));
}

#[gpui_kit::test]
fn cloud_says_why_it_is_off_and_asks_once_before_uploading(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let png = f.png("a.png");

    // Not ready: Cloud is shown but does nothing.
    let (window, view) = f.quick(cli(vec![png.clone()], Some("webp"), None), cx);
    assert_eq!(
        label(cx, window, "where-local").as_deref(),
        Some(theme::this_machine_label())
    );
    assert_eq!(label(cx, window, "where-cloud").as_deref(), Some("Cloud"));
    click(cx, window, "where-cloud");
    cx.read(|cx| assert!(!view.read(cx).cloud));
    assert!(!shown(cx, window, "cloud-consent"));

    // Ready: picking Cloud asks first, and Convert waits for the answer.
    cx.update(|cx| cx.set_global(super::quick::TestCloud(CloudAccess::Ready)));
    click(cx, window, "where-cloud");
    cx.read(|cx| assert!(view.read(cx).cloud));
    assert!(shown(cx, window, "cloud-consent"));
    click(cx, window, "convert");
    cx.read(|cx| assert!(view.read(cx).jobs.is_empty() && view.read(cx).error.is_none()));
    click(cx, window, "cloud-consent-cancel");
    cx.read(|cx| assert!(!view.read(cx).cloud));
    assert!(!shown(cx, window, "cloud-consent"));
    cx.read(|cx| assert!(!f.app.read(cx).settings.cloud_consent));

    click(cx, window, "where-cloud");
    click(cx, window, "cloud-consent-agree");
    cx.read(|cx| assert!(f.app.read(cx).settings.cloud_consent));
    assert!(f.settings_file().contains("cloud_consent = true"));
    assert!(!shown(cx, window, "cloud-consent"));
    // The cloud takes no options, so it says so instead of offering them.
    assert!(shown(cx, window, "cloud-options"));
    click(cx, window, "where-local");
    assert!(!shown(cx, window, "cloud-options"));
    click(cx, window, "where-cloud");

    // Convert goes to the cloud, not to a local job; what it can't do shows
    // like any other error.
    click(cx, window, "convert");
    cx.read(|cx| {
        let v = view.read(cx);
        assert!(!v.jobs.is_empty() || v.error.is_some());
    });
    if let Some(error) = cx.read(|cx| view.read(cx).error.clone()) {
        assert_eq!(label(cx, window, "error"), Some(error));
    }

    // Asked once: a new window doesn't ask again.
    let (window, view) = f.quick(cli(vec![png], Some("webp"), None), cx);
    click(cx, window, "where-cloud");
    cx.read(|cx| assert!(view.read(cx).cloud));
    assert!(!shown(cx, window, "cloud-consent"));
}

#[gpui_kit::test]
fn activity_marks_jobs_that_ran_in_the_cloud(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let png = f.png("a.png");
    let webp = format_by_id("webp").unwrap();
    let (window, _) = f.main(cx);
    let (local, cloud) = cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            // Waiting jobs that never start: no runner picks them up.
            let local = s.queue.add(&convt_core::Job::new(&png, webp));
            let cloud = s.queue.add_cloud(&convt_core::Job::new(&png, webp));
            for (id, cloud) in [(1, false), (2, true)] {
                s.recent.push(crate::history::Record {
                    id,
                    finished_at: 0,
                    input: png.clone(),
                    to: "webp".into(),
                    outcome: Outcome::Done(vec![png.clone()]),
                    setup: Some(crate::history::Setup {
                        cloud,
                        ..Default::default()
                    }),
                });
            }
            cx.notify();
            (local, cloud)
        })
    });
    assert!(shown(cx, window, &format!("status-{local}")));
    assert!(!shown(cx, window, &format!("job-cloud-{local}")));
    assert_eq!(
        label(cx, window, &format!("job-cloud-{cloud}")).as_deref(),
        Some("Cloud")
    );
    assert!(shown(cx, window, "record-status-1"));
    assert!(!shown(cx, window, "record-cloud-1"));
    assert!(shown(cx, window, "record-cloud-2"));
}

#[gpui_kit::test]
fn a_failed_conversion_can_be_retried(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let broken = f.dir.path().join("broken.bmp");
    std::fs::write(&broken, "not a bmp").unwrap();
    let (window, view) = f.main(cx);
    view.update(cx, |v, cx| v.add(std::slice::from_ref(&broken), cx));
    let (quick, _) = last_quick(cx);
    click(cx, quick, "to-png");
    click(cx, quick, "convert");
    wait_until(cx, "the failure", |cx| f.app.read(cx).recent.len() == 1);
    assert_eq!(label(cx, quick, "jobs-title").as_deref(), Some("Finished"));
    let record = cx.read(|cx| f.app.read(cx).recent[0].clone());
    assert!(matches!(record.outcome, Outcome::Failed(_)));
    assert_eq!(
        label(cx, window, &format!("record-status-{}", record.id)).as_deref(),
        Some("Failed")
    );
    let before = f.jobs(cx);
    click(cx, window, &format!("retry-{}", record.id));
    assert_eq!(f.jobs(cx), before + 1);
    wait_until(cx, "the retry", |cx| f.app.read(cx).recent.len() == 2);
}

#[gpui_kit::test]
fn presets_are_saved_used_and_deleted(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let (window, view) = f.settings(SettingsTab::Presets, cx);

    let fill = |cx: &mut TestAppContext, name: &str, quality: &str| {
        let (n, q) = cx.read(|cx| {
            let v = view.read(cx);
            (v.preset_name.clone(), v.preset_quality.clone())
        });
        set_input(cx, window, &n, name);
        set_input(cx, window, &q, quality);
    };

    fill(cx, "web", "abc");
    click(cx, window, "save-preset");
    assert_eq!(
        label(cx, window, "error").as_deref(),
        Some("Quality must be a whole number.")
    );

    fill(cx, "../web", "80");
    click(cx, window, "save-preset");
    assert_eq!(
        label(cx, window, "error").as_deref(),
        Some("Use letters, numbers, spaces, - and _ for the name.")
    );

    fill(cx, "web", "80");
    click(cx, window, "new-to-webp");
    click(cx, window, "save-preset");
    assert!(!shown(cx, window, "error"));
    let file = f.dir.path().join("presets/web.toml");
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(
        saved.contains("to = \"webp\"") && saved.contains("quality = 80"),
        "{saved}"
    );
    cx.read(|cx| assert!(view.read(cx).preset_to.is_none(), "the form resets"));

    // Quick convert offers the preset, and picking it selects its target.
    let (quick, quick_view) = f.quick(
        Request {
            files: vec![f.png("a.png")],
            ..Request::default()
        },
        cx,
    );
    click(cx, quick, "preset-web");
    cx.read(|cx| {
        let v = quick_view.read(cx);
        assert_eq!(v.to.map(|f| f.id), Some("webp"));
        assert_eq!(v.conversion_options().quality, Some(80));
    });

    click(cx, window, "delete-preset-web");
    assert!(!file.exists());
    assert!(!shown(cx, window, "delete-preset-web"));
}

#[gpui_kit::test]
fn settings_change_and_persist(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let (window, _) = f.settings(SettingsTab::General, cx);
    let auto = crate::settings::auto_concurrency();

    assert_eq!(
        label(cx, window, "output").as_deref(),
        Some("Next to the original")
    );
    // Jobs at once defaults to Auto, one per core.
    assert_eq!(
        label(cx, window, "concurrency"),
        Some(format!("Auto ({auto})"))
    );
    cx.read(|cx| assert_eq!(f.app.read(cx).settings.concurrency(), auto));

    click(cx, window, "jobs");
    click(cx, window, "jobs-2");
    assert_eq!(label(cx, window, "concurrency").as_deref(), Some("2"));
    assert!(f.settings_file().contains("concurrency = 2"));
    assert!(!shown(cx, window, "jobs-menu"), "picking closes the menu");

    click(cx, window, "jobs");
    click(cx, window, "jobs-auto");
    assert_eq!(
        label(cx, window, "concurrency"),
        Some(format!("Auto ({auto})"))
    );
    assert!(!f.settings_file().contains("concurrency"));

    assert!(cx.read(|cx| f.app.read(cx).settings.notifications));
    // Screen readers hear each switch's setting, not just On or Off.
    assert_eq!(
        label(cx, window, "notifications").as_deref(),
        Some("Show a notification")
    );
    assert_eq!(
        label(cx, window, "reveal"),
        Some(format!(
            "Reveal it in {}",
            super::theme::file_manager_name()
        ))
    );
    assert_eq!(toggled(cx, window, "notifications"), Some(true));
    click(cx, window, "notifications");
    assert_eq!(toggled(cx, window, "notifications"), Some(false));
    click(cx, window, "reveal");
    assert_eq!(toggled(cx, window, "reveal"), Some(true));
    let saved = if cfg!(target_os = "macos") {
        click(cx, window, "menu-bar-icon");
        let saved = f.settings_file();
        assert!(saved.contains("menu_bar_icon = false"), "{saved}");
        saved
    } else {
        assert!(
            !shown(cx, window, "menu-bar-icon"),
            "no menu bar switch where it does nothing"
        );
        f.settings_file()
    };
    for line in ["notifications = false", "reveal_when_done = true"] {
        assert!(saved.contains(line), "{line} in {saved}");
    }

    click(cx, window, "output");
    click(cx, window, "output-beside");
    assert_eq!(
        label(cx, window, "output").as_deref(),
        Some("Next to the original")
    );

    // A fresh app reads what was saved.
    let reloaded = crate::settings::Settings::load(&f.dir.path().join("settings.toml")).unwrap();
    assert!(!reloaded.notifications && reloaded.reveal_when_done);
    assert_eq!(reloaded.concurrency, None);
}

fn save_preset(f: &Fixture, cx: &mut TestAppContext, name: &str, preset: Preset) {
    let app = f.app.clone();
    cx.update(|cx| app.update(cx, |s, cx| s.save_preset(name, &preset, cx)))
        .unwrap();
}

#[gpui_kit::test]
fn editing_a_preset_keeps_fields_the_form_does_not_show(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    save_preset(
        &f,
        cx,
        "clip",
        Preset {
            to: Some("mp4".into()),
            options: Options {
                quality: Some(50),
                video_height: Some(720),
                ..Options::default()
            },
        },
    );
    let (window, view) = f.settings(SettingsTab::Presets, cx);
    let input = |cx: &mut TestAppContext, pick: fn(&SettingsView) -> &Entity<InputState>| {
        cx.read(|cx| pick(view.read(cx)).clone())
    };
    let file = f.dir.path().join("presets/clip.toml");
    let before = std::fs::read_to_string(&file).unwrap();

    // A new preset can't take an existing name, in any case.
    for name in ["clip", "CLIP"] {
        let name_input = input(cx, |v| &v.preset_name);
        set_input(cx, window, &name_input, name);
        click(cx, window, "save-preset");
        assert_eq!(
            label(cx, window, "error"),
            Some(format!(
                "There is already a preset named \"{name}\". Use Edit to change it."
            ))
        );
    }
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);

    click(cx, window, "edit-preset-clip");
    cx.read(|cx| {
        let v = view.read(cx);
        assert_eq!(v.editing.as_deref(), Some("clip"));
        assert_eq!(v.preset_name.read(cx).value(), "clip");
        assert_eq!(v.preset_quality.read(cx).value(), "50");
        assert_eq!(v.preset_to.map(|f| f.id), Some("mp4"));
    });
    let quality = input(cx, |v| &v.preset_quality);
    set_input(cx, window, &quality, "70");
    click(cx, window, "save-preset");
    assert!(!shown(cx, window, "error"));
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(
        saved.contains("quality = 70") && saved.contains("video_height = 720"),
        "{saved}"
    );
    cx.read(|cx| assert_eq!(view.read(cx).editing, None, "the form resets"));
    assert!(!shown(cx, window, "cancel-edit"));
}

#[gpui_kit::test]
fn a_preset_for_an_unreachable_format_clears_the_target(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    save_preset(
        &f,
        cx,
        "podcast",
        Preset {
            to: Some("mp3".into()),
            options: Options::default(),
        },
    );
    let png = f.png("a.png");
    let (window, view) = f.quick(
        Request {
            source: None,
            ..cli(vec![png], None, None)
        },
        cx,
    );
    // A preset these files can't use isn't offered.
    assert!(!shown(cx, window, "preset-podcast"));
    cx.read(|cx| assert!(view.read(cx).error.is_none()));

    // Asked for by name, it says why and clears the target.
    let (window, view) = f.quick(
        Request {
            source: None,
            ..cli(vec![f.png("b.png")], None, Some("podcast"))
        },
        cx,
    );
    cx.read(|cx| assert_eq!(view.read(cx).to, None));
    assert!(shown(cx, window, "error"));
    click(cx, window, "convert");
    cx.read(|cx| assert!(view.read(cx).jobs.is_empty(), "nothing converts"));
}

#[gpui_kit::test]
fn quick_convert_keeps_results_after_the_queue_is_cleared(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let png = f.png("a.png");
    let (window, view) = f.quick(cli(vec![png], Some("jpeg"), None), cx);
    click(cx, window, "convert");
    let job = f.last_job(cx);
    wait_for_label(cx, window, &format!("status-{job}"), |s| {
        s.starts_with("Saved")
    });

    let app = f.app.clone();
    cx.update(|cx| app.update(cx, |s, cx| s.clear_records(&[], cx)));
    cx.read(|cx| assert!(f.app.read(cx).entry(job).is_none()));
    assert_eq!(
        label(cx, window, &format!("status-{job}")).as_deref(),
        Some("Saved a.jpg")
    );
    assert!(shown(cx, window, "show-in-folder"));
    cx.read(|cx| assert!(view.read(cx).finished()));
}

#[gpui_kit::test]
fn the_first_conversion_starts_the_trial(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    let (main, _) = f.main(cx);
    assert_eq!(
        label(cx, main, "trial-card").as_deref(),
        Some("Free trial: 7 days, starting with your first conversion.")
    );

    let png = f.png("a.png");
    cx.update(|cx| super::route(cli(vec![png], Some("jpeg"), None), cx));
    wait_until(cx, "the conversion", |cx| f.app.read(cx).recent.len() == 1);
    let started = std::fs::read_to_string(f.dir.path().join("trial")).unwrap();
    assert_eq!(
        started.trim(),
        convt_license::date::from_days(client::today())
    );
    assert_eq!(
        label(cx, main, "trial-card").as_deref(),
        Some("Free trial: 7 days left.")
    );
}

#[gpui_kit::test]
fn the_sidebar_sign_in_button_opens_the_license_tab(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    // Shipped builds have no local trial; only tests keep it on by default.
    f.app.update(cx, |s, cx| {
        s.licensing.disable_local_trial();
        s.license = s.licensing.state();
        cx.notify();
    });
    let (main, _) = f.main(cx);
    assert_eq!(
        label(cx, main, "trial-card").as_deref(),
        Some("Sign in to start your free trial.")
    );
    click(cx, main, "trial-buy");
    let (settings, view) = window_of::<SettingsView>(cx);
    cx.read(|cx| assert_eq!(view.read(cx).tab, SettingsTab::License));
    assert_eq!(
        cx.opened_url(),
        None,
        "Sign in must not open the pricing page"
    );
    settings
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();

    // A refused conversion says to sign in, and offers it.
    let png = f.png("a.png");
    cx.update(|cx| super::route(cli(vec![png], Some("jpeg"), None), cx));
    let (quick, _) = window_of::<QuickView>(cx);
    assert_eq!(
        label(cx, quick, "license-banner").as_deref(),
        Some("Sign in to start your free trial.")
    );
    click(cx, quick, "sign-in-banner");
    let (_, view) = window_of::<SettingsView>(cx);
    cx.read(|cx| assert_eq!(view.read(cx).tab, SettingsTab::License));
    assert_eq!(cx.opened_url(), None);
    assert_eq!(f.jobs(cx), 0);
}

#[gpui_kit::test]
fn an_ended_trial_stops_conversions_until_a_license_is_entered(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, Some("2026-01-01"), None);
    let png = f.png("a.png");
    // The silent conversion is refused, so Quick convert opens to say why.
    cx.update(|cx| super::route(cli(vec![png.clone()], Some("jpeg"), None), cx));
    assert_eq!(f.jobs(cx), 0);
    let quick = cx.update(|cx| cx.windows()[0]);
    let ended = label(cx, quick, "license-banner").unwrap();
    assert!(ended.starts_with("Your convt trial has ended."), "{ended}");
    assert!(shown(cx, quick, "buy"));
    click(cx, quick, "convert");
    assert_eq!(f.jobs(cx), 0);

    // Enter license opens the License tab of Settings.
    click(cx, quick, "enter-license");
    let (settings, view) = window_of::<SettingsView>(cx);
    cx.read(|cx| assert_eq!(view.read(cx).tab, SettingsTab::License));
    assert_eq!(label(cx, settings, "license-status"), Some(ended));

    type_key(cx, settings, &view, "not a key");
    click(cx, settings, "activate");
    assert_eq!(
        label(cx, settings, "error").as_deref(),
        Some("That license key isn't valid. Check that you pasted all of it.")
    );
    let other = SigningKey::from_bytes(&[12; 32]);
    let forged = convt_license::sign(
        &License {
            id: "x".into(),
            email: "a-tester".into(),
            plan: Plan::Desktop,
            issued: "2026-09-01".into(),
            updates_until: "2027-10-01".into(),
        },
        &other,
    );
    type_key(cx, settings, &view, &forged);
    click(cx, settings, "activate");
    assert!(shown(cx, settings, "error"), "a key from another signer");

    let key = license_key("a-tester", "2027-10-01");
    type_key(cx, settings, &view, &format!("  {key}\n"));
    click(cx, settings, "activate");
    assert!(!shown(cx, settings, "error"));
    assert_eq!(
        label(cx, settings, "license-notice").as_deref(),
        Some("License activated for ***.")
    );
    assert_eq!(
        label(cx, settings, "license-status").as_deref(),
        Some("You have a convt license")
    );
    let stored = std::fs::read_to_string(f.dir.path().join("license.key")).unwrap();
    assert_eq!(stored.trim(), key);

    // Quick convert converts now.
    assert!(!shown(cx, quick, "license-banner"));
    click(cx, quick, "convert");
    let job = f.last_job(cx);
    wait_for_label(cx, quick, &format!("status-{job}"), |s| {
        s.starts_with("Saved")
    });

    // Removing the license brings the ended trial back.
    click(cx, settings, "remove-license");
    assert_eq!(
        label(cx, settings, "license-notice").as_deref(),
        Some("Removed the license from this machine.")
    );
    assert!(!f.dir.path().join("license.key").exists());
    let status = label(cx, settings, "license-status").unwrap();
    assert!(
        status.starts_with("Your convt trial has ended."),
        "{status}"
    );
}

#[gpui_kit::test]
fn a_license_older_than_the_build_says_so(cx: &mut TestAppContext) {
    let key = license_key("a-tester", "2026-06-30");
    let f = Fixture::licensed(cx, Some("2026-01-01"), Some(&key));
    let (main, view) = f.main(cx);
    let card = label(cx, main, "trial-card").unwrap();
    assert!(
        card.starts_with("This build is newer than your license covers.")
            && card.contains("2026-06-30"),
        "{card}"
    );

    let bmp = f.bmp("a.bmp");
    view.update(cx, |v, cx| v.add(std::slice::from_ref(&bmp), cx));
    let (quick, _) = last_quick(cx);
    assert!(shown(cx, quick, "download"));
    assert_eq!(f.jobs(cx), 0);

    let (quick, _) = f.quick(cli(vec![bmp], Some("jpeg"), None), cx);
    assert!(shown(cx, quick, "download"));
    let (settings, _) = f.settings(SettingsTab::License, cx);
    assert!(shown(cx, settings, "remove-license"));
}

#[gpui_kit::test]
fn an_activate_link_fills_in_the_key_without_activating(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    let key = license_key("a-tester", "2027-10-01");
    let link = format!("convt://activate?key={key}");
    let request = crate::request::parse_url(&link).unwrap();
    cx.update(|cx| super::route(request, cx));
    let (settings, view) = window_of::<SettingsView>(cx);
    cx.read(|cx| {
        let view = view.read(cx);
        assert_eq!(view.tab, SettingsTab::License);
        assert_eq!(view.license_key.read(cx).value(), key.as_str());
    });
    assert!(!f.dir.path().join("license.key").exists());
    click(cx, settings, "activate");
    assert!(f.dir.path().join("license.key").exists());
}

/// Finishes onboarding's last moment: the main window opens after it.
fn finish_onboarding(cx: &mut TestAppContext, view: &Entity<FirstRunView>) {
    cx.read(|cx| assert_eq!(view.read(cx).screen, Screen::Calibrating));
    cx.executor()
        .advance_clock(super::first_run::CALIBRATE + Duration::from_millis(50));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn first_run_shows_once_in_licensed_builds(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    cx.update(|cx| super::route(Request::default(), cx));
    let (window, view) = window_of::<FirstRunView>(cx);
    cx.read(|cx| assert!(!f.app.read(cx).settings.first_run_done));
    assert!(!f.settings_file().contains("first_run_done = true"));
    // Sign in, or a key. No local trial.
    assert!(shown(cx, window, "onboarding-google"));
    assert!(shown(cx, window, "onboarding-email"));
    assert!(!shown(cx, window, "plan-trial"));

    // "I have a license key" activates the key and shows it.
    click(cx, window, "onboarding-key-link");
    cx.read(|cx| assert_eq!(view.read(cx).screen, Screen::Key));
    let input = cx.read(|cx| view.read(cx).key.clone());
    set_input(cx, window, &input, "nonsense");
    click(cx, window, "onboarding-activate");
    assert!(shown(cx, window, "error"));
    let key = license_key("a-tester", "2027-10-01");
    set_input(cx, window, &input, &key);
    click(cx, window, "onboarding-activate");
    cx.read(|cx| {
        assert_eq!(
            view.read(cx).screen,
            Screen::Account,
            "{:?}",
            view.read(cx).error
        )
    });
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("You have a convt license")
    );
    assert!(f.dir.path().join("license.key").exists());
    assert!(!f.dir.path().join("trial").exists(), "no trial started");

    // Continue, the last moment, then the main window.
    click(cx, window, "onboarding-primary");
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("Setting convt up…")
    );
    cx.read(|cx| assert!(!f.app.read(cx).settings.first_run_done));
    finish_onboarding(cx, &view);
    window_of::<MainView>(cx);
    cx.read(|cx| assert!(f.app.read(cx).settings.first_run_done));
    assert!(f.settings_file().contains("first_run_done = true"));

    // A second launch goes straight to the main window.
    let before = cx.update(|cx| cx.windows().len());
    cx.update(|cx| super::route(Request::default(), cx));
    assert_eq!(cx.update(|cx| cx.windows().len()), before);
}

#[gpui_kit::test]
fn closing_first_run_before_the_end_shows_it_again(cx: &mut TestAppContext) {
    let key = pro_key("pro-tester", "2027-10-01");
    let f = Fixture::licensed(cx, None, Some(&key));
    cx.update(|cx| super::route(Request::default(), cx));
    let (window, view) = window_of::<FirstRunView>(cx);
    click(cx, window, "onboarding-primary");
    cx.read(|cx| assert_eq!(view.read(cx).screen, Screen::Calibrating));
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.read(|cx| assert!(!f.app.read(cx).settings.first_run_done));
    cx.update(|cx| super::route(Request::default(), cx));
    window_of::<FirstRunView>(cx);
    cx.read(|cx| assert!(!f.app.read(cx).settings.first_run_done));
}

#[gpui_kit::test]
fn onboarding_opens_at_three_quarters_of_the_display(cx: &mut TestAppContext) {
    let _f = Fixture::licensed(cx, None, None);
    let (display, wanted) = cx.update(super::first_run_bounds);
    let visible = cx.update(|cx| cx.primary_display().map(|d| d.visible_bounds().size));
    if let Some(visible) = visible {
        assert!(display.is_some());
        assert_eq!(wanted, super::first_run_fit(visible));
    }
}

#[test]
fn the_glow_breathes_without_a_jump() {
    use super::first_run::breath;
    assert!(
        (breath(0.) - breath(1.)).abs() < 1e-4,
        "a breath ends where it starts"
    );
    for i in 0..=100 {
        let o = breath(i as f32 / 100.);
        assert!((0.78..=1.).contains(&o), "{o}");
    }
    assert!((breath(0.5) - 1.).abs() < 1e-4);
}

#[test]
fn onboarding_fits_small_normal_and_large_displays() {
    use super::first_run_fit;
    let fit = |w: f32, h: f32| {
        let s = first_run_fit(size(px(w), px(h)));
        (f32::from(s.width), f32::from(s.height))
    };
    // A normal laptop or desktop: three quarters.
    assert_eq!(fit(1440., 900.), (1080., 675.));
    assert_eq!(fit(1920., 1080.), (1440., 810.));
    // 4K at 1x: three quarters, however large.
    assert_eq!(fit(3840., 2160.), (2880., 1620.));
    // Where three quarters is too small, the least size, while it fits.
    assert_eq!(fit(1024., 768.), (900., 640.));
    assert_eq!(fit(1180., 800.), (900., 640.));
    // Smaller than that: 95%, never past the edges.
    assert_eq!(fit(800., 600.), (760., 570.));
    assert_eq!(fit(640., 480.), (608., 456.));
    for (w, h) in [
        (640., 480.),
        (800., 600.),
        (1024., 768.),
        (1280., 720.),
        (3840., 2160.),
    ] {
        let (fw, fh) = fit(w, h);
        assert!(fw <= w * 0.95 + 0.5 && fh <= h * 0.95 + 0.5, "{w}x{h}");
    }
}

#[gpui_kit::test]
fn the_sign_in_buttons_ask_for_their_provider(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    cx.update(|cx| super::route(Request::default(), cx));
    let (window, view) = window_of::<FirstRunView>(cx);
    for (button, provider) in [
        ("onboarding-google", Provider::Google),
        ("onboarding-email", Provider::Email),
    ] {
        click(cx, window, button);
        let page = cx.opened_url().expect("the device page opened");
        assert!(
            page.starts_with(&format!("{ACCOUNT_URL}/device?state=")),
            "{page}"
        );
        cx.read(|cx| {
            assert_eq!(view.read(cx).stage(cx), Stage::Waiting);
            assert_eq!(view.read(cx).provider, provider);
        });
        assert_eq!(
            label(cx, window, "onboarding-title").as_deref(),
            Some("Continue in your browser")
        );
        click(cx, window, "onboarding-reopen");
        assert_eq!(cx.opened_url().as_deref(), Some(page.as_str()));
        click(cx, window, "onboarding-cancel");
        cx.read(|cx| assert_eq!(view.read(cx).stage(cx), Stage::SignIn));
    }
    // A failed sign-in says why and tries again with the same button.
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.account.sign_in = SignIn::Failed("The link expired.".into());
            cx.notify();
        })
    });
    assert_eq!(
        label(cx, window, "error").as_deref(),
        Some("The link expired.")
    );
    click(cx, window, "onboarding-retry");
    cx.read(|cx| assert_eq!(view.read(cx).stage(cx), Stage::Waiting));
    assert_eq!(f.jobs(cx), 0);
}

#[gpui_kit::test]
fn each_account_state_offers_its_own_next_step(cx: &mut TestAppContext) {
    let f = Fixture::signed_in(cx, None, "pro-tester");
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.update_settings(|s| s.first_run_done = false, cx)
        })
    });
    let app = f.app.clone();
    let (window, view) = open(cx, move |window, cx| {
        cx.new(|cx| FirstRunView::new(app, Screen::Account, window, cx))
    });
    let set = |cx: &mut TestAppContext, access: Option<Access>| {
        cx.update(|cx| {
            f.app.update(cx, |s, cx| {
                s.account.access = access;
                s.account.awaiting_trial = false;
                cx.notify();
            })
        })
    };
    // No answer from the account: onboarding asks, once.
    set(cx, None);
    wait_until(cx, "the account answered", |cx| {
        f.app.read(cx).account.refresh != Refresh::Running
    });
    assert_eq!(f.api.calls(), (0, 1, 0));
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("Couldn't reach convt.app")
    );
    assert!(!shown(cx, window, "onboarding-primary"));
    // And says so while it waits.
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.account.refresh = Refresh::Running;
            cx.notify();
        })
    });
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("Checking your account…")
    );
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.account.refresh = Refresh::Failed("Offline.".into());
            cx.notify();
        })
    });

    let checkout = "https://checkout.example.com/trial";
    for (access, title, primary) in [
        (Access::Pro, "You have convt Pro", "Continue"),
        (
            Access::Trial {
                ends_on: "2026-10-15".into(),
            },
            "Your free trial is on",
            "Continue",
        ),
        (
            Access::CanStartTrial {
                checkout_url: checkout.into(),
            },
            "Start your 7-day free trial",
            "Start free trial",
        ),
        (Access::Lapsed, "Your Pro plan has ended", "Get convt Pro"),
    ] {
        set(cx, Some(access.clone()));
        assert_eq!(
            label(cx, window, "onboarding-title").as_deref(),
            Some(title)
        );
        assert_eq!(
            label(cx, window, "onboarding-primary").as_deref(),
            Some(primary)
        );
        assert_eq!(
            label(cx, window, "account-status").as_deref(),
            Some("Signed in as ***")
        );
    }

    // The trial starts through checkout, and onboarding waits for it.
    set(
        cx,
        Some(Access::CanStartTrial {
            checkout_url: checkout.into(),
        }),
    );
    click(cx, window, "onboarding-primary");
    assert_eq!(cx.opened_url().as_deref(), Some(checkout));
    cx.read(|cx| assert_eq!(view.read(cx).stage(cx), Stage::AwaitingTrial));
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("Finish checkout in your browser")
    );
    // "Open checkout again" opens the same page; it doesn't start over.
    cx.update(|cx| cx.open_url("about:blank"));
    click(cx, window, "onboarding-reopen");
    assert_eq!(cx.opened_url().as_deref(), Some(checkout));
    cx.read(|cx| {
        assert!(f.app.read(cx).account.awaiting_trial);
        assert_eq!(view.read(cx).stage(cx), Stage::AwaitingTrial);
    });
    // One check from onboarding, one as checkout opens; reopening adds none.
    assert_eq!(f.api.calls(), (0, 2, 0));
    // Lapsed buys.
    set(cx, Some(Access::Lapsed));
    click(cx, window, "onboarding-primary");
    assert_eq!(cx.opened_url().as_deref(), Some(BUY_URL));
    // "Not now" goes on without a plan.
    click(cx, window, "onboarding-not-now");
    cx.read(|cx| assert_eq!(view.read(cx).screen, Screen::Calibrating));
}

#[gpui_kit::test]
fn a_signed_in_relaunch_never_waits_on_the_account_forever(cx: &mut TestAppContext) {
    // Signed in, no key here, and the launch check already ran today, so
    // nothing else is going to ask.
    let f = Fixture::signed_in(cx, None, "pro-tester");
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.update_settings(
                |s| {
                    s.first_run_done = false;
                    s.license_checked = Some(today());
                },
                cx,
            );
            s.renew_on_launch(cx);
        })
    });
    assert_eq!(f.api.calls(), (0, 0, 0));
    cx.update(|cx| super::route(Request::default(), cx));
    let (window, view) = window_of::<FirstRunView>(cx);
    let settled = |cx: &mut TestAppContext| {
        wait_until(cx, "the account answered", |cx| {
            f.app.read(cx).account.refresh != Refresh::Running
        })
    };

    // Offline: say so, with Retry and the key as ways on.
    settled(cx);
    assert_eq!(f.api.calls(), (0, 1, 0), "onboarding asked the account");
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("Couldn't reach convt.app")
    );
    assert!(shown(cx, window, "onboarding-retry"));
    assert!(shown(cx, window, "onboarding-key-link"));
    assert_eq!(
        label(cx, window, "account-status").as_deref(),
        Some("Signed in as ***")
    );
    // Drawing again doesn't ask again.
    cx.update(|cx| f.app.update(cx, |_, cx| cx.notify()));
    cx.run_until_parked();
    assert_eq!(f.api.calls(), (0, 1, 0));

    // An answer that doesn't say what the account allows is no dead end
    // either.
    f.api.answer_key(Ok(None));
    click(cx, window, "onboarding-retry");
    settled(cx);
    assert_eq!(f.api.calls(), (0, 2, 0));
    cx.read(|cx| assert!(matches!(view.read(cx).stage(cx), Stage::CheckFailed(_))));
    assert!(shown(cx, window, "onboarding-retry"));
    // "I have a license key" is the other way on.
    click(cx, window, "onboarding-key-link");
    cx.read(|cx| assert_eq!(view.read(cx).screen, Screen::Key));
    click(cx, window, "onboarding-back");
    cx.read(|cx| assert_eq!(view.read(cx).screen, Screen::Account));
    assert_eq!(f.api.calls(), (0, 2, 0), "going back didn't ask again");

    // Retry with convt.app back.
    f.api
        .answer_key(Ok(Some(pro_key("pro-tester", "2027-10-01"))));
    click(cx, window, "onboarding-retry");
    settled(cx);
    assert_eq!(f.api.calls(), (0, 3, 0));
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("You have convt Pro")
    );
}

#[gpui_kit::test]
fn yes_to_documents_installs_the_pack(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    let key = pro_key("pro-tester", "2027-10-01");
    let f = Fixture::licensed_with_packs(cx, packs.clone(), Some(&key));
    cx.update(|cx| super::route(Request::default(), cx));
    let (window, view) = window_of::<FirstRunView>(cx);
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("You have convt Pro")
    );
    click(cx, window, "onboarding-primary");
    cx.read(|cx| assert_eq!(view.read(cx).screen, Screen::Question(Question::Documents)));
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("Convert PDFs and documents too?")
    );
    // The arrow keys move between the tiles; nothing downloads until a pick.
    cx.simulate_keystrokes(window, "right");
    cx.read(|cx| assert!(!view.read(cx).yes));
    cx.simulate_keystrokes(window, "left");
    cx.read(|cx| assert!(view.read(cx).yes));
    assert_eq!(packs.installs(), 0);
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, "the pack installed", |cx| {
        f.app.read(cx).documents_supported()
    });
    assert_eq!(packs.installs(), 1);
    cx.read(|cx| assert_eq!(view.read(cx).screen, Screen::Calibrating));
    finish_onboarding(cx, &view);
    window_of::<MainView>(cx);
}

#[gpui_kit::test]
fn no_to_documents_downloads_nothing(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    let key = pro_key("pro-tester", "2027-10-01");
    let _f = Fixture::licensed_with_packs(cx, packs.clone(), Some(&key));
    cx.update(|cx| super::route(Request::default(), cx));
    let (window, view) = window_of::<FirstRunView>(cx);
    click(cx, window, "onboarding-primary");
    click(cx, window, "question-no");
    cx.read(|cx| assert_eq!(view.read(cx).screen, Screen::Calibrating));
    cx.run_until_parked();
    assert_eq!(packs.installs(), 0);
}

#[gpui_kit::test]
fn the_finder_question_opens_system_settings_on_yes(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    let app = f.app.clone();
    let (window, view) = open(cx, move |window, cx| {
        cx.new(|cx| FirstRunView::new(app, Screen::Question(Question::Finder), window, cx))
    });
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("Add convt to Finder?")
    );
    click(cx, window, "question-yes");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some(crate::finder::EXTENSION_SETTINGS)
    );
    cx.read(|cx| assert_eq!(view.read(cx).screen, Screen::Calibrating));
}

#[gpui_kit::test]
fn activity_offers_finder_setup_until_the_extension_is_on(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.update_settings(|s| s.first_run_done = true, cx);
            s.finder_on = Some(false);
        })
    });
    cx.update(|cx| super::route(Request::default(), cx));
    let (window, _) = window_of::<MainView>(cx);
    assert!(shown(cx, window, "finder-setup"));
    assert_eq!(
        label(cx, window, "enable-finder").as_deref(),
        Some("Open System Settings")
    );
    click(cx, window, "enable-finder");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some(crate::finder::EXTENSION_SETTINGS)
    );
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.finder_on = Some(true);
            cx.notify();
        })
    });
    assert!(!shown(cx, window, "finder-setup"));

    let (settings, _) = f.settings(SettingsTab::General, cx);
    assert_eq!(
        label(cx, settings, "finder-status").as_deref(),
        Some("On. Right-click a file in Finder to convert.")
    );
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.finder_on = Some(false);
            cx.notify();
        })
    });
    let status = label(cx, settings, "finder-status").expect("status");
    assert!(
        status.contains("Off") && status.contains("Extensions"),
        "{status}"
    );
    click(cx, settings, "manage-finder");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some(crate::finder::EXTENSION_SETTINGS)
    );
}

#[gpui_kit::test]
fn a_build_from_source_never_shows_first_run(cx: &mut TestAppContext) {
    let _f = Fixture::new(cx);
    cx.update(|cx| super::route(Request::default(), cx));
    window_of::<MainView>(cx);
    cx.read(|cx| assert!(Open::<FirstRunView>::get(cx).is_none()));
}

#[gpui_kit::test]
fn signing_in_from_onboarding_brings_it_back_with_the_key(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    cx.update(|cx| super::route(Request::default(), cx));
    let (window, view) = window_of::<FirstRunView>(cx);
    click(cx, window, "onboarding-google");
    let page = cx.opened_url().expect("the device page opened");

    // The browser's answer brings onboarding back, signed in, with the key.
    *f.api.exchange.lock().unwrap() = Ok(Session {
        email: "pro-tester".into(),
        token: "cvd_new".into(),
    });
    f.api
        .answer_key(Ok(Some(pro_key("pro-tester", "2026-11-01"))));
    let link = format!("convt://auth?state={}&code=onetime", query(&page, "state"));
    cx.update(|cx| super::route(crate::request::parse_url(&link).unwrap(), cx));
    wait_until(cx, "the key arrived", |cx| {
        matches!(f.app.read(cx).license, client::State::Licensed(_))
    });
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("You have convt Pro")
    );
    assert_eq!(
        label(cx, window, "account-status").as_deref(),
        Some("Signed in as ***")
    );
    cx.read(|cx| assert!(Open::<SettingsView>::get(cx).is_none()));
    assert_eq!(f.jobs(cx), 0);
    click(cx, window, "onboarding-primary");
    cx.read(|cx| assert_eq!(view.read(cx).screen, Screen::Calibrating));
}

#[gpui_kit::test]
fn first_run_shows_a_license_it_already_has(cx: &mut TestAppContext) {
    for (key, title) in [
        (pro_key("pro-tester", "2027-10-01"), "You have convt Pro"),
        (
            license_key("a-tester", "2027-10-01"),
            "You have a convt license",
        ),
    ] {
        let f = Fixture::licensed(cx, None, Some(&key));
        cx.update(|cx| super::route(Request::default(), cx));
        let (window, view) = window_of::<FirstRunView>(cx);
        assert_eq!(
            label(cx, window, "onboarding-title").as_deref(),
            Some(title)
        );
        assert!(!shown(cx, window, "onboarding-google"));
        // Return continues, without starting a trial or asking for a key.
        cx.simulate_keystrokes(window, "enter");
        assert!(!f.dir.path().join("trial").exists(), "no trial started");
        finish_onboarding(cx, &view);
        cx.read(|cx| assert!(f.app.read(cx).settings.first_run_done));
        cx.update(|cx| {
            cx.windows()
                .iter()
                .for_each(|w| drop(w.update(cx, |_, w, _| w.remove_window())))
        });
    }
}

#[gpui_kit::test]
fn the_popover_converts_a_dropped_file_and_copies_it(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let bmp = f.bmp("Screenshot.bmp");
    let (window, view) = cx.update(super::open_popover).unwrap();
    assert!(shown(cx, window, "drop-bar"));
    view.update(cx, |v, cx| v.drop_files(&[bmp], cx));
    let job = f.last_job(cx);
    wait_for_label(cx, window, &format!("copied-{job}"), |s| {
        s == "Copied to your clipboard"
    });
    let item = cx.read_from_clipboard().expect("something was copied");
    let output = f.dir.path().join("Screenshot.png");
    assert!(is_png(&output));
    assert!(
        item.entries().iter().any(|e| matches!(
            e,
            ClipboardEntry::Image(image)
                if image.format == ImageFormat::Png
                    && image.bytes == std::fs::read(&output).unwrap()
        )),
        "{item:?}"
    );
    cx.read(|cx| assert!(view.read(cx).drops[0].copied));

    click(cx, window, "open-settings");
    window_of::<SettingsView>(cx);
    click(cx, window, "open-convt");
    window_of::<MainView>(cx);
}

#[gpui_kit::test]
fn automation_switches_are_saved(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let (popover, _) = cx.update(super::open_popover).unwrap();
    let rules = cx.read(|cx| f.app.read(cx).settings.automations.clone());
    assert_eq!(rules.len(), 3);
    assert!(!rules[2].enabled);
    assert_eq!(
        label(cx, popover, "popover-automation-2").as_deref(),
        Some("HEIC → JPEG"),
        "a switch is named for its rule"
    );
    assert_eq!(toggled(cx, popover, "popover-automation-2"), Some(false));
    click(cx, popover, "popover-automation-2");
    assert_eq!(toggled(cx, popover, "popover-automation-2"), Some(true));
    cx.read(|cx| assert!(f.app.read(cx).settings.automations[2].enabled));
    let reloaded = crate::settings::Settings::load(&f.dir.path().join("settings.toml")).unwrap();
    assert!(reloaded.automations[2].enabled);

    // The main window shows the same rules.
    click(cx, popover, "manage-rules");
    let (main, view) = window_of::<MainView>(cx);
    cx.read(|cx| assert_eq!(view.read(cx).page, Page::Automations));
    assert_eq!(
        label(cx, main, "automation-2").as_deref(),
        Some("HEIC → JPEG")
    );
    assert_eq!(toggled(cx, main, "automation-2"), Some(true));
    let intro = label(cx, main, "automations-intro").expect("intro");
    assert!(intro.contains("screenshot"), "{intro}");
    assert_eq!(
        label(cx, main, "automation-0-copy").as_deref(),
        Some("Copy the converted file")
    );
    click(cx, main, "automation-0-copy");
    cx.read(|cx| assert!(!f.app.read(cx).settings.automations[0].copies_to_clipboard()));
    click(cx, main, "automation-0");
    cx.read(|cx| assert!(!f.app.read(cx).settings.automations[0].enabled));
}

#[gpui_kit::test]
fn a_screenshot_automation_converts_and_can_copy(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let shots = f.dir.path().join("Desktop");
    std::fs::create_dir(&shots).unwrap();
    f.app.update(cx, |s, cx| {
        s.update_settings(
            |set| {
                set.automations[0].folder = Some(shots.clone());
                set.automations[0].copy_to_clipboard = Some(true);
                set.automations[1].enabled = false;
            },
            cx,
        );
        s.poll_automations(cx);
    });

    let input = shots.join("Screenshot 1.bmp");
    image::RgbImage::from_pixel(4, 4, image::Rgb([200, 40, 40]))
        .save(&input)
        .unwrap();
    f.app.update(cx, |s, cx| s.poll_automations(cx));
    f.app.update(cx, |s, cx| s.poll_automations(cx));
    wait_until(cx, "the screenshot", |cx| f.app.read(cx).recent.len() == 1);
    let output = shots.join("Screenshot 1.png");
    assert!(is_png(&output));
    let item = cx.read_from_clipboard().expect("the result was copied");
    assert!(
        item.entries().iter().any(|e| matches!(
            e,
            ClipboardEntry::Image(image)
                if image.format == ImageFormat::Png
                    && image.bytes == std::fs::read(&output).unwrap()
        )),
        "{item:?}"
    );

    // Ordinary photos in the same Desktop-like folder are left alone.
    image::RgbImage::from_pixel(4, 4, image::Rgb([20, 80, 200]))
        .save(shots.join("IMG_2041.bmp"))
        .unwrap();
    f.app.update(cx, |s, cx| s.poll_automations(cx));
    f.app.update(cx, |s, cx| s.poll_automations(cx));
    assert_eq!(cx.read(|cx| f.app.read(cx).recent.len()), 1);
    assert!(!shots.join("IMG_2041.png").exists());
}

#[gpui_kit::test]
fn every_window_renders_in_both_themes(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, Some("2026-09-30"), None);
    let png = f.png("a.png");
    for dark in [false, true] {
        cx.update(|cx| theme::set_dark(dark, cx));
        cx.read(|cx| assert_eq!(theme::palette(cx).dark, dark));
        let (main, _) = f.main(cx);
        assert!(shown(cx, main, "trial-card"));
        let (quick, _) = f.quick(cli(vec![png.clone()], None, None), cx);
        assert!(shown(cx, quick, "convert"));
        for tab in [
            SettingsTab::General,
            SettingsTab::Presets,
            SettingsTab::License,
        ] {
            let (settings, _) = f.settings(tab, cx);
            assert!(shown(cx, settings, "tab-general"));
        }
        let app = f.app.clone();
        let (first, _) = open(cx, move |window, cx| {
            cx.new(|cx| FirstRunView::new(app, Screen::Account, window, cx))
        });
        assert!(shown(cx, first, "onboarding-google"));
        let app = f.app.clone();
        let (first, _) = open(cx, move |window, cx| {
            cx.new(|cx| FirstRunView::new(app, Screen::Question(Question::Documents), window, cx))
        });
        assert!(shown(cx, first, "question-yes"));
        let app = f.app.clone();
        let (first, _) = open(cx, move |window, cx| {
            cx.new(|cx| FirstRunView::new(app, Screen::Calibrating, window, cx))
        });
        assert!(shown(cx, first, "onboarding-title"));
        let app = f.app.clone();
        let (popover, _) = open(cx, move |window, cx| {
            cx.new(|cx| PopoverView::new(app, window, cx))
        });
        assert!(shown(cx, popover, "drop-bar"));
    }
}

#[gpui_kit::test]
fn a_silent_convert_writes_beside_the_original_whatever_the_save_setting(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let elsewhere = f.dir.path().join("converted");
    std::fs::create_dir(&elsewhere).unwrap();
    let app = f.app.clone();
    let dir = elsewhere.clone();
    cx.update(|cx| {
        app.update(cx, |s, cx| {
            s.update_settings(|s| s.output_dir = Some(dir), cx)
        })
    });
    let png = f.png("a.png");
    cx.update(|cx| super::route(cli(vec![png], Some("jpeg"), None), cx));
    wait_until(cx, "the conversion", |cx| f.app.read(cx).recent.len() == 1);
    assert!(
        is_jpeg(&f.dir.path().join("a.jpg")),
        "written beside the original"
    );
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);

    // A conversion started from a window still uses the setting.
    let b = f.png("b.png");
    let (window, _) = f.quick(cli(vec![b], Some("jpeg"), None), cx);
    click(cx, window, "convert");
    wait_until(cx, "the window's conversion", |cx| {
        f.app.read(cx).recent.len() == 2
    });
    assert!(is_jpeg(&elsewhere.join("b.jpg")));
}

#[gpui_kit::test]
fn silent_conversions_neither_notify_nor_reveal(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let app = f.app.clone();
    cx.update(|cx| {
        app.update(cx, |s, cx| {
            s.update_settings(
                |s| {
                    s.notifications = true;
                    s.reveal_when_done = true;
                },
                cx,
            )
        })
    });
    let png = f.png("a.png");
    cx.update(|cx| super::route(cli(vec![png.clone()], Some("jpeg"), None), cx));
    wait_until(cx, "the silent conversion", |cx| {
        f.app.read(cx).recent.len() == 1
    });
    assert!(cx.shown_system_notifications().is_empty());
    cx.read(|cx| assert!(f.app.read(cx).revealed.is_empty()));

    // The same conversion from a window, also in the background, does both.
    cx.update(|cx| {
        app.update(cx, |s, cx| {
            let to = convt_core::format_by_id("webp").unwrap();
            s.convert(&[png], to, &Options::default(), cx).unwrap();
        })
    });
    wait_until(cx, "the window's conversion", |cx| {
        f.app.read(cx).recent.len() == 2
    });
    assert_eq!(cx.shown_system_notifications().len(), 1);
    cx.read(|cx| assert_eq!(f.app.read(cx).revealed, [f.dir.path().join("a.webp")]));
}

#[gpui_kit::test]
fn balanced_and_original_clear_what_a_preset_set(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    save_preset(
        &f,
        cx,
        "web",
        Preset {
            to: Some("webp".into()),
            options: Options {
                quality: Some(80),
                max_size: Some(1024),
                ..Options::default()
            },
        },
    );
    let (window, view) = f.quick(
        Request {
            files: vec![f.png("a.png")],
            ..Request::default()
        },
        cx,
    );
    click(cx, window, "preset-web");
    click(cx, window, "options-toggle");
    // The controls show the preset's values.
    cx.read(|cx| {
        let v = view.read(cx);
        assert_eq!((v.quality, v.size), (Some(80), Some(1024)));
        let o = v.conversion_options();
        assert_eq!((o.quality, o.max_size), (Some(80), Some(1024)));
    });
    assert_eq!(
        label(cx, window, "quality-preset").as_deref(),
        Some("Preset (80)")
    );
    assert_eq!(label(cx, window, "size").as_deref(), Some("1024 px"));

    click(cx, window, "quality-balanced");
    click(cx, window, "size");
    click(cx, window, "size-original");
    cx.read(|cx| {
        let o = view.read(cx).conversion_options();
        assert_eq!((o.quality, o.max_size), (None, None), "{o:?}");
    });
    assert!(!shown(cx, window, "quality-preset"));
    assert_eq!(label(cx, window, "size").as_deref(), Some("Original"));
}

#[gpui_kit::test]
fn quick_convert_shows_errors_the_license_banner_does_not(cx: &mut TestAppContext) {
    // A licensed build with nowhere to record the trial's start.
    let f = Fixture::build(cx, |dir| client::Config {
        enforce: true,
        public_key: Some(test_key().verifying_key()),
        build_date: BUILD_DATE.into(),
        trial_file: None,
        store: KeyStore::File(dir.join("license.key")),
    });
    let (window, _) = f.quick(cli(vec![f.png("a.png")], Some("jpeg"), None), cx);
    assert!(
        !shown(cx, window, "license-banner"),
        "the trial still allows converting"
    );
    click(cx, window, "convert");
    assert_eq!(f.jobs(cx), 0);
    let error = label(cx, window, "error").expect("the error is shown");
    assert!(error.contains("start date couldn't be saved"), "{error}");
}

#[gpui_kit::test]
fn the_popover_copies_every_drop_even_past_the_listed_few(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let files: Vec<PathBuf> = (0..5).map(|i| f.bmp(&format!("shot {i}.bmp"))).collect();
    let (window, view) = cx.update(super::open_popover).unwrap();
    view.update(cx, |v, cx| v.drop_files(&files, cx));
    let jobs: Vec<JobId> =
        cx.read(|cx| f.app.read(cx).queue.entries.iter().map(|e| e.id).collect());
    assert_eq!(jobs.len(), 5);
    wait_until(cx, "every result copied", |cx| {
        view.read(cx).copied.len() == 5
    });
    cx.read(|cx| {
        let v = view.read(cx);
        let mut copied = v.copied.clone();
        copied.sort();
        assert_eq!(copied, jobs);
        assert!(
            v.drops.len() <= 3,
            "settled drops past the list are forgotten"
        );
    });
    assert!(shown(cx, window, &format!("copied-{}", jobs[4])));
    assert!(
        !shown(cx, window, &format!("copied-{}", jobs[0])),
        "only three are listed"
    );
}

#[gpui_kit::test]
fn quick_convert_offers_a_background_for_images(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let logo = f.dir.path().join("logo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([0, 0, 0, 0]))
        .save(&logo)
        .unwrap();
    save_preset(
        &f,
        cx,
        "orange",
        Preset {
            to: Some("jpeg".into()),
            options: Options {
                background: Some("#ff8800".parse().unwrap()),
                ..Options::default()
            },
        },
    );
    let (window, view) = f.quick(cli(vec![logo], None, None), cx);

    // JPEG can't be transparent: White by default, and no Transparent choice.
    click(cx, window, "to-jpeg");
    click(cx, window, "options-toggle");
    assert_eq!(label(cx, window, "background").as_deref(), Some("White"));
    cx.read(|cx| assert_eq!(view.read(cx).conversion_options(), Options::default()));
    click(cx, window, "background");
    assert!(shown(cx, window, "background-white") && shown(cx, window, "background-black"));
    assert!(!shown(cx, window, "background-transparent"));
    click(cx, window, "background-black");
    assert_eq!(label(cx, window, "background").as_deref(), Some("Black"));
    cx.read(|cx| {
        assert_eq!(
            view.read(cx).conversion_options().background,
            Some(Background::BLACK)
        )
    });

    // WebP keeps transparency, so Transparent is offered.
    click(cx, window, "to-webp");
    click(cx, window, "background");
    click(cx, window, "background-transparent");
    assert_eq!(
        label(cx, window, "background").as_deref(),
        Some("Transparent")
    );
    cx.read(|cx| {
        assert_eq!(
            view.read(cx).conversion_options().background,
            Some(Background::Transparent)
        )
    });
    // Back to JPEG, the Transparent pick falls back to White instead of failing.
    click(cx, window, "to-jpeg");
    assert_eq!(label(cx, window, "background").as_deref(), Some("White"));
    cx.read(|cx| assert_eq!(view.read(cx).conversion_options().background, None));

    // A preset's own color shows and stays pickable.
    click(cx, window, "preset-orange");
    assert_eq!(label(cx, window, "background").as_deref(), Some("#FF8800"));
    click(cx, window, "background");
    assert!(shown(cx, window, "background-#ff8800"));
    click(cx, window, "background-white");
    click(cx, window, "background");
    assert!(shown(cx, window, "background-#ff8800"));
    click(cx, window, "background-#ff8800");
    click(cx, window, "quality-smaller");
    let before = cx.read(|cx| view.read(cx).conversion_options());
    click(cx, window, "background");
    assert!(shown(cx, window, "background-automatic"));
    click(cx, window, "background-automatic");
    cx.read(|cx| {
        assert_eq!(
            view.read(cx).conversion_options(),
            Options {
                background: None,
                ..before
            }
        )
    });
}

#[gpui_kit::test]
fn quick_convert_background_follows_the_source(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    // Targets come from the extension, so the contents don't matter here.
    let pdf = f.dir.path().join("page.pdf");
    std::fs::write(&pdf, "not really a pdf").unwrap();
    let (window, view) = f.quick(cli(vec![pdf], None, None), cx);
    let targets = cx.read(|cx| view.read(cx).targets.formats.clone());
    if targets.iter().any(|t| t.id == "png") {
        // PDF pages render on white unless Transparent is picked.
        click(cx, window, "to-png");
        click(cx, window, "options-toggle");
        assert_eq!(label(cx, window, "background").as_deref(), Some("White"));
        click(cx, window, "background");
        assert!(shown(cx, window, "background-transparent"));
    } else {
        eprintln!("skipping PDF: PDFium missing");
    }

    let clip = f.dir.path().join("clip.mov");
    std::fs::write(&clip, "not really a movie").unwrap();
    let (window, view) = f.quick(cli(vec![clip], None, None), cx);
    let targets = cx.read(|cx| view.read(cx).targets.formats.clone());
    if !targets.iter().any(|t| t.id == "gif") {
        eprintln!("skipping video: FFmpeg missing");
        return;
    }
    // GIF from video takes no background color; a still frame does.
    click(cx, window, "to-gif");
    click(cx, window, "options-toggle");
    assert!(!shown(cx, window, "background"));
    if targets.iter().any(|t| t.id == "jpeg") {
        click(cx, window, "to-jpeg");
        assert_eq!(label(cx, window, "background").as_deref(), Some("White"));
    }
    // A preset's color doesn't follow the movie into GIF, where nothing could clear it.
    save_preset(
        &f,
        cx,
        "white",
        Preset {
            to: None,
            options: Options {
                background: Some(Background::WHITE),
                ..Options::default()
            },
        },
    );
    let clip = f.dir.path().join("clip2.mov");
    std::fs::write(&clip, "not really a movie").unwrap();
    let (window, view) = f.quick(cli(vec![clip], None, Some("white")), cx);
    click(cx, window, "to-gif");
    cx.read(|cx| assert_eq!(view.read(cx).conversion_options().background, None));

    // A PDF renders on white while a PNG keeps its transparency: Automatic.
    let pdf = f.dir.path().join("mixed.pdf");
    let png = f.dir.path().join("mixed.png");
    std::fs::write(&pdf, "not really a pdf").unwrap();
    image::RgbaImage::new(2, 2).save(&png).unwrap();
    let (window, view) = f.quick(cli(vec![pdf, png], None, None), cx);
    let targets = cx.read(|cx| view.read(cx).targets.formats.clone());
    if targets.iter().any(|t| t.id == "webp") {
        click(cx, window, "to-webp");
        click(cx, window, "options-toggle");
        assert_eq!(
            label(cx, window, "background").as_deref(),
            Some("Automatic")
        );
    }
}

#[gpui_kit::test]
fn quick_convert_offers_codec_and_keep_audio_for_video(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    // Targets come from the extension, so the contents don't matter here.
    let clip = f.dir.path().join("clip.mov");
    std::fs::write(&clip, "not really a movie").unwrap();
    save_preset(
        &f,
        cx,
        "small",
        Preset {
            to: Some("mkv".into()),
            options: Options {
                video_codec: Some(VideoCodec::Hevc),
                strip_audio: true,
                ..Options::default()
            },
        },
    );
    let (window, view) = f.quick(cli(vec![clip], None, None), cx);
    let targets = cx.read(|cx| view.read(cx).targets.formats.clone());
    if !targets.iter().any(|t| t.id == "mp4") {
        eprintln!("skipping: no engine converts MOV here (is FFmpeg installed?)");
        return;
    }
    click(cx, window, "to-mp4");
    click(cx, window, "options-toggle");
    // Defaults change nothing.
    assert_eq!(label(cx, window, "codec").as_deref(), Some("H.264"));
    assert_eq!(
        label(cx, window, "keep-audio").as_deref(),
        Some("Keep audio")
    );
    cx.read(|cx| assert_eq!(view.read(cx).conversion_options(), Options::default()));

    click(cx, window, "codec");
    click(cx, window, "codec-hevc");
    click(cx, window, "keep-audio");
    assert_eq!(label(cx, window, "codec").as_deref(), Some("HEVC"));
    cx.read(|cx| {
        let o = view.read(cx).conversion_options();
        assert_eq!(
            (o.video_codec, o.strip_audio),
            (Some(VideoCodec::Hevc), true)
        );
    });
    click(cx, window, "keep-audio");
    cx.read(|cx| assert!(!view.read(cx).conversion_options().strip_audio));

    // WebM has no codec choice but can drop audio; GIF has neither.
    click(cx, window, "to-webm");
    assert!(!shown(cx, window, "codec") && shown(cx, window, "keep-audio"));
    click(cx, window, "to-gif");
    assert!(!shown(cx, window, "codec") && !shown(cx, window, "keep-audio"));

    // A preset loads both controls, and they can be changed back.
    click(cx, window, "preset-small");
    cx.read(|cx| {
        let v = view.read(cx);
        assert_eq!(
            (v.video_codec, v.strip_audio),
            (Some(VideoCodec::Hevc), true)
        );
    });
    click(cx, window, "codec");
    click(cx, window, "codec-h264");
    click(cx, window, "keep-audio");
    cx.read(|cx| {
        let o = view.read(cx).conversion_options();
        assert_eq!(
            (o.video_codec, o.strip_audio),
            (Some(VideoCodec::H264), false)
        );
    });
}

/// Whether the element is drawn inside the window, not cut off below it.
fn fits(cx: &mut TestAppContext, handle: AnyWindowHandle, name: &str) -> bool {
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let bounds = window.find(id(name)).bounds();
        let viewport = window.viewport_size();
        bounds.bottom() <= viewport.height && bounds.right() <= viewport.width
    })
    .unwrap()
}

#[gpui_kit::test]
fn windows_fit_their_content_at_their_opening_sizes(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, Some("2026-09-30"), None);
    let app = f.app.clone();
    cx.update(|cx| {
        app.update(cx, |s, cx| {
            s.update_settings(|s| s.first_run_done = true, cx)
        })
    });
    let pngs = vec![f.png("a.png"), f.png("b.png")];
    for dark in [false, true] {
        cx.update(|cx| theme::set_dark(dark, cx));
        cx.update(|cx| super::open_quick(cli(vec![pngs[0].clone()], None, None), cx));
        let quick = *cx.update(|cx| cx.windows()).last().unwrap();
        click(cx, quick, "to-webp");
        assert!(fits(cx, quick, "convert"), "Quick convert");
        // An image's Options row, where it runs and Save fit without scrolling.
        assert!(fits(cx, quick, "change-folder"), "Quick convert, Save");
        cx.update(|cx| super::open_quick(cli(pngs.clone(), None, None), cx));
        let quick = *cx.update(|cx| cx.windows()).last().unwrap();
        click(cx, quick, "to-jpeg");
        assert!(fits(cx, quick, "convert"), "Quick convert, two files");
        assert!(fits(cx, quick, "where-cloud"), "Quick convert, Cloud");
        cx.update(|cx| cx.set_global(super::quick::TestCloud(CloudAccess::Ready)));
        click(cx, quick, "where-cloud");
        assert!(
            fits(cx, quick, "cloud-consent-agree"),
            "Quick convert, consent"
        );
        cx.update(|cx| cx.remove_global::<super::quick::TestCloud>());

        for tab in [SettingsTab::General, SettingsTab::License] {
            cx.update(|cx| super::show_settings(tab, cx));
            let (settings, _) = window_of::<SettingsView>(cx);
            let last = if tab == SettingsTab::General {
                "jobs"
            } else {
                "settings-buy"
            };
            assert!(fits(cx, settings, last), "{tab:?}");
        }

        // Onboarding at its smallest size: sign in, waiting, a key with an
        // error, and a question.
        let app = f.app.clone();
        cx.update(|cx| {
            super::show(
                size(px(super::FIRST_RUN_SIZE.0), px(super::FIRST_RUN_SIZE.1)),
                "first run",
                cx,
                |window, cx| cx.new(|cx| FirstRunView::new(app, Screen::Account, window, cx)),
            )
        });
        let (first, view) = window_of::<FirstRunView>(cx);
        for id in [
            "onboarding-google",
            "onboarding-email",
            "onboarding-key-link",
            "privacy",
        ] {
            assert!(fits(cx, first, id), "onboarding, {id}");
        }
        click(cx, first, "onboarding-google");
        assert!(fits(cx, first, "onboarding-cancel"), "onboarding, waiting");
        click(cx, first, "onboarding-cancel");
        cx.update(|cx| super::route(auth_link("not_ours", "code=x"), cx));
        assert!(fits(cx, first, "privacy"), "onboarding, notice");
        click(cx, first, "onboarding-key-link");
        click(cx, first, "onboarding-activate");
        cx.read(|cx| assert!(view.read(cx).error.is_some()));
        assert!(
            fits(cx, first, "onboarding-back"),
            "onboarding, key and error"
        );
        first
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        let app = f.app.clone();
        cx.update(|cx| {
            super::show(
                size(px(super::FIRST_RUN_SIZE.0), px(super::FIRST_RUN_SIZE.1)),
                "first run",
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        FirstRunView::new(app, Screen::Question(Question::Documents), window, cx)
                    })
                },
            )
        });
        let (first, _) = window_of::<FirstRunView>(cx);
        assert!(fits(cx, first, "question-no"), "onboarding, question");
        first
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();

        let missing = f
            .dir
            .path()
            .join("a folder with a long name that is not there yet");
        f.app.update(cx, |s, cx| {
            s.update_settings(|s| s.automations[0].folder = Some(missing.clone()), cx)
        });
        let (popover, _) = cx.update(super::open_popover).unwrap();
        assert!(fits(cx, popover, "open-settings"), "popover");
        assert!(
            fits(cx, popover, "popover-automation-0"),
            "popover, long rule"
        );

        cx.update(super::show_about);
        let (about, _) = window_of::<AboutView>(cx);
        assert!(fits(cx, about, "about-source"), "About");

        cx.update(super::show_main);
        let (main, _) = window_of::<MainView>(cx);
        assert!(fits(cx, main, "trial-buy"), "main window");
        assert!(fits(cx, main, "empty-add-files"), "main window, empty");
    }
}

// Unix permission bits make the folder unreadable; Windows has no equivalent here.
#[cfg(unix)]
#[test]
fn an_unreadable_folder_does_not_hide_the_others() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let readable = dir.path().join("readable");
    let locked = dir.path().join("locked");
    std::fs::create_dir(&readable).unwrap();
    std::fs::create_dir(&locked).unwrap();
    image::RgbImage::new(2, 2)
        .save(readable.join("a.png"))
        .unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_dir(&locked).is_ok() {
        // Running as root: permissions don't apply.
        return;
    }
    let registry = convt_engines::default_registry();
    let expanded = crate::model::expand(&registry, &[locked.clone(), readable.clone()]);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(expanded.files, vec![readable.join("a.png")]);
    assert_eq!(expanded.unreadable.len(), 1);
    assert!(
        expanded.unreadable[0].contains("locked"),
        "{:?}",
        expanded.unreadable
    );
}

#[test]
fn the_icons_the_windows_draw_are_bundled() {
    use super::theme::IconName;
    use gpui_kit::AssetSource;
    use gpui_kit::component::IconNamed;
    let assets = super::assets();
    for icon in IconName::ALL {
        let path = icon.path();
        let svg = assets.load(&path).unwrap();
        let svg = svg.unwrap_or_else(|| panic!("{path} is missing, so it would draw empty"));
        // Hugeicons, not a Lucide icon with the same name.
        assert!(
            String::from_utf8_lossy(&svg).contains("stroke-width=\"1.5\""),
            "{path} isn't a Hugeicon"
        );
    }
    // The component library's own icons (the spinner, a field's clear
    // button) answer with Hugeicons too.
    for path in ["icons/loader.svg", "icons/close.svg", "icons/check.svg"] {
        let svg = assets.load(path).unwrap().unwrap();
        assert!(
            String::from_utf8_lossy(&svg).contains("stroke-width=\"1.5\""),
            "{path}"
        );
    }
    assert!(assets.load("icons/google-g.svg").unwrap().is_some());
}

#[test]
fn only_linux_windows_left_undecorated_draw_a_title_bar() {
    use super::chrome::draws_title_bar;
    assert!(!draws_title_bar(Decorations::Server));
    assert_eq!(
        draws_title_bar(Decorations::Client {
            tiling: Tiling::default()
        }),
        cfg!(target_os = "linux")
    );
}

#[gpui_kit::test]
fn a_drawn_title_bar_sits_above_the_window_and_closes_it(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    // Without client-side decorations, which the test platform never
    // reports, the window draws no bar.
    let (main, _) = f.main(cx);
    assert!(!shown(cx, main, "window-close"));
    assert!(shown(cx, main, "add-files"));

    cx.update(|cx| cx.set_global(super::chrome::ForceTitleBar::default()));
    let app = f.app.clone();
    let (main, _) = cx.update(|cx| {
        let options = super::window_options(size(px(1040.), px(640.)), "convt", cx);
        super::open_window(options, cx, |window, cx| {
            cx.new(|cx| MainView::new(app, window, cx))
        })
        .unwrap()
    });
    assert_eq!(label(cx, main, "window-title").as_deref(), Some("convt"));
    assert_eq!(
        label(cx, main, "window-minimize").as_deref(),
        Some("Minimize")
    );
    assert_eq!(
        label(cx, main, "window-maximize").as_deref(),
        Some("Maximize")
    );
    assert_eq!(label(cx, main, "window-close").as_deref(), Some("Close"));
    assert!(
        shown(cx, main, "add-files"),
        "the window's own view still shows"
    );
    let bounds = |cx: &mut TestAppContext, name: &str| {
        cx.update_window(main, |_, window, cx| {
            window.render_frame(cx);
            window.find(id(name)).bounds()
        })
        .unwrap()
    };
    let bar = bounds(cx, "title-bar");
    assert_eq!(bar.size.height, px(super::chrome::TITLE_BAR_HEIGHT));
    assert!(
        bounds(cx, "add-files").top() >= bar.bottom(),
        "the view starts below the bar"
    );

    click(cx, main, "window-close");
    assert!(
        cx.update_window(main, |_, _, _| ()).is_err(),
        "Close removes the window"
    );
}

#[gpui_kit::test]
fn the_title_bar_moves_the_window_only_for_a_held_drag(cx: &mut TestAppContext) {
    use gpui_kit::{Modifiers, MouseButton, VisualTestContext};
    let f = Fixture::new(cx);
    cx.update(|cx| cx.set_global(super::chrome::ForceTitleBar::default()));
    let (main, _) = f.main(cx);
    let bounds = |cx: &mut TestAppContext, name: &str| {
        cx.update_window(main, |_, window, cx| {
            window.render_frame(cx);
            window.find(id(name)).bounds()
        })
        .unwrap()
    };
    let moves =
        |cx: &mut TestAppContext| cx.read(|cx| cx.global::<super::chrome::ForceTitleBar>().moves);
    let bar = bounds(cx, "title-bar");
    let on_bar = point(bar.left() + px(200.), bar.center().y);
    let below = point(on_bar.x, bar.bottom() + px(120.));
    let close = bounds(cx, "window-close").center();
    let mut w = VisualTestContext::from_window(main, cx);
    let none = Modifiers::default();

    // A fast drag whose first motion already left the bar still moves.
    w.simulate_mouse_down(on_bar, MouseButton::Left, none);
    w.simulate_mouse_move(below, MouseButton::Left, none);
    w.simulate_mouse_up(below, MouseButton::Left, none);
    assert_eq!(moves(cx), 1);

    // A press whose release the bar never saw doesn't move on a later hover.
    let mut w = VisualTestContext::from_window(main, cx);
    w.simulate_mouse_down(on_bar, MouseButton::Left, none);
    w.simulate_mouse_move(on_bar, None, none);
    w.simulate_mouse_move(below, MouseButton::Left, none);
    assert_eq!(moves(cx), 1);

    // A press on a window button never starts a drag.
    let mut w = VisualTestContext::from_window(main, cx);
    w.simulate_mouse_down(close, MouseButton::Left, none);
    w.simulate_mouse_move(below, MouseButton::Left, none);
    assert_eq!(moves(cx), 1);
}

#[test]
fn add_files_picks_files_where_the_picker_cannot_mix_them_with_folders() {
    let prompt = super::main_window::add_files_prompt(false);
    assert!(prompt.files && !prompt.directories && prompt.multiple);
    let prompt = super::main_window::add_files_prompt(true);
    assert!(prompt.files && prompt.directories);
}

#[gpui_kit::test]
fn text_fields_have_room_for_a_whole_line(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let (settings, _) = f.settings(SettingsTab::Presets, cx);
    let (quick, _) = f.quick(cli(vec![f.png("a.png")], None, None), cx);
    click(cx, quick, "to-webp");
    click(cx, quick, "file-name-edit");
    let fields = [
        (settings, "preset-name", theme::FIELD_HEIGHT),
        (settings, "preset-quality", theme::FIELD_HEIGHT),
        (settings, "preset-max-size", theme::FIELD_HEIGHT),
        (quick, "file-name", theme::SMALL_FIELD_HEIGHT),
    ];
    for (handle, name, height) in fields {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let bounds = window.find(id(name)).bounds();
            // The component draws a line 1.25 rem tall inside a 1px border.
            let line = window.rem_size() * 1.25;
            assert_eq!(
                bounds.size.height,
                px(height),
                "{name}: {bounds:?}, rem {:?}",
                window.rem_size()
            );
            assert!(
                bounds.size.height - px(2.) >= line,
                "{name} cuts off its text"
            );
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn dropdowns_open_below_their_box(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let (settings, _) = f.settings(SettingsTab::General, cx);
    click(cx, settings, "jobs");
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        let select = window.find(id("jobs")).bounds();
        let first = window.find(id("jobs-auto")).bounds();
        assert!(
            first.top() >= select.bottom(),
            "the menu covers its box: {select:?}, first choice {first:?}"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn retry_keeps_the_options_and_folder_a_conversion_asked_for(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let input = f.dir.path().join("photo.png");
    std::fs::write(&input, "not a png yet").unwrap();
    let out = f.dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let (window, _) = f.main(cx);
    let webp = convt_core::format_by_id("webp").unwrap();
    let options = Options {
        max_size: Some(2),
        ..Options::default()
    };
    let output = convt_core::Output::Dir(out.clone());
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.convert_to(std::slice::from_ref(&input), webp, &options, output, cx)
        })
    })
    .unwrap();
    wait_until(cx, "the failure", |cx| f.app.read(cx).recent.len() == 1);
    let record = cx.read(|cx| f.app.read(cx).recent[0].clone());
    assert!(matches!(record.outcome, Outcome::Failed(_)));

    // The file is fixed; Retry runs the same conversion into the same folder.
    f.png("photo.png");
    click(cx, window, &format!("retry-{}", record.id));
    wait_until(cx, "the retry", |cx| f.app.read(cx).recent.len() == 2);
    let retried = cx.read(|cx| f.app.read(cx).recent[0].clone());
    let Outcome::Done(outputs) = &retried.outcome else {
        panic!("the retry failed: {:?}", retried.outcome);
    };
    // Compare canonical paths: on macOS the temp dir is /var, a symlink to /private/var.
    assert_eq!(
        outputs[0].parent().map(|p| p.canonicalize().unwrap()),
        Some(out.canonicalize().unwrap())
    );
    let (w, h) = image::image_dimensions(&outputs[0]).unwrap();
    assert_eq!((w, h), (2, 2), "the size option was dropped");
}

#[gpui_kit::test]
fn a_key_that_does_not_cover_this_build_is_saved_not_celebrated(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, Some("2026-01-01"), None);
    let (window, view) = f.settings(SettingsTab::License, cx);
    let input = cx.read(|cx| view.read(cx).license_key.clone());
    set_input(cx, window, &input, &license_key("old-tester", "2025-01-01"));
    click(cx, window, "activate");
    assert!(f.dir.path().join("license.key").exists());
    assert_eq!(
        label(cx, window, "license-notice").as_deref(),
        Some("Saved the license for ***.")
    );
    assert_eq!(label(cx, window, "settings-buy").as_deref(), Some("Renew"));

    set_input(cx, window, &input, &license_key("new-tester", "2027-10-01"));
    click(cx, window, "activate");
    assert_eq!(
        label(cx, window, "license-notice").as_deref(),
        Some("License activated for ***.")
    );
    assert!(!shown(cx, window, "settings-buy"));
}

#[gpui_kit::test]
fn first_run_does_not_call_a_key_that_misses_this_build_ready(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    cx.update(|cx| super::route(Request::default(), cx));
    let (window, view) = window_of::<FirstRunView>(cx);
    click(cx, window, "onboarding-key-link");
    let input = cx.read(|cx| view.read(cx).key.clone());
    set_input(cx, window, &input, &license_key("old-tester", "2025-01-01"));
    click(cx, window, "onboarding-activate");
    assert!(f.dir.path().join("license.key").exists(), "the key is kept");
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("Your license is saved")
    );
    assert_eq!(
        label(cx, window, "onboarding-primary").as_deref(),
        Some("Renew")
    );
    click(cx, window, "onboarding-primary");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some(convt_license::client::BUY_URL)
    );
    assert!(fits(cx, window, "onboarding-not-now"));

    // A key that covers this build is ready.
    click(cx, window, "onboarding-key-link");
    set_input(cx, window, &input, &license_key("new-tester", "2027-10-01"));
    click(cx, window, "onboarding-activate");
    assert_eq!(
        label(cx, window, "onboarding-title").as_deref(),
        Some("You have a convt license")
    );
}

#[gpui_kit::test]
fn output_folders_are_stored_as_absolute_paths(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let input = f.png("a.png");
    let cwd = std::env::current_dir().unwrap();
    // On Windows a relative path can't leave the working directory's drive,
    // and the temp folder may be on another one.
    let near = cfg!(windows).then(|| tempfile::tempdir_in(&cwd).unwrap());
    let out = near.as_ref().map_or(f.dir.path(), |d| d.path()).join("out");
    std::fs::create_dir(&out).unwrap();
    // The same folder, relative to the working directory.
    fn named(path: &Path) -> impl Iterator<Item = std::path::Component<'_>> {
        path.components()
            .filter(|c| matches!(c, std::path::Component::Normal(_)))
    }
    let mut relative = PathBuf::new();
    for _ in named(&cwd) {
        relative.push("..");
    }
    relative.extend(named(&out));
    assert!(relative.is_relative());
    let webp = convt_core::format_by_id("webp").unwrap();
    let output = convt_core::Output::Dir(relative);
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.convert_to(
                std::slice::from_ref(&input),
                webp,
                &Options::default(),
                output,
                cx,
            )
        })
    })
    .unwrap();
    wait_until(cx, "the conversion", |cx| f.app.read(cx).recent.len() == 1);
    let record = cx.read(|cx| f.app.read(cx).recent[0].clone());
    assert_eq!(
        record.setup.map(|s| s.output),
        Some(convt_core::Output::Dir(out.canonicalize().unwrap()))
    );
}

/// The Quick convert window opened last, by `open_quick`.
fn last_quick(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<QuickView>) {
    window_of::<QuickView>(cx)
}

fn pack_phase(f: &Fixture, cx: &mut TestAppContext) -> PackPhase {
    cx.read(|cx| f.app.read(cx).pack.phase.clone())
}

#[gpui_kit::test]
fn a_document_offers_the_pack_and_downloads_only_after_the_click(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    let f = Fixture::with_packs(cx, packs.clone());
    let docx = f.docx("Report.docx");
    for dark in [false, true] {
        cx.update(|cx| theme::set_dark(dark, cx));
        let (window, view) = f.quick(cli(vec![docx.clone()], None, None), cx);
        assert_eq!(
            label(cx, window, "pack-card").as_deref(),
            Some("Document support isn't installed")
        );
        assert!(
            label(cx, window, "pack-body")
                .unwrap()
                .contains("convt downloads once")
        );
        assert_eq!(
            label(cx, window, "pack-download").as_deref(),
            Some("Download (150 MB)")
        );
        // No dead end: the picker and "can't be converted" give way to the card.
        assert!(!shown(cx, window, "skipped"));
        assert!(!shown(cx, window, "to-pdf"));
        cx.read(|cx| assert!(view.read(cx).targets.formats.is_empty()));
        // Settings shows the same state, and offers the same button.
        let (settings, _) = f.settings(SettingsTab::General, cx);
        assert_eq!(
            label(cx, settings, "pack-status").as_deref(),
            Some("Not installed")
        );
        assert!(shown(cx, settings, "pack-download"));
    }
    std::thread::sleep(Duration::from_millis(50));
    cx.run_until_parked();
    assert_eq!(packs.installs(), 0, "nothing downloads before the click");

    let (window, view) = last_quick_or(cx, &f, &docx);
    click(cx, window, "pack-download");
    // The install runs on its own thread.
    wait_until(cx, "the download", |_| packs.installs() == 1);
    wait_for_label(cx, window, "pack-done", |s| {
        s.starts_with("Document support")
    });
    assert_eq!(pack_phase(&f, cx), PackPhase::Done);
    assert!(!shown(cx, window, "pack-card"));
    // The engines now offer document targets, and only those.
    cx.read(|cx| {
        let ids: Vec<_> = view.read(cx).targets.formats.iter().map(|f| f.id).collect();
        assert_eq!(ids, ["pdf", "txt"]);
    });
    assert_eq!(f.jobs(cx), 0, "converting still takes a click");
    click(cx, window, "to-pdf");
    click(cx, window, "convert");
    let job = f.last_job(cx);
    wait_for_label(cx, window, &format!("status-{job}"), |s| {
        s.starts_with("Saved")
    });
    assert!(f.dir.path().join("Report.pdf").exists());
    assert_eq!(packs.installs(), 1);
}

/// Quick convert for `file`, opened as Add files would.
fn last_quick_or(
    cx: &mut TestAppContext,
    f: &Fixture,
    file: &Path,
) -> (AnyWindowHandle, Entity<QuickView>) {
    f.quick(cli(vec![file.to_path_buf()], None, None), cx)
}

#[gpui_kit::test]
fn a_silent_convert_of_a_document_opens_the_offer_and_never_downloads(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    let f = Fixture::with_packs(cx, packs.clone());
    let docx = f.docx("Notes.docx");
    cx.update(|cx| super::route(cli(vec![docx.clone()], Some("pdf"), None), cx));
    let (window, view) = last_quick(cx);
    assert_eq!(f.jobs(cx), 0);
    assert_eq!(packs.installs(), 0, "a right-click never downloads");
    assert!(shown(cx, window, "pack-card"));
    assert!(
        !shown(cx, window, "error"),
        "the card explains, not an error"
    );
    cx.read(|cx| assert_eq!(view.read(cx).wanted.map(|f| f.id), Some("pdf")));

    click(cx, window, "pack-download");
    wait_for_label(cx, window, "pack-done", |_| true);
    // The target the menu asked for is picked; Convert finishes the job.
    cx.read(|cx| assert_eq!(view.read(cx).to.map(|f| f.id), Some("pdf")));
    assert_eq!(f.jobs(cx), 0);
    click(cx, window, "convert");
    let job = f.last_job(cx);
    wait_for_label(cx, window, &format!("status-{job}"), |s| {
        s.starts_with("Saved")
    });
    assert!(f.dir.path().join("Notes.pdf").exists());

    // With the pack installed, the same right-click converts silently.
    let other = f.docx("Other.docx");
    let windows = cx.update(|cx| cx.windows().len());
    cx.update(|cx| super::route(cli(vec![other], Some("pdf"), None), cx));
    assert_eq!(cx.update(|cx| cx.windows().len()), windows);
    let job = f.last_job(cx);
    wait_until(cx, "the silent job", |cx| {
        f.app
            .read(cx)
            .entry(job)
            .is_some_and(|e| e.status.is_finished())
    });
    assert!(f.dir.path().join("Other.pdf").exists());
}

#[gpui_kit::test]
fn documents_added_dropped_or_in_folders_reach_the_offer(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    let f = Fixture::with_packs(cx, packs.clone());
    let docx = f.docx("Plan.docx");
    let png = f.png("a.png");
    let bmp = f.bmp("a.bmp");
    let folder = f.dir.path().join("folder");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("Budget.xlsx"), b"PK").unwrap();

    // Add files: Quick convert offers the pack beside the BMP's formats.
    let (_, main) = f.main(cx);
    main.update(cx, |v, cx| v.add(&[bmp.clone(), docx.clone()], cx));
    let (window, view) = last_quick(cx);
    cx.read(|cx| assert_eq!(view.read(cx).files, [bmp, docx.clone()]));
    assert!(shown(cx, window, "pack-download"));

    // A folder's documents aren't dropped on the floor.
    main.update(cx, |v, cx| v.add(std::slice::from_ref(&folder), cx));
    let (window, view) = last_quick(cx);
    cx.read(|cx| assert_eq!(view.read(cx).files, [folder.join("Budget.xlsx")]));
    assert!(shown(cx, window, "pack-card"));

    // The popover's drop bar does the same.
    let (_, popover) = cx.update(super::open_popover).unwrap();
    popover.update(cx, |v, cx| v.drop_files(std::slice::from_ref(&docx), cx));
    let (window, _) = last_quick(cx);
    assert!(shown(cx, window, "pack-card"));

    // A mixed selection converts what it can and says documents wait.
    let (window, _) = f.quick(cli(vec![png, docx], None, None), cx);
    assert!(
        label(cx, window, "pack-body")
            .unwrap()
            .ends_with("Until then, documents are skipped.")
    );
    assert!(shown(cx, window, "to-webp"));
    assert!(!shown(cx, window, "skipped"));
    assert_eq!(packs.installs(), 0);
}

#[gpui_kit::test]
fn the_download_shows_progress_and_can_be_cancelled(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    packs.hold.store(true, Ordering::SeqCst);
    let f = Fixture::with_packs(cx, packs.clone());
    let docx = f.docx("Report.docx");
    let (window, _) = f.quick(cli(vec![docx], None, None), cx);
    click(cx, window, "pack-download");
    wait_for_label(cx, window, "pack-progress", |s| s == "10 MB of 150 MB");
    assert_eq!(
        label(cx, window, "pack-card").as_deref(),
        Some("Downloading document support")
    );
    assert!(
        !shown(cx, window, "pack-download"),
        "one download at a time"
    );
    let (settings, _) = f.settings(SettingsTab::General, cx);
    assert_eq!(
        label(cx, settings, "pack-status").as_deref(),
        Some("Downloading…")
    );
    assert!(shown(cx, settings, "pack-progress"));

    click(cx, window, "pack-cancel");
    wait_for_label(cx, window, "pack-detail", |s| {
        s.starts_with("Download stopped")
    });
    assert_eq!(
        label(cx, window, "pack-card").as_deref(),
        Some("Document support isn't installed")
    );
    assert_eq!(
        label(cx, window, "pack-download").as_deref(),
        Some("Download (150 MB)")
    );
    assert!(matches!(
        pack_phase(&f, cx),
        PackPhase::Failed(Failure {
            kind: FailureKind::Cancelled,
            ..
        })
    ));

    packs.hold.store(false, Ordering::SeqCst);
    click(cx, window, "pack-download");
    wait_for_label(cx, window, "pack-done", |_| true);
    assert_eq!(packs.installs(), 2);
}

#[gpui_kit::test]
fn failed_downloads_say_what_happened_and_retry_on_a_click(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    let f = Fixture::with_packs(cx, packs.clone());
    let docx = f.docx("Report.docx");
    let (window, _) = f.quick(cli(vec![docx], None, None), cx);

    packs.fail_next(
        FailureKind::Network,
        "Document pack download failed. Check your connection and run Install again to resume.: Connection refused",
    );
    click(cx, window, "pack-download");
    wait_for_label(cx, window, "pack-card", |s| {
        s == "Couldn't download document support"
    });
    assert!(
        label(cx, window, "pack-detail")
            .unwrap()
            .ends_with("Connection refused")
    );
    assert_eq!(
        label(cx, window, "pack-download").as_deref(),
        Some("Try again (150 MB)")
    );
    assert_eq!(packs.installs(), 1, "no automatic retry");

    packs.fail_next(
        FailureKind::Checksum,
        "Document pack SHA-256 mismatch. Removed the download; no code was installed or run.",
    );
    click(cx, window, "pack-download");
    wait_for_label(cx, window, "pack-card", |s| {
        s == "The download didn't check out"
    });
    assert!(
        label(cx, window, "pack-body")
            .unwrap()
            .contains("Nothing was installed or run")
    );

    click(cx, window, "pack-download");
    wait_for_label(cx, window, "pack-done", |_| true);
    assert_eq!(packs.installs(), 3);
}

#[gpui_kit::test]
fn a_rejected_pack_says_why_in_plain_words(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    let reason = "document-pack path must be owned by the current user and not group/world writable: /data/convt/packs/documents";
    *packs.rejected.lock().unwrap() = Some(reason.into());
    let f = Fixture::with_packs(cx, packs.clone());
    let docx = f.docx("Report.docx");
    let (window, _) = f.quick(cli(vec![docx], None, None), cx);
    assert_eq!(
        label(cx, window, "pack-card").as_deref(),
        Some("Document support needs reinstalling")
    );
    assert_eq!(
        label(cx, window, "pack-body").as_deref(),
        Some(
            "Other users on this computer could change its files, so convt won't run it. Download it again to fix this."
        )
    );
    assert_eq!(label(cx, window, "pack-detail").as_deref(), Some(reason));
    assert_eq!(
        label(cx, window, "pack-download").as_deref(),
        Some("Download again (150 MB)")
    );

    let (settings, _) = f.settings(SettingsTab::General, cx);
    assert!(
        label(cx, settings, "pack-status")
            .unwrap()
            .starts_with("Other users")
    );
    assert!(shown(cx, settings, "pack-remove"));
    assert_eq!(packs.installs(), 0);
}

#[gpui_kit::test]
fn settings_removes_the_pack_after_asking_and_waits_for_document_jobs(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    packs.installed.store(true, Ordering::SeqCst);
    let f = Fixture::with_packs(cx, packs.clone());
    let (settings, view) = f.settings(SettingsTab::General, cx);
    assert_eq!(
        label(cx, settings, "pack-status").as_deref(),
        Some("Installed")
    );
    assert!(!shown(cx, settings, "pack-download"));

    click(cx, settings, "pack-remove");
    assert!(shown(cx, settings, "pack-remove-confirm"));
    click(cx, settings, "pack-remove-keep");
    assert!(!shown(cx, settings, "pack-remove-confirm"));
    assert_eq!(packs.removes.load(Ordering::SeqCst), 0);

    // Not while a document is converting.
    let docx = f.docx("Report.docx");
    let app = f.app.clone();
    cx.update(|cx| {
        app.update(cx, |s, cx| {
            let pdf = format_by_id("pdf").unwrap();
            s.convert(std::slice::from_ref(&docx), pdf, &Options::default(), cx)
                .unwrap();
            assert!(s.remove_pack(cx).unwrap_err().contains("Wait"));
        })
    });
    assert_eq!(packs.removes.load(Ordering::SeqCst), 0);
    wait_until(cx, "the document job", |cx| {
        app.read(cx).queue.active() == 0
    });

    click(cx, settings, "pack-remove");
    click(cx, settings, "pack-remove-confirm");
    wait_for_label(cx, settings, "pack-status", |s| s == "Not installed");
    assert_eq!(packs.removes.load(Ordering::SeqCst), 1);
    assert!(shown(cx, settings, "pack-download"));
    cx.read(|cx| {
        assert!(!view.read(cx).confirm_remove_pack);
        assert!(!f.app.read(cx).documents_supported());
    });
    assert_eq!(packs.installs(), 0);
}

#[gpui_kit::test]
fn a_build_without_a_pinned_pack_offers_no_download(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks {
        unconfigured: true,
        ..TestPacks::default()
    });
    let f = Fixture::with_packs(cx, packs.clone());
    let docx = f.docx("Report.docx");
    let (window, _) = f.quick(cli(vec![docx], None, None), cx);
    assert!(
        label(cx, window, "pack-body")
            .unwrap()
            .starts_with("This build of convt has no document pack")
    );
    assert!(!shown(cx, window, "pack-download"));
    // Even a direct call does nothing without a pinned pack.
    let app = f.app.clone();
    cx.update(|cx| app.update(cx, |s, cx| s.download_pack(cx)));
    assert_eq!(packs.installs(), 0);
}

/// The network rule in code: the engines' installer is reached only through
/// `pack::Engines::install`, which only `AppState::download_pack` calls, which
/// only the Download button's click handler calls.
#[test]
fn only_the_download_button_reaches_the_installer() {
    let sources = [
        ("main.rs", include_str!("../main.rs")),
        ("model.rs", include_str!("../model.rs")),
        ("pack.rs", include_str!("../pack.rs")),
        ("instance.rs", include_str!("../instance.rs")),
        ("request.rs", include_str!("../request.rs")),
        ("thumbs.rs", include_str!("../thumbs.rs")),
        ("ui/mod.rs", include_str!("mod.rs")),
        ("ui/main_window.rs", include_str!("main_window.rs")),
        ("ui/quick.rs", include_str!("quick.rs")),
        ("ui/popover.rs", include_str!("popover.rs")),
        ("ui/settings_window.rs", include_str!("settings_window.rs")),
        ("ui/first_run.rs", include_str!("first_run.rs")),
        ("ui/pack.rs", include_str!("pack.rs")),
    ];
    let uses = |needle: &str| -> Vec<&str> {
        sources
            .iter()
            .flat_map(|(name, src)| src.matches(needle).map(move |_| *name))
            .collect()
    };
    assert_eq!(uses("install_documents"), ["pack.rs"]);
    assert_eq!(uses("ureq"), Vec::<&str>::new());
    // The definition, and the one call in `start_install`.
    assert_eq!(uses("download_pack("), ["model.rs", "ui/pack.rs"]);
    // Which only the Download button's click and onboarding's Yes call.
    assert_eq!(
        uses("start_install("),
        ["ui/first_run.rs", "ui/pack.rs", "ui/pack.rs"]
    );
    // A Windows checkout may have CRLF line endings.
    let pack = include_str!("pack.rs").replace("\r\n", "\n");
    let button = pack.split("fn download_button").nth(1).unwrap();
    let button = &button[..button.find("\n}\n").unwrap()];
    assert!(button.contains(".on_click(") && button.contains("start_install(&app, cx)"));
    let first_run = include_str!("first_run.rs").replace("\r\n", "\n");
    let answer = first_run.split("fn answer").nth(1).unwrap();
    let answer = &answer[..answer.find("\n    }\n").unwrap()];
    assert!(answer.contains("if yes") && answer.contains("start_install(&self.app, cx)"));
    // `backend.install(` runs inside download_pack's worker thread only.
    assert_eq!(uses("backend.install("), ["model.rs"]);
    let model = include_str!("../model.rs").replace("\r\n", "\n");
    let body = &model[model.find("pub fn download_pack").unwrap()..];
    let body = &body[..body.find("\n    }\n").unwrap()];
    assert!(body.contains("backend.install("));
}

#[gpui_kit::test]
fn local_failures_get_local_advice(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    let f = Fixture::with_packs(cx, packs.clone());
    let docx = f.docx("Report.docx");
    let (window, _) = f.quick(cli(vec![docx], None, None), cx);
    for (kind, title, says) in [
        (
            FailureKind::Permission,
            "convt can't write where document support goes",
            "isn't allowed to write",
        ),
        (
            FailureKind::DiskFull,
            "Not enough disk space",
            "needs about 560 MB free",
        ),
        (
            FailureKind::HttpStatus(503),
            "The download server had a problem",
            "HTTP 503",
        ),
        (
            FailureKind::Network,
            "Couldn't download document support",
            "Check your internet connection",
        ),
    ] {
        packs.fail_next(kind, "the engines' detail");
        click(cx, window, "pack-download");
        wait_for_label(cx, window, "pack-card", |s| s == title);
        let body = label(cx, window, "pack-body").unwrap();
        assert!(body.contains(says), "{kind:?}: {body}");
        if kind != FailureKind::Network {
            assert!(!body.contains("internet"), "{kind:?}: {body}");
        }
        assert_eq!(
            label(cx, window, "pack-detail").as_deref(),
            Some("the engines' detail")
        );
    }
}

#[gpui_kit::test]
fn the_destination_has_its_own_line_from_home(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    let f = Fixture::with_packs(cx, packs);
    let docx = f.docx("Report.docx");
    let (window, _) = f.quick(cli(vec![docx], None, None), cx);
    let expected = if cfg!(windows) {
        home()
            .join(".local/share/convt/packs/documents")
            .display()
            .to_string()
    } else {
        "~/.local/share/convt/packs/documents".into()
    };
    assert_eq!(
        label(cx, window, "pack-destination").as_deref(),
        Some(expected.as_str())
    );
}

/// The home folder `TestPacks` installs into.
fn home() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    PathBuf::from(std::env::var_os(var).unwrap_or_default())
}

#[gpui_kit::test]
fn documents_wait_while_the_pack_is_removed(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    packs.installed.store(true, Ordering::SeqCst);
    packs.hold_remove.store(true, Ordering::SeqCst);
    let f = Fixture::with_packs(cx, packs.clone());
    let docx = f.docx("Report.docx");
    let app = f.app.clone();
    cx.update(|cx| app.update(cx, |s, cx| s.remove_pack(cx).unwrap()));
    wait_until(cx, "the removal to start", |_| {
        packs.removes.load(Ordering::SeqCst) == 1
    });
    // The registry has no documents before any file is deleted, and no
    // document job can start.
    cx.update(|cx| {
        app.update(cx, |s, cx| {
            assert!(!s.documents_supported());
            let pdf = format_by_id("pdf").unwrap();
            let error = s
                .convert(std::slice::from_ref(&docx), pdf, &Options::default(), cx)
                .unwrap_err();
            assert!(error.contains("being removed"), "{error}");
            assert!(s.add_files(std::slice::from_ref(&docx), cx).jobs.is_empty());
        })
    });
    assert_eq!(f.jobs(cx), 0);
    // A right-click on a document opens the card, which says why.
    cx.update(|cx| super::route(cli(vec![docx.clone()], Some("pdf"), None), cx));
    let (window, _) = last_quick(cx);
    assert_eq!(
        label(cx, window, "pack-card").as_deref(),
        Some("Removing document support")
    );
    assert!(!shown(cx, window, "pack-download"));
    assert_eq!(f.jobs(cx), 0);

    packs.hold_remove.store(false, Ordering::SeqCst);
    wait_until(cx, "the removal", |cx| !app.read(cx).pack.removing);
    wait_for_label(cx, window, "pack-card", |s| {
        s == "Document support isn't installed"
    });
}

#[gpui_kit::test]
fn a_reinstall_waits_for_document_jobs_and_holds_new_ones(cx: &mut TestAppContext) {
    let packs = Arc::new(TestPacks::default());
    packs.installed.store(true, Ordering::SeqCst);
    let f = Fixture::with_packs(cx, packs.clone());
    let docx = f.docx("Report.docx");
    let app = f.app.clone();
    let pdf = format_by_id("pdf").unwrap();
    cx.update(|cx| {
        app.update(cx, |s, cx| {
            s.convert(std::slice::from_ref(&docx), pdf, &Options::default(), cx)
                .unwrap();
            s.download_pack(cx);
        })
    });
    assert_eq!(packs.installs(), 0, "not while a document converts");
    let (settings, _) = f.settings(SettingsTab::General, cx);
    assert!(
        label(cx, settings, "error")
            .unwrap()
            .starts_with("Wait for the document conversions to finish")
    );
    wait_until(cx, "the document job", |cx| {
        app.read(cx).queue.active() == 0
    });

    // Once nothing converts, it reinstalls, and documents wait for it.
    packs.hold.store(true, Ordering::SeqCst);
    cx.update(|cx| app.update(cx, |s, cx| s.download_pack(cx)));
    wait_until(cx, "the download", |_| packs.installs() == 1);
    cx.update(|cx| {
        app.update(cx, |s, cx| {
            let error = s
                .convert(std::slice::from_ref(&docx), pdf, &Options::default(), cx)
                .unwrap_err();
            assert!(error.contains("being reinstalled"), "{error}");
            // Other files still convert.
            let png = f.png("a.png");
            let jpeg = format_by_id("jpeg").unwrap();
            s.convert(&[png], jpeg, &Options::default(), cx).unwrap();
        })
    });
    packs.hold.store(false, Ordering::SeqCst);
    wait_until(cx, "the reinstall", |cx| {
        app.read(cx).pack.phase == PackPhase::Done
    });
    cx.update(|cx| {
        app.update(cx, |s, cx| {
            s.convert(std::slice::from_ref(&docx), pdf, &Options::default(), cx)
                .unwrap();
        })
    });
}

/// A Pro key signed with [`test_key`].
fn pro_key(email: &str, updates_until: &str) -> String {
    let license = License {
        id: format!("lic_pro_{updates_until}"),
        email: email.into(),
        plan: Plan::Pro,
        issued: "2026-09-01".into(),
        updates_until: updates_until.into(),
    };
    convt_license::sign(&license, &test_key())
}

/// One query value of a URL the app opened. The values the app writes are
/// base64url or percent-encoded.
fn query(url: &str, key: &str) -> String {
    url.split(['?', '&'])
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("{key} is not in {url}"))
        .to_string()
}

fn auth_link(state: &str, rest: &str) -> Request {
    crate::request::parse_url(&format!("convt://auth?state={state}&{rest}")).unwrap()
}

fn today() -> String {
    convt_license::date::from_days(client::today())
}

#[gpui_kit::test]
fn signing_in_from_settings_trades_the_code_and_fetches_the_pro_key(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, Some("2026-09-01"), None);
    let (settings, _) = f.settings(SettingsTab::License, cx);
    assert_eq!(
        label(cx, settings, "sign-in").as_deref(),
        Some("Sign in with convt.app")
    );
    click(cx, settings, "sign-in");
    let page = cx.opened_url().expect("the device page opened");
    let state = query(&page, "state");
    let challenge = query(&page, "challenge");
    assert!(query(&page, "os").len() > 1);
    assert_eq!(query(&page, "version"), crate::account::VERSION);
    assert!(
        label(cx, settings, "account-status")
            .unwrap()
            .starts_with("Waiting for your browser")
    );
    // "Open the page again" reopens the same flow.
    cx.update(|cx| cx.open_url("about:blank"));
    click(cx, settings, "sign-in-reopen");
    assert_eq!(cx.opened_url().as_deref(), Some(page.as_str()));
    // Waiting is not signed in, and nothing reached the network.
    assert_eq!(f.api.calls(), (0, 0, 0));

    *f.api.exchange.lock().unwrap() = Ok(Session {
        email: "pro-tester".into(),
        token: "cvd_issued".into(),
    });
    f.api
        .answer_key(Ok(Some(pro_key("pro-tester", "2026-11-01"))));
    cx.update(|cx| super::route(auth_link(&state, "code=c0de"), cx));
    wait_until(cx, "the refresh finished", |cx| {
        matches!(
            f.app.read(cx).account.refresh,
            crate::account::Refresh::Done(_)
        )
    });
    // The code went back with the verifier whose hash the page got.
    let (code, verifier) = f.api.traded.lock().unwrap().clone().unwrap();
    assert_eq!(code, "c0de");
    assert_eq!(challenge_of(&verifier), challenge);
    assert!(!page.contains(&verifier));
    assert_eq!(f.api.calls(), (1, 1, 0));
    // The token is in the store, the key arrived without a confirm click.
    let stored = std::fs::read_to_string(f.dir.path().join("account.json")).unwrap();
    assert!(stored.contains("cvd_issued"));
    cx.read(|cx| {
        let s = f.app.read(cx);
        assert!(matches!(&s.license, client::State::Licensed(l) if l.plan == Plan::Pro));
        assert_eq!(
            s.settings.license_checked.as_deref(),
            Some(today().as_str())
        );
    });
    assert_eq!(
        label(cx, settings, "account-status").as_deref(),
        Some("Signed in to convt.app as ***.")
    );
    assert_eq!(
        label(cx, settings, "refresh-status").as_deref(),
        Some("Got your Pro key, with updates until 2026-11-01.")
    );
    assert!(f.dir.path().join("license.key").exists());
}

#[gpui_kit::test]
fn unsolicited_replayed_and_stale_links_never_sign_in(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    *f.api.exchange.lock().unwrap() = Ok(Session {
        email: "pro-tester".into(),
        token: "cvd_issued".into(),
    });
    f.api.answer_key(Ok(None));

    // No sign-in started: the link is dropped, with no network call.
    cx.update(|cx| super::route(auth_link("forged_state", "code=stolen"), cx));
    let (settings, _) = window_of::<SettingsView>(cx);
    assert!(
        label(cx, settings, "account-notice")
            .unwrap()
            .contains("didn't start")
    );
    assert_eq!(f.api.calls(), (0, 0, 0));
    assert!(!f.dir.path().join("account.json").exists());

    // While waiting, a link with another state is dropped and the wait goes on.
    click(cx, settings, "sign-in");
    let state = query(&cx.opened_url().unwrap(), "state");
    cx.update(|cx| super::route(auth_link("forged_state", "code=stolen"), cx));
    assert_eq!(f.api.calls(), (0, 0, 0));
    cx.read(|cx| {
        assert_eq!(
            f.app.read(cx).account.sign_in,
            crate::account::SignIn::Waiting
        )
    });

    // The right link signs in once.
    cx.update(|cx| super::route(auth_link(&state, "code=good"), cx));
    wait_until(cx, "signed in", |cx| {
        f.app.read(cx).account.session.is_some()
    });
    wait_until(cx, "refreshed", |cx| {
        !matches!(
            f.app.read(cx).account.refresh,
            crate::account::Refresh::Running
        )
    });
    assert_eq!(f.api.calls(), (1, 1, 0));

    // Replaying it does nothing: the flow it belonged to is used up.
    cx.update(|cx| super::route(auth_link(&state, "code=good"), cx));
    cx.run_until_parked();
    assert_eq!(f.api.calls(), (1, 1, 0));
    assert!(
        label(cx, settings, "account-notice")
            .unwrap()
            .contains("didn't start")
    );
    cx.read(|cx| assert_eq!(f.app.read(cx).account.email(), Some("pro-tester")));

    // After a cancel, the cancelled flow's link is dropped too.
    click(cx, settings, "sign-out");
    wait_until(cx, "revoked", |_| f.api.calls().2 == 1);
    click(cx, settings, "sign-in");
    let state = query(&cx.opened_url().unwrap(), "state");
    click(cx, settings, "sign-in-cancel");
    cx.update(|cx| super::route(auth_link(&state, "code=late"), cx));
    cx.run_until_parked();
    assert_eq!(f.api.calls().0, 1);

    // A flow left too long fails instead of signing in.
    click(cx, settings, "sign-in");
    let state = query(&cx.opened_url().unwrap(), "state");
    cx.update(|cx| {
        f.app.update(cx, |s, _| {
            s.age_sign_in(convt_license::account::SIGN_IN_TIMEOUT)
        })
    });
    cx.update(|cx| super::route(auth_link(&state, "code=slow"), cx));
    cx.run_until_parked();
    assert_eq!(f.api.calls().0, 1);
    assert!(
        label(cx, settings, "account-status")
            .unwrap()
            .contains("took too long")
    );
    assert!(!f.dir.path().join("account.json").exists());
}

#[gpui_kit::test]
fn a_sign_in_cancelled_or_refused_in_the_browser_fails(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    let (settings, _) = f.settings(SettingsTab::License, cx);
    click(cx, settings, "sign-in");
    let state = query(&cx.opened_url().unwrap(), "state");
    cx.update(|cx| super::route(auth_link(&state, "error=access_denied"), cx));
    assert_eq!(
        label(cx, settings, "account-status").as_deref(),
        Some("Sign-in was cancelled in the browser.")
    );
    assert_eq!(label(cx, settings, "sign-in").as_deref(), Some("Try again"));
    assert_eq!(f.api.calls(), (0, 0, 0));

    // convt.app refuses the code (used or expired on its side).
    click(cx, settings, "sign-in");
    let state = query(&cx.opened_url().unwrap(), "state");
    cx.update(|cx| super::route(auth_link(&state, "code=used"), cx));
    wait_until(cx, "the exchange failed", |cx| {
        matches!(
            f.app.read(cx).account.sign_in,
            crate::account::SignIn::Failed(_)
        )
    });
    assert!(
        label(cx, settings, "account-status")
            .unwrap()
            .contains("didn't accept")
    );
    assert!(!f.dir.path().join("account.json").exists());
    assert_eq!(f.api.calls(), (1, 0, 0));
}

#[gpui_kit::test]
fn launch_renews_once_a_day_and_offline_keeps_the_key(cx: &mut TestAppContext) {
    let current = pro_key("pro-tester", "2026-10-15");
    let f = Fixture::signed_in(cx, Some(&current), "pro-tester");
    // Offline at launch: one try, the key stays, the failure shows in Settings.
    cx.update(|cx| f.app.update(cx, |s, cx| s.renew_on_launch(cx)));
    wait_until(cx, "the refresh failed", |cx| {
        matches!(
            f.app.read(cx).account.refresh,
            crate::account::Refresh::Failed(_)
        )
    });
    assert_eq!(f.api.calls(), (0, 1, 0));
    let (settings, _) = f.settings(SettingsTab::License, cx);
    let status = label(cx, settings, "refresh-status").unwrap();
    assert!(
        status.contains("couldn't be reached") && status.contains("stays as it is"),
        "{status}"
    );
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("license.key")).unwrap(),
        current
    );
    cx.read(|cx| assert!(matches!(f.app.read(cx).license, client::State::Licensed(_))));
    // Signed in still: offline is not signed out.
    cx.read(|cx| assert!(f.app.read(cx).account.session.is_some()));

    // A second launch the same day asks nothing.
    cx.update(|cx| f.app.update(cx, |s, cx| s.renew_on_launch(cx)));
    cx.run_until_parked();
    assert_eq!(f.api.calls(), (0, 1, 0));
    // A launch on a later day asks again.
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.update_settings(|s| s.license_checked = Some("2026-01-01".into()), cx)
        })
    });
    cx.update(|cx| f.app.update(cx, |s, cx| s.renew_on_launch(cx)));
    wait_until(cx, "the second launch's refresh", |_| f.api.calls().1 == 2);

    // Refresh license asks whenever clicked, and stores the next period's key.
    let next = pro_key("pro-tester", "2026-11-15");
    f.api.answer_key(Ok(Some(next.clone())));
    wait_until(cx, "idle", |cx| {
        f.app.read(cx).account.refresh != crate::account::Refresh::Running
    });
    click(cx, settings, "refresh-license");
    wait_for_label(cx, settings, "refresh-status", |s| {
        s.starts_with("Got your Pro key")
    });
    assert_eq!(f.api.calls(), (0, 3, 0));
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("license.key"))
            .unwrap()
            .trim(),
        next
    );

    // Lapsed: the server's newest key is the last one, which changes nothing.
    f.api.answer_key(Ok(Some(current.clone())));
    click(cx, settings, "refresh-license");
    wait_for_label(cx, settings, "refresh-status", |s| {
        s.starts_with("Your license is up to date")
    });
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("license.key"))
            .unwrap()
            .trim(),
        next
    );
    // No Pro key at all: the stored key stays.
    f.api.answer_key(Ok(None));
    click(cx, settings, "refresh-license");
    wait_for_label(cx, settings, "refresh-status", |s| s.contains("no Pro key"));
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("license.key"))
            .unwrap()
            .trim(),
        next
    );
    // A key that doesn't verify changes nothing.
    f.api.answer_key(Ok(Some("forged.key".into())));
    click(cx, settings, "refresh-license");
    wait_for_label(cx, settings, "refresh-status", |s| {
        s.contains("doesn't accept")
    });
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("license.key"))
            .unwrap()
            .trim(),
        next
    );
}

#[gpui_kit::test]
fn signed_out_the_app_never_calls_convt_app(cx: &mut TestAppContext) {
    let f = Fixture::licensed(
        cx,
        Some("2026-09-30"),
        Some(&license_key("a-tester", "2027-10-01")),
    );
    cx.update(|cx| f.app.update(cx, |s, cx| s.renew_on_launch(cx)));
    cx.update(|cx| f.app.update(cx, |s, cx| s.refresh_license(cx)));
    let (settings, _) = f.settings(SettingsTab::License, cx);
    assert!(!shown(cx, settings, "refresh-license"));
    cx.run_until_parked();
    assert_eq!(f.api.calls(), (0, 0, 0));
    cx.read(|cx| assert_eq!(f.app.read(cx).settings.license_checked, None));
    // A Desktop owner sees why they don't need to sign in.
    let (general, _) = f.settings(SettingsTab::General, cx);
    assert!(
        label(cx, general, "network-refresh")
            .unwrap()
            .contains("only while you're signed in")
    );
    assert!(
        label(cx, general, "network-updates")
            .unwrap()
            .starts_with("Update checks")
    );
}

#[gpui_kit::test]
fn a_device_revoked_on_the_dashboard_signs_out_here_and_keeps_the_key(cx: &mut TestAppContext) {
    let key = pro_key("pro-tester", "2026-10-15");
    let f = Fixture::signed_in(cx, Some(&key), "pro-tester");
    f.api.answer_key(Err(ApiError::SignedOut));
    let (settings, _) = f.settings(SettingsTab::License, cx);
    click(cx, settings, "refresh-license");
    wait_until(cx, "signed out", |cx| {
        f.app.read(cx).account.session.is_none()
    });
    assert!(!f.dir.path().join("account.json").exists());
    assert!(
        label(cx, settings, "refresh-status")
            .unwrap()
            .contains("signed out of convt.app")
    );
    assert_eq!(
        label(cx, settings, "sign-in").as_deref(),
        Some("Sign in with convt.app")
    );
    cx.read(|cx| assert!(matches!(f.app.read(cx).license, client::State::Licensed(_))));
    // Nothing more is asked while signed out.
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.update_settings(|s| s.license_checked = None, cx);
            s.renew_on_launch(cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(f.api.calls(), (0, 1, 0));
}

#[gpui_kit::test]
fn sign_out_forgets_the_token_and_revokes_it(cx: &mut TestAppContext) {
    let key = pro_key("pro-tester", "2026-10-15");
    let f = Fixture::signed_in(cx, Some(&key), "pro-tester");
    let (settings, _) = f.settings(SettingsTab::License, cx);
    click(cx, settings, "sign-out");
    assert!(!f.dir.path().join("account.json").exists());
    wait_until(cx, "revoked on the server", |_| f.api.calls().2 == 1);
    assert_eq!(
        label(cx, settings, "account-notice").as_deref(),
        Some("Signed out. The license on this computer stays.")
    );
    assert!(f.dir.path().join("license.key").exists());
}

#[gpui_kit::test]
fn signing_out_ends_the_account_trial_on_screen(cx: &mut TestAppContext) {
    let f = Fixture::signed_in(cx, None, "trial-tester");
    // No local trial was ever started on this computer.
    std::fs::remove_file(f.dir.path().join("trial")).unwrap();
    let ends_on = convt_license::date::from_days(client::today() + 5);
    f.app.update(cx, |s, cx| {
        s.licensing.disable_local_trial();
        s.licensing
            .set_account_trial_exact(Some(ends_on.clone()), Some(format!("{ends_on}T12:00:00Z")));
        s.license = s.licensing.state();
        cx.notify();
    });
    let (main, _) = f.main(cx);
    let (settings, _) = f.settings(SettingsTab::License, cx);
    let status = label(cx, settings, "license-status").unwrap();
    assert!(status.starts_with("Pro trial"), "{status}");

    click(cx, settings, "sign-out");
    let signed_out = Some("Sign in to start your free trial.".to_string());
    assert_eq!(label(cx, settings, "license-status"), signed_out);
    assert_eq!(label(cx, main, "trial-card"), signed_out);
    cx.read(|cx| assert!(!f.app.read(cx).license.allows_conversion()));
}

#[gpui_kit::test]
fn every_sign_in_state_renders_in_both_themes(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, Some("2026-09-30"), None);
    for dark in [false, true] {
        cx.update(|cx| theme::set_dark(dark, cx));
        let (settings, _) = f.settings(SettingsTab::License, cx);
        let app = f.app.clone();
        let (first, _) = open(cx, move |window, cx| {
            cx.new(|cx| FirstRunView::new(app, Screen::Account, window, cx))
        });
        // Signed out (the second pass starts where the first failed).
        assert!(shown(cx, settings, "sign-in"));
        assert!(shown(cx, first, "onboarding-google") || shown(cx, first, "onboarding-retry"));
        assert!(shown(cx, settings, "refresh-note"));
        // Waiting.
        click(cx, settings, "sign-in");
        assert!(shown(cx, settings, "sign-in-cancel") && shown(cx, first, "onboarding-cancel"));
        // Failed.
        let state = query(&cx.opened_url().unwrap(), "state");
        cx.update(|cx| super::route(auth_link(&state, "error=denied"), cx));
        assert!(shown(cx, first, "onboarding-retry"));
        assert_eq!(label(cx, settings, "sign-in").as_deref(), Some("Try again"));
        cx.update(|cx| {
            cx.windows()
                .iter()
                .for_each(|w| drop(w.update(cx, |_, w, _| w.remove_window())))
        });
    }
}

#[gpui_kit::test]
fn a_second_sign_in_cannot_start_while_the_first_is_finishing(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    *f.api.exchange.lock().unwrap() = Ok(Session {
        email: "pro-tester".into(),
        token: "cvd_first".into(),
    });
    f.api.answer_key(Ok(None));
    let (settings, _) = f.settings(SettingsTab::License, cx);
    click(cx, settings, "sign-in");
    let page = cx.opened_url().unwrap();
    f.api.hold_exchange.store(true, Ordering::SeqCst);
    cx.update(|cx| super::route(auth_link(&query(&page, "state"), "code=first"), cx));
    wait_until(cx, "the exchange started", |_| f.api.calls().0 == 1);
    cx.read(|cx| {
        assert_eq!(
            f.app.read(cx).account.sign_in,
            crate::account::SignIn::Finishing
        )
    });
    // No sign-in button shows meanwhile, and starting anyway does nothing.
    assert!(!shown(cx, settings, "sign-in"));
    cx.update(|cx| cx.open_url("about:blank"));
    cx.update(|cx| f.app.update(cx, |s, cx| s.start_sign_in(cx)));
    assert_eq!(cx.opened_url().as_deref(), Some("about:blank"));
    cx.read(|cx| {
        assert_eq!(
            f.app.read(cx).account.sign_in,
            crate::account::SignIn::Finishing
        )
    });
    f.api.hold_exchange.store(false, Ordering::SeqCst);
    wait_until(cx, "signed in", |cx| {
        f.app.read(cx).account.session.is_some()
    });
    assert_eq!(f.api.calls().0, 1);
}

/// The key test update manifests are signed with. Made up, and not the
/// license test key: the two must never be the same.
fn update_key() -> SigningKey {
    SigningKey::from_bytes(&[23; 32])
}

/// A signed manifest issued an hour ago, listing `builds` as (version, date).
///
/// The app compares builds against its own crate version, which Changesets
/// bumps on every release. Builds meant to be newer use 9.x so a version bump
/// never turns them into downgrades; 0.1.0 stays older than any real version.
fn manifest(sequence: u64, builds: &[(&str, &str)], key: &SigningKey) -> Vec<u8> {
    use base64::Engine as _;
    use ed25519_dalek::Signer as _;
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let artifact = |platform: &str, kind: &str| convt_update::Artifact {
        platform: platform.into(),
        kind: kind.into(),
        url: "https://downloads.convt.test/convt".into(),
        size: 1,
        sha256: "a".repeat(64),
    };
    let m = convt_update::Manifest {
        schema_version: 1,
        sequence,
        issued_at: now - 3600,
        expires_at: now + 30 * 86_400,
        distribution_ready: true,
        purchase_url: "https://convt.test/pricing".into(),
        builds: builds
            .iter()
            .map(|(v, d)| convt_update::Build {
                version: (*v).into(),
                build_date: (*d).into(),
                artifacts: vec![artifact("linux-x86_64", "AppImage")],
                source: artifact("source", "tar.gz"),
            })
            .collect(),
    };
    let payload = b64.encode(serde_json::to_vec(&m).unwrap());
    let signature = b64.encode(
        key.sign(format!("{}{payload}", convt_update::SIGNING_DOMAIN).as_bytes())
            .to_bytes(),
    );
    serde_json::to_vec(&convt_update::SignedManifest { payload, signature }).unwrap()
}

fn wait_for_check(f: &Fixture, cx: &mut TestAppContext) -> Update {
    wait_until(cx, "the update check", |cx| {
        f.app.read(cx).update != Update::Checking
    });
    cx.read(|cx| f.app.read(cx).update.clone())
}

fn launch_check(f: &Fixture, cx: &mut TestAppContext) {
    cx.update(|cx| f.app.update(cx, |s, cx| s.start_update_checks(cx)));
}

fn manual_check(f: &Fixture, cx: &mut TestAppContext) -> Update {
    cx.update(|cx| f.app.update(cx, |s, cx| s.check_updates(cx)));
    wait_for_check(f, cx)
}

#[gpui_kit::test]
fn a_covered_update_shows_and_opens_the_download_page(cx: &mut TestAppContext) {
    // The license covers builds through 2026-10-03.
    let f = Fixture::licensed(cx, None, Some(&license_key("a-tester", "2026-10-03")));
    let builds = [
        ("0.1.0", "2026-10-01"),
        ("9.2.0", "2026-10-03"),
        ("9.3.0", "2026-10-04"),
    ];
    f.releases.serve(Ok(manifest(7, &builds, &update_key())));
    launch_check(&f, cx);
    assert_eq!(
        wait_for_check(&f, cx),
        Update::Available {
            version: "9.2.0".into(),
            date: "2026-10-03".into(),
            uncovered: Some("9.3.0".into()),
        }
    );
    cx.read(|cx| {
        let s = &f.app.read(cx).settings;
        assert_eq!(s.update_sequence, 7);
        assert!(s.update_checked_at.is_some());
    });
    assert!(f.settings_file().contains("update_sequence = 7"));

    let (main, _) = f.main(cx);
    assert_eq!(
        label(cx, main, "update-card").as_deref(),
        Some("Update available: convt 9.2.0")
    );
    click(cx, main, "update-download");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some(convt_license::client::DOWNLOAD_URL)
    );
    let (settings, view) = f.settings(SettingsTab::General, cx);
    view.update(cx, |v, cx| v.reveal_updates(cx));
    let status = label(cx, settings, "update-status").unwrap();
    assert!(
        status.starts_with("convt 9.2.0 is available Built Oct 3, 2026.")
            && status.contains("9.3.0 is out too and needs a renewed license"),
        "{status}"
    );
    assert_eq!(
        label(cx, settings, "update-version").as_deref(),
        Some(format!("convt {}", crate::account::VERSION).as_str())
    );
    let last = label(cx, settings, "update-last-checked").unwrap();
    assert!(
        last.starts_with("Built ") && last.contains(" · Last checked today at "),
        "{last}"
    );
    click(cx, settings, "update-notes");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://convt.app/changelog#v9.2.0")
    );
    click(cx, settings, "update-download");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some(convt_license::client::DOWNLOAD_URL)
    );

    // Every launch checks, even the same day.
    launch_check(&f, cx);
    wait_for_check(&f, cx);
    assert_eq!(f.releases.fetches(), 2);
    // Nothing was downloaded or installed: the only fetches were the list.
}

#[gpui_kit::test]
fn a_running_app_checks_again_every_few_hours(cx: &mut TestAppContext) {
    use crate::update::{CHECK_INTERVAL, SCHEDULE_TICK};
    let f = Fixture::licensed(cx, Some("2026-09-30"), None);
    f.releases
        .serve(Ok(manifest(1, &[("0.1.0", "2026-10-01")], &update_key())));
    launch_check(&f, cx);
    assert_eq!(wait_for_check(&f, cx), Update::UpToDate);
    assert_eq!(f.releases.fetches(), 1);
    assert!(CHECK_INTERVAL >= Duration::from_secs(4 * 3600));
    assert!(CHECK_INTERVAL <= Duration::from_secs(6 * 3600));

    // The schedule looks again soon, but the last check is recent.
    cx.executor().advance_clock(SCHEDULE_TICK);
    cx.run_until_parked();
    assert_eq!(f.releases.fetches(), 1);

    // Once the interval has passed, the next tick checks.
    let long_ago = crate::update::now_unix() - CHECK_INTERVAL.as_secs();
    cx.update(|cx| f.app.update(cx, |s, _| s.update_attempted = Some(long_ago)));
    cx.executor().advance_clock(SCHEDULE_TICK);
    wait_for_check(&f, cx);
    assert_eq!(f.releases.fetches(), 2);

    // With automatic checks off, a due tick asks nothing.
    cx.update(|cx| f.app.update(cx, |s, cx| s.set_update_checks(false, cx)));
    cx.update(|cx| f.app.update(cx, |s, _| s.update_attempted = Some(long_ago)));
    cx.executor().advance_clock(SCHEDULE_TICK);
    cx.run_until_parked();
    assert_eq!(f.releases.fetches(), 2);
}

#[gpui_kit::test]
fn a_newer_build_the_license_does_not_cover_offers_renewal(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, Some(&license_key("a-tester", "2026-10-02")));
    f.releases.serve(Ok(manifest(
        3,
        &[("0.1.0", "2026-10-01"), ("9.2.0", "2026-10-03")],
        &update_key(),
    )));
    assert!(matches!(
        manual_check(&f, cx),
        Update::NotCovered { version, .. } if version == "9.2.0"
    ));
    let (main, _) = f.main(cx);
    assert_eq!(
        label(cx, main, "update-card").as_deref(),
        Some("New version: convt 9.2.0 needs a renewed license")
    );
    click(cx, main, "update-renew");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://convt.test/pricing")
    );
    let (settings, _) = f.settings(SettingsTab::General, cx);
    assert!(
        label(cx, settings, "update-status")
            .unwrap()
            .contains("Renew to get it")
    );
    assert!(shown(cx, settings, "update-renew"));
    // Check now stays offered next to Renew.
    assert!(shown(cx, settings, "check-updates"));

    // Up to date: nothing newer than this build.
    f.releases
        .serve(Ok(manifest(4, &[("0.1.0", "2026-10-01")], &update_key())));
    assert_eq!(manual_check(&f, cx), Update::UpToDate);
    assert!(!shown(cx, main, "update-card"));
    assert!(
        label(cx, settings, "update-status")
            .unwrap()
            .starts_with("You're up to date.")
    );
    assert!(!shown(cx, settings, "update-notes"));
}

#[gpui_kit::test]
fn check_now_shows_that_it_is_checking(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, Some("2026-09-30"), None);
    let (settings, view) = f.settings(SettingsTab::General, cx);
    view.update(cx, |v, cx| v.reveal_updates(cx));
    let built = cx.read(|cx| f.app.read(cx).licensing.build_date().to_string());
    assert_eq!(
        label(cx, settings, "update-last-checked"),
        Some(format!("Built {}", super::update::long_date(&built)))
    );
    assert!(
        label(cx, settings, "update-status")
            .unwrap()
            .contains("at launch and every 5 hours")
    );
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.update = Update::Checking;
            cx.notify();
        })
    });
    assert_eq!(
        label(cx, settings, "check-updates").as_deref(),
        Some("Checking…")
    );
    // A click while it checks starts nothing more.
    click(cx, settings, "check-updates");
    cx.run_until_parked();
    assert_eq!(f.releases.fetches(), 0);
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.update = Update::Idle;
            cx.notify();
        })
    });
    assert_eq!(
        label(cx, settings, "check-updates").as_deref(),
        Some("Check now")
    );
}

#[test]
fn update_dates_read_as_words() {
    use super::update::long_date;
    assert_eq!(long_date("2026-10-03"), "Oct 3, 2026");
    assert_eq!(long_date("2026-01-31"), "Jan 31, 2026");
    assert_eq!(long_date("2026-13-01"), "2026-13-01");
    assert_eq!(long_date("unknown"), "unknown");
}

#[gpui_kit::test]
fn bad_manifests_and_failures_are_quiet_and_change_nothing(cx: &mut TestAppContext) {
    use base64::Engine as _;
    let f = Fixture::licensed(cx, None, Some(&license_key("a-tester", "2027-10-01")));
    let newer = [("0.1.0", "2026-10-01"), ("9.2.0", "2026-10-03")];
    // Accept sequence 10 first.
    f.releases.serve(Ok(manifest(10, &newer, &update_key())));
    assert!(matches!(manual_check(&f, cx), Update::Available { .. }));
    let (main, _) = f.main(cx);
    assert!(shown(cx, main, "update-card"));

    type Answer = Result<Vec<u8>, FetchError>;
    let failures: Vec<(&str, Answer, &str)> = vec![
        (
            "wrong key",
            Ok(manifest(11, &newer, &test_key())),
            "didn't check out",
        ),
        (
            "tampered",
            Ok({
                let mut env: convt_update::SignedManifest =
                    serde_json::from_slice(&manifest(12, &newer, &update_key())).unwrap();
                let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
                let mut payload = String::from_utf8(b64.decode(&env.payload).unwrap()).unwrap();
                payload = payload.replace("2026-10-03", "2026-10-04");
                env.payload = b64.encode(payload);
                serde_json::to_vec(&env).unwrap()
            }),
            "didn't check out",
        ),
        ("garbage", Ok(b"<html>".to_vec()), "didn't check out"),
        (
            "rollback",
            Ok(manifest(9, &newer, &update_key())),
            "older than one seen before",
        ),
        ("offline", Err(FetchError::Offline), "couldn't be reached"),
        ("server error", Err(FetchError::Status(503)), "HTTP 503"),
    ];
    for (what, answer, note) in failures {
        f.releases.serve(answer);
        let update = manual_check(&f, cx);
        assert!(
            matches!(&update, Update::Failed(m) if m.contains(note)),
            "{what}: {update:?}"
        );
        // Silent outside Settings: no card, no notification, no window.
        assert!(!shown(cx, main, "update-card"), "{what}");
        cx.read(|cx| assert_eq!(f.app.read(cx).settings.update_sequence, 10, "{what}"));
    }
    assert!(cx.shown_system_notifications().is_empty());
    let (settings, _) = f.settings(SettingsTab::General, cx);
    let status = label(cx, settings, "update-status").unwrap();
    assert!(
        status.starts_with("Couldn't check for updates."),
        "{status}"
    );
    assert!(shown(cx, settings, "check-updates"));
    // A newer sequence is accepted again.
    f.releases.serve(Ok(manifest(11, &newer, &update_key())));
    assert!(matches!(manual_check(&f, cx), Update::Available { .. }));
    cx.read(|cx| assert_eq!(f.app.read(cx).settings.update_sequence, 11));
}

#[gpui_kit::test]
fn update_checks_off_ask_only_when_the_user_does(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, Some("2026-09-30"), None);
    f.releases
        .serve(Ok(manifest(1, &[("9.2.0", "2026-10-03")], &update_key())));
    let (settings, view) = f.settings(SettingsTab::General, cx);
    view.update(cx, |v, cx| v.reveal_updates(cx));
    assert_eq!(
        label(cx, settings, "update-checks").as_deref(),
        Some("Check automatically")
    );
    assert_eq!(toggled(cx, settings, "update-checks"), Some(true));
    click(cx, settings, "update-checks");
    assert_eq!(toggled(cx, settings, "update-checks"), Some(false));
    assert!(f.settings_file().contains("update_checks = false"));
    assert!(
        label(cx, settings, "update-status")
            .unwrap()
            .starts_with("Automatic checks are off.")
    );
    launch_check(&f, cx);
    cx.run_until_parked();
    assert_eq!(f.releases.fetches(), 0);
    // Check now still works: the user asked. A trial covers every build.
    click(cx, settings, "check-updates");
    assert!(matches!(wait_for_check(&f, cx), Update::Available { .. }));
    assert_eq!(f.releases.fetches(), 1);
    // Switching it back on is a click, so it checks right away.
    click(cx, settings, "update-checks");
    wait_for_check(&f, cx);
    assert_eq!(f.releases.fetches(), 2);
    assert!(
        label(cx, settings, "network-updates")
            .unwrap()
            .contains("signed list of releases")
    );
}

#[gpui_kit::test]
fn a_build_without_an_update_key_never_fetches(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    cx.update(|cx| f.app.update(cx, |s, _| s.update_config.key = None));
    launch_check(&f, cx);
    cx.run_until_parked();
    assert_eq!(f.releases.fetches(), 0);
    cx.read(|cx| {
        assert!(matches!(&f.app.read(cx).update, Update::Failed(m) if m.contains("no key")))
    });
}

#[gpui_kit::test]
fn every_update_state_renders_in_both_themes(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, Some(&license_key("a-tester", "2026-10-02")));
    let states = [
        Update::Idle,
        Update::Checking,
        Update::UpToDate,
        Update::Available {
            version: "9.2.0".into(),
            date: "2026-10-03".into(),
            uncovered: Some("9.3.0".into()),
        },
        Update::NotCovered {
            version: "9.2.0".into(),
            date: "2026-10-03".into(),
            purchase_url: "https://convt.test/pricing".into(),
        },
        Update::Failed("convt.app couldn't be reached.".into()),
    ];
    for dark in [false, true] {
        cx.update(|cx| theme::set_dark(dark, cx));
        let (main, _) = f.main(cx);
        let (settings, _) = f.settings(SettingsTab::General, cx);
        for state in &states {
            cx.update(|cx| {
                f.app.update(cx, |s, cx| {
                    s.update = state.clone();
                    cx.notify();
                })
            });
            assert!(shown(cx, settings, "update-status"), "{state:?}");
            let card = matches!(state, Update::Available { .. } | Update::NotCovered { .. });
            assert_eq!(shown(cx, main, "update-card"), card, "{state:?}");
            if card {
                assert!(fits(cx, main, "update-card"), "{state:?}");
            }
        }
    }
}

/// Makes `settings.toml` unsaveable: a directory takes its name.
fn break_settings(dir: &Path) {
    let path = dir.join("settings.toml");
    let _ = std::fs::remove_file(&path);
    std::fs::create_dir_all(path.join("blocker")).unwrap();
}

#[gpui_kit::test]
fn nothing_is_accepted_unless_the_guard_reaches_the_disk(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, Some(&license_key("a-tester", "2027-10-01")));
    let newer = [("0.1.0", "2026-10-01"), ("9.2.0", "2026-10-03")];
    f.releases.serve(Ok(manifest(8, &newer, &update_key())));
    // The sequence can't be saved: the result is not shown or remembered.
    let dir = f.dir.path().to_path_buf();
    *f.releases.during.lock().unwrap() = Some(Box::new(move || break_settings(&dir)));
    let update = manual_check(&f, cx);
    assert!(
        matches!(&update, Update::Failed(m) if m.contains("couldn't be saved")),
        "{update:?}"
    );
    cx.read(|cx| assert_eq!(f.app.read(cx).settings.update_sequence, 0));
    let (main, _) = f.main(cx);
    assert!(!shown(cx, main, "update-card"));
    // A launch check with settings still unsaveable is refused the same way.
    launch_check(&f, cx);
    let update = wait_for_check(&f, cx);
    assert!(
        matches!(&update, Update::Failed(m) if m.contains("couldn't be saved")),
        "{update:?}"
    );
    assert_eq!(f.releases.fetches(), 2);
    cx.read(|cx| {
        let s = &f.app.read(cx).settings;
        assert_eq!(s.update_sequence, 0);
        assert_eq!(s.update_checked_at, None);
    });
}

#[gpui_kit::test]
fn a_new_license_reselects_the_update_without_another_request(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, Some(&license_key("a-tester", "2026-10-02")));
    f.releases.serve(Ok(manifest(
        3,
        &[("0.1.0", "2026-10-01"), ("9.2.0", "2026-10-03")],
        &update_key(),
    )));
    assert!(matches!(manual_check(&f, cx), Update::NotCovered { .. }));
    // Activating (or renewing to) a key that covers it turns Renew into Download.
    cx.update(|cx| {
        f.app
            .update(cx, |s, cx| {
                s.activate(&license_key("a-tester", "2027-10-01"), cx)
            })
            .unwrap();
    });
    cx.read(|cx| {
        assert!(matches!(&f.app.read(cx).update, Update::Available { version, .. } if version == "9.2.0"))
    });
    // And a renewal through convt.app does the same.
    let f2 = Fixture::signed_in(cx, Some(&pro_key("pro-tester", "2026-10-02")), "pro-tester");
    f2.releases.serve(Ok(manifest(
        3,
        &[("0.1.0", "2026-10-01"), ("9.2.0", "2026-10-03")],
        &update_key(),
    )));
    assert!(matches!(manual_check(&f2, cx), Update::NotCovered { .. }));
    f2.api
        .answer_key(Ok(Some(pro_key("pro-tester", "2026-11-01"))));
    cx.update(|cx| f2.app.update(cx, |s, cx| s.refresh_license(cx)));
    wait_until(cx, "renewed", |cx| {
        matches!(
            f2.app.read(cx).account.refresh,
            crate::account::Refresh::Done(_)
        )
    });
    cx.read(|cx| assert!(matches!(&f2.app.read(cx).update, Update::Available { .. })));
    assert_eq!(f2.releases.fetches(), 1);
}

#[gpui_kit::test]
fn an_uncovered_running_build_is_not_promised_to_keep_working(cx: &mut TestAppContext) {
    // The license ended before this build (2026-10-01) too.
    let f = Fixture::licensed(cx, None, Some(&license_key("a-tester", "2026-09-15")));
    f.releases.serve(Ok(manifest(
        2,
        &[("0.1.0", "2026-10-01"), ("9.2.0", "2026-10-03")],
        &update_key(),
    )));
    assert!(matches!(manual_check(&f, cx), Update::NotCovered { .. }));
    let (settings, _) = f.settings(SettingsTab::General, cx);
    let status = label(cx, settings, "update-status").unwrap();
    assert!(
        !status.contains("keeps working") && status.contains("renew to convert again"),
        "{status}"
    );
}

/// The menu bar as macOS shows it: each menu with its item names, "-" for
/// a separator.
fn menu_bar(cx: &mut TestAppContext) -> Vec<(String, Vec<String>)> {
    use gpui_kit::OwnedMenuItem;
    cx.update(|cx| {
        cx.set_menus(menus::menus());
        cx.get_menus().unwrap()
    })
    .into_iter()
    .map(|menu| {
        let items = menu
            .items
            .iter()
            .map(|item| match item {
                OwnedMenuItem::Separator => "-".to_string(),
                OwnedMenuItem::Action { name, .. } => name.clone(),
                OwnedMenuItem::Submenu(m) => m.name.to_string(),
                OwnedMenuItem::SystemMenu(m) => m.name.to_string(),
            })
            .collect();
        (menu.name.to_string(), items)
    })
    .collect()
}

#[gpui_kit::test]
fn the_menu_bar_has_the_menus_mac_apps_have(cx: &mut TestAppContext) {
    let bar = menu_bar(cx);
    let names: Vec<&str> = bar.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["convt", "File", "Edit", "Window", "Help"]);
    let items = |menu: &str| -> Vec<String> {
        bar.iter()
            .find(|(n, _)| n == menu)
            .map(|(_, items)| items.clone())
            .unwrap()
    };
    assert_eq!(
        items("convt"),
        [
            "About convt",
            "-",
            "Check for Updates…",
            "Settings…",
            "-",
            "Services",
            "-",
            "Hide convt",
            "Hide Others",
            "Show All",
            "-",
            "Quit convt",
        ]
    );
    assert_eq!(items("File"), ["Add Files…", "-", "Close Window"]);
    assert_eq!(
        items("Edit"),
        ["Undo", "Redo", "-", "Cut", "Copy", "Paste", "Select All"]
    );
    assert_eq!(items("Window"), ["Minimize", "Zoom", "-", "Activity"]);
    assert_eq!(
        items("Help"),
        ["convt Help", "Release Notes", "-", "Contact Support"]
    );
}

#[gpui_kit::test]
fn the_menu_shortcuts_are_the_usual_ones(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, Some("2026-09-30"), None);
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.update_settings(|s| s.first_run_done = true, cx)
        });
        menus::init(cx);
        cx.bind_keys(menus::mac_key_bindings());
    });
    let (main, _) = f.main(cx);
    cx.simulate_keystrokes(main, "cmd-,");
    cx.run_until_parked();
    let (settings, view) = window_of::<SettingsView>(cx);
    cx.read(|cx| assert_eq!(view.read(cx).tab, SettingsTab::General));
    let windows = cx.update(|cx| cx.windows().len());
    // Close Window closes the key window, as macOS makes the one typed in.
    settings
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    cx.simulate_keystrokes(settings, "secondary-w");
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), windows - 1);
}

#[gpui_kit::test]
fn quit_and_close_window_shortcuts_work_on_every_platform(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    let f = Fixture::licensed(cx, Some("2026-09-30"), None);
    let quits = std::rc::Rc::new(std::cell::Cell::new(0));
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.update_settings(|s| s.first_run_done = true, cx)
        });
        menus::init(cx);
        // Registered after the app's handler, so it hears Quit first; it
        // counts it and passes it on.
        let quits = quits.clone();
        cx.on_action(move |_: &menus::Quit, cx| {
            quits.set(quits.get() + 1);
            cx.propagate();
        });
    });
    // ⌘Q and ⌘W on macOS, Ctrl+Q and Ctrl+W on Linux and Windows. On macOS
    // the menu bar also shows each item's shortcut from these bindings.
    let (quit, close) = if cfg!(target_os = "macos") {
        ("cmd-q", "cmd-w")
    } else {
        ("ctrl-q", "ctrl-w")
    };
    for (keys, action) in [
        (quit, &menus::Quit as &dyn gpui_kit::Action),
        (close, &menus::CloseWindow),
    ] {
        let keystroke = gpui_kit::Keystroke::parse(keys).unwrap();
        let bindings = cx.update(|cx| cx.all_bindings_for_input(&[keystroke]));
        assert_eq!(bindings.len(), 1, "{keys}");
        assert!(bindings[0].action().partial_eq(action), "{keys}");
    }
    // With the menu bar icon on, the app runs with no window; Quit is still
    // enabled in the menu and still quits.
    assert!(cx.update(|cx| cx.windows()).is_empty());
    assert!(cx.update(|cx| cx.is_action_available(&menus::Quit)));
    cx.update(|cx| cx.dispatch_action(&menus::Quit));
    cx.run_until_parked();
    assert_eq!(quits.get(), 1);

    let (main, _) = f.main(cx);
    let (settings, view) = f.settings(SettingsTab::License, cx);
    let field = cx.read(|cx| view.read(cx).license_key.clone());
    let activate = |window: AnyWindowHandle, cx: &mut TestAppContext| {
        window
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
    };

    activate(main, cx);
    cx.simulate_keystrokes(main, quit);
    cx.run_until_parked();
    assert_eq!(quits.get(), 2);

    // A focused text field doesn't keep either shortcut for itself.
    activate(settings, cx);
    cx.update_window(settings, |_, window, cx| {
        window.focus(&field.read(cx).focus_handle(cx), cx)
    })
    .unwrap();
    cx.simulate_keystrokes(settings, quit);
    cx.run_until_parked();
    assert_eq!(quits.get(), 3);
    let windows = cx.update(|cx| cx.windows().len());
    cx.simulate_keystrokes(settings, close);
    cx.run_until_parked();
    let open = cx.update(|cx| cx.windows());
    assert_eq!(open.len(), windows - 1);
    assert!(!open.contains(&settings), "Settings closed");
    assert!(open.contains(&main), "only the window typed in closes");

    activate(main, cx);
    cx.simulate_keystrokes(main, close);
    cx.run_until_parked();
    assert!(!cx.update(|cx| cx.windows()).contains(&main));
}

#[gpui_kit::test]
fn check_for_updates_checks_now_and_shows_the_result(cx: &mut TestAppContext) {
    // Automatic checks are off: the menu item checks anyway, as Check now does.
    let f = Fixture::licensed(cx, Some("2026-09-30"), None);
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.update_settings(|s| s.update_checks = false, cx)
        });
        menus::init(cx);
    });
    f.releases
        .serve(Ok(manifest(2, &[("9.4.0", "2026-10-03")], &update_key())));
    let windows = cx.update(|cx| cx.windows().len());
    cx.update(|cx| cx.dispatch_action(&menus::CheckForUpdates));
    let (settings, view) = window_of::<SettingsView>(cx);
    cx.read(|cx| assert_eq!(view.read(cx).tab, SettingsTab::General));
    assert!(
        matches!(wait_for_check(&f, cx), Update::Available { version, .. } if version == "9.4.0")
    );
    assert_eq!(f.releases.fetches(), 1);
    // Settings opens scrolled to the Updates card, so Download is in view.
    assert!(
        label(cx, settings, "update-status")
            .unwrap()
            .starts_with("convt 9.4.0 is available")
    );
    assert!(fits(cx, settings, "update-download"));
    assert!(fits(cx, settings, "update-checks"));

    // Up to date, and an error, show in the same place.
    f.releases
        .serve(Ok(manifest(3, &[("0.1.0", "2026-10-01")], &update_key())));
    cx.update(|cx| cx.dispatch_action(&menus::CheckForUpdates));
    assert_eq!(wait_for_check(&f, cx), Update::UpToDate);
    assert!(
        label(cx, settings, "update-status")
            .unwrap()
            .starts_with("You're up to date.")
    );
    f.releases.serve(Err(FetchError::Offline));
    cx.update(|cx| cx.dispatch_action(&menus::CheckForUpdates));
    assert!(matches!(wait_for_check(&f, cx), Update::Failed(_)));
    assert_eq!(
        label(cx, settings, "update-status").as_deref(),
        Some("Couldn't check for updates. convt.app couldn't be reached.")
    );
    assert_eq!(f.releases.fetches(), 3);
    // Still only one Settings window.
    assert_eq!(cx.update(|cx| cx.windows().len()), windows + 1);
}

#[gpui_kit::test]
fn about_and_help_open_what_they_say(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, Some("2026-09-30"), None);
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.update_settings(|s| s.first_run_done = true, cx)
        });
        menus::init(cx);
    });
    cx.update(|cx| cx.dispatch_action(&menus::About));
    let (about, _) = window_of::<AboutView>(cx);
    let version = label(cx, about, "about-version").unwrap();
    assert!(
        version.starts_with(&format!("Version {} · built ", crate::account::VERSION)),
        "{version}"
    );
    click(cx, about, "about-source");
    assert_eq!(cx.opened_url().as_deref(), Some(menus::SOURCE_URL));
    click(cx, about, "about-notes");
    assert_eq!(
        cx.opened_url(),
        Some(format!(
            "https://convt.app/changelog#v{}",
            crate::account::VERSION
        ))
    );
    // A second About brings the same window forward.
    let windows = cx.update(|cx| cx.windows().len());
    cx.update(|cx| cx.dispatch_action(&menus::About));
    assert_eq!(cx.update(|cx| cx.windows().len()), windows);

    for (action, url) in [
        (
            Box::new(menus::OpenHelp) as Box<dyn gpui_kit::Action>,
            "https://convt.app/docs",
        ),
        (
            Box::new(menus::OpenReleaseNotes),
            "https://convt.app/changelog",
        ),
        (Box::new(menus::ContactSupport), "https://convt.app/contact"),
    ] {
        cx.update(|cx| cx.dispatch_action(&*action));
        assert_eq!(cx.opened_url().as_deref(), Some(url));
    }

    cx.update(|cx| cx.dispatch_action(&menus::ShowActivity));
    let (_, main) = window_of::<MainView>(cx);
    cx.read(|cx| assert_eq!(main.read(cx).page, Page::Activity));
    cx.update(|cx| cx.dispatch_action(&menus::OpenSettings));
    window_of::<SettingsView>(cx);
}

#[gpui_kit::test]
fn the_edit_menu_reaches_the_text_fields(cx: &mut TestAppContext) {
    use gpui_kit::{Focusable as _, OwnedMenuItem};
    let f = Fixture::licensed(cx, Some("2026-09-30"), None);
    let (settings, view) = f.settings(SettingsTab::License, cx);
    let field = cx.read(|cx| view.read(cx).license_key.clone());
    set_input(cx, settings, &field, "CONVT-1234");
    cx.update_window(settings, |_, window, cx| {
        window.focus(&field.read(cx).focus_handle(cx), cx)
    })
    .unwrap();
    let edit = cx.update(|cx| {
        cx.set_menus(menus::menus());
        cx.get_menus().unwrap()
    });
    let edit = edit.into_iter().find(|m| m.name == "Edit").unwrap();
    let action = |name: &str| {
        edit.items
            .iter()
            .find_map(|item| match item {
                OwnedMenuItem::Action {
                    name: n, action, ..
                } if n == name => Some(action.boxed_clone()),
                _ => None,
            })
            .unwrap()
    };
    for name in ["Select All", "Copy"] {
        let action = action(name);
        cx.update_window(settings, |_, window, cx| window.dispatch_action(action, cx))
            .unwrap();
    }
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("CONVT-1234")
    );
    let cut = action("Cut");
    cx.update_window(settings, |_, window, cx| window.dispatch_action(cut, cx))
        .unwrap();
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(field.read(cx).value().as_ref(), ""));
    let undo = action("Undo");
    cx.update_window(settings, |_, window, cx| window.dispatch_action(undo, cx))
        .unwrap();
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(field.read(cx).value().as_ref(), "CONVT-1234"));
}

/// The signed-in address and a key's address show masked everywhere, as
/// convt.app masks them: the first letter and the domain.
#[gpui_kit::test]
fn addresses_show_masked(cx: &mut TestAppContext) {
    let address = ["pro.tester", "example.com"].join("@");
    let key = pro_key(&address, "2026-11-01");
    let f = Fixture::signed_in(cx, Some(&key), &address);
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.account.access = Some(Access::Trial {
                ends_on: "2026-10-15".into(),
            });
            cx.notify();
        })
    });
    let app = f.app.clone();
    let (window, _) = open(cx, move |window, cx| {
        cx.new(|cx| FirstRunView::new(app, Screen::Account, window, cx))
    });
    assert_eq!(
        label(cx, window, "account-status").as_deref(),
        Some("Signed in as p***@example.com")
    );
    let (settings, _) = f.settings(SettingsTab::License, cx);
    assert_eq!(
        label(cx, settings, "account-status").as_deref(),
        Some("Signed in to convt.app as p***@example.com.")
    );
    for (window, id) in [(window, "account-status"), (settings, "account-status")] {
        assert!(!label(cx, window, id).unwrap().contains(&address));
    }
}
