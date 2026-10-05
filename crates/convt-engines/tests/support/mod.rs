#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output as ProcessOutput};

use convt_core::{Category, Format, Options};
use image::{DynamicImage, ImageFormat, ImageReader};
use serde_json::Value;

pub const MARKER: &str = "CONVT_MATRIX_7F3A";
pub type Check<T> = Result<T, String>;

pub fn tool(name: &str) -> Option<PathBuf> {
    let env = match name {
        "ffmpeg" => "CONVT_FFMPEG",
        "ffprobe" => "CONVT_FFPROBE",
        "soffice" => "CONVT_SOFFICE",
        _ => "",
    };
    std::env::var_os(env)
        .map(PathBuf::from)
        .or_else(|| which::which(name).ok())
}

pub fn command(cmd: &mut Command) -> Check<ProcessOutput> {
    let out = cmd.output().map_err(|e| format!("{cmd:?}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{cmd:?}: {} {} {}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out)
}

pub fn helper(mode: &str, path: &Path, args: &[&str]) -> Check<ProcessOutput> {
    command(
        Command::new("python3")
            .arg(
                std::env::var_os("CONVT_MATRIX_HELPER")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../scripts/matrix-fixtures.py")
                    }),
            )
            .arg(mode)
            .arg(path)
            .args(args),
    )
}

pub fn pattern() -> image::RgbaImage {
    image::RgbaImage::from_fn(64, 48, |x, y| {
        image::Rgba(match (x < 32, y < 24) {
            (true, true) => [240, 24, 24, 255],
            (false, true) => [24, 224, 24, 255],
            (true, false) => [24, 24, 240, 255],
            (false, false) => [224, 224, 24, 128],
        })
    })
}

#[derive(Clone)]
pub struct Fixture {
    pub path: PathBuf,
    pub format: &'static Format,
    pub alpha: bool,
    pub duration: Option<f64>,
    pub audio: bool,
    pub pages: usize,
    pub reference: Option<DynamicImage>,
}

