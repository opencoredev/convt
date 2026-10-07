//! Orientation through the same registry the desktop app uses.
//!
//! Each case starts from an asymmetric 64×48 pattern (red / green / blue /
//! yellow quadrants) and checks a known point after the transform, the
//! display size, and that the output no longer carries an orientation tag.

use std::path::{Path, PathBuf};

use convt_core::{Cancel, Category, FORMATS, Job, Options, Output, format_by_id};
use image::metadata::Orientation;
use image::{DynamicImage, RgbImage, RgbaImage};

use super::orientation::{
    decoder_orientation, heif_display_orientation, heif_exif_orientation, inject_heif_exif,
    inject_heif_transforms, jpeg_with_orientation, open_stored,
};

const W: u32 = 64;
const H: u32 = 48;

fn pattern() -> RgbaImage {
    RgbaImage::from_fn(W, H, |x, y| {
        image::Rgba(match (x < W / 2, y < H / 2) {
            (true, true) => [240, 24, 24, 255],
            (false, true) => [24, 224, 24, 255],
            (true, false) => [24, 24, 240, 255],
            (false, false) => [224, 224, 24, 128],
        })
    })
}

/// Where stored pixel `(x, y)` of a `w × h` image lands after EXIF `tag`.
fn map_point(tag: u8, x: u32, y: u32, w: u32, h: u32) -> (u32, u32, u32, u32) {
    match tag {
        1 => (x, y, w, h),
        2 => (w - 1 - x, y, w, h),
        3 => (w - 1 - x, h - 1 - y, w, h),
        4 => (x, h - 1 - y, w, h),
        5 => (y, x, h, w),
        6 => (h - 1 - y, x, h, w),
        7 => (h - 1 - y, w - 1 - x, h, w),
        8 => (y, w - 1 - x, h, w),
        _ => panic!("exif {tag}"),
    }
}

fn red_sample(tag: u8) -> ((u32, u32), (u32, u32)) {
    // Centre of the red quadrant in stored pixels.
    let (x, y, dw, dh) = map_point(tag, W / 4, H / 4, W, H);
    ((x, y), (dw, dh))
}

fn convert(input: &Path, to: &str) -> PathBuf {
    let out = input.parent().unwrap().join(format!("out-{to}"));
    std::fs::create_dir_all(&out).unwrap();
    let job = Job {
        output: Output::Dir(out),
        options: Options {
            quality: Some(95),
            ..Options::default()
        },
        ..Job::new(input, format_by_id(to).unwrap())
    };
    let files = crate::default_registry()
        .run(&job, &|_| {}, &Cancel::new())
        .unwrap_or_else(|e| panic!("{} -> {to}: {e}", input.display()));
    assert_eq!(
        files.len(),
        1,
        "{} -> {to} wrote {}",
        input.display(),
        files.len()
    );
    files.into_iter().next().unwrap()
}

fn image_targets() -> Vec<&'static str> {
    let registry = crate::default_registry();
    let from = format_by_id("jpeg").unwrap();
    registry
        .targets(from)
        .into_iter()
        .filter(|f| f.category == Category::Image)
        .map(|f| f.id)
        .collect()
}

fn heic_targets() -> Vec<&'static str> {
    let registry = crate::default_registry();
    let from = format_by_id("heic").unwrap();
    registry
        .targets(from)
        .into_iter()
        .filter(|f| f.category == Category::Image && !matches!(f.id, "avif" | "heic"))
        .map(|f| f.id)
        .collect()
}

fn open_output(path: &Path, to: &str) -> Option<DynamicImage> {
    match open_stored(path) {
        Ok(img) => Some(img),
        Err(e) if matches!(to, "avif" | "heic") => {
            eprintln!("skipping pixel check for {to}: {e}");
            None
        }
        Err(e) => panic!("{}: {e}", path.display()),
    }
}

fn assert_oriented(path: &Path, tag: u8, to: &str) {
    let Some(stored) = open_output(path, to) else {
        return;
    };
    let ((x, y), (dw, dh)) = red_sample(tag);
    assert_eq!(
        (stored.width(), stored.height()),
        (dw, dh),
        "{to} size after EXIF {tag}"
    );
    let px = stored.to_rgba8().get_pixel(x, y).0;
    let tol = match to {
        "jpeg" | "webp" | "avif" | "heic" | "gif" => 45,
        _ => 20,
    };
    assert!(
        px[0].abs_diff(240) <= tol && px[1].abs_diff(24) <= tol && px[2].abs_diff(24) <= tol,
        "{to} EXIF {tag}: red at ({x},{y}) is {px:?}"
    );
    if matches!(to, "ico" | "avif" | "heic") {
        return;
    }
    let leftover = decoder_orientation(path)
        .unwrap_or_else(|e| panic!("{to} leftover orientation unreadable: {e}"));
    assert_eq!(
        leftover,
        Orientation::NoTransforms,
        "{to} left orientation {leftover:?}, which would double-rotate"
    );
}

