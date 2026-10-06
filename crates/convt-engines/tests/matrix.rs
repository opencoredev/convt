mod support;

use std::collections::BTreeMap;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Instant;

use convt_core::{
    Cancel, Category, Ctx, Engine, FORMATS, Job, Options, Output, Registry, Step, format_by_id,
};
use support::{Check, Fixture};

#[test]
fn every_engine_converts_a_declared_step() {
    let started = Instant::now();
    let root = tempfile::Builder::new()
        .prefix("convt-fast-")
        .tempdir()
        .unwrap();
    let registry = convt_engines::default_registry();
    let mut exercised = Vec::new();
    for engine in registry.engines() {
        let (from, to) = match engine.id() {
            "image" => ("png", "qoi"),
            "svg" => ("svg", "png"),
            "libheif" => {
                let steps = engine.steps();
                let step = steps.first().expect("ready libheif has a usable step");
                (step.from.id, step.to.id)
            }
            "imageio" => ("heic", "png"),
            "ffmpeg" => ("mp4", "webm"),
            "pdfium" => ("pdf", "jpeg"),
            "libreoffice" => ("odt", "docx"),
            id => panic!("new engine {id} needs a fast semantic case"),
        };
        if let Some(tool) =
            support::missing_validation_tool(format_by_id(from).unwrap(), format_by_id(to).unwrap())
        {
            eprintln!("SKIP {}: {tool} missing", engine.id());
            continue;
        }
        let fixture = match Fixture::make(root.path(), format_by_id(from).unwrap()) {
            Ok(f) => f,
            Err(e) if from == "heic" && e.contains("encoder unavailable") => {
                eprintln!("SKIP libheif: {e}");
                continue;
            }
            Err(e) => panic!("fixture {from}: {e}"),
        };
        let step = Step {
            from: fixture.format,
            to: format_by_id(to).unwrap(),
        };
        assert!(engine.steps().contains(&step));
        let out = root.path().join(engine.id());
        std::fs::create_dir_all(&out).unwrap();
        let options = Options {
            dpi: Some(72),
            ..Options::default()
        };
        let cancel = Cancel::new();
        let outputs = engine
            .convert(
                &Ctx::new(step, &options, &|_| {}, &cancel),
                &fixture.path,
                &out,
            )
            .unwrap_or_else(|e| panic!("{} {from}->{to}: {e}", engine.id()));
        support::validate(&fixture, step.to, &outputs, &options)
            .unwrap_or_else(|e| panic!("{} {from}->{to}: {e}", engine.id()));
        exercised.push(engine.id());
    }
    for (id, reason) in registry.unavailable() {
        eprintln!("SKIP {id}: {reason}");
    }
    eprintln!(
        "Fast semantic engines: {exercised:?}; {:.3}s",
        started.elapsed().as_secs_f64()
    );
}

#[test]
fn declared_routes_do_not_invent_media_from_stills() {
    let registry = convt_engines::default_registry();
    for from in FORMATS {
        for to in registry.targets(from) {
            let plan = registry.plan(from, to).unwrap();
            if !matches!(from.category, Category::Video | Category::Audio)
                && matches!(to.category, Category::Video | Category::Audio)
            {
                assert!(plan.hops.len() == 1, "nonsense route: {}", plan.describe());
            }
        }
    }
}

