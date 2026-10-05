//! Headless window tests. They open real windows on GPUI's test platform,
//! click the same elements a user would and run real conversions with the
//! default registry, so they need no display.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use convt_core::{Ctx, Engine, Options, Preset, Registry, VideoCodec, format_by_id};
use convt_license::client::{self, KeyStore};
use convt_license::{License, Plan};
use ed25519_dalek::SigningKey;
use gpui_kit::component::input::InputState;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, ClipboardEntry, ElementId, Entity, ImageFormat,
    Render, SharedString, TestAppContext, Window, WindowBounds, WindowOptions, point, px, size,
};
use tempfile::TempDir;

use super::first_run::{FirstRunView, Step};
use super::main_window::{MainView, Page};
use super::quick::QuickView;
use super::settings_window::{SettingsTab, SettingsView};
use super::{Open, PopoverView, theme};
use crate::history::Outcome;
use crate::jobs::JobId;
use crate::model::{AppState, PackPhase, Paths, Shared};
use crate::pack::{self, Failure, FailureKind};
use crate::request::{Request, Source};
use crate::tray::{self, Indicator};

struct Fixture {
    dir: TempDir,
    app: Entity<AppState>,
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
            destination: Some(
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                    .join(".local/share/convt/packs/documents"),
            ),
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
        let paths = Paths {
            settings: Some(dir.path().join("settings.toml")),
            history: Some(dir.path().join("history.db")),
            presets: Some(dir.path().join("presets")),
            license: license(dir.path()),
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
        Self { dir, app }
    }

    /// A 4x4 PNG in the fixture directory.
    fn png(&self, name: &str) -> PathBuf {
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
                size: size(px(1040.), px(760.)),
            })),
            ..Default::default()
        };
        let (handle, view) = gpui_kit::open_window(options, cx, build).expect("open a test window");
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
    for done in ["show-in-folder", "open", "close"] {
        assert!(shown(cx, window, done), "{done} is missing");
    }
    assert!(!shown(cx, window, "convert"), "the picker should be gone");
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
fn add_files_converts_right_away_and_lists_the_results(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let a = f.png("a.png");
    let b = f.png("b.png");
    let (window, view) = f.main(cx);
    assert!(shown(cx, window, "empty"));
    assert_eq!(
        label(cx, window, "defaults").as_deref(),
        Some("Images → WebP, Video → MP4, Audio → MP3, Documents → PDF")
    );

    // The same file twice converts once.
    view.update(cx, |v, cx| v.add(&[a.clone(), b, a], cx));
    assert_eq!(f.jobs(cx), 2);
    wait_until(cx, "both files", |cx| f.app.read(cx).recent.len() == 2);
    assert!(is_webp(&f.dir.path().join("a.webp")) && is_webp(&f.dir.path().join("b.webp")));
    let records = cx.read(|cx| f.app.read(cx).recent.clone());
    for record in &records {
        let status = label(cx, window, &format!("record-status-{}", record.id)).unwrap();
        assert!(status.starts_with("Done · "), "{status}");
        assert!(shown(cx, window, &format!("show-{}", record.id)));
    }
    assert!(!shown(cx, window, "empty"));

    // Changing a default changes what Add files does.
    click(cx, window, "change-defaults");
    click(cx, window, "default-images-jpeg");
    cx.read(|cx| assert_eq!(f.app.read(cx).settings.defaults.images, "jpeg"));
    assert!(f.settings_file().contains("images = \"jpeg\""));
    let c = f.png("c.png");
    view.update(cx, |v, cx| v.add(&[c], cx));
    wait_until(cx, "c.jpg", |cx| f.app.read(cx).recent.len() == 3);
    assert!(is_jpeg(&f.dir.path().join("c.jpg")));

    // A file with no usable default (a JPEG with JPEG as the default) asks.
    let windows = cx.update(|cx| cx.windows().len());
    let jpg = f.dir.path().join("c.jpg");
    view.update(cx, |v, cx| v.add(&[jpg], cx));
    assert_eq!(cx.update(|cx| cx.windows().len()), windows + 1);

    click(cx, window, "clear-finished");
    cx.read(|cx| {
        let state = f.app.read(cx);
        assert!(state.recent.is_empty() && state.queue.entries.is_empty());
    });
    assert!(shown(cx, window, "empty"));
}