impl Fixture {
    pub fn make(root: &Path, format: &'static Format) -> Check<Self> {
        let dir = root.join(format.id);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("sample.{}", format.extension()));
        let mut fixture = Self {
            path,
            format,
            alpha: false,
            duration: None,
            audio: false,
            pages: 1,
            reference: None,
        };
        match format.category {
            Category::Image if format.id == "heic" => {
                std::fs::write(dir.join("pattern.rgba"), pattern().as_raw())
                    .map_err(|e| e.to_string())?;
                match helper("heic", &dir, &[]) {
                    Ok(out) => fixture.alpha = out.stdout == b"alpha",
                    Err(e) if e.contains("exit status: 77") => {
                        if let Some(sample) = std::env::var_os("CONVT_TEST_HEIC") {
                            std::fs::copy(sample, &fixture.path).map_err(|e| e.to_string())?;
                            let reference = heif_decode(&fixture.path)?;
                            fixture.alpha = reference.color().has_alpha();
                            fixture.reference = Some(reference);
                        }
                        if fixture.reference.is_none() {
                            return Err(format!(
                                "HEIC encoder unavailable; set CONVT_TEST_HEIC: {e}"
                            ));
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
            Category::Image if format.id == "avif" => {
                std::fs::write(dir.join("pattern.rgba"), pattern().as_raw())
                    .map_err(|e| e.to_string())?;
                match helper("avif", &dir, &[]) {
                    Ok(_) => fixture.alpha = true,
                    Err(e) if e.contains("exit status: 77") => {
                        // Fall back to FFmpeg when libheif lacks an AV1 encoder.
                        let png = dir.join("source.png");
                        pattern().save(&png).map_err(|e| e.to_string())?;
                        command(
                            Command::new(tool("ffmpeg").ok_or("ffmpeg missing")?)
                                .args(["-v", "error", "-y", "-i"])
                                .arg(png)
                                .args([
                                    "-c:v",
                                    "libaom-av1",
                                    "-still-picture",
                                    "1",
                                    "-crf",
                                    "10",
                                    "-threads",
                                    "1",
                                ])
                                .arg(&fixture.path),
                        )?;
                    }
                    Err(e) => return Err(e),
                }
            }
            Category::Image if format.id == "gif" => {
                let mut encoder = image::codecs::gif::GifEncoder::new(
                    std::fs::File::create(&fixture.path).map_err(|e| e.to_string())?,
                );
                let opaque = DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(pattern()).to_rgb8())
                    .to_rgba8();
                for pixels in [opaque.clone(), image::imageops::flip_horizontal(&opaque)] {
                    encoder
                        .encode_frame(image::Frame::from_parts(
                            pixels,
                            0,
                            0,
                            image::Delay::from_numer_denom_ms(500, 1),
                        ))
                        .map_err(|e| e.to_string())?;
                }
                fixture.duration = Some(1.0);
            }
            Category::Image => {
                let img = DynamicImage::ImageRgba8(pattern());
                let img = match format.id {
                    "jpeg" | "ppm" => DynamicImage::ImageRgb8(img.to_rgb8()),
                    "exr" => DynamicImage::ImageRgba32F(img.to_rgba32f()),
                    _ => img,
                };
                fixture.alpha = !matches!(format.id, "jpeg" | "ppm");
                img.save(&fixture.path).map_err(|e| e.to_string())?;
            }
            Category::Vector => {
                std::fs::write(&fixture.path, r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48"><path fill="#f01818" d="M0 0h32v24H0z"/><path fill="#18e018" d="M32 0h32v24H32z"/><path fill="#1818f0" d="M0 24h32v24H0z"/><path fill="#e0e018" fill-opacity="0.502" d="M32 24h32v24H32z"/></svg>"##).map_err(|e| e.to_string())?;
                fixture.alpha = true;
            }
            Category::Video => {
                let pixels = DynamicImage::ImageRgba8(pattern()).to_rgb8();
                pixels
                    .save(dir.join("frame-1.png"))
                    .map_err(|e| e.to_string())?;
                image::imageops::flip_horizontal(&pixels)
                    .save(dir.join("frame-2.png"))
                    .map_err(|e| e.to_string())?;
                let mut cmd = Command::new(tool("ffmpeg").ok_or("ffmpeg missing")?);
                cmd.args(["-v", "error", "-y", "-framerate", "2", "-i"])
                    .arg(dir.join("frame-%d.png"))
                    .args([
                        "-f",
                        "lavfi",
                        "-i",
                        "sine=frequency=440:sample_rate=48000:duration=1",
                        "-t",
                        "1",
                        "-r",
                        "12",
                    ]);
                match format.id {
                    "webm" => {
                        cmd.args([
                            "-c:v",
                            "libvpx-vp9",
                            "-deadline",
                            "realtime",
                            "-cpu-used",
                            "8",
                            "-c:a",
                            "libopus",
                        ]);
                    }
                    "avi" => {
                        cmd.args(["-c:v", "mpeg4", "-c:a", "libmp3lame"]);
                    }
                    _ => {
                        cmd.args(["-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac"]);
                    }
                }
                command(
                    cmd.args(["-pix_fmt", "yuv420p", "-threads", "1"])
                        .arg(&fixture.path),
                )?;
                fixture.duration = Some(1.0);
                fixture.audio = true;
            }
            Category::Audio => {
                let codec = match format.id {
                    "mp3" => "libmp3lame",
                    "wav" => "pcm_s16le",
                    "flac" => "flac",
                    "aac" | "m4a" => "aac",
                    "ogg" => "libvorbis",
                    "opus" => "libopus",
                    _ => unreachable!(),
                };
                command(
                    Command::new(tool("ffmpeg").ok_or("ffmpeg missing")?)
                        .args([
                            "-v",
                            "error",
                            "-y",
                            "-f",
                            "lavfi",
                            "-i",
                            "sine=frequency=440:sample_rate=48000:duration=1",
                            "-c:a",
                            codec,
                            "-threads",
                            "1",
                        ])
                        .arg(&fixture.path),
                )?;
                fixture.duration = Some(1.0);
                fixture.audio = true;
            }
            Category::Pdf => {
                helper("pdf", &fixture.path, &[])?;
                fixture.pages = 2;
            }
            category => {
                let base = match category {
                    Category::Document => "odt",
                    Category::Presentation => "odp",
                    Category::Spreadsheet => "ods",
                    _ => unreachable!(),
                };
                helper("office-fixture", &dir, &[base, format.extension()])?;
                let reference = dir.join("reference/sample.pdf");
                if reference.exists() {
                    fixture.reference =
                        Some(raw_image(&helper("pdf-pixels", &reference, &[])?.stdout)?);
                }
            }
        }
        Ok(fixture)
    }
}

pub fn raw_image(bytes: &[u8]) -> Check<DynamicImage> {
    if bytes.len() < 8 {
        return Err("validator returned no pixels".into());
    }
    let width = u32::from_le_bytes(bytes[..4].try_into().unwrap());
    let height = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    image::RgbaImage::from_raw(width, height, bytes[8..].to_vec())
        .map(DynamicImage::ImageRgba8)
        .ok_or("validator returned wrong pixel count".into())
}

pub fn heif_decode(path: &Path) -> Check<DynamicImage> {
    raw_image(&helper("heif-pixels", path, &[])?.stdout)
}

pub fn decode(path: &Path, id: &str) -> Check<DynamicImage> {
    if id == "heic" {
        return heif_decode(path);
    }
    if id == "avif" {
        // FFmpeg 7 exposes HEIF auxiliary alpha as a separate default stream.
        // Select the colour stream explicitly, then validate alpha with libheif.
        let probe = command(
            Command::new(tool("ffprobe").ok_or("ffprobe missing")?)
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "stream=index,pix_fmt",
                    "-of",
                    "json",
                ])
                .arg(path),
        )?;
        let streams: Value = serde_json::from_slice(&probe.stdout).map_err(|e| e.to_string())?;
        let index = streams["streams"]
            .as_array()
            .ok_or("no AVIF streams")?
            .iter()
            .find(|s| {
                s["pix_fmt"]
                    .as_str()
                    .is_some_and(|f| !f.starts_with("gray"))
            })
            .and_then(|s| s["index"].as_u64())
            .ok_or("no AVIF colour stream")?;
        let out = command(
            Command::new(tool("ffmpeg").ok_or("ffmpeg missing for AVIF validation")?)
                .args(["-v", "error", "-i"])
                .arg(path)
                .args(["-map", &format!("0:{index}")])
                .args([
                    "-frames:v",
                    "1",
                    "-threads",
                    "1",
                    "-f",
                    "image2pipe",
                    "-c:v",
                    "png",
                    "-",
                ]),
        )?;
        let mut rgb = image::load_from_memory(&out.stdout)
            .map_err(|e| e.to_string())?
            .to_rgba8();
        let alpha = heif_decode(path)?.to_rgba8();
        if rgb.dimensions() != alpha.dimensions() {
            return Err("AVIF decoders disagree on dimensions".into());
        }
        for (p, a) in rgb.pixels_mut().zip(alpha.pixels()) {
            p.0[3] = a.0[3];
        }
        Ok(DynamicImage::ImageRgba8(rgb))
    } else {
        ImageReader::open(path)
            .map_err(|e| e.to_string())?
            .with_guessed_format()
            .map_err(|e| e.to_string())?
            .decode()
            .map_err(|e| e.to_string())
    }
}

pub fn image_magic(path: &Path, id: &str) -> Check<()> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if id == "tga" {
        if data.len() < 18 || !matches!(data[2], 2 | 10) {
            return Err("wrong TGA header".into());
        }
    } else if matches!(id, "avif" | "heic") {
        if data.get(4..8) != Some(b"ftyp")
            || !data
                .windows(4)
                .take(16)
                .any(|w| w == if id == "heic" { b"heic" } else { b"avif" })
        {
            return Err("wrong AVIF brand".into());
        }
    } else {
        let expected = match id {
            "jpeg" => ImageFormat::Jpeg,
            "png" => ImageFormat::Png,
            "webp" => ImageFormat::WebP,
            "gif" => ImageFormat::Gif,
            "tiff" => ImageFormat::Tiff,
            "bmp" => ImageFormat::Bmp,
            "ico" => ImageFormat::Ico,
            "ppm" => ImageFormat::Pnm,
            "qoi" => ImageFormat::Qoi,
            "exr" => ImageFormat::OpenExr,
            _ => return Err(format!("unvalidated image format {id}")),
        };
        let actual = image::guess_format(&data).map_err(|e| e.to_string())?;
        if actual != expected {
            return Err(format!("wrong magic: {actual:?}, expected {expected:?}"));
        }
    }
    Ok(())
}

