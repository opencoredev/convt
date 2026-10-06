//! What transparent areas become (Options::background), end to end through
//! the registry: images, SVG and PDF pages. Cases that need an engine this
//! machine lacks print SKIP and pass.

use std::path::{Path, PathBuf};

use convt_core::{Background, Cancel, Error, Job, Options, Output, Registry, format_by_id};

fn registry() -> Registry {
    convt_engines::default_registry()
}

/// A 40x40 PNG: transparent, with an opaque red square in the middle and a
/// half-transparent red block in the bottom-left corner (big enough that
/// JPEG's color subsampling doesn't blur it into its neighbours).
fn logo(dir: &Path) -> PathBuf {
    let mut img = image::RgbaImage::from_pixel(40, 40, image::Rgba([0, 0, 0, 0]));
    for y in 10..30 {
        for x in 10..30 {
            img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
        }
    }
    for y in 32..40 {
        for x in 0..8 {
            img.put_pixel(x, y, image::Rgba([255, 0, 0, 128]));
        }
    }
    let path = dir.join("logo.png");
    img.save(&path).unwrap();
    path
}

fn convert(input: &Path, to: &str, background: Option<Background>) -> Result<Vec<PathBuf>, Error> {
    let out = input.parent().unwrap().join(format!(
        "out-{to}-{}",
        background.map_or("default".into(), |b| b.id())
    ));
    std::fs::create_dir_all(&out).unwrap();
    let job = Job {
        output: Output::Dir(out),
        options: Options {
            background,
            ..Options::default()
        },
        ..Job::new(input, format_by_id(to).unwrap())
    };
    registry().run(&job, &|_| {}, &Cancel::new())
}

fn pixel(path: &Path, x: u32, y: u32) -> [u8; 4] {
    image::open(path).unwrap().to_rgba8().get_pixel(x, y).0
}

/// JPEG is lossy, so compare colors within a tolerance.
fn near(got: [u8; 4], want: [u8; 3]) -> bool {
    got[..3].iter().zip(want).all(|(&g, w)| g.abs_diff(w) <= 10)
}

#[test]
fn jpeg_defaults_to_a_white_background() {
    let dir = tempfile::tempdir().unwrap();
    let files = convert(&logo(dir.path()), "jpeg", None).unwrap();
    let corner = pixel(&files[0], 0, 0);
    assert!(
        near(corner, [255, 255, 255]),
        "transparent area became {corner:?}, not white"
    );
    assert!(near(pixel(&files[0], 20, 20), [255, 0, 0]));
    // Half-transparent red over white is pink, not dark red.
    assert!(
        near(pixel(&files[0], 3, 36), [255, 127, 127]),
        "{:?}",
        pixel(&files[0], 3, 36)
    );
}

#[test]
fn jpeg_takes_the_chosen_background() {
    let dir = tempfile::tempdir().unwrap();
    let src = logo(dir.path());
    for (background, want) in [
        ("black", [0, 0, 0]),
        ("white", [255, 255, 255]),
        ("#ff8800", [255, 136, 0]),
        ("#08f", [0, 136, 255]),
    ] {
        let files = convert(&src, "jpeg", Some(background.parse().unwrap())).unwrap();
        let corner = pixel(&files[0], 0, 0);
        assert!(near(corner, want), "{background}: {corner:?}");
    }
}

#[test]
fn transparent_is_refused_where_the_format_cannot_store_it() {
    let dir = tempfile::tempdir().unwrap();
    let err = convert(&logo(dir.path()), "jpeg", Some(Background::Transparent)).unwrap_err();
    assert!(
        err.to_string().contains("can't store transparency"),
        "unexpected error: {err}"
    );
}

#[test]
fn formats_with_transparency_keep_it_unless_a_color_is_chosen() {
    let dir = tempfile::tempdir().unwrap();
    let src = logo(dir.path());
    for to in ["webp", "gif", "tiff", "qoi", "tga", "bmp"] {
        for background in [None, Some(Background::Transparent)] {
            let files = convert(&src, to, background).unwrap();
            assert_eq!(
                pixel(&files[0], 0, 0)[3],
                0,
                "{to} {background:?} lost transparency"
            );
        }
        let files = convert(&src, to, Some(Background::WHITE)).unwrap();
        assert_eq!(
            pixel(&files[0], 0, 0),
            [255, 255, 255, 255],
            "{to} on white"
        );
        assert_eq!(pixel(&files[0], 20, 20), [255, 0, 0, 255], "{to} on white");
    }
}

