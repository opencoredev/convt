#[path = "../../convt-engines/tests/support/mod.rs"]
mod support;

use std::path::Path;
use std::process::{Command, Output};

use convt_core::{Options, format_by_id};
use serde_json::Value;

fn cli(root: &Path, args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_convt"))
        .env("CONVT_LICENSE_STORE", "file")
        .env("CONVT_CONFIG_DIR", root.join("config"))
        .env("CONVT_DATA_DIR", root.join("data"))
        .env_remove("CONVT_LICENSE_ENFORCE")
        .args(args)
        .output()
        .unwrap()
}

fn events(out: &Output) -> Vec<Value> {
    String::from_utf8(out.stdout.clone())
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap_or_else(|e| panic!("bad JSON {s}: {e}")))
        .collect()
}

fn ok(out: &Output) {
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Linux can return ETXTBSY if we exec a binary whose write has not settled.
/// Windows may deny the first exec while a scanner still has the copy open.
fn output_of_copied(binary: &Path, args: &[&str]) -> Output {
    let mut last = None;
    for attempt in 0..10 {
        match Command::new(binary).args(args).output() {
            Ok(out) => return out,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ExecutableFileBusy | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(20 * (attempt + 1)));
            }
            Err(e) => panic!("{e}"),
        }
    }
    panic!("{}", last.unwrap());
}

#[test]
fn skill_flag_and_subcommand_print_the_same_skill_md() {
    let root = tempfile::tempdir().unwrap();
    let flag = cli(root.path(), &["--skill".as_ref()]);
    let sub = cli(root.path(), &["skill".as_ref()]);
    ok(&flag);
    ok(&sub);
    assert_eq!(flag.status.code(), Some(0));
    assert_eq!(sub.status.code(), Some(0));
    assert_eq!(flag.stdout, sub.stdout);
    assert!(
        flag.stderr.is_empty(),
        "{:?}",
        String::from_utf8_lossy(&flag.stderr)
    );
    let text = String::from_utf8(flag.stdout).unwrap();
    assert!(text.starts_with("---\nname: convt\n"), "{text}");
    assert!(text.contains("description:"));
    assert!(text.contains("\n---\n"));
    assert!(text.contains("--to"));
    assert!(text.contains("--json"));
    assert!(text.contains("`jpeg`"));
    assert!(
        !text.contains('@'),
        "skill must not contain email addresses"
    );
}

#[test]
fn help_mentions_skill() {
    let root = tempfile::tempdir().unwrap();
    let out = cli(root.path(), &["--help".as_ref()]);
    ok(&out);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("--skill"), "{text}");
    assert!(
        text.contains("convt --skill") || text.contains("`convt --skill`"),
        "{text}"
    );
}

#[test]
fn renamed_binary_uses_convt_in_help_and_errors() {
    let root = tempfile::tempdir().unwrap();
    let binary = root.path().join("convt.bin");
    std::fs::copy(env!("CARGO_BIN_EXE_convt"), &binary).unwrap();
    // FlushFileBuffers needs write access on Windows; a read-only open fails there.
    #[cfg(unix)]
    if let Ok(file) = std::fs::File::open(&binary) {
        let _ = file.sync_all();
    }
    for args in [
        vec!["--help"],
        vec!["pack", "status", "--help"],
        vec!["--unknown-option"],
    ] {
        let out = output_of_copied(&binary, &args);
        assert_eq!(out.status.success(), args.last() == Some(&"--help"));
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(text.contains("Usage: convt "), "{text}");
        assert!(!text.contains("convt.bin"), "{text}");
    }
}

#[test]
fn pack_status_accepts_optional_documents_name() {
    let root = tempfile::tempdir().unwrap();
    let default = cli(root.path(), &["pack".as_ref(), "status".as_ref()]);
    let explicit = cli(
        root.path(),
        &["pack".as_ref(), "status".as_ref(), "documents".as_ref()],
    );
    ok(&default);
    ok(&explicit);
    assert_eq!(default.stdout, explicit.stdout);
    assert!(String::from_utf8_lossy(&default.stdout).starts_with("documents:"));
    let invalid = cli(
        root.path(),
        &["pack".as_ref(), "status".as_ref(), "unknown".as_ref()],
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("possible values: documents"));
}

