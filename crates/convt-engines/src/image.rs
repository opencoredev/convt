use std::fs::File;
use std::io::{BufWriter, Write};
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
/// Formats re-encoded to themselves, to compress, resize or strip them.
/// GIF is left to FFmpeg, which keeps the animation this engine would drop.
const SELF: &[&str] = &["png", "jpeg", "webp", "tiff"];

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
        crate::steps(INPUTS, OUTPUTS)
            .chain(SELF.iter().map(|id| {
                let f = format_by_id(id).expect("known format");
                Step { from: f, to: f }
            }))
            .collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        // This engine keeps one frame. Converting to another format says
        // so by the format, but a "compressed" copy must not lose frames.
        if ctx.step.from == ctx.step.to && has_more_frames(input, ctx.step.from.id) {
            return Err(Error::InvalidOption(format!(
                "this {} has more than one frame or page, and compressing it would keep only the first",
                ctx.step.from.name
            )));
        }
        let img = crate::orientation::open_oriented(input)?;
        ctx.check()?;
        ctx.progress(0.5);
        let output = ctx.artifact(out_dir, 0);
        encode_with_pixel_aspect(
            fit(img, ctx.options.max_size),
            ctx.step.to.id,
            ctx.options,
            &output,
            None,
            &|| ctx.check(),
        )?;
        Ok(vec![output])
    }
}

/// Whether `input` holds more than one frame (APNG, animated WebP) or page
/// (TIFF). Unreadable files say no and fail later with a decode error.
fn has_more_frames(input: &Path, format: &str) -> bool {
    let Ok(file) = File::open(input) else {
        return false;
    };
    let reader = std::io::BufReader::new(file);
    match format {
        "png" => png::Decoder::new(reader).read_info().is_ok_and(|r| {
            r.info()
                .animation_control()
                .is_some_and(|a| a.num_frames > 1)
        }),
        "webp" => image::codecs::webp::WebPDecoder::new(reader).is_ok_and(|d| d.has_animation()),
        "tiff" => tiff_pages(reader).is_some_and(|n| n > 1),
        _ => false,
    }
}

/// Counts a TIFF's pages (IFDs), stopping at the second.
fn tiff_pages(mut r: impl std::io::Read + std::io::Seek) -> Option<u32> {
    use std::io::SeekFrom;
    let mut head = [0u8; 8];
    r.read_exact(&mut head).ok()?;
    let big = match &head[..2] {
        b"II" => false,
        b"MM" => true,
        _ => return None,
    };
    let u16_at = |b: [u8; 2]| {
        if big {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        }
    };
    let u32_at = |b: [u8; 4]| {
        if big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        }
    };
    // Classic TIFF only; BigTIFF (43) is rare enough to treat as one page.
    if u16_at([head[2], head[3]]) != 42 {
        return None;
    }
    let first = u32_at([head[4], head[5], head[6], head[7]]);
    r.seek(SeekFrom::Start(u64::from(first))).ok()?;
    let mut count = [0u8; 2];
    r.read_exact(&mut count).ok()?;
    let entries = u64::from(u16_at(count));
    r.seek(SeekFrom::Current((entries * 12) as i64)).ok()?;
    let mut next = [0u8; 4];
    r.read_exact(&mut next).ok()?;
    Some(if u32_at(next) == 0 { 1 } else { 2 })
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

