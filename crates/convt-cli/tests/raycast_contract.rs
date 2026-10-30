//! The CLI output the Raycast extension parses (integrations/raycast, published
//! as https://www.raycast.com/0xdhrv/convt). If one of these fails, the
//! extension breaks for everyone who updates convt; change the extension in the
//! same pull request, or keep the old shape.
//!
//! Only PNG, JPEG and WebP are converted: the `image` engine is always there.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// A small PNG with a transparent half, so `--background` has work to do.
    fn png(&self, name: &str) -> PathBuf {
        let path = self.path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbaImage::from_fn(8, 8, |x, _| {
            image::Rgba([200, 40, 40, if x < 4 { 255 } else { 0 }])
        })
        .save(&path)
        .unwrap();
        path
    }

    fn run<S: AsRef<std::ffi::OsStr>>(&self, args: &[S]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_convt"))
            .args(args)
            .env("CONVT_CONFIG_DIR", self.path("config"))
            .env("CONVT_DATA_DIR", self.path("data"))
            .env("CONVT_LICENSE_STORE", "file")
            .env("CONVT_LICENSE_ENFORCE", "0")
            .output()
            .unwrap()
    }

    fn ok<S: AsRef<std::ffi::OsStr>>(&self, args: &[S]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}

/// How the extension reads `convt targets`: whitespace-separated ids.
fn ids(stdout: &str) -> Vec<String> {
    stdout.split_whitespace().map(str::to_owned).collect()
}

/// The extension's `parseFormats` only accepts ids matching `^[a-z0-9]+$`.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

fn known_ids(fx: &Fixture) -> Vec<String> {
    let list: Vec<Value> = serde_json::from_str(&fx.ok(&["formats", "--json"])).unwrap();
    list.iter()
        .map(|f| f["id"].as_str().unwrap().to_owned())
        .collect()
}

fn events(stdout: &[u8]) -> Vec<Value> {
    String::from_utf8(stdout.to_vec())
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON: {l}: {e}")))
        .collect()
}

fn named<'a>(events: &'a [Value], name: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["event"] == name).collect()
}

/// Every event the extension's `parseEvent` accepts, checked the way it checks
/// them. An event it doesn't know makes it report "Update Convt".
fn check_event(event: &Value) {
    let name = event["event"].as_str().expect("event name");
    if name == "summary" {
        for key in ["done", "failed", "cancelled"] {
            assert!(event[key].is_u64(), "summary.{key}: {event}");
        }
        return;
    }
    assert!(event["index"].is_u64(), "index: {event}");
    match name {
        "started" => assert!(event["input"].is_string(), "{event}"),
        "progress" => {
            let f = &event["fraction"];
            assert!(
                f.is_null() || f.as_f64().is_some_and(|f| (0.0..=1.0).contains(&f)),
                "fraction: {event}"
            );
        }
        "done" => {
            assert!(event["input"].is_string(), "{event}");
            let outputs = event["outputs"].as_array().expect("outputs");
            assert!(!outputs.is_empty(), "{event}");
            for output in outputs {
                let output = Path::new(output.as_str().expect("output path"));
                assert!(output.is_absolute(), "{event}");
                assert!(output.is_file(), "{event}");
            }
        }
        "failed" => {
            for key in ["input", "kind", "message"] {
                assert!(event[key].is_string(), "failed.{key}: {event}");
            }
        }
        other => panic!("the extension does not know the event {other:?}: {event}"),
    }
}

#[test]
fn formats_json_has_the_fields_the_extension_reads() {
    let fx = Fixture::new();
    let list: Vec<Value> = serde_json::from_str(&fx.ok(&["formats", "--json"])).unwrap();
    assert!(!list.is_empty());
    for format in &list {
        let id = format["id"].as_str().expect("id");
        assert!(valid_id(id), "id {id:?}");
        assert!(format["name"].is_string(), "{format}");
        let category = format["category"].as_str().expect("category");
        assert!(
            [
                "image",
                "vector",
                "video",
                "audio",
                "pdf",
                "document",
                "spreadsheet",
                "presentation"
            ]
            .contains(&category),
            "category {category:?}"
        );
        let extensions = format["extensions"].as_array().expect("extensions");
        assert!(!extensions.is_empty(), "{format}");
        for ext in extensions {
            let ext = ext.as_str().expect("extension");
            assert!(!ext.starts_with('.') && ext == ext.to_lowercase(), "{ext}");
        }
    }
    // The extension identifies sources by extension and needs these to be found.
    for (id, ext) in [("png", "png"), ("jpeg", "jpg"), ("webp", "webp")] {
        assert!(
            list.iter()
                .any(|f| f["id"] == id
                    && f["extensions"].as_array().unwrap().iter().any(|e| e == ext)),
            "{id} with .{ext}"
        );
    }
}