// Capability gates come from the backends, independently of registered edges.
const REQUIRED_ROUTES: &[(&str, &str, &str)] = &[
    ("png", "qoi", "image"),
    ("png", "avif", "image"),
    ("jpeg", "ico", "image"),
    ("exr", "png", "image"),
    ("svg", "png", "svg"),
    ("heic", "png", "hevc"),
    ("png", "heic", "hevc-encoder"),
    ("jpeg", "heic", "hevc-encoder"),
    ("svg", "heic", "hevc-encoder"),
    ("avif", "png", "av1"),
    ("mp4", "webm", "ffmpeg"),
    ("webm", "mp4", "ffmpeg"),
    ("mkv", "mp4", "ffmpeg"),
    ("mkv", "webm", "ffmpeg"),
    ("mkv", "gif", "ffmpeg"),
    ("mkv", "png", "ffmpeg"),
    ("mkv", "wav", "ffmpeg"),
    ("mp4", "gif", "ffmpeg"),
    ("webm", "gif", "ffmpeg"),
    ("webm", "png", "ffmpeg"),
    ("webm", "wav", "ffmpeg"),
    ("mp4", "png", "ffmpeg"),
    ("mp4", "wav", "ffmpeg"),
    ("wav", "flac", "ffmpeg"),
    ("gif", "mp4", "ffmpeg"),
    ("pdf", "png", "pdfium"),
    ("pdf", "jpeg", "pdfium"),
    ("odt", "docx", "libreoffice"),
    ("html", "docx", "libreoffice"),
    ("html", "doc", "libreoffice"),
    ("html", "rtf", "libreoffice"),
    ("txt", "docx", "libreoffice"),
    ("docx", "pdf", "libreoffice"),
    ("odp", "pptx", "libreoffice"),
    ("pptx", "pdf", "libreoffice"),
    ("xlsx", "csv", "libreoffice"),
    ("xlsx", "pdf", "libreoffice"),
    ("csv", "ods", "libreoffice"),
];