pub fn check_pattern(img: &DynamicImage, alpha: bool) -> Check<()> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    for (x, y, expected) in [
        (w / 4, h / 4, [240, 24, 24]),
        (3 * w / 4, h / 4, [24, 224, 24]),
        (w / 4, 3 * h / 4, [24, 24, 240]),
        (3 * w / 4, 3 * h / 4, [224, 224, 24]),
    ] {
        let p = rgba.get_pixel(x, y).0;
        if p[..3]
            .iter()
            .zip(expected)
            .any(|(&a, b)| a.abs_diff(b) > 35)
        {
            return Err(format!("pattern at {x},{y}: {p:?} expected {expected:?}"));
        }
    }
    if alpha && rgba.get_pixel(3 * w / 4, 3 * h / 4).0[3].abs_diff(128) > 5 {
        return Err(format!(
            "alpha lost: {:?}",
            rgba.get_pixel(3 * w / 4, 3 * h / 4)
        ));
    }
    if rgba.get_pixel(w / 4, h / 4).0[3] < 245 {
        return Err("opaque quadrant became transparent".into());
    }
    Ok(())
}

pub fn compare_images(
    actual: &DynamicImage,
    reference: &DynamicImage,
    to: &str,
    options: &Options,
) -> Check<()> {
    let max = options
        .max_size
        .unwrap_or(reference.width().max(reference.height()));
    let max = if to == "ico" { max.min(256) } else { max };
    let reference = if reference.width().max(reference.height()) > max {
        reference.resize(max, max, image::imageops::FilterType::Lanczos3)
    } else {
        reference.clone()
    };
    if actual.width() != reference.width() || actual.height() != reference.height() {
        return Err(format!(
            "dimensions {:?}, expected {:?}",
            (actual.width(), actual.height()),
            (reference.width(), reference.height())
        ));
    }
    let a = actual.to_rgba8();
    let b = reference.to_rgba8();
    let error = a
        .pixels()
        .zip(b.pixels())
        .flat_map(|(a, b)| (0..3).map(move |i| a.0[i].abs_diff(b.0[i]) as f64))
        .sum::<f64>()
        / (a.width() as f64 * a.height() as f64 * 3.0);
    if error > 12.0 {
        return Err(format!(
            "pixel mean absolute error {error:.3}, expected <=12"
        ));
    }
    // Weight foreground separately: blank or unrelated text can look close
    // under a page-wide metric when most of the reference is white paper.
    let foreground: Vec<_> = a
        .pixels()
        .zip(b.pixels())
        .filter(|(_, p)| p.0[..3].iter().any(|&c| c < 200))
        .collect();
    if !foreground.is_empty() {
        let error = foreground
            .iter()
            .flat_map(|(a, b)| (0..3).map(move |i| a.0[i].abs_diff(b.0[i]) as f64))
            .sum::<f64>()
            / (foreground.len() as f64 * 3.0);
        if error > 45.0 {
            return Err(format!("foreground pixel error {error:.3}, expected <=45"));
        }
    }
    if !matches!(to, "jpeg" | "ppm" | "gif") {
        let error = a
            .pixels()
            .zip(b.pixels())
            .map(|(a, b)| a.0[3].abs_diff(b.0[3]) as f64)
            .sum::<f64>()
            / (a.width() as f64 * a.height() as f64);
        if error > 8.0 {
            return Err(format!("alpha pixel error {error:.3}, expected <=8"));
        }
    }
    Ok(())
}