#[test]
fn targets_and_menu_targets_print_known_ids() {
    let fx = Fixture::new();
    let png = fx.png("photo.png");
    let known = known_ids(&fx);

    let all = ids(&fx.ok(&["targets".as_ref(), png.as_os_str()]));
    let menu = ids(&fx.ok(&["targets".as_ref(), png.as_os_str(), "--menu".as_ref()]));
    assert!(all.contains(&"webp".to_owned()), "{all:?}");
    assert!(all.contains(&"jpeg".to_owned()), "{all:?}");
    assert!(
        !all.contains(&"png".to_owned()),
        "a file's own format: {all:?}"
    );
    assert!(!menu.is_empty());
    for id in all.iter().chain(&menu) {
        assert!(valid_id(id) && known.contains(id), "{id}");
    }
    // The extension shows menu targets as suggestions among all targets.
    for id in &menu {
        assert!(all.contains(id), "menu target {id} is not a target");
    }
}

#[test]
fn targets_rejects_an_unknown_extension() {
    let fx = Fixture::new();
    let file = fx.path("notes.unknownext");
    std::fs::write(&file, "x").unwrap();
    let out = fx.run(&["targets".as_ref(), file.as_os_str()]);
    assert!(!out.status.success());
    assert!(
        out.stdout.is_empty(),
        "nothing on stdout the extension could parse"
    );
}

