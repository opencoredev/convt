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

#[test]
fn jpeg_video_frames_keep_the_pixel_aspect_ratio() {
    let Some(ffmpeg) = convt_engines::ffmpeg::ffmpeg_path() else {
        eprintln!("SKIP pixel aspect ratio: FFmpeg missing");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("anamorphic.mov");
    let status = std::process::Command::new(ffmpeg)
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=720x576:d=1",
            "-vf",
            "setsar=16/15",
            "-c:v",
            "qtrle",
        ])
        .arg(&input)
        .status()
        .unwrap();
    assert!(status.success());
    for (max_size, expected) in [(None, [0, 16, 0, 15]), (Some(319), [1, 84, 1, 63])] {
        let out = dir.path().join(format!("out-{max_size:?}"));
        std::fs::create_dir(&out).unwrap();
        let job = Job {
            output: Output::Dir(out),
            options: Options {
                max_size,
                ..Options::default()
            },
            ..Job::new(&input, format_by_id("jpeg").unwrap())
        };
        let files = registry().run(&job, &|_| {}, &Cancel::new()).unwrap();
        let jpeg = std::fs::read(&files[0]).unwrap();
        let jfif = jpeg.windows(5).position(|w| w == b"JFIF\0").unwrap();
        assert_eq!(jpeg[jfif + 7], 0, "density represents pixel aspect ratio");
        assert_eq!(&jpeg[jfif + 8..jfif + 12], &expected);
    }
}

#[test]
fn scaled_jpeg_frames_keep_ratios_larger_than_jfif_density_fields() {
    let Some(ffmpeg) = convt_engines::ffmpeg::ffmpeg_path() else {
        eprintln!("SKIP large pixel aspect ratio: FFmpeg missing");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("wide.mov");
    assert!(
        std::process::Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=red:s=1920x1080:d=0.1",
                "-vf",
                "setsar=16/15",
                "-c:v",
                "qtrle"
            ])
            .arg(&input)
            .status()
            .unwrap()
            .success()
    );
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let job = Job {
        output: Output::Dir(out),
        options: Options {
            max_size: Some(1279),
            ..Options::default()
        },
        ..Job::new(&input, format_by_id("jpeg").unwrap())
    };
    let files = registry().run(&job, &|_| {}, &Cancel::new()).unwrap();
    let jpeg = std::fs::read(&files[0]).unwrap();
    let jfif = jpeg.windows(5).position(|w| w == b"JFIF\0").unwrap();
    let num = u16::from_be_bytes(jpeg[jfif + 8..jfif + 10].try_into().unwrap());
    let den = u16::from_be_bytes(jpeg[jfif + 10..jfif + 12].try_into().unwrap());
    let img = image::open(&files[0]).unwrap();
    let display_ratio =
        f64::from(img.width()) / f64::from(img.height()) * f64::from(num) / f64::from(den);
    assert!(
        (display_ratio - 256. / 135.).abs() < 0.0001,
        "display ratio: {display_ratio}"
    );
}

#[test]
fn opaque_images_reject_transparent_jpeg_and_ppm() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("opaque.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([255, 0, 0]))
        .save(&src)
        .unwrap();
    for to in ["jpeg", "ppm"] {
        let error = convert(&src, to, Some(Background::Transparent)).unwrap_err();
        assert!(
            error.to_string().contains("can't store transparency"),
            "{error}"
        );
    }
}

#[test]
fn colored_png_video_frames_keep_the_pixel_aspect_ratio() {
    let Some(ffmpeg) = convt_engines::ffmpeg::ffmpeg_path() else {
        eprintln!("SKIP PNG pixel aspect ratio: FFmpeg missing");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("anamorphic.mov");
    assert!(
        std::process::Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=red@0:s=720x576:d=0.1,format=rgba,setsar=16/15",
                "-c:v",
                "qtrle"
            ])
            .arg(&input)
            .status()
            .unwrap()
            .success()
    );
    let plain = convert(&input, "png", None).unwrap();
    let colored = convert(&input, "png", Some(Background::BLACK)).unwrap();
    fn phys(path: &Path) -> Vec<u8> {
        let bytes = std::fs::read(path).unwrap();
        let pos = bytes
            .windows(4)
            .position(|w| w == b"pHYs")
            .expect("PNG pixel density");
        bytes[pos + 4..pos + 13].to_vec()
    }
    assert_eq!(phys(&colored[0]), phys(&plain[0]));
    assert_eq!(pixel(&colored[0], 1, 1), [0, 0, 0, 255]);
}