fn require_routes(registry: &Registry) -> Check<()> {
    let heif = convt_engines::heic::LibheifEngine::new();
    let mut available = BTreeMap::from([
        ("image", true),
        ("svg", true),
        ("hevc", heif.supports_input("heic")),
        ("hevc-encoder", heif.supports_output("heic")),
        ("av1", heif.supports_input("avif")),
        (
            "ffmpeg",
            convt_engines::ffmpeg::FfmpegEngine::new()
                .unavailable_reason()
                .is_none(),
        ),
        (
            "libreoffice",
            convt_engines::office::OfficeEngine::new()
                .unavailable_reason()
                .is_none(),
        ),
    ]);
    #[cfg(feature = "pdfium")]
    available.insert(
        "pdfium",
        convt_engines::pdfium::PdfiumEngine::new()
            .unavailable_reason()
            .is_none(),
    );
    #[cfg(not(feature = "pdfium"))]
    available.insert("pdfium", false);
    let mut failures = Vec::new();
    for &(from, to, backend) in REQUIRED_ROUTES {
        if !available[backend] {
            eprintln!("SKIP required {from} -> {to}: {backend} unavailable");
        } else if registry
            .plan(format_by_id(from).unwrap(), format_by_id(to).unwrap())
            .is_err()
        {
            failures.push(format!(
                "required {from} -> {to} is missing with {backend} available"
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn required_routes_remain_offered() {
    require_routes(&convt_engines::default_registry()).unwrap();
    let error = require_routes(&Registry::new()).unwrap_err();
    assert!(error.contains("required jpeg -> ico"), "{error}");
    assert!(error.contains("required svg -> png"), "{error}");
}

#[test]
fn unsupported_avif_inputs_and_outputs_skip_cleanly() {
    let missing = serde_json::json!({"pdfium":true,"libheif":false,"hevc":true});
    assert_eq!(
        support::missing_native_validator(
            format_by_id("avif").unwrap(),
            format_by_id("png").unwrap(),
            &missing
        ),
        Some("libheif AV1 validator")
    );
    assert_eq!(
        support::missing_native_validator(
            format_by_id("png").unwrap(),
            format_by_id("avif").unwrap(),
            &missing
        ),
        Some("libheif AV1 validator")
    );
    assert_eq!(
        support::missing_native_validator(
            format_by_id("png").unwrap(),
            format_by_id("qoi").unwrap(),
            &missing
        ),
        None
    );
}

#[test]
fn pdf_render_without_text_is_rejected() {
    if support::missing_validation_tool(format_by_id("pdf").unwrap(), format_by_id("png").unwrap())
        .is_some()
    {
        eprintln!("SKIP PDF text regression: validator missing");
        return;
    }
    let registry = convt_engines::default_registry();
    if registry
        .plan(format_by_id("pdf").unwrap(), format_by_id("png").unwrap())
        .is_err()
    {
        eprintln!("SKIP PDF text regression: PDFium missing");
        return;
    }
    let root = tempfile::Builder::new()
        .prefix("convt-pdf-text-")
        .tempdir()
        .unwrap();
    let fixture = Fixture::make(root.path(), format_by_id("pdf").unwrap()).unwrap();
    let output = root.path().join("output");
    std::fs::create_dir(&output).unwrap();
    let options = Options {
        dpi: Some(72),
        ..Options::default()
    };
    let job = Job {
        output: Output::Dir(output),
        options: options.clone(),
        ..Job::new(&fixture.path, format_by_id("png").unwrap())
    };
    let files = registry.run(&job, &|_| {}, &Cancel::new()).unwrap();
    support::validate(&fixture, job.to, &files, &options).unwrap();
    for file in &files {
        let mut pixels = image::open(file).unwrap().to_rgba8();
        for y in 0..34 {
            for x in 0..pixels.width() {
                pixels.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
            }
        }
        pixels.save(file).unwrap();
    }
    assert!(
        support::validate(&fixture, job.to, &files, &options)
            .unwrap_err()
            .contains("rendered PDF text missing")
    );
}

#[test]
#[ignore = "full semantic matrix; bun run test:matrix"]
fn full_matrix() {
    let started = Instant::now();
    let root = tempfile::Builder::new()
        .prefix("convt-matrix-")
        .tempdir()
        .unwrap();
    let registry = convt_engines::default_registry();
    require_routes(&registry).unwrap();
    let workers = std::env::var("CONVT_MATRIX_JOBS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(8)
        .clamp(1, 32);
    eprintln!(
        "Matrix fixtures: {}; workers: {workers}; formats: {}",
        root.path().display(),
        FORMATS.len()
    );
    for (id, reason) in registry.unavailable() {
        eprintln!("UNAVAILABLE {id}: {reason}");
    }
    let formats: Vec<_> = FORMATS
        .iter()
        .filter(|f| support::selected(f.id, "CONVT_MATRIX_INPUT"))
        .collect();
    let fixtures: Mutex<BTreeMap<String, Check<Fixture>>> = Mutex::new(BTreeMap::new());
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(format) = formats.get(i) else { break };
                    let result = if registry.targets(format).is_empty() {
                        Err("no available engine for this input".into())
                    } else {
                        Fixture::make(root.path(), format)
                    };
                    fixtures
                        .lock()
                        .unwrap()
                        .insert(format.id.to_owned(), result);
                }
            });
        }
    });
    let mut fixtures = fixtures.into_inner().unwrap();
    let mut av1_inputs = Vec::new();
    if convt_engines::ffmpeg::FfmpegEngine::new()
        .unavailable_reason()
        .is_none()
    {
        for from in &formats {
            if matches!(from.id, "mp4" | "webm" | "mkv") {
                let label = format!("{}-AV1", from.id);
                // A ready FFmpeg must exercise AV1 inputs even if its software
                // decoder is absent. Only the independent encoder is a prerequisite.
                let fixture = Fixture::make_av1_video(root.path(), from)
                    .unwrap_or_else(|e| panic!("fixture {label}: {e}"));
                fixtures.insert(label.clone(), Ok(fixture));
                av1_inputs.push((*from, label));
            }
        }
    }
    eprintln!(
        "Fixtures generated in {:.3}s",
        started.elapsed().as_secs_f64()
    );
    let mut cases = Vec::new();
    let mut skips = BTreeMap::new();
    for from in &formats {
        let targets = registry.targets(from);
        if let Some(bin) = std::env::var_os("CONVT_BIN") {
            let output = support::command(
                std::process::Command::new(bin)
                    .arg("targets")
                    .arg(format!("sample.{}", from.extension())),
            )
            .unwrap();
            let actual: Vec<_> = String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect();
            assert_eq!(
                actual,
                targets.iter().map(|f| f.id.to_owned()).collect::<Vec<_>>(),
                "CLI and registry targets differ for {}",
                from.id
            );
        }
        if targets.is_empty() {
            if matches!(
                from.category,
                Category::Document | Category::Presentation | Category::Spreadsheet
            ) {
                *skips
                    .entry(support::category_name(from.category))
                    .or_default() += 1;
                eprintln!("SKIP input {}: document pack unavailable", from.id);
                continue;
            }
            if support::tool("python3").is_some()
                && matches!(from.id, "heic" | "avif")
                && let Some(reason) = support::missing_native_validator(
                    from,
                    format_by_id("png").unwrap(),
                    support::native_validators(),
                )
            {
                *skips
                    .entry(support::category_name(from.category))
                    .or_default() += 1;
                eprintln!("SKIP input {}: {reason} missing", from.id);
                continue;
            }
            eprintln!("GAP {}: no offered targets", from.id);
        }
        for to in targets
            .into_iter()
            .filter(|f| support::selected(f.id, "CONVT_MATRIX_TARGET"))
        {
            if let Some(tool) = support::missing_validation_tool(from, to) {
                *skips
                    .entry(support::category_name(from.category))
                    .or_default() += 1;
                eprintln!("SKIP {} -> {}: {tool} missing", from.id, to.id);
            } else if fixtures[from.id]
                .as_ref()
                .is_err_and(|e| e.contains("HEIC encoder unavailable"))
            {
                *skips
                    .entry(support::category_name(from.category))
                    .or_default() += 1;
                eprintln!(
                    "SKIP {} -> {}: {}",
                    from.id,
                    to.id,
                    fixtures[from.id].as_ref().err().unwrap()
                );
            } else {
                cases.push((*from, to, from.id.to_owned()));
            }
        }
    }
    for (from, label) in av1_inputs {
        for to in registry
            .targets(from)
            .into_iter()
            .filter(|f| support::selected(f.id, "CONVT_MATRIX_TARGET"))
        {
            // Unlike optional native validators, missing AV1 media tooling is
            // a failure: otherwise a ready but decoder-less bundle could pass.
            if let Some(tool) = support::missing_validation_tool(from, to) {
                panic!("{label} -> {}: required {tool} validator missing", to.id);
            }
            cases.push((from, to, label.clone()));
        }
    }
    // Reachability expectations live separately from Registry::targets, so lost
    // routes cannot silently shrink the test set. Gaps are reported, not invented.
    for from in &formats {
        for to in FORMATS {
            let related = from.category == to.category
                && matches!(
                    from.category,
                    Category::Image
                        | Category::Video
                        | Category::Audio
                        | Category::Document
                        | Category::Presentation
                        | Category::Spreadsheet
                );
            if from != &to && related && registry.plan(from, to).is_err() {
                if to.id == "heic"
                    && from.category == Category::Image
                    && !convt_engines::heic::LibheifEngine::new().supports_output("heic")
                {
                    eprintln!("KNOWN GAP {} -> heic: no encoder offered", from.id);
                } else {
                    eprintln!("GAP {} -> {}", from.id, to.id);
                }
            }
        }
    }
    let results = Mutex::new(Vec::new());
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((from, to, label)) = cases.get(i) else {
                        break;
                    };
                    let case_started = Instant::now();
                    let result = (|| {
                        let fixture = fixtures[label]
                            .as_ref()
                            .map_err(|e| format!("fixture generation failed: {e}"))?;
                        let out = root.path().join(format!("{label}-{}", to.id));
                        std::fs::create_dir(&out).map_err(|e| e.to_string())?;
                        let options = Options {
                            dpi: Some(72),
                            ..Options::default()
                        };
                        let job = Job {
                            output: Output::Dir(out),
                            options: options.clone(),
                            ..Job::new(&fixture.path, to)
                        };
                        let files = if let Some(bin) = std::env::var_os("CONVT_BIN") {
                            support::command(
                                std::process::Command::new(bin)
                                    .arg(&fixture.path)
                                    .args(["--to", to.id, "--dpi", "72", "--json", "-o"])
                                    .arg(match &job.output {
                                        Output::Dir(d) => d,
                                        _ => unreachable!(),
                                    }),
                            )?;
                            let mut files = std::fs::read_dir(match &job.output {
                                Output::Dir(d) => d,
                                _ => unreachable!(),
                            })
                            .map_err(|e| e.to_string())?
                            .map(|e| e.map(|e| e.path()))
                            .collect::<std::io::Result<Vec<_>>>()
                            .map_err(|e| e.to_string())?;
                            files.sort_by_key(|p| {
                                p.file_stem()
                                    .unwrap()
                                    .to_string_lossy()
                                    .rsplit_once('-')
                                    .and_then(|(_, n)| n.parse::<usize>().ok())
                                    .unwrap_or(0)
                            });
                            files
                        } else {
                            registry.run(&job, &|_| {}, &Cancel::new()).map_err(|e| {
                                format!("{}: {e}", registry.plan(from, to).unwrap().describe())
                            })?
                        };
                        support::validate(fixture, to, &files, &options)?;
                        if from.category == Category::Pdf {
                            let names: Vec<_> = files
                                .iter()
                                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                                .collect();
                            let expected = vec![
                                format!("sample.{}", to.extension()),
                                format!("sample-2.{}", to.extension()),
                            ];
                            if names != expected {
                                return Err(format!("page names {names:?}; expected {expected:?}"));
                            }
                        }
                        Ok(())
                    })();
                    let elapsed = case_started.elapsed();
                    eprintln!(
                        "{} {} -> {} {:.3}s",
                        if result.is_ok() { "PASS" } else { "FAIL" },
                        label,
                        to.id,
                        elapsed.as_secs_f64()
                    );
                    results.lock().unwrap().push((
                        support::category_name(from.category),
                        label.to_owned(),
                        to.id.to_owned(),
                        elapsed,
                        result,
                    ));
                }
            });
        }
    });
    let results = results.into_inner().unwrap();
    support::print_summary(&results, &skips);
    eprintln!(
        "Full matrix wall time: {:.3}s",
        started.elapsed().as_secs_f64()
    );
    if let Ok(path) = std::env::var("CONVT_MATRIX_REPORT") {
        let report:Vec<_>=results.iter().map(|(category,from,to,time,result)|serde_json::json!({"category":category,"from":from,"to":to,"seconds":time.as_secs_f64(),"error":result.as_ref().err()})).collect();
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    if std::env::var_os("CONVT_MATRIX_KEEP").is_some() {
        eprintln!("Retained matrix artifacts: {}", root.keep().display());
    }
    assert!(
        !results.is_empty() || !skips.is_empty(),
        "filters selected no conversions"
    );
    assert!(
        results.iter().all(|r| r.4.is_ok()),
        "matrix failures listed above"
    );
}