fn write_oriented_jpeg(dir: &Path, tag: u8) -> PathBuf {
    let raw = dir.join(format!("raw-{tag}.jpeg"));
    DynamicImage::ImageRgb8(RgbImage::from_fn(W, H, |x, y| {
        let p = pattern().get_pixel(x, y).0;
        image::Rgb([p[0], p[1], p[2]])
    }))
    .save(&raw)
    .unwrap();
    let path = dir.join(format!("exif-{tag}.jpeg"));
    std::fs::write(
        &path,
        jpeg_with_orientation(&std::fs::read(&raw).unwrap(), tag).unwrap(),
    )
    .unwrap();
    path
}

#[test]
fn jpeg_exif_all_eight_to_every_image_target() {
    let targets = image_targets();
    assert!(
        targets.contains(&"webp") && targets.contains(&"png"),
        "image engine missing: {targets:?}"
    );
    let dir = tempfile::tempdir().unwrap();
    for tag in 1..=8 {
        let src = write_oriented_jpeg(dir.path(), tag);
        for to in &targets {
            let out = convert(&src, to);
            assert_oriented(&out, tag, to);
        }
    }
}

fn encode_heic(dir: &Path) -> Option<PathBuf> {
    let png = dir.join("pattern.png");
    pattern().save(&png).unwrap();
    let registry = crate::default_registry();
    let from = format_by_id("png").unwrap();
    let to = format_by_id("heic").unwrap();
    match registry.plan(from, to) {
        Ok(plan) => println!(
            "HEIC fixture route on {}: {}",
            std::env::consts::OS,
            plan.describe()
        ),
        Err(e) => {
            if cfg!(target_os = "macos") {
                panic!(
                    "macOS must encode PNG→HEIC via sips so HEIC orientation cases run in CI: {e}"
                );
            }
            eprintln!("skipping HEIC orientation: no PNG→HEIC route ({e})");
            return None;
        }
    }
    let path = convert(&png, "heic");
    let bytes = std::fs::read(&path).unwrap();
    println!(
        "HEIC fixture: {} ({} bytes), irot/imir={:?}, EXIF={:?}",
        path.display(),
        bytes.len(),
        heif_display_orientation(&bytes).map(Orientation::to_exif),
        heif_exif_orientation(&bytes).map(Orientation::to_exif)
    );
    Some(path)
}

/// HEIF transforms that produce each EXIF value: (exif, irot, imir).
const HEIF_CASES: &[(u8, Option<u8>, Option<u8>)] = &[
    (1, Some(0), None),
    (2, None, Some(1)),
    (3, Some(2), None),
    (4, None, Some(0)),
    (5, Some(3), Some(1)),
    (6, Some(3), None),
    (7, Some(1), Some(1)),
    (8, Some(1), None),
];

#[test]
fn heic_orientation_irot_imir_to_every_image_target() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(base) = encode_heic(tmp.path()) else {
        return;
    };
    let bytes = std::fs::read(&base).unwrap();
    let targets = heic_targets();
    assert!(
        targets.contains(&"webp") && targets.contains(&"png") && targets.contains(&"jpeg"),
        "HEIC route missing: {targets:?}"
    );
    println!("HEIC image targets: {targets:?}");
    let dir = tempfile::tempdir().unwrap();
    for &(tag, irot, imir) in HEIF_CASES {
        let patched = inject_heif_transforms(&bytes, irot, imir).unwrap();
        assert_eq!(
            heif_display_orientation(&patched)
                .map(Orientation::to_exif)
                .unwrap_or(1),
            tag,
            "injected irot={irot:?} imir={imir:?}"
        );
        let src = dir.path().join(format!("irot-{tag}.heic"));
        std::fs::write(&src, patched).unwrap();
        for to in &targets {
            let out = convert(&src, to);
            assert_oriented(&out, tag, to);
        }
        println!("heic_orientation irot={irot:?} imir={imir:?} EXIF {tag}: passed");
    }
}

#[test]
fn heic_orientation_exif_only_to_every_image_target() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(base) = encode_heic(tmp.path()) else {
        return;
    };
    let bytes = std::fs::read(&base).unwrap();
    let targets = heic_targets();
    let dir = tempfile::tempdir().unwrap();
    for tag in 1..=8 {
        let patched = inject_heif_exif(&bytes, tag).unwrap();
        assert!(
            heif_display_orientation(&patched).is_none(),
            "EXIF-only fixture grew an irot/imir"
        );
        assert_eq!(
            heif_exif_orientation(&patched).map(Orientation::to_exif),
            Some(tag),
            "injected EXIF {tag}"
        );
        let src = dir.path().join(format!("exif-{tag}.heic"));
        std::fs::write(&src, patched).unwrap();
        for to in &targets {
            let out = convert(&src, to);
            assert_oriented(&out, tag, to);
        }
        println!("heic_orientation EXIF-only {tag}: passed");
    }
}