#[test]
fn tagged_video_frames_keep_png_color_metadata_and_jpeg_colors() {
    let Some(ffmpeg) = convt_engines::ffmpeg::ffmpeg_path() else {
        eprintln!("SKIP tagged video colors: FFmpeg missing");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("tagged.mp4");
    assert!(
        std::process::Command::new(&ffmpeg)
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=red:s=128x96:d=0.1,drawbox=x=32:y=0:w=32:h=96:color=green:t=fill,drawbox=x=64:y=0:w=32:h=96:color=blue:t=fill,drawbox=x=96:y=0:w=32:h=96:color=white:t=fill,scale=in_color_matrix=bt601:out_color_matrix=bt709",
                "-c:v",
                "libx264",
                "-colorspace",
                "bt709",
                "-color_primaries",
                "bt709",
                "-color_trc",
                "bt709"
            ])
            .arg(&input)
            .status()
            .unwrap()
            .success()
    );
    let plain = convert(&input, "png", None).unwrap();
    let colored = convert(&input, "png", Some(Background::BLACK)).unwrap();
    let info = |path: &Path| {
        png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
            .read_info()
            .unwrap()
            .info()
            .clone()
    };
    let before = info(&plain[0]);
    let after = info(&colored[0]);
    if before.gamma().is_none() {
        eprintln!("FFmpeg did not emit PNG transfer metadata; comparing available tags only");
    }
    assert_eq!(after.gamma(), before.gamma());
    assert_eq!(after.chromaticities(), before.chromaticities());
    assert_eq!(
        after.coding_independent_code_points,
        before.coding_independent_code_points
    );
    // JPEG viewers decode YCbCr as BT.601. Make the reference conversion
    // honor the input BT.709 matrix explicitly, rather than mistagging its
    // YCbCr samples as JPEG. Compare solid interiors to avoid chroma edges.
    let reference = dir.path().join("reference.jpg");
    assert!(
        std::process::Command::new(ffmpeg)
            .args(["-v", "error", "-y", "-i"])
            .arg(&input)
            .args(["-vf", "scale=in_color_matrix=bt709:out_color_matrix=bt601:in_range=limited:out_range=full", "-pix_fmt", "yuvj444p", "-frames:v", "1"])
            .arg(&reference)
            .status()
            .unwrap()
            .success()
    );
    let jpeg = convert(&input, "jpeg", None).unwrap();
    let want = image::open(reference).unwrap().to_rgb8();
    let got = image::open(&jpeg[0]).unwrap().to_rgb8();
    assert_eq!(got.dimensions(), want.dimensions());
    // Avoid chroma boundaries and compression noise: solid patch interiors.
    for (x, y) in [(16, 48), (48, 48), (80, 48), (112, 48)] {
        let a = got.get_pixel(x, y).0;
        let b = want.get_pixel(x, y).0;
        assert!(
            a.iter().zip(b).all(|(&a, b)| a.abs_diff(b) <= 10),
            "tagged frame at ({x},{y}): PNG-to-JPEG {a:?}, direct JPEG {b:?}"
        );
    }
}

#[test]
fn colored_grayscale_png_frames_keep_sixteen_bit_precision() {
    let Some(ffmpeg) = convt_engines::ffmpeg::ffmpeg_path() else {
        eprintln!("SKIP grayscale precision: FFmpeg missing");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("gray.mkv");
    assert!(
        std::process::Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=gray:s=16x16:d=0.1,format=gray16le",
                "-c:v",
                "ffv1"
            ])
            .arg(&input)
            .status()
            .unwrap()
            .success()
    );
    let plain = convert(&input, "png", None).unwrap();
    let colored = convert(&input, "png", Some(Background::BLACK)).unwrap();
    let before = image::open(&plain[0]).unwrap();
    let after = image::open(&colored[0]).unwrap();
    assert_eq!(before.color(), image::ColorType::L16);
    assert_eq!(after.color(), before.color());
    assert_eq!(after.to_luma16(), before.to_luma16());
}