#[test]
fn raster_heic_outputs_skip_without_independent_validator() {
    let from = format_by_id("png").unwrap();
    let to = format_by_id("heic").unwrap();
    let missing = serde_json::json!({"pdfium":true,"libheif":true,"hevc":false});
    assert_eq!(
        support::missing_validation_tool_with(
            from,
            to,
            |name| name != "python3",
            || panic!("must skip before Python helper")
        ),
        Some("python3")
    );
    assert_eq!(
        support::missing_validation_tool_with(from, to, |_| true, || &missing),
        Some("libheif HEVC validator")
    );
}

#[test]
fn av1_video_fixtures_preserve_media_semantics() {
    let registry = convt_engines::default_registry();
    if convt_engines::ffmpeg::FfmpegEngine::new()
        .unavailable_reason()
        .is_some()
    {
        eprintln!("SKIP AV1 fixture regression: FFmpeg engine unavailable");
        return;
    }
    let root = tempfile::Builder::new()
        .prefix("convt-av1-")
        .tempdir()
        .unwrap();
    for container in ["mp4", "webm", "mkv"] {
        let from = format_by_id(container).unwrap();
        let fixture = Fixture::make_av1_video(root.path(), from).unwrap();
        // Include the other video container: same-format routes intentionally
        // do not exercise conversion in the registry.
        for target in ["mp4", "webm", "gif", "png", "wav"] {
            if target == container {
                continue;
            }
            let to = format_by_id(target).unwrap();
            assert!(
                registry.plan(from, to).is_ok(),
                "required AV1 {container} -> {target} route missing"
            );
            let out = root.path().join(format!("{container}-AV1-{target}"));
            std::fs::create_dir(&out).unwrap();
            let options = Options::default();
            let job = Job {
                output: Output::Dir(out),
                options: options.clone(),
                ..Job::new(&fixture.path, to)
            };
            let files = registry
                .run(&job, &|_| {}, &Cancel::new())
                .unwrap_or_else(|e| panic!("AV1 {container} -> {target}: {e}"));
            support::validate(&fixture, to, &files, &options)
                .unwrap_or_else(|e| panic!("AV1 {container} -> {target}: {e}"));
        }
    }
}
