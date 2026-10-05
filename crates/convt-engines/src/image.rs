use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use convt_core::{Ctx, Engine, Error, Options, Result, Step};
use image::codecs::avif::AvifEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader};

pub(crate) const INPUTS: &[&str] = &[
    "png", "jpeg", "webp", "gif", "tiff", "bmp", "ico", "tga", "ppm", "qoi", "exr",
];
const OUTPUTS: &[&str] = &[
    "png", "jpeg", "webp", "gif", "tiff", "bmp", "ico", "tga", "ppm", "qoi", "exr", "avif",
];

/// Pure-Rust image conversion with the `image` crate. It needs no system
/// libraries, so it works everywhere. libvips and the OS image APIs (ImageIO,
/// WIC) will take over HEIC and the heavy lifting with a higher priority.
pub struct ImageEngine;

fn failed(e: impl std::fmt::Display) -> Error {
    Error::EngineFailed {
        engine: "image",
        message: e.to_string(),
    }
}

impl Engine for ImageEngine {
    fn id(&self) -> &'static str {
        "image"
    }

    fn steps(&self) -> Vec<Step> {
        crate::steps(INPUTS, OUTPUTS).collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let img = ImageReader::open(input)?
            .with_guessed_format()?
            .decode()
            .map_err(failed)?;
        ctx.check()?;
        ctx.progress(0.5);
        let output = ctx.artifact(out_dir, 0);
        encode(
            fit(img, ctx.options.max_size),
            ctx.step.to.id,
            ctx.options,
            &output,
        )?;
        Ok(vec![output])
    }
}

/// Scales `img` down so its longest edge is at most `max`. Never scales up.
pub(crate) fn fit(img: DynamicImage, max: Option<u32>) -> DynamicImage {
    match max {
        Some(m) if img.width() > m || img.height() > m => img.resize(m, m, FilterType::Lanczos3),
        _ => img,
    }
}

/// Encodes `img` as `to` into `output`. The writer is flushed explicitly: dropping a `BufWriter` swallows write errors, which would let a
/// truncated file through to publishing.
pub(crate) fn encode(img: DynamicImage, to: &str, options: &Options, output: &Path) -> Result<()> {
    // These encoders accept only 8-bit pixels. ICO requires RGBA PNG data,
    // even when its source is RGB. Keep higher precision for PNG and TIFF.
    let img = match to {
        "ico" | "gif" | "qoi" | "avif" | "webp" => DynamicImage::ImageRgba8(img.to_rgba8()),
        "png"
            if matches!(
                img,
                DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_)
            ) =>
        {
            DynamicImage::ImageRgba16(img.to_rgba16())
        }
        _ => img,
    };
    let quality = options.quality;
    let mut w = BufWriter::new(File::create(output)?);
    match to {
        "jpeg" => DynamicImage::ImageRgb8(img.to_rgb8())
            .write_with_encoder(JpegEncoder::new_with_quality(&mut w, quality.unwrap_or(90))),
        "avif" => img.write_with_encoder(AvifEncoder::new_with_speed_quality(
            &mut w,
            4,
            quality.unwrap_or(80),
        )),
        "ico" if img.width() > 256 || img.height() > 256 => {
            img.thumbnail(256, 256).write_to(&mut w, ImageFormat::Ico)
        }
        "exr" => {
            DynamicImage::ImageRgba32F(img.to_rgba32f()).write_to(&mut w, ImageFormat::OpenExr)
        }
        "ppm" => DynamicImage::ImageRgb8(img.to_rgb8()).write_to(&mut w, ImageFormat::Pnm),
        id => {
            let format = ImageFormat::from_extension(id)
                .ok_or_else(|| failed(format!("no encoder for {id}")))?;
            img.write_to(&mut w, format)
        }
    }
    .map_err(failed)?;
    w.into_inner().map_err(|e| Error::Io(e.into_error()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use convt_core::{Cancel, format_by_id};

    fn run(src: &Path, to: &str, options: &Options) -> image::DynamicImage {
        let out = tempfile::tempdir().unwrap();
        let step = Step {
            from: format_by_id("png").unwrap(),
            to: format_by_id(to).unwrap(),
        };
        let cancel = Cancel::new();
        let ctx = Ctx::new(step, options, &|_| {}, &cancel);
        let files = ImageEngine.convert(&ctx, src, out.path()).unwrap();
        assert_eq!(files.len(), 1);
        image::open(&files[0]).unwrap()
    }

    #[test]
    fn png_to_other_formats() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.png");
        image::RgbaImage::from_pixel(64, 48, image::Rgba([20, 160, 96, 128]))
            .save(&src)
            .unwrap();
        for to in ["jpeg", "webp", "ico", "bmp", "qoi"] {
            let back = run(&src, to, &Options::default());
            assert_eq!((back.width(), back.height()), (64, 48), "{to}");
        }
        let small = Options {
            max_size: Some(32),
            ..Options::default()
        };
        let back = run(&src, "png", &small);
        assert_eq!((back.width(), back.height()), (32, 24));
    }

    #[test]
    fn encoder_adapts_float_and_rgb_without_losing_supported_precision() {
        let dir = tempfile::tempdir().unwrap();
        let rgb = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            64,
            48,
            image::Rgb([240, 24, 24]),
        ));
        let ico = dir.path().join("rgb.ico");
        encode(rgb, "ico", &Options::default(), &ico).unwrap();
        assert_eq!(
            image::open(ico).unwrap().to_rgba8().get_pixel(0, 0).0,
            [240, 24, 24, 255]
        );
        let float = DynamicImage::ImageRgba32F(image::Rgba32FImage::from_pixel(
            8,
            8,
            image::Rgba([0.2, 0.7, 0.4, 0.5]),
        ));
        for to in ["png", "gif", "avif", "qoi", "ico"] {
            encode(
                float.clone(),
                to,
                &Options::default(),
                &dir.path().join(format!("float.{to}")),
            )
            .unwrap();
        }
        let img = DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(
            8,
            8,
            image::Rgba([12345u16, 45678, 23456, 65535]),
        ));
        for to in ["png", "tiff", "exr"] {
            let out = dir.path().join(format!("precise.{to}"));
            encode(img.clone(), to, &Options::default(), &out).unwrap();
            let decoded = image::open(out).unwrap();
            assert_eq!(
                decoded.color(),
                if to == "exr" {
                    image::ColorType::Rgba32F
                } else {
                    image::ColorType::Rgba16
                }
            );
            assert_eq!(decoded.to_rgba16(), img.to_rgba16());
        }
    }

    #[test]
    fn quality_changes_jpeg_size() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("noise.png");
        image::RgbImage::from_fn(128, 128, |x, y| {
            image::Rgb([((x * 7) ^ (y * 13)) as u8, (x * y) as u8, (x + y * 3) as u8])
        })
        .save(&src)
        .unwrap();
        let size = |q| {
            let out = tempfile::tempdir().unwrap();
            let o = Options {
                quality: Some(q),
                ..Options::default()
            };
            let step = Step {
                from: format_by_id("png").unwrap(),
                to: format_by_id("jpeg").unwrap(),
            };
            let cancel = Cancel::new();
            let files = ImageEngine
                .convert(&Ctx::new(step, &o, &|_| {}, &cancel), &src, out.path())
                .unwrap();
            std::fs::metadata(&files[0]).unwrap().len()
        };
        assert!(size(20) < size(95));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn write_errors_are_reported() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::new(8, 8));
        for to in ["jpeg", "png", "webp"] {
            let r = encode(img.clone(), to, &Options::default(), Path::new("/dev/full"));
            assert!(r.is_err(), "{to}");
        }
    }
}