#[test]
fn svg_renders_onto_the_background() {
    let dir = tempfile::tempdir().unwrap();
    let svg = dir.path().join("icon.svg");
    std::fs::write(
        &svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"><rect x="10" y="10" width="20" height="20" fill="#00ff00"/></svg>"##,
    )
    .unwrap();
    let png = convert(&svg, "png", None).unwrap();
    assert_eq!(
        pixel(&png[0], 0, 0)[3],
        0,
        "SVG to PNG keeps the transparent canvas"
    );
    let black = convert(&svg, "png", Some(Background::BLACK)).unwrap();
    assert_eq!(pixel(&black[0], 0, 0), [0, 0, 0, 255]);
    let jpeg = convert(&svg, "jpeg", None).unwrap();
    assert!(
        near(pixel(&jpeg[0], 0, 0), [255, 255, 255]),
        "SVG to JPEG defaults to white"
    );
}

/// A one-page PDF with a red square and no page background. PDFium tolerates
/// the approximate xref, but the offsets are computed anyway.
fn pdf(dir: &Path) -> PathBuf {
    let content = "1 0 0 rg 20 20 32 32 re f";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Contents 4 0 R >>".to_string(),
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
    ];
    let mut body = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(body.len());
        body += &format!("{} 0 obj\n{object}\nendobj\n", i + 1);
    }
    let xref = body.len();
    body += &format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        body += &format!("{offset:010} 00000 n \n");
    }
    body += &format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    let path = dir.join("page.pdf");
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn pdf_pages_render_onto_the_background() {
    if registry()
        .plan(format_by_id("pdf").unwrap(), format_by_id("png").unwrap())
        .is_err()
    {
        eprintln!("SKIP PDF background: PDFium missing");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path());
    let white = convert(&src, "png", None).unwrap();
    assert_eq!(
        pixel(&white[0], 1, 1),
        [255, 255, 255, 255],
        "PDF pages default to white"
    );
    let clear = convert(&src, "png", Some(Background::Transparent)).unwrap();
    assert_eq!(pixel(&clear[0], 1, 1)[3], 0, "transparent page background");
    let img = image::open(&clear[0]).unwrap().to_rgba8();
    let (w, h) = img.dimensions();
    assert!(
        near(img.get_pixel(w / 2, h / 2).0, [255, 0, 0]),
        "the drawing still renders"
    );
    let black = convert(&src, "jpeg", Some(Background::BLACK)).unwrap();
    assert!(near(pixel(&black[0], 1, 1), [0, 0, 0]));
    assert!(convert(&src, "jpeg", Some(Background::Transparent)).is_err());
}

/// A one-second MOV with an alpha channel: transparent, with a green box in
/// the middle. `None` when FFmpeg or its QuickTime Animation encoder is missing.
fn alpha_video(dir: &Path) -> Option<PathBuf> {
    let ffmpeg = convt_engines::ffmpeg::ffmpeg_path()?;
    let path = dir.join("alpha.mov");
    let ok = std::process::Command::new(ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("color=c=red@0.0:s=64x48:r=10,format=rgba,drawbox=x=16:y=12:w=32:h=24:color=green@1:t=fill")
        .args(["-t", "1", "-c:v", "qtrle"])
        .arg(&path)
        .status()
        .is_ok_and(|s| s.success());
    ok.then_some(path)
}

#[test]
fn video_frames_take_the_background() {
    let dir = tempfile::tempdir().unwrap();
    let Some(src) = alpha_video(dir.path()) else {
        eprintln!("SKIP video background: FFmpeg with qtrle missing");
        return;
    };
    if registry()
        .plan(format_by_id("mov").unwrap(), format_by_id("jpeg").unwrap())
        .is_err()
    {
        eprintln!("SKIP video background: no MOV to JPEG route");
        return;
    }
    let jpeg = convert(&src, "jpeg", None).unwrap();
    assert!(
        near(pixel(&jpeg[0], 1, 1), [255, 255, 255]),
        "{:?}",
        pixel(&jpeg[0], 1, 1)
    );
    let orange = convert(&src, "jpeg", Some("#ff8800".parse().unwrap())).unwrap();
    assert!(near(pixel(&orange[0], 1, 1), [255, 136, 0]));
    let png = convert(&src, "png", None).unwrap();
    assert_eq!(pixel(&png[0], 1, 1)[3], 0, "PNG frames keep transparency");
    let black = convert(&src, "png", Some(Background::BLACK)).unwrap();
    assert_eq!(pixel(&black[0], 1, 1), [0, 0, 0, 255]);
    let gif = convert(&src, "gif", Some(Background::WHITE)).unwrap_err();
    assert!(
        gif.to_string().contains("isn't supported for video to GIF"),
        "{gif}"
    );
}