#[test]
fn json_conversion_emits_the_events_the_extension_parses() {
    let fx = Fixture::new();
    let png = fx.png("photo.png");
    let out_dir = fx.path("out");
    std::fs::create_dir(&out_dir).unwrap();

    // The upstream extension's argument order: options, then `--`, then files.
    let out = fx.run(&[
        "--json".as_ref(),
        "--jobs".as_ref(),
        "1".as_ref(),
        "--to".as_ref(),
        "webp".as_ref(),
        "--out-dir".as_ref(),
        out_dir.as_os_str(),
        "--quality".as_ref(),
        "80".as_ref(),
        "--max-size".as_ref(),
        "4".as_ref(),
        "--".as_ref(),
        png.as_os_str(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let events = events(&out.stdout);
    events.iter().for_each(check_event);

    assert_eq!(events.first().unwrap()["event"], "started");
    assert_eq!(events.last().unwrap()["event"], "summary");
    let done = named(&events, "done");
    assert_eq!(done.len(), 1);
    assert_eq!(done[0]["index"], 0);
    assert_eq!(done[0]["input"], png.display().to_string());
    let output = PathBuf::from(done[0]["outputs"][0].as_str().unwrap());
    assert_eq!(output.parent().unwrap(), out_dir);
    assert_eq!(output.extension().unwrap(), "webp");
    let summary = events.last().unwrap();
    assert_eq!(
        (&summary["done"], &summary["failed"], &summary["cancelled"]),
        (&1.into(), &0.into(), &0.into())
    );
}

#[test]
fn json_conversion_reports_a_failed_file_and_exits_1() {
    let fx = Fixture::new();
    let good = fx.png("good.png");
    let bad = fx.path("bad.png");
    std::fs::write(&bad, "not a png").unwrap();

    let out = fx.run(&[
        "--json".as_ref(),
        "--jobs".as_ref(),
        "1".as_ref(),
        "--to".as_ref(),
        "jpeg".as_ref(),
        "--".as_ref(),
        good.as_os_str(),
        bad.as_os_str(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let events = events(&out.stdout);
    events.iter().for_each(check_event);
    let failed = named(&events, "failed");
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["index"], 1);
    assert_eq!(failed[0]["input"], bad.display().to_string());
    assert_ne!(failed[0]["kind"], "cancelled");
    assert_eq!(named(&events, "done").len(), 1);
    let summary = events.last().unwrap();
    assert_eq!(
        (&summary["done"], &summary["failed"]),
        (&1.into(), &1.into())
    );
}

/// Folders expand into more jobs than arguments, so the extension matches
/// results by each event's `input`, not by the argument index.
#[test]
fn folder_inputs_report_each_expanded_file() {
    let fx = Fixture::new();
    let folder = fx.path("photos");
    let a = fx.png("photos/a.png");
    let b = fx.png("photos/nested/b.png");
    // Audio can't become WebP with any engine. A text file can wherever
    // LibreOffice and PDFium are installed (txt -> pdf -> png -> webp).
    std::fs::write(folder.join("song.mp3"), "skipped quietly").unwrap();

    let out = fx.run(&[
        "--json".as_ref(),
        "--jobs".as_ref(),
        "2".as_ref(),
        "--recursive".as_ref(),
        "--to".as_ref(),
        "webp".as_ref(),
        "--".as_ref(),
        folder.as_os_str(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let events = events(&out.stdout);
    events.iter().for_each(check_event);

    let mut inputs: Vec<_> = named(&events, "done")
        .iter()
        .map(|e| PathBuf::from(e["input"].as_str().unwrap()))
        .collect();
    inputs.sort();
    assert_eq!(inputs, [a, b]);
    let mut indices: Vec<_> = named(&events, "started")
        .iter()
        .map(|e| e["index"].as_u64().unwrap())
        .collect();
    indices.sort();
    assert_eq!(indices, [0, 1]);
    assert_eq!(events.last().unwrap()["done"], 2);
}

/// Every option the upgraded extension can pass is accepted together, even
/// when some don't apply to the target.
#[test]
fn every_option_the_extension_passes_is_accepted() {
    let fx = Fixture::new();
    let png = fx.png("photo.png");
    let out = fx.run(&[
        "--json".as_ref(),
        "--jobs".as_ref(),
        "1".as_ref(),
        "--to".as_ref(),
        "jpeg".as_ref(),
        "--quality".as_ref(),
        "85".as_ref(),
        "--max-size".as_ref(),
        "4".as_ref(),
        "--dpi".as_ref(),
        "150".as_ref(),
        "--background".as_ref(),
        "#ff8800".as_ref(),
        "--video-height".as_ref(),
        "720".as_ref(),
        "--video-codec".as_ref(),
        "hevc".as_ref(),
        "--audio-bitrate".as_ref(),
        "128".as_ref(),
        "--no-audio".as_ref(),
        "--".as_ref(),
        png.as_os_str(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(named(&events(&out.stdout), "done").len(), 1);

    // JPEG can't keep transparency; the extension shows this message.
    let out = fx.run(&[
        "--to".as_ref(),
        "jpeg".as_ref(),
        "--background".as_ref(),
        "transparent".as_ref(),
        "--".as_ref(),
        png.as_os_str(),
    ]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("can't store transparency"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn presets_list_and_preset_flag() {
    let fx = Fixture::new();
    let presets = fx.path("config/presets");
    std::fs::create_dir_all(&presets).unwrap();
    std::fs::write(presets.join("web.toml"), "to = \"webp\"\nquality = 80\n").unwrap();

    // The extension reads the first word of each line as the preset name.
    let list = fx.ok(&["presets"]);
    let names: Vec<_> = list
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(names, ["web"], "{list}");
    assert_eq!(list.split_whitespace().nth(1), Some("webp"), "{list}");

    let png = fx.png("photo.png");
    let out = fx.run(&[
        "--json".as_ref(),
        "--preset".as_ref(),
        "web".as_ref(),
        "--".as_ref(),
        png.as_os_str(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let events = events(&out.stdout);
    let done = named(&events, "done");
    assert_eq!(done.len(), 1);
    assert!(done[0]["outputs"][0].as_str().unwrap().ends_with(".webp"));
}

/// The extension offers to install the document pack when Office files have
/// no targets; this is the command it runs.
#[test]
fn pack_commands_exist() {
    let fx = Fixture::new();
    let status = fx.ok(&["pack", "status"]);
    assert!(status.starts_with("documents: "), "{status}");
    let help = fx.ok(&["pack", "install", "--help"]);
    assert!(help.contains("documents"), "{help}");
}