/// Applies the options to the PNG at `path` in place, for engines that get
/// PNG from an outside tool (FFmpeg, macOS `sips`, resvg): a chosen
/// background color, `strip_metadata` (the rewrite keeps only the color
/// tags, dropping EXIF and text chunks) and `target_size` (the image shrinks
/// until the file fits, color tags and pixel ratio included). With none of
/// them the file is left alone.
pub(crate) fn background_png(path: &Path, options: &Options) -> Result<()> {
    let target = options
        .target_size
        .filter(|&t| std::fs::metadata(path).is_ok_and(|m| m.len() > t));
    if !matches!(options.background, Some(Background::Color(_)))
        && !options.strip_metadata
        && target.is_none()
    {
        return Ok(());
    }
    let reader = png::Decoder::new(std::io::BufReader::new(File::open(path)?))
        .read_info()
        .map_err(failed)?;
    let source = reader.info().clone();
    drop(reader);
    let img = apply_background(crate::orientation::open_oriented(path)?, "png", options)?;
    let Some(target) = target else {
        return write_png(&img, &source, path);
    };
    let longest = img.width().max(img.height());
    fit_target(target, "png", None, longest, |_, edge| {
        match edge {
            Some(edge) => write_png(&img.resize(edge, edge, FilterType::Lanczos3), &source, path)?,
            None => write_png(&img, &source, path)?,
        }
        Ok(std::fs::metadata(path)?.len())
    })
}

