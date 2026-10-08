use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use convt_core::{Background, Ctx, Engine, Error, Options, Result, Step, format_by_id};
use image::codecs::avif::AvifEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat};

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

pub(crate) fn failed(e: impl std::fmt::Display) -> Error {
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
        let img = crate::orientation::open_oriented(input)?;
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

/// Applies `options.background` for output in `to`: a chosen color flattens
/// transparency onto it, `transparent` keeps it, and with nothing chosen a
/// format that can't store transparency (JPEG, PPM) gets white. Without
/// this, dropping the alpha channel left fully transparent pixels black.
pub(crate) fn apply_background(
    img: DynamicImage,
    to: &str,
    options: &Options,
) -> Result<DynamicImage> {
    let keeps = format_by_id(to).is_some_and(|f| f.keeps_transparency());
    match (options.background, keeps) {
        (Some(Background::Color(color)), _) => Ok(flatten(img, color)),
        (Some(Background::Transparent), true) | (None, true) => Ok(img),
        (Some(Background::Transparent), false) => Err(Error::InvalidOption(no_transparency(to))),
        (_, false) => Ok(flatten(img, [255, 255, 255])),
    }
}

/// Applies a chosen background color to the PNG at `path` in place, for
/// engines that get PNG from an outside tool (FFmpeg, macOS `sips`). Anything
/// but a color leaves the file alone, since PNG keeps transparency.
pub(crate) fn background_png(path: &Path, options: &Options) -> Result<()> {
    if !matches!(options.background, Some(Background::Color(_))) {
        return Ok(());
    }
    let reader = png::Decoder::new(std::io::BufReader::new(File::open(path)?))
        .read_info()
        .map_err(failed)?;
    let source = reader.info();
    let img = apply_background(crate::orientation::open_oriented(path)?, "png", options)?;
    let mut info = png::Info::with_size(img.width(), img.height());
    info.pixel_dims = source.pixel_dims;
    info.source_gamma = source.gamma();
    info.source_chromaticities = source.chromaticities();
    info.srgb = source.srgb;
    info.icc_profile = source.icc_profile.clone();
    let bytes = match img {
        DynamicImage::ImageLuma16(gray) => {
            info.color_type = png::ColorType::Grayscale;
            info.bit_depth = png::BitDepth::Sixteen;
            gray.into_raw()
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect()
        }
        DynamicImage::ImageLuma8(gray) => {
            info.color_type = png::ColorType::Grayscale;
            info.bit_depth = png::BitDepth::Eight;
            gray.into_raw()
        }
        DynamicImage::ImageRgb16(rgb) => {
            info.color_type = png::ColorType::Rgb;
            info.bit_depth = png::BitDepth::Sixteen;
            rgb.into_raw()
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect()
        }
        _ => {
            info.color_type = png::ColorType::Rgb;
            info.bit_depth = png::BitDepth::Eight;
            img.to_rgb8().into_raw()
        }
    };
    let mut out = BufWriter::new(File::create(path)?);
    let mut writer = png::Encoder::with_info(&mut out, info)
        .map_err(failed)?
        .write_header()
        .map_err(failed)?;
    // png 0.18 decodes cICP, but its encoder does not emit it from Info.
    if let Some(cicp) = source.coding_independent_code_points {
        writer
            .write_chunk(
                png::chunk::cICP,
                &[
                    cicp.color_primaries,
                    cicp.transfer_function,
                    cicp.matrix_coefficients,
                    u8::from(cicp.is_video_full_range_image),
                ],
            )
            .map_err(failed)?;
    }
    writer.write_image_data(&bytes).map_err(failed)?;
    writer.finish().map_err(failed)?;
    std::io::Write::flush(&mut out)?;
    Ok(())
}

/// The error for asking a format without transparency to keep it.
pub(crate) fn no_transparency(to: &str) -> String {
    let name = format_by_id(to).map_or(to, |f| f.name);
    format!("{name} can't store transparency; choose a background color such as white or black")
}

/// Composites `img` over an opaque `color`. Images without an alpha channel
/// pass through untouched; 16-bit and float images keep their precision.
pub(crate) fn flatten(img: DynamicImage, color: [u8; 3]) -> DynamicImage {
    if !img.color().has_alpha() {
        return img;
    }
    match img {
        DynamicImage::ImageLumaA8(_) | DynamicImage::ImageRgba8(_) => {
            let mut rgba = img.to_rgba8();
            let out = image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
                let p = rgba.get_pixel_mut(x, y).0;
                let a = p[3] as u32;
                image::Rgb(std::array::from_fn(|i| {
                    ((p[i] as u32 * a + color[i] as u32 * (255 - a) + 127) / 255) as u8
                }))
            });
            DynamicImage::ImageRgb8(out)
        }
        DynamicImage::ImageLumaA16(_) | DynamicImage::ImageRgba16(_) => {
            let rgba = img.to_rgba16();
            let bg = color.map(|c| c as u32 * 257);
            DynamicImage::ImageRgb16(image::ImageBuffer::from_fn(
                rgba.width(),
                rgba.height(),
                |x, y| {
                    let p = rgba.get_pixel(x, y).0;
                    let a = p[3] as u32;
                    image::Rgb(std::array::from_fn(|i| {
                        ((p[i] as u32 * a + bg[i] * (65535 - a) + 32767) / 65535) as u16
                    }))
                },
            ))
        }
        _ => {
            let rgba = img.to_rgba32f();
            let bg = color.map(|c| c as f32 / 255.0);
            DynamicImage::ImageRgb32F(image::ImageBuffer::from_fn(
                rgba.width(),
                rgba.height(),
                |x, y| {
                    let p = rgba.get_pixel(x, y).0;
                    let a = p[3].clamp(0.0, 1.0);
                    image::Rgb(std::array::from_fn(|i| p[i] * a + bg[i] * (1.0 - a)))
                },
            ))
        }
    }
}