#[gpui_kit::test]
fn a_failed_conversion_can_be_retried(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let broken = f.dir.path().join("broken.png");
    std::fs::write(&broken, "not a png").unwrap();
    let (window, view) = f.main(cx);
    view.update(cx, |v, cx| v.add(std::slice::from_ref(&broken), cx));
    wait_until(cx, "the failure", |cx| f.app.read(cx).recent.len() == 1);
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
    click(cx, window, "notifications");
    click(cx, window, "reveal");
    click(cx, window, "menu-bar-icon");
    let saved = f.settings_file();
    for line in [
        "notifications = false",
        "reveal_when_done = true",
        "menu_bar_icon = false",
    ] {
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
    cx.update(|cx| app.update(cx, |s, cx| s.clear_finished(cx)));
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
            email: "a@example.com".into(),
            plan: Plan::Desktop,
            issued: "2026-09-01".into(),
            updates_until: "2027-10-01".into(),
        },
        &other,
    );
    type_key(cx, settings, &view, &forged);
    click(cx, settings, "activate");
    assert!(shown(cx, settings, "error"), "a key from another signer");

    let key = license_key("a@example.com", "2027-10-01");
    type_key(cx, settings, &view, &format!("  {key}\n"));
    click(cx, settings, "activate");
    assert!(!shown(cx, settings, "error"));
    assert_eq!(
        label(cx, settings, "license-notice").as_deref(),
        Some("License activated for a@example.com.")
    );
    assert_eq!(
        label(cx, settings, "license-status").as_deref(),
        Some("Licensed to a@example.com (Desktop), with updates until 2027-10-01.")
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
    let key = license_key("a@example.com", "2026-06-30");
    let f = Fixture::licensed(cx, Some("2026-01-01"), Some(&key));
    let (main, view) = f.main(cx);
    let card = label(cx, main, "trial-card").unwrap();
    assert!(
        card.starts_with("This build is newer than your license covers.")
            && card.contains("2026-06-30"),
        "{card}"
    );

    let png = f.png("a.png");
    view.update(cx, |v, cx| v.add(std::slice::from_ref(&png), cx));
    assert_eq!(f.jobs(cx), 0);
    assert!(
        label(cx, main, "error")
            .unwrap()
            .starts_with("This build is newer")
    );

    let (quick, _) = f.quick(cli(vec![png], Some("jpeg"), None), cx);
    assert!(shown(cx, quick, "download"));
    let (settings, _) = f.settings(SettingsTab::License, cx);
    assert!(shown(cx, settings, "remove-license"));
}

#[gpui_kit::test]
fn an_activate_link_fills_in_the_key_without_activating(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    let key = license_key("a@example.com", "2027-10-01");
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

#[gpui_kit::test]
fn first_run_shows_once_in_licensed_builds(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    cx.update(|cx| super::route(Request::default(), cx));
    let (window, view) = window_of::<FirstRunView>(cx);
    cx.read(|cx| assert!(f.app.read(cx).settings.first_run_done));
    assert!(f.settings_file().contains("first_run_done = true"));
    let start = super::first_run::first_step();
    // The first screen is step 1, whether or not this platform has the
    // Finder step.
    let total = if start == Step::Finder { 3 } else { 2 };
    assert_eq!(
        label(cx, window, "step"),
        Some(format!("STEP 1 OF {total}"))
    );
    if start == Step::Finder {
        click(cx, window, "first-run-back");
    }

    // "I have a license" activates the key and moves on.
    click(cx, window, "plan-key");
    let input = cx.read(|cx| view.read(cx).key.clone());
    set_input(cx, window, &input, "nonsense");
    click(cx, window, "first-run-next");
    assert!(shown(cx, window, "error"));
    let key = license_key("a@example.com", "2027-10-01");
    set_input(cx, window, &input, &key);
    click(cx, window, "first-run-next");
    cx.read(|cx| assert_eq!(view.read(cx).step, Step::Done, "{:?}", view.read(cx).error));
    assert!(f.dir.path().join("license.key").exists());

    // "Start converting" closes it and opens the main window.
    click(cx, window, "first-run-next");
    window_of::<MainView>(cx);

    // A second launch goes straight to the main window.
    let before = cx.update(|cx| cx.windows().len());
    cx.update(|cx| super::route(Request::default(), cx));
    assert_eq!(cx.update(|cx| cx.windows().len()), before);
}

#[gpui_kit::test]
fn a_build_from_source_never_shows_first_run(cx: &mut TestAppContext) {
    let _f = Fixture::new(cx);
    cx.update(|cx| super::route(Request::default(), cx));
    window_of::<MainView>(cx);
    cx.read(|cx| assert!(Open::<FirstRunView>::get(cx).is_none()));
}

#[gpui_kit::test]
fn signing_in_opens_the_browser_and_a_link_brings_the_account_back(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    let app = f.app.clone();
    let (window, view) = open(cx, move |window, cx| {
        cx.new(|cx| FirstRunView::new(app, Step::Plan, window, cx))
    });
    cx.update(|cx| cx.set_global(Open(window, view.downgrade())));
    click(cx, window, "sign-in-google");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://convt.app/signin?client=desktop&method=google")
    );
    cx.read(|cx| assert_eq!(view.read(cx).step, Step::Done));
    click(cx, window, "first-run-back");

    // The site hands the session back with a link; it starts nothing.
    let link = crate::request::parse_url("convt://signin?email=a%40example.com").unwrap();
    cx.update(|cx| super::route(link, cx));
    cx.read(|cx| {
        assert_eq!(
            f.app.read(cx).settings.account.as_deref(),
            Some("a@example.com")
        );
        assert_eq!(view.read(cx).step, Step::Done);
    });
    assert!(f.settings_file().contains("account = \"a@example.com\""));
    assert_eq!(f.jobs(cx), 0);
    click(cx, window, "first-run-back");
    assert_eq!(
        label(cx, window, "account").as_deref(),
        Some("Signed in as a@example.com")
    );
}