#[test]
fn heic_orientation_matching_exif_and_irot_does_not_double_rotate() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(base) = encode_heic(tmp.path()) else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let with_irot = inject_heif_transforms(&std::fs::read(&base).unwrap(), Some(3), None).unwrap();
    let both = inject_heif_exif(&with_irot, 6).unwrap();
    assert_eq!(
        heif_display_orientation(&both),
        Some(Orientation::Rotate90),
        "rewriting EXIF dropped the irot transform"
    );
    assert_eq!(
        heif_exif_orientation(&both).map(Orientation::to_exif),
        Some(6)
    );
    let src = dir.path().join("both.heic");
    std::fs::write(&src, both).unwrap();
    let out = convert(&src, "webp");
    // irot 3 == EXIF 6 == 90° CW. Applying both would be 180°.
    assert_oriented(&out, 6, "webp");
    println!("heic_orientation matching irot+EXIF: passed (single 90° CW)");
}

/// iPhone photos store HEIF `irot` 3 and a matching EXIF Orientation 6.
/// Applying both would land the picture upside down.
#[test]
fn heic_orientation_iphone_irot_and_exif_6() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(base) = encode_heic(tmp.path()) else {
        return;
    };
    let with_irot = inject_heif_transforms(&std::fs::read(&base).unwrap(), Some(3), None).unwrap();
    let iphone = inject_heif_exif(&with_irot, 6).unwrap();
    assert_eq!(
        heif_display_orientation(&iphone),
        Some(Orientation::Rotate90)
    );
    assert_eq!(
        heif_exif_orientation(&iphone).map(Orientation::to_exif),
        Some(6)
    );
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("iphone-irot-exif6.heic");
    std::fs::write(&src, &iphone).unwrap();
    println!(
        "iphone-style HEIC: irot=3 and EXIF Orientation=6, {} bytes, os={}",
        iphone.len(),
        std::env::consts::OS
    );
    let out = convert(&src, "webp");
    let stored = open_stored(&out).expect("webp pixels");
    println!(
        "iphone-style HEIC→WebP: {}x{} (expect 48x64), leftover orientation={:?}",
        stored.width(),
        stored.height(),
        decoder_orientation(&out).ok()
    );
    assert_oriented(&out, 6, "webp");
    println!("iphone-style HEIC→WebP: passed (single 90° CW, not 180°)");
}

#[test]
fn unoriented_photo_keeps_alpha_size_and_bit_depth() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("photo.png");
    pattern().save(&png).unwrap();
    let registry = crate::default_registry();
    let from = format_by_id("png").unwrap();
    for to in registry
        .targets(from)
        .into_iter()
        .filter(|f| f.category == Category::Image || matches!(f.id, "svg"))
    {
        if to.id == "svg" {
            // Raster to SVG is unsupported.
            continue;
        }
        let out = convert(&png, to.id);
        let Some(img) = open_output(&out, to.id) else {
            continue;
        };
        let max = if to.id == "ico" { 256 } else { u32::MAX };
        let (w, h) = (W.min(max), H.min(max));
        if to.id == "ico" && (W > 256 || H > 256) {
            continue;
        }
        assert_eq!((img.width(), img.height()), (w, h), "{}", to.id);
        let px = img.to_rgba8().get_pixel(W / 4, H / 4).0;
        let tol = match to.id {
            "jpeg" | "webp" | "avif" | "heic" | "gif" => 45,
            _ => 20,
        };
        assert!(
            px[0].abs_diff(240) <= tol && px[1] <= 24 + tol,
            "{} colour shifted: {px:?}",
            to.id
        );
        if format_by_id(to.id).unwrap().keeps_transparency() && !matches!(to.id, "gif" | "heic") {
            let a = img.to_rgba8().get_pixel(3 * W / 4, 3 * H / 4).0[3];
            if to.id != "avif" {
                assert!(
                    a.abs_diff(128) <= 10 || to.id == "bmp",
                    "{} alpha {a}",
                    to.id
                );
            }
        }
    }

    let deep = dir.path().join("deep.png");
    DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(
        16,
        12,
        image::Rgba([12345u16, 45678, 23456, 65535]),
    ))
    .save(&deep)
    .unwrap();
    for to in ["png", "tiff", "exr"] {
        if registry.plan(from, format_by_id(to).unwrap()).is_err() {
            continue;
        }
        let out = convert(&deep, to);
        let decoded = image::open(&out).unwrap();
        assert_eq!(
            decoded.color(),
            if to == "exr" {
                image::ColorType::Rgba32F
            } else {
                image::ColorType::Rgba16
            },
            "{to} bit depth"
        );
    }
}

#[test]
fn every_image_format_is_in_the_matrix() {
    let ids: Vec<_> = FORMATS
        .iter()
        .filter(|f| f.category == Category::Image)
        .map(|f| f.id)
        .collect();
    for id in ["jpeg", "png", "webp", "heic", "avif", "gif", "tiff", "bmp"] {
        assert!(ids.contains(&id), "{id} missing from FORMATS");
    }
}