pub fn probe(path: &Path) -> Check<Value> {
    let out = command(
        Command::new(tool("ffprobe").ok_or("ffprobe missing")?)
            .args([
                "-v",
                "error",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
            ])
            .arg(path),
    )?;
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

pub fn check_tone(path: &Path) -> Check<()> {
    let out = command(
        Command::new(tool("ffmpeg").ok_or("ffmpeg missing")?)
            .args(["-v", "error", "-i"])
            .arg(path)
            .args([
                "-vn", "-ss", "0.2", "-t", "0.5", "-ac", "1", "-ar", "8000", "-f", "f32le", "-",
            ]),
    )?;
    let samples: Vec<_> = out
        .stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    if samples.len() < 3000 {
        return Err("audio truncated".into());
    }
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    let crossings = samples
        .windows(2)
        .filter(|s| s[0] <= 0.0 && s[1] > 0.0)
        .count();
    let frequency = crossings as f32 * 8000.0 / samples.len() as f32;
    if !(0.02..0.2).contains(&rms) || (frequency - 440.0).abs() > 8.0 {
        return Err(format!("tone wrong: {frequency} Hz, RMS {rms}"));
    }
    Ok(())
}

pub fn validate(
    fixture: &Fixture,
    to: &'static Format,
    files: &[PathBuf],
    options: &Options,
) -> Check<()> {
    let count = if fixture.format.category == Category::Pdf {
        (0..fixture.pages)
            .filter(|&i| options.pages.is_none_or(|r| r.contains(i)))
            .count()
    } else {
        1
    };
    if files.len() != count {
        return Err(format!("{} artifacts; expected {count}", files.len()));
    }
    for (index, path) in files.iter().enumerate() {
        match to.category {
            Category::Image => {
                image_magic(path, to.id)?;
                let img = decode(path, to.id)?;
                if matches!(
                    fixture.format.category,
                    Category::Pdf
                        | Category::Document
                        | Category::Presentation
                        | Category::Spreadsheet
                ) {
                    if let Some(reference) = &fixture.reference {
                        compare_images(&img, reference, to.id, options)?;
                    }
                    let rgba = img.to_rgba8();
                    let colored = rgba
                        .pixels()
                        .filter(|p| p.0[..3].iter().any(|&c| c < 200))
                        .count();
                    if colored < 10 {
                        return Err("rendered page is blank".into());
                    }
                    if fixture.format.category == Category::Pdf {
                        let dpi = options.dpi.unwrap_or(72);
                        let expected = (144 * dpi / 72, 72 * dpi / 72);
                        if (img.width(), img.height()) != expected {
                            return Err(format!(
                                "PDF render dimensions {:?}; expected {expected:?}",
                                (img.width(), img.height())
                            ));
                        }
                        // The black CONVT block is above the colored page marker.
                        // A renderer that preserves geometry but drops text must fail.
                        let dark = (10 * dpi / 72..30 * dpi / 72)
                            .flat_map(|y| (8 * dpi / 72..96 * dpi / 72).map(move |x| (x, y)))
                            .filter(|&(x, y)| rgba.get_pixel(x, y).0[..3].iter().all(|&c| c < 100))
                            .count();
                        let minimum = (120u64 * dpi as u64 * dpi as u64 / (72 * 72)) as usize;
                        if dark < minimum {
                            return Err(format!(
                                "rendered PDF text missing: {dark} dark pixels, expected at least {minimum}"
                            ));
                        }
                        let page = options.pages.map_or(1, |p| p.first) as usize + index;
                        let pixel = rgba.get_pixel(20 * dpi / 72, 52 * dpi / 72).0;
                        let color = if page == 1 {
                            [255u8, 0, 0]
                        } else {
                            [0, 0, 255]
                        };
                        if pixel[..3]
                            .iter()
                            .zip(color)
                            .any(|(&a, b)| a.abs_diff(b) > 35)
                        {
                            return Err(format!("wrong PDF page {page}: {pixel:?}"));
                        }
                    }
                } else {
                    if let Some(reference) = &fixture.reference {
                        compare_images(&img, reference, to.id, options)?;
                        continue;
                    }
                    let width = if fixture.format.category == Category::Vector {
                        64 * options.dpi.unwrap_or(96) / 96
                    } else {
                        64
                    };
                    let max = options.max_size.unwrap_or(width).min(width);
                    let expected = (max, 48 * max / 64);
                    if (img.width(), img.height()) != expected {
                        return Err(format!(
                            "image dimensions {:?}; expected {expected:?}",
                            (img.width(), img.height())
                        ));
                    }
                    // AVIF/GIF can quantize alpha; GIF is binary, JPEG and PPM drop it.
                    let alpha = fixture.alpha && !matches!(to.id, "jpeg" | "ppm" | "gif");
                    check_pattern(&img, alpha)?;
                }
                if to.id == "gif" && fixture.format.category == Category::Video {
                    validate_media(fixture, to, path, options)?;
                }
            }
            Category::Video | Category::Audio => validate_media(fixture, to, path, options)?,
            Category::Pdf => {
                let data = std::fs::read(path).map_err(|e| e.to_string())?;
                if !data.starts_with(b"%PDF-") {
                    return Err("wrong PDF magic".into());
                }
                let out = helper("pdf-check", path, &[])?;
                let info: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
                if info["pages"] != 1 || !info["text"].as_str().unwrap_or("").contains(MARKER) {
                    return Err(format!("PDF lost pages/text: {info}"));
                }
                if fixture.format.category == Category::Spreadsheet
                    && !["42", "3.5", "KnownCell"]
                        .iter()
                        .all(|s| info["text"].as_str().unwrap_or("").contains(s))
                {
                    return Err(format!("PDF lost spreadsheet values: {info}"));
                }
            }
            Category::Document | Category::Presentation | Category::Spreadsheet => {
                helper("office-check", path, &[&format!("{:?}", to.category)])?;
            }
            _ => return Err(format!("unvalidated target {}", to.id)),
        }
    }
    Ok(())
}

fn validate_media(fixture: &Fixture, to: &Format, path: &Path, options: &Options) -> Check<()> {
    let info = probe(path)?;
    let streams = info["streams"].as_array().ok_or("no streams")?;
    let video = streams.iter().find(|s| s["codec_type"] == "video");
    let audio = streams.iter().find(|s| s["codec_type"] == "audio");
    let has_video = matches!(to.category, Category::Video) || to.id == "gif";
    let has_audio =
        to.category == Category::Audio || (fixture.audio && to.id != "gif" && !options.strip_audio);
    if video.is_some() != has_video || audio.is_some() != has_audio {
        return Err(format!(
            "wrong streams: expected video={has_video}, audio={has_audio}: {streams:?}"
        ));
    }
    let expected_container = match to.id {
        "mp4" | "mov" | "m4a" => "mov",
        "webm" | "mkv" => "matroska",
        "opus" => "ogg",
        id => id,
    };
    let container = info["format"]["format_name"].as_str().unwrap_or("");
    if !container.split(',').any(|s| s == expected_container) {
        return Err(format!(
            "wrong container {container}, expected {expected_container}"
        ));
    }
    if let Some(video) = video {
        let expected_codec = match to.id {
            "mp4" | "mov" | "mkv" => {
                if options.video_codec == Some(convt_core::VideoCodec::Hevc) {
                    "hevc"
                } else {
                    "h264"
                }
            }
            "webm" => "vp9",
            "avi" => "mpeg4",
            "gif" => "gif",
            _ => unreachable!(),
        };
        let frames = command(
            Command::new(tool("ffmpeg").ok_or("ffmpeg missing")?)
                .args(["-v", "error", "-i"])
                .arg(path)
                .args([
                    "-an", "-threads", "1", "-pix_fmt", "rgba", "-f", "rawvideo", "-",
                ]),
        )?;
        let width = video["width"].as_u64().ok_or("missing width")? as u32;
        let height = video["height"].as_u64().ok_or("missing height")? as u32;
        let frame_size = width as usize * height as usize * 4;
        if frame_size == 0
            || frames.stdout.len() < frame_size * 2
            || frames.stdout.len() % frame_size != 0
        {
            return Err("video has missing/truncated frames".into());
        }
        for (name, bytes, flipped) in [
            ("first", &frames.stdout[..frame_size], false),
            (
                "last",
                &frames.stdout[frames.stdout.len() - frame_size..],
                true,
            ),
        ] {
            let img = DynamicImage::ImageRgba8(
                image::RgbaImage::from_raw(width, height, bytes.to_vec())
                    .ok_or("invalid frame dimensions")?,
            );
            check_pattern(&if flipped { img.fliph() } else { img }, false)
                .map_err(|e| format!("{name} video frame: {e}"))?;
        }
        if video["codec_name"] != expected_codec {
            return Err(format!("wrong video codec: {video}"));
        }
        let h = options.video_height.unwrap_or(48).min(48);
        if video["width"] != 64 * h / 48 || video["height"] != h {
            return Err(format!("wrong video dimensions: {video}"));
        }
    }
    if let Some(audio) = audio {
        let expected = match to.id {
            "wav" => "pcm_s16le",
            "flac" => "flac",
            "aac" | "m4a" | "mp4" | "mov" | "mkv" => "aac",
            "mp3" | "avi" => "mp3",
            "ogg" => "vorbis",
            "opus" | "webm" => "opus",
            _ => unreachable!(),
        };
        if audio["codec_name"] != expected {
            return Err(format!("wrong audio codec: {audio}"));
        }
        check_tone(path)?;
    }
    let duration = info["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or("no media duration")?;
    let expected = fixture.duration.ok_or("input has no duration")?;
    // Encoder priming and AVI frame/audio padding are bounded for the 1s clip.
    if (duration - expected).abs() > 0.20 {
        return Err(format!(
            "duration {duration}, expected {expected} +/-0.20 s"
        ));
    }
    Ok(())
}

pub fn missing_validation_tool(from: &Format, to: &Format) -> Option<&'static str> {
    missing_validation_tool_with(from, to, |name| tool(name).is_some(), native_validators)
}

pub fn missing_validation_tool_with<'a>(
    from: &Format,
    to: &Format,
    has_tool: impl Fn(&str) -> bool,
    native: impl FnOnce() -> &'a Value,
) -> Option<&'static str> {
    let office = matches!(
        from.category,
        Category::Document | Category::Presentation | Category::Spreadsheet
    ) || matches!(
        to.category,
        Category::Document | Category::Presentation | Category::Spreadsheet
    );
    let python = office
        || from.id == "pdf"
        || matches!(from.id, "heic" | "avif")
        || matches!(to.id, "heic" | "avif")
        || to.id == "pdf";
    let media = matches!(from.category, Category::Video | Category::Audio)
        || matches!(to.category, Category::Video | Category::Audio)
        || from.id == "gif" && to.id == "gif";
    let missing = [
        (python, "python3"),
        (media || to.id == "avif", "ffmpeg"),
        (media || to.id == "avif", "ffprobe"),
        (office, "soffice"),
    ]
    .into_iter()
    .find_map(|(needed, name)| (needed && !has_tool(name)).then_some(name));
    if missing.is_some() {
        return missing;
    }
    if from.id == "pdf"
        || to.id == "pdf"
        || to.category == Category::Presentation
        || matches!(from.id, "heic" | "avif")
        || matches!(to.id, "heic" | "avif")
    {
        missing_native_validator(from, to, native())
    } else {
        None
    }
}