#[gpui_kit::test]
fn the_popover_converts_a_dropped_file_and_copies_it(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let png = f.png("Screenshot.png");
    let (window, view) = cx.update(super::open_popover).unwrap();
    assert!(shown(cx, window, "drop-bar"));
    view.update(cx, |v, cx| v.drop_files(&[png], cx));
    let job = f.last_job(cx);
    wait_for_label(cx, window, &format!("copied-{job}"), |s| {
        s == "Copied to your clipboard"
    });
    let item = cx.read_from_clipboard().expect("something was copied");
    let output = f.dir.path().join("Screenshot.webp");
    assert!(is_webp(&output));
    assert!(
        item.entries().iter().any(|e| matches!(
            e,
            ClipboardEntry::Image(image)
                if image.format == ImageFormat::Webp
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
        Some("Off")
    );
    click(cx, popover, "popover-automation-2");
    assert_eq!(
        label(cx, popover, "popover-automation-2").as_deref(),
        Some("On")
    );
    cx.read(|cx| assert!(f.app.read(cx).settings.automations[2].enabled));
    let reloaded = crate::settings::Settings::load(&f.dir.path().join("settings.toml")).unwrap();
    assert!(reloaded.automations[2].enabled);

    // The main window shows the same rules.
    click(cx, popover, "new-rule");
    let (main, view) = window_of::<MainView>(cx);
    cx.read(|cx| assert_eq!(view.read(cx).page, Page::Automations));
    assert_eq!(label(cx, main, "automation-2").as_deref(), Some("On"));
    click(cx, main, "automation-0");
    cx.read(|cx| assert!(!f.app.read(cx).settings.automations[0].enabled));
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
            cx.new(|cx| FirstRunView::new(app, Step::Done, window, cx))
        });
        assert!(shown(cx, first, "first-run-next"));
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
    let files: Vec<PathBuf> = (0..5).map(|i| f.png(&format!("shot {i}.png"))).collect();
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
        cx.update(|cx| super::open_quick(cli(pngs.clone(), None, None), cx));
        let quick = *cx.update(|cx| cx.windows()).last().unwrap();
        click(cx, quick, "to-jpeg");
        assert!(fits(cx, quick, "convert"), "Quick convert, two files");

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

        let app = f.app.clone();
        cx.update(|cx| {
            super::show(size(px(420.), px(420.)), "first run", cx, |window, cx| {
                cx.new(|cx| FirstRunView::new(app, Step::Plan, window, cx))
            })
        });
        let (first, view) = window_of::<FirstRunView>(cx);
        assert!(fits(cx, first, "first-run-next"), "first run, trial");
        click(cx, first, "plan-key");
        click(cx, first, "first-run-next");
        cx.read(|cx| assert!(view.read(cx).error.is_some()));
        assert!(
            fits(cx, first, "first-run-next"),
            "first run, key and error"
        );
        first
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();

        let (popover, _) = cx.update(super::open_popover).unwrap();
        assert!(fits(cx, popover, "open-settings"), "popover");

        cx.update(super::show_main);
        let (main, _) = window_of::<MainView>(cx);
        assert!(fits(cx, main, "trial-buy"), "main window");
    }
}

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
    use gpui_kit::AssetSource;
    use gpui_kit::component::{IconName, IconNamed};
    let assets = super::assets();
    for icon in [IconName::Check, IconName::ChevronDown, IconName::ArrowDown] {
        let path = icon.path();
        assert!(
            assets.load(&path).unwrap().is_some(),
            "{path} is missing, so it would draw empty"
        );
    }
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
    assert_eq!(outputs[0].parent(), Some(out.as_path()));
    let (w, h) = image::image_dimensions(&outputs[0]).unwrap();
    assert_eq!((w, h), (2, 2), "the size option was dropped");
}