/// Writes `img` to `path` as PNG with `source`'s color tags and pixel ratio.
fn write_png(img: &DynamicImage, source: &png::Info<'static>, path: &Path) -> Result<()> {
    let mut info = png::Info::with_size(img.width(), img.height());
    info.pixel_dims = source.pixel_dims;
    info.source_gamma = source.gamma();
    info.source_chromaticities = source.chromaticities();
    info.srgb = source.srgb;
    info.icc_profile = source.icc_profile.clone();
    let bytes = match img.clone() {
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
        // Without a color, transparency survives the rewrite.
        DynamicImage::ImageLumaA8(gray) => {
            info.color_type = png::ColorType::GrayscaleAlpha;
            info.bit_depth = png::BitDepth::Eight;
            gray.into_raw()
        }
        DynamicImage::ImageLumaA16(gray) => {
            info.color_type = png::ColorType::GrayscaleAlpha;
            info.bit_depth = png::BitDepth::Sixteen;
            gray.into_raw()
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect()
        }
        DynamicImage::ImageRgba16(rgba) => {
            info.color_type = png::ColorType::Rgba;
            info.bit_depth = png::BitDepth::Sixteen;
            rgba.into_raw()
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect()
        }
        img if img.color().has_alpha() => {
            info.color_type = png::ColorType::Rgba;
            info.bit_depth = png::BitDepth::Eight;
            img.to_rgba8().into_raw()
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

/// Encodes `img` as `to` into `output`, after [`apply_background`]. With
/// `options.target_size` set, quality and then dimensions come down until
/// the file fits.
pub(crate) fn encode(img: DynamicImage, to: &str, options: &Options, output: &Path) -> Result<()> {
    encode_with_pixel_aspect(img, to, options, output, None, &|| Ok(()))
}

/// [`encode`] with a JPEG pixel aspect ratio, and `check` run before each
/// attempt of a size search so a cancelled job stops searching.
pub(crate) fn encode_with_pixel_aspect(
    img: DynamicImage,
    to: &str,
    options: &Options,
    output: &Path,
    pixel_aspect: Option<(u16, u16)>,
    check: &dyn Fn() -> Result<()>,
) -> Result<()> {
    let img = prepare(apply_background(img, to, options)?, to);
    let mut w = BufWriter::new(File::create(output)?);
    match options.target_size {
        // Without a size, stream straight to the file.
        None => encode_into(&img, to, options.quality, pixel_aspect, &mut w)?,
        Some(target) => {
            let longest = img.width().max(img.height());
            let cap = options.quality.unwrap_or(default_quality(to));
            let mut best = Vec::new();
            fit_target(target, to, Some(cap), longest, |quality, edge| {
                check()?;
                // libwebp refuses anything larger, so the search starts there.
                let edge = match edge {
                    None if to == "webp" && longest > WEBP_MAX => Some(WEBP_MAX),
                    edge => edge,
                };
                let scaled = match edge {
                    Some(edge) => &img.resize(edge, edge, FilterType::Lanczos3),
                    None => &img,
                };
                let mut buf = std::io::Cursor::new(Vec::new());
                encode_into(scaled, to, Some(quality), pixel_aspect, &mut buf)?;
                best = buf.into_inner();
                Ok(best.len() as u64)
            })?;
            w.write_all(&best)?;
        }
    }
    // Flush explicitly: dropping a `BufWriter` swallows write errors, which
    // would let a truncated file through to publishing.
    w.into_inner().map_err(|e| Error::Io(e.into_error()))?;
    Ok(())
}

/// The largest width or height libwebp encodes.
const WEBP_MAX: u32 = libwebp_sys::WEBP_MAX_DIMENSION;

/// Converts pixels to what `to`'s encoder accepts. These encoders take only
/// 8-bit pixels, and ICO requires RGBA PNG data even from an RGB source.
/// PNG and TIFF keep higher precision.
fn prepare(img: DynamicImage, to: &str) -> DynamicImage {
    match to {
        "ico" | "gif" | "qoi" | "avif" => DynamicImage::ImageRgba8(img.to_rgba8()),
        // Opaque WebP skips the alpha plane, which saves space.
        "webp" if img.color().has_alpha() => DynamicImage::ImageRgba8(img.to_rgba8()),
        "webp" => DynamicImage::ImageRgb8(img.to_rgb8()),
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
    }
}

/// Lossy quality the size search never goes below: under it, smaller
/// dimensions look better than more compression.
const MIN_QUALITY: u8 = 10;

/// Formats whose size comes down with quality. Every other format is
/// lossless and only gets smaller dimensions.
pub(crate) fn is_lossy(to: &str) -> bool {
    matches!(to, "jpeg" | "webp" | "avif" | "heic")
}

/// Finds an encoding of at most `target` bytes. `attempt(quality, edge)`
/// encodes at `quality` with the longest edge scaled to `edge` (`None`
/// keeps the size) and returns the encoded length; the last call is always
/// one that fit. For lossy formats quality is searched first, from `cap`
/// (the user's quality, if set) down to [`MIN_QUALITY`]; when even that is
/// too big, the image shrinks and the search runs again.
pub(crate) fn fit_target(
    target: u64,
    to: &str,
    cap: Option<u8>,
    longest: u32,
    mut attempt: impl FnMut(u8, Option<u32>) -> Result<u64>,
) -> Result<()> {
    let lossy = is_lossy(to);
    // Lossless formats ignore quality. 100 stays the first try, so WebP
    // that fits losslessly stays lossless.
    let cap = if lossy { cap.unwrap_or(90) } else { 100 };
    let floor = cap.min(MIN_QUALITY);
    let mut edge = None;
    for _ in 0..12 {
        let size = attempt(cap, edge)?;
        if size <= target {
            return Ok(());
        }
        let mut smallest = size;
        if lossy && floor < cap {
            let at_floor = attempt(floor, edge)?;
            if at_floor <= target {
                // `lo` fits and `hi` doesn't. Narrow to within two steps,
                // then make sure the last encoding is the fitting one.
                let (mut lo, mut hi, mut last) = (floor, cap, floor);
                while hi - lo > 2 {
                    let mid = lo + (hi - lo) / 2;
                    last = mid;
                    if attempt(mid, edge)? <= target {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                if last != lo {
                    attempt(lo, edge)?;
                }
                return Ok(());
            }
            smallest = at_floor;
        }
        // Size scales roughly with pixel count, so scale each edge by the
        // square root of the ratio, with a margin.
        let current = edge.unwrap_or(longest);
        if current <= 16 {
            break;
        }
        let ratio = ((target as f64 / smallest as f64).sqrt() * 0.95).min(0.9);
        edge = Some(((current as f64 * ratio) as u32).clamp(16, current - 1));
    }
    Err(Error::EngineFailed {
        engine: "image",
        message: format!(
            "could not make the {} smaller than {}",
            convt_core::format_by_id(to).map_or(to, |f| f.name),
            convt_core::format_size(target)
        ),
    })
}

/// The quality `to` encodes at when none is chosen.
fn default_quality(to: &str) -> u8 {
    match to {
        "webp" | "avif" => 80,
        _ => 90,
    }
}

/// Encodes prepared pixels as `to` into `w`. `quality` is the user's lossy
/// quality, or the format's default when unset.
fn encode_into<W: Write + std::io::Seek>(
    img: &DynamicImage,
    to: &str,
    quality: Option<u8>,
    pixel_aspect: Option<(u16, u16)>,
    mut w: &mut W,
) -> Result<()> {
    match to {
        "jpeg" => {
            let quality = quality.unwrap_or(default_quality(to));
            let mut encoder = JpegEncoder::new_with_quality(&mut w, quality);
            if let Some((num, den)) = pixel_aspect {
                encoder.set_pixel_density(image::codecs::jpeg::PixelDensity {
                    density: (num, den),
                    unit: image::codecs::jpeg::PixelDensityUnit::PixelAspectRatio,
                });
            }
            DynamicImage::ImageRgb8(img.to_rgb8()).write_with_encoder(encoder)
        }
        "webp" => {
            let bytes = webp(img, quality.unwrap_or(default_quality(to)))?;
            return Ok(w.write_all(&bytes)?);
        }
        "avif" => img.write_with_encoder(AvifEncoder::new_with_speed_quality(
            &mut w,
            4,
            quality.unwrap_or(default_quality(to)),
        )),
        "ico" if img.width() > 256 || img.height() > 256 => {
            img.thumbnail(256, 256).write_to(&mut w, ImageFormat::Ico)
        }
        "exr" => {
            DynamicImage::ImageRgba32F(img.to_rgba32f()).write_to(&mut w, ImageFormat::OpenExr)
        }
        "ppm" => DynamicImage::ImageRgb8(img.to_rgb8()).write_to(&mut w, ImageFormat::Pnm),
        "bmp" => {
            let mut bmp = BmpV5Writer::new(&mut w);
            img.write_with_encoder(image::codecs::bmp::BmpEncoder::new(&mut bmp))
                .and_then(|()| bmp.finish().map_err(image::ImageError::IoError))
        }
        id => {
            let format = ImageFormat::from_extension(id)
                .ok_or_else(|| failed(format!("no encoder for {id}")))?;
            img.write_to(&mut w, format)
        }
    }
    .map_err(failed)?;
    Ok(())
}

/// Encodes WebP with libwebp: lossy at `quality`, lossless at 100, the way
/// HEIC and AVIF treat it. The `image` crate only writes lossless WebP,
/// which made a photo several times larger than its JPEG.
fn webp(img: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    use libwebp_sys as webp;
    let (width, height) = (img.width(), img.height());
    if width > WEBP_MAX || height > WEBP_MAX {
        return Err(Error::InvalidOption(format!(
            "WebP images can be at most {WEBP_MAX}x{WEBP_MAX} pixels; set a smaller max size"
        )));
    }
    let (pixels, channels): (&[u8], u32) = match img {
        DynamicImage::ImageRgb8(rgb) => (rgb.as_raw(), 3),
        DynamicImage::ImageRgba8(rgba) => (rgba.as_raw(), 4),
        _ => return Err(failed("WebP needs 8-bit RGB or RGBA pixels")),
    };
    // The dimension check keeps every value well inside c_int.
    let (w, h, stride) = (width as i32, height as i32, (width * channels) as i32);
    let mut out = std::ptr::null_mut();
    // SAFETY: `pixels` holds `height` rows of `stride` bytes, and libwebp
    // only reads them. On success it allocates `len` bytes at `out`, which
    // are copied and then released with WebPFree.
    unsafe {
        let len = match (channels, quality) {
            (3, 100) => webp::WebPEncodeLosslessRGB(pixels.as_ptr(), w, h, stride, &mut out),
            (3, q) => webp::WebPEncodeRGB(pixels.as_ptr(), w, h, stride, f32::from(q), &mut out),
            (_, 100) => webp::WebPEncodeLosslessRGBA(pixels.as_ptr(), w, h, stride, &mut out),
            (_, q) => webp::WebPEncodeRGBA(pixels.as_ptr(), w, h, stride, f32::from(q), &mut out),
        };
        if len == 0 || out.is_null() {
            if !out.is_null() {
                webp::WebPFree(out.cast());
            }
            return Err(failed("libwebp could not encode the image"));
        }
        let bytes = std::slice::from_raw_parts(out, len).to_vec();
        webp::WebPFree(out.cast());
        Ok(bytes)
    }
}

const BMP_V4: u32 = 108;
const BMP_V5: u32 = 124;
const BMP_V4_END: usize = 14 + BMP_V4 as usize;

/// Passes BMP output through, rewriting a BITMAPV4HEADER (what the encoder
/// writes for transparency) as a BITMAPV5HEADER. Apple's ImageIO refuses
/// every V4 file, so Preview and Finder couldn't open an RGBA BMP; V5 keeps
/// the same alpha mask. Only the headers are held back, so pixels stream.
struct BmpV5Writer<W: Write> {
    inner: W,
    /// The bytes held back until the headers are complete; `None` after.
    head: Option<Vec<u8>>,
}

impl<W: Write> BmpV5Writer<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            head: Some(Vec::with_capacity(BMP_V4_END)),
        }
    }

    /// Writes a file shorter than the V4 headers unchanged.
    fn finish(mut self) -> std::io::Result<()> {
        if let Some(head) = self.head.take() {
            self.inner.write_all(&head)?;
        }
        self.inner.flush()
    }
}

impl<W: Write> Write for BmpV5Writer<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let Some(head) = &mut self.head else {
            return self.inner.write(buf);
        };
        let n = buf.len().min(BMP_V4_END - head.len());
        head.extend_from_slice(&buf[..n]);
        if head.len() == BMP_V4_END {
            let head = self.head.take().unwrap_or_default();
            self.inner.write_all(&bmp_v5_header(head))?;
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn bmp_v5_header(mut head: Vec<u8>) -> Vec<u8> {
    let field = |b: &[u8], at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    if field(&head, 14) != BMP_V4 {
        return head;
    }
    let grow = BMP_V5 - BMP_V4;
    let file_size = field(&head, 2) + grow;
    let pixel_offset = field(&head, 10) + grow;
    head[2..6].copy_from_slice(&file_size.to_le_bytes());
    head[10..14].copy_from_slice(&pixel_offset.to_le_bytes());
    head[14..18].copy_from_slice(&BMP_V5.to_le_bytes());
    // bV5Intent LCS_GM_IMAGES, then no profile data, profile size or reserved.
    head.extend_from_slice(&4u32.to_le_bytes());
    head.extend_from_slice(&[0; 12]);
    head
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
    fn png_rewrites_keep_alpha_and_depth_and_meet_a_size() {
        let dir = tempfile::tempdir().unwrap();
        let strip = Options {
            strip_metadata: true,
            ..Options::default()
        };
        let rgba = dir.path().join("rgba.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([9, 8, 7, 0]))
            .save(&rgba)
            .unwrap();
        background_png(&rgba, &strip).unwrap();
        let back = image::open(&rgba).unwrap();
        assert_eq!(back.color(), image::ColorType::Rgba8);
        assert_eq!(back.to_rgba8().get_pixel(1, 1).0[3], 0);

        let deep = dir.path().join("deep.png");
        let px = image::Rgba([1000u16, 2000, 3000, 4000]);
        DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(4, 4, px))
            .save(&deep)
            .unwrap();
        background_png(&deep, &strip).unwrap();
        let back = image::open(&deep).unwrap();
        assert_eq!(back.to_rgba16().get_pixel(0, 0), &px);
        let gray = dir.path().join("gray.png");
        image::GrayAlphaImage::from_pixel(4, 4, image::LumaA([50, 0]))
            .save(&gray)
            .unwrap();
        background_png(&gray, &strip).unwrap();
        assert_eq!(image::open(&gray).unwrap().color(), image::ColorType::La8);

        let big = dir.path().join("big.png");
        photo(400, 300).save(&big).unwrap();
        let sized = Options {
            target_size: Some(40_000),
            ..Options::default()
        };
        background_png(&big, &sized).unwrap();
        assert!(std::fs::metadata(&big).unwrap().len() <= 40_000);
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
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(bytes[14..18], 124u32.to_le_bytes(), "BITMAPV5HEADER");
        assert_eq!(bytes[2..6], (bytes.len() as u32).to_le_bytes());
    }

    #[test]
    fn bmp_header_rewrite_survives_one_byte_writes() {
        let rgba = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            3,
            2,
            image::Rgba([10, 20, 30, 128]),
        ));
        let mut plain = Vec::new();
        rgba.write_with_encoder(image::codecs::bmp::BmpEncoder::new(&mut plain))
            .unwrap();
        let mut out = Vec::new();
        let mut writer = BmpV5Writer::new(&mut out);
        for byte in &plain {
            writer.write_all(std::slice::from_ref(byte)).unwrap();
        }
        writer.finish().unwrap();
        assert_eq!(out.len(), plain.len() + 16);
        assert_eq!(out[14..18], 124u32.to_le_bytes());
        assert_eq!(out[out.len() - 24..], plain[plain.len() - 24..]);
        let back = image::load_from_memory_with_format(&out, ImageFormat::Bmp).unwrap();
        assert_eq!(back.to_rgba8(), rgba.to_rgba8());
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

    /// A photo-like image: smooth gradients plus noise, which compresses
    /// the way camera output does.
    fn photo(w: u32, h: u32) -> DynamicImage {
        let mut seed = 0x2545_f491_u32;
        DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let n = seed % 48;
            image::Rgb([
                ((x * 255 / w) + n).min(255) as u8,
                ((y * 255 / h) + n).min(255) as u8,
                (((x + y) * 127 / (w + h)) + n).min(255) as u8,
            ])
        }))
    }

    fn encoded_len(img: &DynamicImage, to: &str, options: &Options) -> u64 {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join(format!("out.{to}"));
        encode(img.clone(), to, options, &out).unwrap();
        std::fs::metadata(&out).unwrap().len()
    }

    #[test]
    fn webp_is_lossy_and_follows_quality() {
        let img = photo(256, 192);
        let at = |q| {
            encoded_len(
                &img,
                "webp",
                &Options {
                    quality: Some(q),
                    ..Options::default()
                },
            )
        };
        let (q50, q95, q100) = (at(50), at(95), at(100));
        assert!(q50 * 2 < q95, "q50 {q50} vs q95 {q95}");
        assert!(q95 < q100, "q95 {q95} vs lossless {q100}");
        // The default is lossy too, well under lossless.
        let default = encoded_len(&img, "webp", &Options::default());
        assert!(default * 2 < q100, "default {default} vs lossless {q100}");

        // Lossless keeps every pixel; lossy output still decodes at full size
        // and keeps transparency.
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("a.webp");
        let lossless = Options {
            quality: Some(100),
            ..Options::default()
        };
        encode(img.clone(), "webp", &lossless, &out).unwrap();
        assert_eq!(image::open(&out).unwrap().to_rgb8(), img.to_rgb8());
        let clear = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            32,
            32,
            image::Rgba([10, 20, 30, 0]),
        ));
        encode(clear, "webp", &Options::default(), &out).unwrap();
        let back = image::open(&out).unwrap();
        assert_eq!((back.width(), back.height()), (32, 32));
        assert_eq!(back.to_rgba8().get_pixel(5, 5).0[3], 0);
    }

    #[test]
    fn target_size_fits_lossy_and_lossless_output() {
        let img = photo(512, 384);
        for (to, target) in [
            ("jpeg", 20_000),
            ("webp", 15_000),
            ("png", 60_000),
            ("jpeg", 10_000),
        ] {
            let unbounded = encoded_len(&img, to, &Options::default());
            assert!(unbounded > target, "{to}: {unbounded} already fits");
            let options = Options {
                target_size: Some(target),
                ..Options::default()
            };
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join(format!("out.{to}"));
            encode(img.clone(), to, &options, &out).unwrap();
            let len = std::fs::metadata(&out).unwrap().len();
            assert!(len <= target, "{to}: {len} > {target}");
            // Lossy output got there by quality first, so a reachable target
            // keeps the full dimensions; PNG has to shrink.
            let back = image::open(&out).unwrap();
            if to == "png" {
                assert!(back.width() < 512 && back.width() * 3 == back.height() * 4);
            } else if target >= 15_000 {
                assert_eq!((back.width(), back.height()), (512, 384), "{to}");
            }
        }
        // A target the output already meets changes nothing.
        let roomy = Options {
            target_size: Some(50_000_000),
            ..Options::default()
        };
        for to in ["jpeg", "webp"] {
            assert_eq!(
                encoded_len(&img, to, &roomy),
                encoded_len(&img, to, &Options::default()),
                "{to}"
            );
        }
    }

    #[test]
    fn compressing_never_drops_frames_or_pages() {
        let dir = tempfile::tempdir().unwrap();
        // A two-frame APNG.
        let apng = dir.path().join("anim.png");
        {
            let mut enc = png::Encoder::new(File::create(&apng).unwrap(), 2, 2);
            enc.set_color(png::ColorType::Rgb);
            enc.set_animated(2, 0).unwrap();
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[0; 12]).unwrap();
            w.write_image_data(&[255; 12]).unwrap();
            w.finish().unwrap();
        }
        let still = dir.path().join("still.png");
        image::RgbImage::new(2, 2).save(&still).unwrap();
        assert!(has_more_frames(&apng, "png") && !has_more_frames(&still, "png"));
        let png = format_by_id("png").unwrap();
        let cancel = Cancel::new();
        let options = Options::default();
        let ctx = Ctx::new(Step { from: png, to: png }, &options, &|_| {}, &cancel);
        let out = tempfile::tempdir().unwrap();
        let err = ImageEngine.convert(&ctx, &apng, out.path()).unwrap_err();
        assert!(err.to_string().contains("more than one frame"), "{err}");
        assert!(ImageEngine.convert(&ctx, &still, out.path()).is_ok());

        // TIFF: a little-endian header, then one or two empty IFDs.
        let tiff = |two: bool| {
            let mut b = b"II*\0\x08\0\0\0".to_vec();
            b.extend_from_slice(&[0, 0]);
            b.extend_from_slice(&if two { 14u32 } else { 0 }.to_le_bytes());
            b.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
            std::io::Cursor::new(b)
        };
        assert_eq!(tiff_pages(tiff(false)), Some(1));
        assert_eq!(tiff_pages(tiff(true)), Some(2));
        assert_eq!(tiff_pages(std::io::Cursor::new(b"nope".to_vec())), None);
    }

    #[test]
    fn size_search_keeps_lossless_webp_and_fits_huge_widths() {
        let img = photo(64, 48);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("a.webp");
        let lossless_roomy = Options {
            quality: Some(100),
            target_size: Some(10_000_000),
            ..Options::default()
        };
        encode(img.clone(), "webp", &lossless_roomy, &out).unwrap();
        assert_eq!(image::open(&out).unwrap().to_rgb8(), img.to_rgb8());

        // Wider than libwebp allows: the search starts at its limit.
        let wide = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            WEBP_MAX + 500,
            4,
            image::Rgb([1, 2, 3]),
        ));
        let sized = Options {
            target_size: Some(1_000_000),
            ..Options::default()
        };
        encode(wide, "webp", &sized, &out).unwrap();
        assert!(image::open(&out).unwrap().width() <= WEBP_MAX);
    }

    #[test]
    fn a_cancelled_size_search_stops() {
        let dir = tempfile::tempdir().unwrap();
        let options = Options {
            target_size: Some(10_000),
            ..Options::default()
        };
        let tries = std::cell::Cell::new(0);
        let err = encode_with_pixel_aspect(
            photo(256, 256),
            "jpeg",
            &options,
            &dir.path().join("a.jpg"),
            None,
            &|| {
                tries.set(tries.get() + 1);
                if tries.get() > 1 {
                    Err(Error::Cancelled)
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
        assert!(matches!(err, Error::Cancelled), "{err}");
        assert_eq!(tries.get(), 2);
    }

    #[test]
    fn size_search_picks_the_best_quality_that_fits() {
        // A fake encoder whose size is 1000 bytes per quality point.
        let mut tried = Vec::new();
        fit_target(55_500, "jpeg", None, 100, |q, edge| {
            tried.push((q, edge));
            Ok(u64::from(q) * 1000)
        })
        .unwrap();
        let (last_q, last_edge) = *tried.last().unwrap();
        assert!(last_edge.is_none());
        assert!((53..=55).contains(&last_q), "{tried:?}");
        assert!(tried.len() <= 10, "{tried:?}");

        // The user's quality caps the search.
        let mut first = None;
        fit_target(1_000_000, "webp", Some(40), 100, |q, _| {
            first.get_or_insert(q);
            Ok(1)
        })
        .unwrap();
        assert_eq!(first, Some(40));

        // An impossible target is an error, not an oversized file.
        let err = fit_target(10_000, "png", None, 4000, |_, _| Ok(50_000_000)).unwrap_err();
        assert!(err.to_string().contains("10KB"), "{err}");
    }

    #[test]
    fn same_format_steps_exclude_gif() {
        let steps = ImageEngine.steps();
        let own: Vec<_> = steps
            .iter()
            .filter(|s| s.from == s.to)
            .map(|s| s.from.id)
            .collect();
        assert_eq!(own, SELF);
        assert!(!own.contains(&"gif"));
    }

    #[test]
    fn exif_orientation_is_applied_to_the_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("plain.jpg");
        // 4 wide, 2 tall; the left half is white.
        image::RgbImage::from_fn(4, 2, |x, _| {
            if x < 2 {
                image::Rgb([255; 3])
            } else {
                image::Rgb([0; 3])
            }
        })
        .save(&plain)
        .unwrap();
        // Insert an APP1 Exif segment with orientation 6 (rotate 90° CW)
        // after the SOI marker.
        let exif = b"Exif\0\0MM\0*\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01\0\x06\0\0\0\0\0\0";
        let bytes = std::fs::read(&plain).unwrap();
        let mut oriented = bytes[..2].to_vec();
        oriented.extend_from_slice(&[0xff, 0xe1]);
        oriented.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        oriented.extend_from_slice(exif);
        oriented.extend_from_slice(&bytes[2..]);
        let src = dir.path().join("rotated.jpg");
        std::fs::write(&src, oriented).unwrap();

        let out = tempfile::tempdir().unwrap();
        let jpeg = format_by_id("jpeg").unwrap();
        let options = Options {
            strip_metadata: true,
            quality: Some(100),
            ..Options::default()
        };
        let cancel = Cancel::new();
        let ctx = Ctx::new(
            Step {
                from: jpeg,
                to: jpeg,
            },
            &options,
            &|_| {},
            &cancel,
        );
        let files = ImageEngine.convert(&ctx, &src, out.path()).unwrap();
        let written = std::fs::read(&files[0]).unwrap();
        assert!(
            !written.windows(4).any(|w| w == b"Exif"),
            "output carries no EXIF"
        );
        let back = image::open(&files[0]).unwrap().to_luma8();
        assert_eq!((back.width(), back.height()), (2, 4));
        // Rotated clockwise, the white left half is now on top.
        assert!(back.get_pixel(0, 0).0[0] > 200 && back.get_pixel(0, 3).0[0] < 60);
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