pub fn native_validators() -> &'static Value {
    static NATIVE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    NATIVE.get_or_init(|| {
        let out =
            helper("capabilities", Path::new("."), &[]).expect("native validator discovery failed");
        serde_json::from_slice(&out.stdout).expect("invalid native validator discovery JSON")
    })
}

pub fn missing_native_validator(
    from: &Format,
    to: &Format,
    native: &Value,
) -> Option<&'static str> {
    if (from.id == "pdf" || to.id == "pdf" || to.category == Category::Presentation)
        && native["pdfium"] != true
    {
        return Some("PDFium validator");
    }
    if (from.id == "avif" || to.id == "avif") && native["libheif"] != true {
        return Some("libheif AV1 validator");
    }
    if (from.id == "heic" || to.id == "heic") && native["hevc"] != true {
        return Some("libheif HEVC validator");
    }
    None
}

pub fn selected(id: &str, env: &str) -> bool {
    std::env::var(env)
        .ok()
        .is_none_or(|v| v.split(',').any(|s| s == id))
}

pub fn category_name(category: Category) -> String {
    format!("{category:?}")
}

pub fn print_summary(
    results: &[(String, String, String, std::time::Duration, Check<()>)],
    skips: &BTreeMap<String, usize>,
) {
    let mut counts: BTreeMap<String, [usize; 3]> = BTreeMap::new();
    for (category, _, _, _, result) in results {
        counts.entry(category.clone()).or_default()[usize::from(result.is_err())] += 1;
    }
    for (category, count) in skips {
        counts.entry(category.clone()).or_default()[2] += count;
    }
    eprintln!("\nInput category       pass  fail  skip");
    for (category, count) in &counts {
        eprintln!("{category:20} {:5} {:5} {:5}", count[0], count[1], count[2]);
    }
    let totals = counts.values().fold([0; 3], |mut a, c| {
        for i in 0..3 {
            a[i] += c[i]
        }
        a
    });
    eprintln!(
        "{:<20} {:5} {:5} {:5}",
        "TOTAL", totals[0], totals[1], totals[2]
    );
    let mut slow: Vec<_> = results.iter().collect();
    slow.sort_by_key(|r| std::cmp::Reverse(r.3));
    eprintln!("Slowest cases:");
    for (_, from, to, time, _) in slow.into_iter().take(10) {
        eprintln!("  {from} -> {to}: {:.3}s", time.as_secs_f64());
    }
    for (_, from, to, _, result) in results {
        if let Err(e) = result {
            eprintln!("FAIL {from} -> {to}: {e}");
        }
    }
}