/// Encodes `img` as `to` into `output`, after [`apply_background`]. The writer is flushed explicitly: dropping a `BufWriter` swallows write errors, which would let a
/// truncated file through to publishing.
pub(crate) fn encode(img: DynamicImage, to: &str, options: &Options, output: &Path) -> Result<()> {
    encode_with_pixel_aspect(img, to, options, output, None)
}

pub(crate) fn encode_with_pixel_aspect(
    img: DynamicImage,
    to: &str,
    options: &Options,
    output: &Path,
    pixel_aspect: Option<(u16, u16)>,
) -> Result<()> {
    let img = apply_background(img, to, options)?;
    // These encoders accept only 8-bit pixels. ICO requires RGBA PNG data,
    // even when its source is RGB. Keep higher precision for PNG and TIFF.
    let img = match to {
        "ico" | "gif" | "qoi" | "avif" | "webp" => DynamicImage::ImageRgba8(img.to_rgba8()),
        // The BMP encoder writes RGBA alpha but drops gray-alpha's.
        "bmp" if img.color().has_alpha() => DynamicImage::ImageRgba8(img.to_rgba8()),
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
        "jpeg" => {
            let mut encoder = JpegEncoder::new_with_quality(&mut w, quality.unwrap_or(90));
            if let Some((num, den)) = pixel_aspect {
                encoder.set_pixel_density(image::codecs::jpeg::PixelDensity {
                    density: (num, den),
                    unit: image::codecs::jpeg::PixelDensityUnit::PixelAspectRatio,
                });
            }
            DynamicImage::ImageRgb8(img.to_rgb8()).write_with_encoder(encoder)
        }
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
    fn background_png_flattens_in_place_only_for_a_color() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 0]))
            .save(&png)
            .unwrap();
        background_png(&png, &Options::default()).unwrap();
        assert_eq!(
            image::open(&png).unwrap().to_rgba8().get_pixel(0, 0).0[3],
            0
        );
        let white = Options {
            background: Some(Background::WHITE),
            ..Options::default()
        };
        background_png(&png, &white).unwrap();
        assert_eq!(
            image::open(&png).unwrap().to_rgba8().get_pixel(0, 0).0,
            [255; 4]
        );
    }

    #[test]
    fn gray_alpha_keeps_transparency_in_bmp() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("a.bmp");
        let gray = DynamicImage::ImageLumaA8(image::GrayAlphaImage::from_pixel(
            2,
            2,
            image::LumaA([0, 0]),
        ));
        encode(gray, "bmp", &Options::default(), &out).unwrap();
        assert_eq!(
            image::open(&out).unwrap().to_rgba8().get_pixel(0, 0).0[3],
            0
        );
    }

    #[test]
    fn flattening_keeps_precision_and_blends_by_alpha() {
        let half = image::Rgba([65535u16, 0, 0, 32768]);
        let deep = DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(2, 2, half));
        let flat = flatten(deep, [255, 255, 255]);
        let DynamicImage::ImageRgb16(px) = &flat else {
            panic!("16-bit input stayed 16-bit, got {:?}", flat.color());
        };
        let [r, g, b] = px.get_pixel(0, 0).0;
        assert_eq!(r, 65535);
        assert!(g.abs_diff(32767) <= 1 && g == b, "{g} {b}");

        // Opaque images pass through untouched.
        let opaque = DynamicImage::ImageRgb8(image::RgbImage::new(1, 1));
        assert!(matches!(
            flatten(opaque, [9, 9, 9]),
            DynamicImage::ImageRgb8(_)
        ));
    }

    #[test]
    fn jpeg_exif_orientation_is_baked_into_pixels() {
        use crate::orientation::{decoder_orientation, jpeg_with_orientation};
        use image::metadata::Orientation;

        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("raw.jpeg");
        image::RgbImage::from_fn(32, 24, |x, y| {
            image::Rgb(if x < 16 {
                if y < 12 { [240, 24, 24] } else { [24, 24, 240] }
            } else if y < 12 {
                [24, 224, 24]
            } else {
                [224, 224, 24]
            })
        })
        .save(&raw)
        .unwrap();
        let jpeg = std::fs::read(&raw).unwrap();
        let src = dir.path().join("oriented.jpeg");
        std::fs::write(&src, jpeg_with_orientation(&jpeg, 6).unwrap()).unwrap();
        let webp = run(&src, "webp", &Options::default());
        assert_eq!((webp.width(), webp.height()), (24, 32));
        let px = webp.to_rgb8();
        assert!(
            px.get_pixel(18, 8).0[0] > 200,
            "red should be at top-right after 90° CW"
        );
        assert!(
            px.get_pixel(6, 8).0[2] > 180,
            "blue (was bottom-left) should be at top-left, got {:?}",
            px.get_pixel(6, 8).0
        );
        let out = dir.path().join("check.webp");
        webp.save(&out).unwrap();
        assert_eq!(
            decoder_orientation(&out).unwrap(),
            Orientation::NoTransforms
        );
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