#[test]
fn pack_status_explains_why_a_forged_pack_is_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let hash = "a".repeat(64);
    let packs = root.path().join("data/packs/documents");
    let dir = packs.join(&hash);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(packs.join("current"), &hash).unwrap();
    std::fs::write(dir.join("verified.sha256"), &hash).unwrap();
    std::fs::write(dir.join("soffice"), "forged launcher").unwrap();
    let out = cli(
        root.path(),
        &["pack".as_ref(), "status".as_ref(), "documents".as_ref()],
    );
    ok(&out);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("not installed ("), "{text}");
    assert!(text.contains("pin") || text.contains("writable"), "{text}");
}

#[test]
fn recursive_parallel_json_presets_and_existing_names() {
    let root = tempfile::Builder::new()
        .prefix("convt-cli-")
        .tempdir()
        .unwrap();
    let root = root.path();
    let input = root.join("input");
    let nested = input.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    support::pattern().save(input.join("sample.png")).unwrap();
    support::pattern().save(nested.join("sample.png")).unwrap();
    std::fs::write(input.join("unsupported.xyz"), b"skip this").unwrap();
    support::pattern().save(input.join("already.webp")).unwrap();
    let outdir = root.join("batch");
    let out = cli(
        root,
        &[
            input.as_os_str(),
            "-r".as_ref(),
            "-j".as_ref(),
            "4".as_ref(),
            "--json".as_ref(),
            "--to".as_ref(),
            "webp".as_ref(),
            "-o".as_ref(),
            outdir.as_os_str(),
        ],
    );
    ok(&out);
    let events = events(&out);
    for name in ["started", "progress", "done"] {
        assert_eq!(
            events
                .iter()
                .filter(|e| e["event"] == name)
                .map(|e| e["index"].as_u64().unwrap())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            2,
            "{events:?}"
        );
    }
    let summary = events.last().unwrap();
    assert_eq!(summary["event"], "summary");
    assert_eq!(summary["done"], 2);
    assert_eq!(summary["failed"], 0);
    for path in [
        outdir.join("sample.webp"),
        outdir.join("nested/sample.webp"),
    ] {
        support::image_magic(&path, "webp").unwrap();
        support::check_pattern(&support::decode(&path, "webp").unwrap(), true, false).unwrap();
    }
    assert!(!outdir.join("already.webp").exists());
    assert!(!outdir.join("unsupported.webp").exists());
    // A second batch must preserve the previous output and choose fresh names.
    let before = std::fs::read(outdir.join("sample.webp")).unwrap();
    let out = cli(
        root,
        &[
            input.as_os_str(),
            "-r".as_ref(),
            "-j".as_ref(),
            "2".as_ref(),
            "--to".as_ref(),
            "webp".as_ref(),
            "-o".as_ref(),
            outdir.as_os_str(),
        ],
    );
    ok(&out);
    assert_eq!(std::fs::read(outdir.join("sample.webp")).unwrap(), before);
    assert!(outdir.join("sample (1).webp").exists());
    assert!(outdir.join("nested/sample (1).webp").exists());
    let presets = root.join("config/presets");
    std::fs::create_dir_all(&presets).unwrap();
    std::fs::write(
        presets.join("thumb.toml"),
        "to = \"jpeg\"\nquality = 70\nmax_size = 32\n",
    )
    .unwrap();
    let out = cli(root, &["presets".as_ref()]);
    ok(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("thumb"));
    let src = input.join("sample.png");
    let out = cli(
        root,
        &[
            src.as_os_str(),
            "--preset".as_ref(),
            "thumb".as_ref(),
            "-o".as_ref(),
            outdir.as_os_str(),
        ],
    );
    ok(&out);
    let jpg = outdir.join("sample.jpg");
    support::image_magic(&jpg, "jpeg").unwrap();
    let img = support::decode(&jpg, "jpeg").unwrap();
    assert_eq!((img.width(), img.height()), (32, 24));
    // The sample is transparent in one quadrant, which JPEG puts on white.
    support::check_pattern(&img, false, true).unwrap();
}