#[gpui_kit::test]
fn a_key_that_does_not_cover_this_build_is_saved_not_celebrated(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, Some("2026-01-01"), None);
    let (window, view) = f.settings(SettingsTab::License, cx);
    let input = cx.read(|cx| view.read(cx).license_key.clone());
    set_input(
        cx,
        window,
        &input,
        &license_key("old@example.com", "2025-01-01"),
    );
    click(cx, window, "activate");
    assert!(f.dir.path().join("license.key").exists());
    assert_eq!(
        label(cx, window, "license-notice").as_deref(),
        Some("Saved the license for old@example.com.")
    );
    assert_eq!(label(cx, window, "settings-buy").as_deref(), Some("Renew"));

    set_input(
        cx,
        window,
        &input,
        &license_key("new@example.com", "2027-10-01"),
    );
    click(cx, window, "activate");
    assert_eq!(
        label(cx, window, "license-notice").as_deref(),
        Some("License activated for new@example.com.")
    );
    assert!(!shown(cx, window, "settings-buy"));
}

#[gpui_kit::test]
fn first_run_does_not_call_a_key_that_misses_this_build_ready(cx: &mut TestAppContext) {
    let f = Fixture::licensed(cx, None, None);
    cx.update(|cx| super::route(Request::default(), cx));
    let (window, view) = window_of::<FirstRunView>(cx);
    if super::first_run::first_step() == Step::Finder {
        click(cx, window, "first-run-back");
    }
    click(cx, window, "plan-key");
    let input = cx.read(|cx| view.read(cx).key.clone());
    set_input(
        cx,
        window,
        &input,
        &license_key("old@example.com", "2025-01-01"),
    );
    click(cx, window, "first-run-next");
    assert!(f.dir.path().join("license.key").exists(), "the key is kept");
    assert_eq!(
        label(cx, window, "first-run-title").as_deref(),
        Some("License saved")
    );
    assert_eq!(
        label(cx, window, "first-run-next").as_deref(),
        Some("Open convt")
    );
    assert_eq!(
        label(cx, window, "first-run-renew").as_deref(),
        Some("Renew")
    );
    click(cx, window, "first-run-renew");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some(convt_license::client::BUY_URL)
    );
    for name in ["first-run-next", "first-run-renew"] {
        assert!(fits(cx, window, name), "{name}");
    }

    // A key that covers this build is ready.
    click(cx, window, "first-run-back");
    set_input(
        cx,
        window,
        &input,
        &license_key("new@example.com", "2027-10-01"),
    );
    click(cx, window, "first-run-next");
    assert_eq!(
        label(cx, window, "first-run-title").as_deref(),
        Some("You're set")
    );
    assert!(!shown(cx, window, "first-run-renew"));
}

#[gpui_kit::test]
fn output_folders_are_stored_as_absolute_paths(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    let input = f.png("a.png");
    let out = f.dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    // The same folder, relative to the working directory.
    let cwd = std::env::current_dir().unwrap();
    let mut relative = PathBuf::new();
    for _ in cwd.components().skip(1) {
        relative.push("..");
    }
    relative.push(out.strip_prefix("/").unwrap());
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
    assert_eq!(packs.installs(), 1);
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
    let folder = f.dir.path().join("folder");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("Budget.xlsx"), b"PK").unwrap();

    // Add files: the PNG converts, the document opens the offer.
    let (_, main) = f.main(cx);
    main.update(cx, |v, cx| v.add(&[png.clone(), docx.clone()], cx));
    let (window, view) = last_quick(cx);
    cx.read(|cx| assert_eq!(view.read(cx).files, std::slice::from_ref(&docx)));
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
    // The definition, and the one call in the button's click handler.
    assert_eq!(uses("download_pack("), ["model.rs", "ui/pack.rs"]);
    let button = include_str!("pack.rs")
        .split("fn download_button")
        .nth(1)
        .unwrap();
    let button = &button[..button.find("\n}\n").unwrap()];
    assert!(button.contains(".on_click(") && button.contains("download_pack(cx)"));
    // `backend.install(` runs inside download_pack's worker thread only.
    assert_eq!(uses("backend.install("), ["model.rs"]);
    let model = include_str!("../model.rs");
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
    assert_eq!(
        label(cx, window, "pack-destination").as_deref(),
        Some("~/.local/share/convt/packs/documents")
    );
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