#[test]
fn corrupt_unsupported_and_read_only_outputs_fail_without_artifacts() {
    let root = tempfile::Builder::new()
        .prefix("convt-cli-errors-")
        .tempdir()
        .unwrap();
    let root = root.path();
    let outdir = root.join("out");
    std::fs::create_dir(&outdir).unwrap();
    for (name, kind) in [
        ("bad.png", "engine_failed"),
        ("unknown.xyz", "unsupported_input"),
    ] {
        let input = root.join(name);
        std::fs::write(&input, b"not a file format").unwrap();
        let out = cli(
            root,
            &[
                input.as_os_str(),
                "--to".as_ref(),
                "jpeg".as_ref(),
                "--json".as_ref(),
                "-o".as_ref(),
                outdir.as_os_str(),
            ],
        );
        assert_eq!(out.status.code(), Some(1));
        let events = events(&out);
        assert!(
            events
                .iter()
                .any(|e| e["event"] == "failed" && e["kind"] == kind),
            "{events:?}"
        );
        assert_eq!(std::fs::read_dir(&outdir).unwrap().count(), 0);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let fixture = support::Fixture::make(root, format_by_id("png").unwrap()).unwrap();
        std::fs::set_permissions(&outdir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let out = cli(
            root,
            &[
                fixture.path.as_os_str(),
                "--to".as_ref(),
                "jpeg".as_ref(),
                "--json".as_ref(),
                "-o".as_ref(),
                outdir.as_os_str(),
            ],
        );
        std::fs::set_permissions(&outdir, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Root bypasses Unix mode bits. Do not claim a permission check there.
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("SKIP read-only permission semantics: running as root");
        } else {
            assert_eq!(out.status.code(), Some(1));
            assert!(
                events(&out)
                    .iter()
                    .any(|e| e["event"] == "failed" && e["kind"] == "io"),
                "{}",
                String::from_utf8_lossy(&out.stdout)
            );
            assert_eq!(std::fs::read_dir(&outdir).unwrap().count(), 0);
        }
    }
}

#[test]
fn pdf_pages_keep_original_page_names_through_multiple_hops() {
    let registry = convt_engines::default_registry();
    if !registry.engines().iter().any(|e| e.id() == "pdfium") || support::tool("python3").is_none()
    {
        eprintln!("SKIP PDF CLI test: PDFium/python3 missing");
        return;
    }
    let root = tempfile::Builder::new()
        .prefix("convt-cli-pdf-")
        .tempdir()
        .unwrap();
    let root = root.path();
    let fixture = support::Fixture::make(root, format_by_id("pdf").unwrap()).unwrap();
    let outdir = root.join("pages");
    let out = cli(
        root,
        &[
            fixture.path.as_os_str(),
            "--to".as_ref(),
            "webp".as_ref(),
            "--pages".as_ref(),
            "2-".as_ref(),
            "--dpi".as_ref(),
            "72".as_ref(),
            "--json".as_ref(),
            "-o".as_ref(),
            outdir.as_os_str(),
        ],
    );
    ok(&out);
    let files = vec![outdir.join("sample-2.webp")];
    assert_eq!(std::fs::read_dir(&outdir).unwrap().count(), 1);
    let options = Options {
        pages: Some("2-".parse().unwrap()),
        dpi: Some(72),
        ..Options::default()
    };
    support::validate(&fixture, format_by_id("webp").unwrap(), &files, &options).unwrap();
    let bad = root.join("bad-pages");
    let out = cli(
        root,
        &[
            fixture.path.as_os_str(),
            "--to".as_ref(),
            "png".as_ref(),
            "--pages".as_ref(),
            "3-".as_ref(),
            "--json".as_ref(),
            "-o".as_ref(),
            bad.as_os_str(),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        events(&out)
            .iter()
            .any(|e| e["event"] == "failed" && e["kind"] == "invalid_option")
    );
    assert_eq!(std::fs::read_dir(bad).unwrap().count(), 0);
}
