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
        // A profile that can't be read is left out, as before.
        let icc = image::ImageReader::open(input)
            .and_then(|r| r.with_guessed_format())
            .ok()
            .and_then(|r| r.into_decoder().ok())
            .and_then(|mut d| image::ImageDecoder::icc_profile(&mut d).ok().flatten());
        ctx.check()?;
        ctx.progress(0.5);
        let output = ctx.artifact(out_dir, 0);
        encode_with_meta(
            fit(img, ctx.options.max_size),
            ctx.step.to.id,
            ctx.options,
            &output,
            &Meta {
                pixel_aspect: None,
                icc,
            },
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
/// them the file is left alone. Any EXIF orientation is baked into the
/// pixels; EXIF and text chunks stay unless `strip_metadata` is set, with
/// the orientation reset so nothing rotates the picture again.
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
    let keep_metadata = !options.strip_metadata;
    let mut source = PngMeta::read(path)?;
    // `open_oriented` applies the EXIF orientation, so the kept EXIF must
    // stop asking for it.
    let mut orientation = None;
    if let Some(exif) = &source.info.exif_metadata {
        let mut exif = exif.to_vec();
        orientation = image::metadata::Orientation::remove_from_exif_chunk(&mut exif);
        source.info.exif_metadata = Some(exif.into());
    }
    if orientation.is_some_and(crate::orientation::swaps_dims)
        && let Some(dims) = source.info.pixel_dims.as_mut()
    {
        std::mem::swap(&mut dims.xppu, &mut dims.yppu);
    }
    let img = apply_background(crate::orientation::open_oriented(path)?, "png", options)?;
    let Some(target) = target else {
        return write_png(&img, &source, path, keep_metadata);
    };
    let longest = img.width().max(img.height());
    fit_target(target, "png", None, longest, |_, edge| {
        match edge {
            Some(edge) => write_png(
                &img.resize(edge, edge, FilterType::Lanczos3),
                &source,
                path,
                keep_metadata,
            )?,
            None => write_png(&img, &source, path, keep_metadata)?,
        }
        Ok(std::fs::metadata(path)?.len())
    })
}

/// Rewrites a PNG that macOS `sips` decoded from HEIC or HEIF. iPhone
/// photos are 10-bit HEVC, and `sips` writes them as 16 bits per channel
/// with an alpha channel nobody uses, more than twice the size of the same
/// picture in 8 bits. A 16-bit file goes down to 8 bits only with positive
/// evidence that it is SDR ([`is_confirmed_sdr_png`]); HDR and files that
/// say nothing about their color keep their precision. An alpha channel
/// that is opaque everywhere goes, and alpha that wasn't fully opaque stays
/// under 255 so it isn't dropped as opaque. Color tags (including HDR
/// `mDCV` and `cLLI`), pixel ratio, EXIF and text chunks stay, and EXIF
/// orientation is left for [`crate::orientation::finish_heif_output`] to
/// apply. The rewrite replaces the file only when it comes out smaller.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn optimize_heif_png(path: &Path) -> Result<()> {
    let source = PngMeta::read(path)?;
    if source.info.animation_control.is_some() {
        return Ok(());
    }
    let reduce = source.info.bit_depth == png::BitDepth::Sixteen && is_confirmed_sdr_png(&source);
    // Stored pixels: orientation belongs to `finish_heif_output`, which
    // reads the EXIF this rewrite keeps.
    let img = image::ImageReader::open(path)?
        .with_guessed_format()?
        .decode()
        .map_err(failed)?;
    let img = strip_opaque_alpha(img);
    let img = if reduce { to_eight_bits(img) } else { img };
    let name = path
        .file_name()
        .map_or_else(Default::default, |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!(".{name}.optimizing.png"));
    let written = write_png(&img, &source, &tmp, true).and_then(|()| {
        if std::fs::metadata(&tmp)?.len() < std::fs::metadata(path)?.len() {
            std::fs::rename(&tmp, path)?;
        }
        Ok(())
    });
    let _ = std::fs::remove_file(&tmp);
    written
}

/// 16 bits per channel to 8, rounding. Alpha below 65535 stays below 255:
/// 65530 rounds to 255, and the opaque-alpha check in [`write_png`] would
/// then drop transparency the picture really has.
#[cfg(any(test, target_os = "macos"))]
fn to_eight_bits(img: DynamicImage) -> DynamicImage {
    let alpha = |a16: u16, a8: u8| if a16 < u16::MAX { a8.min(254) } else { a8 };
    match img {
        DynamicImage::ImageLuma16(_) => DynamicImage::ImageLuma8(img.to_luma8()),
        DynamicImage::ImageRgb16(_) => DynamicImage::ImageRgb8(img.to_rgb8()),
        DynamicImage::ImageLumaA16(ref deep) => {
            let mut out = img.to_luma_alpha8();
            for (o, d) in out.pixels_mut().zip(deep.pixels()) {
                o.0[1] = alpha(d.0[1], o.0[1]);
            }
            DynamicImage::ImageLumaA8(out)
        }
        DynamicImage::ImageRgba16(ref deep) => {
            let mut out = img.to_rgba8();
            for (o, d) in out.pixels_mut().zip(deep.pixels()) {
                o.0[3] = alpha(d.0[3], o.0[3]);
            }
            DynamicImage::ImageRgba8(out)
        }
        img => img,
    }
}

/// The SDR transfer characteristics of ITU-T H.273 (`cICP`, ICC `cicp`):
/// BT.709, gamma 2.2, gamma 2.8, BT.601 and sRGB. PQ (16), HLG (18) and
/// linear (8) are not among them.
#[cfg(any(test, target_os = "macos"))]
fn is_sdr_transfer(transfer: u8) -> bool {
    matches!(transfer, 1 | 4 | 5 | 6 | 13)
}

/// Descriptions of standard SDR ICC profiles, lowercase, matched as
/// substrings of the profile's `desc`.
#[cfg(any(test, target_os = "macos"))]
const SDR_PROFILE_NAMES: [&str; 10] = [
    "display p3",
    "srgb",
    "apple rgb",
    "generic rgb",
    "adobe rgb",
    "color lcd",
    "rec.709",
    "bt.709",
    "itu-r bt.709",
    "camera rgb",
];

/// Whether a PNG says it is SDR, so 8 bits per channel lose nothing that a
/// display shows. Anything hinting at HDR or linear light means no: an
/// `mDCV` or `cLLI` chunk, a `cICP` transfer that isn't SDR
/// ([`is_sdr_transfer`]), or an ICC profile that [`icc_signals_hdr`]. Then
/// the file needs positive evidence: an SDR `cICP` transfer, an `sRGB`
/// chunk, or an ICC profile that [`icc_confirms_sdr`] (an SDR `cicp` tag,
/// or a known SDR name with tone curves that check out). A file with none
/// of them, such as one with a custom profile under a neutral name or an
/// SDR name over a linear curve, is ambiguous and is not treated as SDR.
#[cfg(any(test, target_os = "macos"))]
fn is_confirmed_sdr_png(source: &PngMeta) -> bool {
    let info = &source.info;
    if !source.hdr_chunks.is_empty()
        || info.mastering_display_color_volume.is_some()
        || info.content_light_level.is_some()
    {
        return false;
    }
    let cicp = info.coding_independent_code_points;
    if cicp.is_some_and(|c| !is_sdr_transfer(c.transfer_function)) {
        return false;
    }
    let icc = info.icc_profile.as_deref();
    if icc.is_some_and(icc_signals_hdr) {
        return false;
    }
    cicp.is_some() || info.srgb.is_some() || icc.is_some_and(icc_confirms_sdr)
}

/// The transfer characteristic of an ICC 4.4 `cicp` tag.
#[cfg(any(test, target_os = "macos"))]
fn icc_cicp_transfer(data: &[u8]) -> Option<u8> {
    (data.len() >= 12 && data.starts_with(b"cicp")).then(|| data[9])
}

/// Whether an ICC profile describes HDR or linear light: a `cicp` tag
/// (ICC 4.4) with a transfer that isn't SDR, or a description naming PQ,
/// HLG, HDR, BT.2100, ST 2084, linear or scRGB, as Apple's and Adobe's HDR
/// and linear profiles do. In case the tag table can't be read, the whole
/// profile is also searched, in ASCII and UTF-16, for the names too long to
/// turn up by chance in binary data.
#[cfg(any(test, target_os = "macos"))]
fn icc_signals_hdr(icc: &[u8]) -> bool {
    let mut text = String::new();
    for (signature, data) in icc_tags(icc) {
        match &signature {
            b"cicp" if icc_cicp_transfer(data).is_some_and(|t| !is_sdr_transfer(t)) => {
                return true;
            }
            b"desc" | b"dmdd" | b"dmnd" => {
                text.push_str(&icc_text(data));
                text.push('\n');
            }
            _ => {}
        }
    }
    let text = text.to_ascii_uppercase();
    if ["PQ", "HLG", "2100", "HDR", "2084", "LINEAR", "SCRGB"]
        .iter()
        .any(|m| text.contains(m))
    {
        return true;
    }
    let upper: Vec<u8> = icc.iter().map(u8::to_ascii_uppercase).collect();
    let contains = |needle: &[u8]| upper.windows(needle.len()).any(|w| w == needle);
    ["HLG", "2100", "HDR", "2084", "LINEAR", "SCRGB"]
        .iter()
        .any(|m| {
            let utf16: Vec<u8> = m.bytes().flat_map(|b| [0, b]).collect();
            contains(m.as_bytes()) || contains(&utf16)
        })
}

/// Whether an ICC profile that doesn't [`icc_signals_hdr`] is known to be
/// SDR: a `cicp` tag with an SDR transfer, or a description from
/// [`SDR_PROFILE_NAMES`] whose tone curves also check out
/// ([`has_verified_sdr_trc`]). A name alone is not enough, since anyone can
/// call a linear or HDR profile "Display P3". A custom profile, whatever its
/// curves, is not SDR either.
#[cfg(any(test, target_os = "macos"))]
fn icc_confirms_sdr(icc: &[u8]) -> bool {
    let tags = icc_tags(icc);
    if tags.iter().any(|(signature, data)| {
        signature == b"cicp" && icc_cicp_transfer(data).is_some_and(is_sdr_transfer)
    }) {
        return true;
    }
    let named_sdr = tags.iter().any(|(signature, data)| {
        signature == b"desc" && {
            let name = icc_text(data).to_lowercase();
            SDR_PROFILE_NAMES.iter().any(|n| name.contains(n))
        }
    });
    named_sdr && has_verified_sdr_trc(icc)
}

/// Whether an RGB ICC profile's `rTRC`, `gTRC` and `bTRC` tags all exist,
/// lie inside the profile, and each describe an SDR display gamma
/// ([`is_sdr_trc_tag`]).
#[cfg(any(test, target_os = "macos"))]
fn has_verified_sdr_trc(icc: &[u8]) -> bool {
    let tags = icc_tags(icc);
    [b"rTRC", b"gTRC", b"bTRC"].iter().all(|want| {
        tags.iter()
            .find(|(signature, _)| signature == *want)
            .is_some_and(|(_, data)| is_sdr_trc_tag(data))
    })
}

/// Display gammas that count as SDR: 1.8 (old Apple) to 2.6 (DCI), around
/// the 2.2 and 2.4 of sRGB, Display P3 and BT.1886.
#[cfg(any(test, target_os = "macos"))]
const SDR_GAMMA: std::ops::RangeInclusive<f32> = 1.75..=2.65;

/// Whether one ICC tone curve is an SDR gamma. A `curv` with no entries is
/// linear, one entry is a u8Fixed8 gamma, and more are a lookup table that
/// must run from 0 to 65535 without falling and pass its midpoint where a
/// gamma of about 1.6 to 2.7 would (0.5^2.2 is 0.214; linear is 0.5 and PQ
/// is near 0.01). A `para` of any of the five ICC function types needs its
/// gamma in [`SDR_GAMMA`]. Anything else, or a tag too short for its own
/// header, is not SDR.
#[cfg(any(test, target_os = "macos"))]
fn is_sdr_trc_tag(data: &[u8]) -> bool {
    let be16 = |at: usize| {
        data.get(at..at + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
    };
    match data.get(..4) {
        Some(b"curv") => {
            let Some(count) = be32(data, 8) else {
                return false;
            };
            match count {
                0 => false,
                1 => be16(12).is_some_and(|g| SDR_GAMMA.contains(&(f32::from(g) / 256.0))),
                _ => {
                    let Some(table) = count
                        .checked_mul(2)
                        .and_then(|len| data.get(12..12usize.checked_add(len)?))
                    else {
                        return false;
                    };
                    let entries: Vec<u16> = table
                        .chunks_exact(2)
                        .map(|c| u16::from_be_bytes([c[0], c[1]]))
                        .collect();
                    let mid = f32::from(entries[count / 2]) / 65535.0;
                    entries[0] == 0
                        && entries[count - 1] == u16::MAX
                        && entries.windows(2).all(|w| w[0] <= w[1])
                        && (0.15..=0.35).contains(&mid)
                }
            }
        }
        Some(b"para") => {
            // Header plus 1, 3, 4, 5 or 7 s15Fixed16 parameters.
            let Some(params) = be16(8).and_then(|t| [1, 3, 4, 5, 7].get(usize::from(t)).copied())
            else {
                return false;
            };
            let Some(g) = data
                .get(12..16)
                .filter(|_| data.len() >= 12 + 4 * params)
                .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            else {
                return false;
            };
            SDR_GAMMA.contains(&(g as f32 / 65536.0))
        }
        _ => false,
    }
}

#[cfg(any(test, target_os = "macos"))]
fn be32(data: &[u8], at: usize) -> Option<usize> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes(bytes.try_into().ok()?) as usize)
}

/// An ICC profile's tags as `(signature, data)`, skipping any entry that
/// points outside the profile.
#[cfg(any(test, target_os = "macos"))]
fn icc_tags(icc: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let Some(count) = be32(icc, 128) else {
        return Vec::new();
    };
    (0..count.min(1024))
        .filter_map(|i| {
            let entry = 132 + i * 12;
            let signature: [u8; 4] = icc.get(entry..entry + 4)?.try_into().ok()?;
            let (offset, size) = (be32(icc, entry + 4)?, be32(icc, entry + 8)?);
            Some((signature, icc.get(offset..offset.checked_add(size)?)?))
        })
        .collect()
}

/// The text of an ICC `desc` (v2 ASCII) or `mluc` (v4, UTF-16 per
/// language) tag; empty for other types.
#[cfg(any(test, target_os = "macos"))]
fn icc_text(data: &[u8]) -> String {
    match data.get(..4) {
        Some(b"desc") => {
            let len = be32(data, 8).unwrap_or(0);
            data.get(12..12usize.saturating_add(len))
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .unwrap_or_default()
        }
        Some(b"mluc") => {
            let (Some(count), Some(record)) = (be32(data, 8), be32(data, 12)) else {
                return String::new();
            };
            let mut out = String::new();
            for i in 0..count.min(256) {
                let at = 16 + i * record.max(12);
                let (Some(len), Some(offset)) = (be32(data, at + 4), be32(data, at + 8)) else {
                    break;
                };
                if let Some(s) = data.get(offset..offset.saturating_add(len)) {
                    let units: Vec<u16> = s
                        .chunks_exact(2)
                        .map(|c| u16::from_be_bytes([c[0], c[1]]))
                        .collect();
                    out.push_str(&String::from_utf16_lossy(&units));
                    out.push('\n');
                }
            }
            out
        }
        _ => String::new(),
    }
}

/// HDR chunks png 0.18 reads but never writes: the mastering display
/// (`mDCV`) and content light level (`cLLI`), plus the spellings of early
/// PNG 3 drafts (`mDCv`, `cLLi`) some encoders still write.
const HDR_CHUNKS: [[u8; 4]; 4] = [*b"mDCV", *b"mDCv", *b"cLLI", *b"cLLi"];

/// Metadata chunks PNG allows after the image data, which
/// [`png::Decoder::read_info`] never reaches: text and EXIF.
const TRAILING_METADATA: [[u8; 4]; 4] = [*b"tEXt", *b"zTXt", *b"iTXt", *b"eXIf"];

/// The largest trailing metadata chunk kept, enough for any XMP packet.
const MAX_TRAILING_CHUNK: u32 = 16 << 20;

/// What a PNG rewrite carries over besides pixels: the decoded header and
/// ancillary chunks, and the raw chunks the encoder can't write from
/// [`png::Info`].
#[derive(Clone)]
pub(crate) struct PngMeta {
    pub info: png::Info<'static>,
    /// `(type, payload)` of each chunk in [`HDR_CHUNKS`], before or after
    /// the image data, in file order.
    pub hdr_chunks: Vec<([u8; 4], Vec<u8>)>,
    /// `(type, payload)` of each [`TRAILING_METADATA`] chunk after the image
    /// data, in file order, kept as they were so a rewrite with metadata
    /// writes them back after its own image data.
    pub trailing_chunks: Vec<([u8; 4], Vec<u8>)>,
}

impl PngMeta {
    pub(crate) fn read(path: &Path) -> Result<Self> {
        let reader = png::Decoder::new(std::io::BufReader::new(File::open(path)?))
            .read_info()
            .map_err(failed)?;
        let info = reader.info().clone();
        drop(reader);
        let (hdr_chunks, mut trailing_chunks) = read_raw_chunks(path)?;
        // PNG allows one eXIf; the decoder already has one from the header.
        if info.exif_metadata.is_some() {
            trailing_chunks.retain(|(kind, _)| kind != b"eXIf");
        }
        Ok(Self {
            info,
            hdr_chunks,
            trailing_chunks,
        })
    }
}

/// The [`HDR_CHUNKS`] anywhere in a PNG, and the [`TRAILING_METADATA`]
/// chunks after its first `IDAT`, reading up to `IEND` and seeking past
/// image data. The decoder has already checked the structure before the
/// image data; after it, a chunk whose CRC doesn't match, or that is cut
/// short, ends the scan and is dropped.
fn read_raw_chunks(path: &Path) -> Result<RawChunks> {
    use std::io::Read;
    let mut file = std::io::BufReader::new(File::open(path)?);
    let mut signature = [0u8; 8];
    file.read_exact(&mut signature)?;
    let (mut hdr, mut trailing) = (Vec::new(), Vec::new());
    let mut after_image = false;
    loop {
        let mut head = [0u8; 8];
        if file.read_exact(&mut head).is_err() {
            break;
        }
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
        let kind = [head[4], head[5], head[6], head[7]];
        match &kind {
            b"IEND" => break,
            b"IDAT" => after_image = true,
            _ => {}
        }
        // HDR chunks are a few bytes long; anything larger is not one.
        let keep_hdr = HDR_CHUNKS.contains(&kind) && len <= 64;
        let keep_trailing =
            after_image && TRAILING_METADATA.contains(&kind) && len <= MAX_TRAILING_CHUNK;
        if !(keep_hdr || keep_trailing) {
            file.seek_relative(i64::from(len) + 4)?;
            continue;
        }
        let mut payload = vec![0; len as usize];
        let mut crc = [0u8; 4];
        if file.read_exact(&mut payload).is_err() || file.read_exact(&mut crc).is_err() {
            if after_image {
                break;
            }
            return Err(failed("truncated PNG chunk"));
        }
        if after_image && png_crc(&kind, &payload) != u32::from_be_bytes(crc) {
            break;
        }
        if keep_hdr {
            hdr.push((kind, payload));
        } else {
            trailing.push((kind, payload));
        }
    }
    Ok((hdr, trailing))
}

/// HDR chunks and trailing metadata chunks, as [`read_raw_chunks`] finds them.
type RawChunks = (Vec<([u8; 4], Vec<u8>)>, Vec<([u8; 4], Vec<u8>)>);

/// The CRC-32 PNG stores after a chunk, over its type and payload.
fn png_crc(kind: &[u8; 4], payload: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in kind.iter().chain(payload) {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Drops an alpha channel that is fully opaque on every pixel, which saves
/// a quarter of an RGBA image's data before compression. Real transparency
/// stays untouched.
pub(crate) fn strip_opaque_alpha(img: DynamicImage) -> DynamicImage {
    match img {
        DynamicImage::ImageRgba8(ref px) if px.pixels().all(|p| p.0[3] == u8::MAX) => {
            DynamicImage::ImageRgb8(img.to_rgb8())
        }
        DynamicImage::ImageLumaA8(ref px) if px.pixels().all(|p| p.0[1] == u8::MAX) => {
            DynamicImage::ImageLuma8(img.to_luma8())
        }
        DynamicImage::ImageRgba16(ref px) if px.pixels().all(|p| p.0[3] == u16::MAX) => {
            DynamicImage::ImageRgb16(img.to_rgb16())
        }
        DynamicImage::ImageLumaA16(ref px) if px.pixels().all(|p| p.0[1] == u16::MAX) => {
            DynamicImage::ImageLuma16(img.to_luma16())
        }
        img => img,
    }
}

/// Writes `img` to `path` as PNG with `source`'s color tags (ICC, sRGB,
/// gAMA, cHRM, cICP, mDCV, cLLI) and pixel ratio, and with `keep_metadata`
/// its EXIF and text chunks too, including those that came after the image
/// data. An alpha channel that is opaque everywhere is dropped.
pub(crate) fn write_png(
    img: &DynamicImage,
    source: &PngMeta,
    path: &Path,
    keep_metadata: bool,
) -> Result<()> {
    let hdr_chunks = &source.hdr_chunks;
    let trailing_chunks = &source.trailing_chunks;
    let source = &source.info;
    let mut info = png::Info::with_size(img.width(), img.height());
    info.pixel_dims = source.pixel_dims;
    info.source_gamma = source.gamma();
    info.source_chromaticities = source.chromaticities();
    info.srgb = source.srgb;
    info.icc_profile = source.icc_profile.clone();
    if keep_metadata {
        info.exif_metadata = source.exif_metadata.clone();
        info.uncompressed_latin1_text = source.uncompressed_latin1_text.clone();
        info.compressed_latin1_text = source.compressed_latin1_text.clone();
        info.utf8_text = source.utf8_text.clone();
    }
    let bytes = match strip_opaque_alpha(img.clone()) {
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
    let mut encoder = png::Encoder::with_info(&mut out, info).map_err(failed)?;
    encoder.set_compression(png::Compression::Balanced);
    encoder.set_filter(png::Filter::Adaptive);
    let mut writer = encoder.write_header().map_err(failed)?;
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
    // Nor mDCV and cLLI, which must also come before the image data.
    for (kind, payload) in hdr_chunks {
        writer
            .write_chunk(png::chunk::ChunkType(*kind), payload)
            .map_err(failed)?;
    }
    writer.write_image_data(&bytes).map_err(failed)?;
    if keep_metadata {
        for (kind, payload) in trailing_chunks {
            writer
                .write_chunk(png::chunk::ChunkType(*kind), payload)
                .map_err(failed)?;
        }
    }
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
    encode_with_meta(img, to, options, output, &Meta::default(), &|| Ok(()))
}

/// What an encoding carries besides pixels.
#[derive(Debug, Clone, Default)]
pub(crate) struct Meta {
    /// JPEG pixel aspect ratio, for non-square video frames.
    pub pixel_aspect: Option<(u16, u16)>,
    /// The source's ICC profile, written where the format can hold one
    /// (JPEG, PNG, TIFF, WebP) so colors look the same.
    pub icc: Option<Vec<u8>>,
}

/// [`encode`] with [`Meta`], and `check` run before each attempt of a size
/// search so a cancelled job stops searching.
pub(crate) fn encode_with_meta(
    img: DynamicImage,
    to: &str,
    options: &Options,
    output: &Path,
    meta: &Meta,
    check: &dyn Fn() -> Result<()>,
) -> Result<()> {
    let img = prepare(apply_background(img, to, options)?, to);
    let mut w = BufWriter::new(File::create(output)?);
    match options.target_size {
        // Without a size, stream straight to the file.
        None => encode_into(&img, to, options.quality, meta, &mut w)?,
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
                encode_into(scaled, to, Some(quality), meta, &mut buf)?;
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
/// PNG and TIFF keep higher precision; PNG drops an alpha channel that is
/// opaque everywhere.
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
            strip_opaque_alpha(DynamicImage::ImageRgba16(img.to_rgba16()))
        }
        // An alpha channel that is opaque everywhere only costs space.
        "png" => strip_opaque_alpha(img),
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
    meta: &Meta,
    mut w: &mut W,
) -> Result<()> {
    use image::ImageEncoder;
    // Every encoder that takes a profile accepts any bytes, so ignoring the
    // error is only for formats that have nowhere to put one.
    let with_icc = |encoder: &mut dyn ImageEncoder| {
        if let Some(icc) = &meta.icc {
            let _ = encoder.set_icc_profile(icc.clone());
        }
    };
    match to {
        "jpeg" => {
            let quality = quality.unwrap_or(default_quality(to));
            let mut encoder = JpegEncoder::new_with_quality(&mut w, quality);
            with_icc(&mut encoder);
            if let Some((num, den)) = meta.pixel_aspect {
                encoder.set_pixel_density(image::codecs::jpeg::PixelDensity {
                    density: (num, den),
                    unit: image::codecs::jpeg::PixelDensityUnit::PixelAspectRatio,
                });
            }
            DynamicImage::ImageRgb8(img.to_rgb8()).write_with_encoder(encoder)
        }
        "webp" => {
            let mut bytes = webp(img, quality.unwrap_or(default_quality(to)))?;
            if let Some(icc) = &meta.icc {
                bytes = webp_with_icc(&bytes, icc)?;
            }
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
        // `ImageFormat::Pnm` writes PAM (P7); a .ppm must be a binary pixmap (P6).
        "ppm" => DynamicImage::ImageRgb8(img.to_rgb8()).write_with_encoder(
            image::codecs::pnm::PnmEncoder::new(&mut w).with_subtype(
                image::codecs::pnm::PnmSubtype::Pixmap(image::codecs::pnm::SampleEncoding::Binary),
            ),
        ),
        "gif" => return gif(img, w),
        "png" => {
            // `PngEncoder::new` picks the fast setting, which leaves photos
            // far larger than other encoders do.
            let mut encoder = image::codecs::png::PngEncoder::new_with_quality(
                &mut w,
                image::codecs::png::CompressionType::Default,
                image::codecs::png::FilterType::Adaptive,
            );
            with_icc(&mut encoder);
            img.write_with_encoder(encoder)
        }
        "tiff" => {
            let mut encoder = image::codecs::tiff::TiffEncoder::new(&mut w);
            with_icc(&mut encoder);
            img.write_with_encoder(encoder)
        }
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

/// Writes a still GIF with its palette as the global color table.
///
/// `image`'s encoder passes no global palette, and the `gif` crate then still
/// sets the table flag and pads two black entries, putting the real palette
/// on the frame. Pillow treats a frame palette that is exactly the 0-255 gray
/// ramp as no palette and falls back to that black table, so any grayscale
/// image using every gray level (a rendered page of text, a scan) opened as
/// solid black.
fn gif(img: &DynamicImage, w: &mut impl Write) -> Result<()> {
    let (Ok(width), Ok(height)) = (u16::try_from(img.width()), u16::try_from(img.height())) else {
        return Err(failed(format!(
            "GIF images can be at most 65535 pixels wide and tall, and this one is {}×{}",
            img.width(),
            img.height()
        )));
    };
    let mut rgba = img.to_rgba8().into_raw();
    // Speed 1, the best palette, as `image`'s encoder used.
    let mut frame = gif::Frame::from_rgba_speed(width, height, &mut rgba, 1);
    let palette = frame.palette.take().unwrap_or_default();
    let mut encoder = gif::Encoder::new(w, width, height, &palette).map_err(failed)?;
    encoder.write_frame(&frame).map_err(failed)?;
    Ok(())
}

/// Adds `icc` to a WebP file as an ICCP chunk, through libwebp's muxer.
fn webp_with_icc(webp: &[u8], icc: &[u8]) -> Result<Vec<u8>> {
    use libwebp_sys as w;
    let image = w::WebPData {
        bytes: webp.as_ptr(),
        size: webp.len(),
    };
    let profile = w::WebPData {
        bytes: icc.as_ptr(),
        size: icc.len(),
    };
    // SAFETY: both WebPData point at live slices for the calls that copy
    // them (copy_data = 1). The mux and the assembled buffer are released
    // here, on every path.
    unsafe {
        let mux = w::WebPMuxCreateInternal(&image, 1, w::WEBP_MUX_ABI_VERSION as i32);
        if mux.is_null() {
            return Err(failed("libwebp could not read its own output"));
        }
        let mut out = w::WebPData {
            bytes: std::ptr::null(),
            size: 0,
        };
        let ok = w::WebPMuxSetChunk(mux, c"ICCP".as_ptr(), &profile, 1)
            == w::WebPMuxError::WEBP_MUX_OK
            && w::WebPMuxAssemble(mux, &mut out) == w::WebPMuxError::WEBP_MUX_OK;
        w::WebPMuxDelete(mux);
        let bytes = (ok && !out.bytes.is_null())
            .then(|| std::slice::from_raw_parts(out.bytes, out.size).to_vec());
        if !out.bytes.is_null() {
            w::WebPFree(out.bytes as *mut _);
        }
        bytes.ok_or_else(|| failed("libwebp could not add the color profile"))
    }
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
pub(crate) mod tests {
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
    fn ppm_is_a_binary_pixmap_not_pam() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        image::RgbaImage::from_pixel(3, 2, image::Rgba([200, 100, 50, 255]))
            .save(&png)
            .unwrap();
        let step = Step {
            from: format_by_id("png").unwrap(),
            to: format_by_id("ppm").unwrap(),
        };
        let (cancel, options) = (Cancel::new(), Options::default());
        let ctx = Ctx::new(step, &options, &|_| {}, &cancel);
        let out = tempfile::tempdir().unwrap();
        let files = ImageEngine.convert(&ctx, &png, out.path()).unwrap();
        let bytes = std::fs::read(&files[0]).unwrap();
        // P6, then whitespace-separated width, height and maximum value.
        assert!(
            bytes.starts_with(b"P6"),
            "{:?}",
            &bytes[..bytes.len().min(16)]
        );
        let header: Vec<&[u8]> = bytes[2..]
            .split(u8::is_ascii_whitespace)
            .filter(|t| !t.is_empty())
            .take(3)
            .collect();
        assert_eq!(header, [&b"3"[..], b"2", b"255"]);
        let back = image::open(&files[0]).unwrap().to_rgb8();
        assert_eq!(back.get_pixel(2, 1).0, [200, 100, 50]);
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
                match to {
                    "exr" => image::ColorType::Rgba32F,
                    // The alpha is opaque everywhere, so PNG leaves it out.
                    "png" => image::ColorType::Rgb16,
                    _ => image::ColorType::Rgba16,
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

    #[test]
    fn png_drops_only_an_opaque_alpha_channel() {
        let dir = tempfile::tempdir().unwrap();
        let rgb = photo(96, 64).to_rgb8();
        let opaque = dir.path().join("opaque.png");
        DynamicImage::ImageRgba8(DynamicImage::ImageRgb8(rgb.clone()).to_rgba8())
            .save(&opaque)
            .unwrap();
        let back = run(&opaque, "png", &Options::default());
        assert_eq!(back.color(), image::ColorType::Rgb8);
        assert_eq!(back.to_rgb8(), rgb);
        let out = tempfile::tempdir().unwrap();
        let png = format_by_id("png").unwrap();
        let (cancel, options) = (Cancel::new(), Options::default());
        let ctx = Ctx::new(Step { from: png, to: png }, &options, &|_| {}, &cancel);
        let files = ImageEngine.convert(&ctx, &opaque, out.path()).unwrap();
        let (before, after) = (
            std::fs::metadata(&opaque).unwrap().len(),
            std::fs::metadata(&files[0]).unwrap().len(),
        );
        assert!(after < before, "{after} >= {before}");

        let mut clear = DynamicImage::ImageRgb8(rgb).to_rgba8();
        clear.get_pixel_mut(3, 4).0[3] = 254;
        clear.get_pixel_mut(10, 20).0[3] = 0;
        let src = dir.path().join("clear.png");
        clear.save(&src).unwrap();
        let back = run(&src, "png", &Options::default());
        assert_eq!(back.color(), image::ColorType::Rgba8);
        assert_eq!(back.to_rgba8(), clear);

        // Gray and 16-bit follow the same rule.
        let la = DynamicImage::ImageLumaA16(image::ImageBuffer::from_pixel(
            2,
            2,
            image::LumaA([1234u16, 65535]),
        ));
        assert!(matches!(
            strip_opaque_alpha(la),
            DynamicImage::ImageLuma16(_)
        ));
        let la = DynamicImage::ImageLumaA8(image::GrayAlphaImage::from_pixel(
            2,
            2,
            image::LumaA([5, 200]),
        ));
        assert!(matches!(
            strip_opaque_alpha(la),
            DynamicImage::ImageLumaA8(_)
        ));
    }

    /// What [`deep_png`] writes besides pixels.
    #[derive(Default)]
    pub(crate) struct DeepPng<'a> {
        pub icc: Option<&'a [u8]>,
        pub srgb: bool,
        pub cicp: Option<png::CodingIndependentCodePoints>,
        /// Raw chunks written before the image data, such as `mDCV`.
        pub chunks: Vec<([u8; 4], Vec<u8>)>,
        /// Raw chunks written after the image data, such as `tEXt`.
        pub trailing: Vec<([u8; 4], Vec<u8>)>,
        pub exif: Option<Vec<u8>>,
        /// Alpha for every pixel; `alpha_at` overrides one.
        pub alpha: Option<u16>,
        pub alpha_at: Option<((u32, u32), u16)>,
    }

    /// Writes a 16-bit RGBA PNG the way `sips` does for an iPhone photo:
    /// ten significant bits and an opaque alpha channel, with the tags in
    /// `meta`.
    pub(crate) fn deep_png(
        path: &Path,
        w: u32,
        h: u32,
        meta: &DeepPng,
    ) -> image::ImageBuffer<image::Rgba<u16>, Vec<u16>> {
        let base = photo(w, h).to_rgb8();
        let mut seed = 0x9e37_79b9_u32;
        let alpha = meta.alpha.unwrap_or(u16::MAX);
        let px = image::ImageBuffer::from_fn(w, h, |x, y| {
            let p = base.get_pixel(x, y).0;
            image::Rgba(std::array::from_fn(|i| {
                if i == 3 {
                    return match meta.alpha_at {
                        Some((at, a)) if at == (x, y) => a,
                        _ => alpha,
                    };
                }
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                // Ten bits, scaled to sixteen.
                let ten = ((p[i] as u32) << 2 | (seed & 3)) as u16;
                ten << 6 | ten >> 4
            }))
        });
        let mut info = png::Info::with_size(w, h);
        info.color_type = png::ColorType::Rgba;
        info.bit_depth = png::BitDepth::Sixteen;
        info.icc_profile = meta.icc.map(|icc| icc.to_vec().into());
        info.exif_metadata = meta.exif.clone().map(Into::into);
        if meta.srgb {
            info.srgb = Some(png::SrgbRenderingIntent::Perceptual);
        }
        info.pixel_dims = Some(png::PixelDimensions {
            xppu: 2835,
            yppu: 5670,
            unit: png::Unit::Meter,
        });
        let mut file = BufWriter::new(File::create(path).unwrap());
        let mut writer = png::Encoder::with_info(&mut file, info)
            .unwrap()
            .write_header()
            .unwrap();
        if let Some(c) = meta.cicp {
            writer
                .write_chunk(
                    png::chunk::cICP,
                    &[
                        c.color_primaries,
                        c.transfer_function,
                        c.matrix_coefficients,
                        u8::from(c.is_video_full_range_image),
                    ],
                )
                .unwrap();
        }
        for (kind, payload) in &meta.chunks {
            writer
                .write_chunk(png::chunk::ChunkType(*kind), payload)
                .unwrap();
        }
        let bytes: Vec<u8> = px.as_raw().iter().flat_map(|v| v.to_be_bytes()).collect();
        writer.write_image_data(&bytes).unwrap();
        for (kind, payload) in &meta.trailing {
            writer
                .write_chunk(png::chunk::ChunkType(*kind), payload)
                .unwrap();
        }
        writer.finish().unwrap();
        px
    }

    fn sips_like_png(
        path: &Path,
        w: u32,
        h: u32,
        icc: &[u8],
        cicp: Option<png::CodingIndependentCodePoints>,
    ) -> image::ImageBuffer<image::Rgba<u16>, Vec<u16>> {
        let meta = DeepPng {
            icc: Some(icc),
            cicp,
            ..DeepPng::default()
        };
        deep_png(path, w, h, &meta)
    }

    /// A minimal ICC v4 profile whose `desc` tag (`mluc`, UTF-16) is
    /// `name`, with the sRGB tone curve ([`srgb_para`]) on all three
    /// channels and filler so it is the size of a real one.
    pub(crate) fn icc_named(name: &str) -> Vec<u8> {
        icc_with(name, &[])
    }

    /// [`icc_named`] plus `extra` tags.
    fn icc_with(name: &str, extra: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
        icc_with_trc(name, Some(srgb_para()), extra)
    }

    /// A `para` tone curve: function type 3 with gamma 2.4 and the sRGB
    /// segments, as Apple's Display P3 and sRGB v4 profiles have.
    fn srgb_para() -> Vec<u8> {
        let fixed = |v: f64| ((v * 65536.0).round() as i32).to_be_bytes();
        let mut tag = b"para\0\0\0\0".to_vec();
        tag.extend_from_slice(&3u16.to_be_bytes());
        tag.extend_from_slice(&[0, 0]);
        for v in [2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045] {
            tag.extend_from_slice(&fixed(v));
        }
        tag
    }

    /// A `curv` tone curve with `entries` (empty for linear).
    fn curv(entries: &[u16]) -> Vec<u8> {
        let mut tag = b"curv\0\0\0\0".to_vec();
        tag.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        tag.extend(entries.iter().flat_map(|e| e.to_be_bytes()));
        tag
    }

    /// A `curv` lookup table of `n` entries following `f` from 0 to 1.
    fn curv_table(n: usize, f: impl Fn(f64) -> f64) -> Vec<u8> {
        let entries: Vec<u16> = (0..n)
            .map(|i| (f(i as f64 / (n - 1) as f64) * 65535.0).round() as u16)
            .collect();
        curv(&entries)
    }

    /// An ICC v4 profile named `name` whose `rTRC`, `gTRC` and `bTRC` are
    /// all `trc`, or that has none, plus `extra` tags.
    fn icc_with_trc(name: &str, trc: Option<Vec<u8>>, extra: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
        let text: Vec<u8> = name.encode_utf16().flat_map(u16::to_be_bytes).collect();
        let mut mluc = b"mluc\0\0\0\0".to_vec();
        mluc.extend_from_slice(&1u32.to_be_bytes());
        mluc.extend_from_slice(&12u32.to_be_bytes());
        mluc.extend_from_slice(b"enUS");
        mluc.extend_from_slice(&(text.len() as u32).to_be_bytes());
        mluc.extend_from_slice(&28u32.to_be_bytes());
        mluc.extend_from_slice(&text);
        let filler: Vec<u8> = (0..512u32).map(|i| (i * 7 % 251) as u8).collect();
        let mut tags = vec![(*b"desc", mluc), (*b"A2B0", filler)];
        if let Some(trc) = trc {
            for sig in [*b"rTRC", *b"gTRC", *b"bTRC"] {
                tags.push((sig, trc.clone()));
            }
        }
        tags.extend_from_slice(extra);
        let mut icc = vec![0u8; 128];
        icc[8..12].copy_from_slice(&[4, 0x40, 0, 0]);
        icc[12..16].copy_from_slice(b"mntr");
        icc[16..20].copy_from_slice(b"RGB ");
        icc[20..24].copy_from_slice(b"XYZ ");
        icc[36..40].copy_from_slice(b"acsp");
        icc.extend_from_slice(&(tags.len() as u32).to_be_bytes());
        let mut at = 128 + 4 + 12 * tags.len();
        for (sig, data) in &tags {
            icc.extend_from_slice(sig);
            icc.extend_from_slice(&(at as u32).to_be_bytes());
            icc.extend_from_slice(&(data.len() as u32).to_be_bytes());
            at += data.len();
        }
        for (_, data) in &tags {
            icc.extend_from_slice(data);
        }
        let size = icc.len() as u32;
        icc[..4].copy_from_slice(&size.to_be_bytes());
        icc
    }

    /// A big-endian TIFF with an Orientation and a Software tag, the way a
    /// PNG `eXIf` chunk holds EXIF.
    pub(crate) fn exif_with_orientation(orientation: u16) -> Vec<u8> {
        let mut tiff = b"MM\0*".to_vec();
        tiff.extend_from_slice(&8u32.to_be_bytes());
        tiff.extend_from_slice(&2u16.to_be_bytes());
        // Orientation, SHORT, one value.
        tiff.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1]);
        tiff.extend_from_slice(&orientation.to_be_bytes());
        tiff.extend_from_slice(&[0, 0]);
        // Software, ASCII, "cvt" inline.
        tiff.extend_from_slice(&[0x01, 0x31, 0, 2, 0, 0, 0, 4]);
        tiff.extend_from_slice(b"cvt\0");
        tiff.extend_from_slice(&0u32.to_be_bytes());
        tiff
    }

    /// `mDCV` for BT.2020 primaries, D65 white, 1000 nits peak, and `cLLI`
    /// for 1000 nits max, 400 average.
    pub(crate) fn hdr10_chunks() -> Vec<([u8; 4], Vec<u8>)> {
        let mut mdcv = Vec::new();
        for v in [35400u16, 14600, 8500, 39850, 6550, 2300, 15635, 16450] {
            mdcv.extend_from_slice(&v.to_be_bytes());
        }
        mdcv.extend_from_slice(&10_000_000u32.to_be_bytes());
        mdcv.extend_from_slice(&50u32.to_be_bytes());
        let mut clli = 10_000_000u32.to_be_bytes().to_vec();
        clli.extend_from_slice(&4_000_000u32.to_be_bytes());
        vec![(*b"mDCV", mdcv), (*b"cLLI", clli)]
    }

    pub(crate) const PQ: png::CodingIndependentCodePoints = png::CodingIndependentCodePoints {
        color_primaries: 9,
        transfer_function: 16,
        matrix_coefficients: 0,
        is_video_full_range_image: true,
    };

    pub(crate) fn png_info(path: &Path) -> png::Info<'static> {
        png::Decoder::new(std::io::BufReader::new(File::open(path).unwrap()))
            .read_info()
            .unwrap()
            .info()
            .clone()
    }

    #[test]
    fn heif_png_goes_to_eight_bits_with_its_profile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("IMG_0001.png");
        let icc = icc_named("Display P3");
        let deep = sips_like_png(&path, 320, 240, &icc, None);
        let before = std::fs::metadata(&path).unwrap().len();
        optimize_heif_png(&path).unwrap();
        let after = std::fs::metadata(&path).unwrap().len();
        assert!(
            (after as f64) < before as f64 * 0.6,
            "{after} is not 40% under {before}"
        );
        let info = png_info(&path);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(info.icc_profile.as_deref(), Some(&icc[..]));
        let back = image::open(&path).unwrap();
        assert_eq!(back.color(), image::ColorType::Rgb8);
        let back = back.to_rgb8();
        for (a, b) in deep.pixels().zip(back.pixels()) {
            for i in 0..3 {
                let want = (a.0[i] as f64 / 257.0).round() as i32;
                assert!((want - b.0[i] as i32).abs() <= 1, "{a:?} vs {b:?}");
            }
        }
        // Nothing left beside it.
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["IMG_0001.png"]);
    }

    #[test]
    fn heif_png_keeps_hdr_depth() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hdr.png");
        let pq = png::CodingIndependentCodePoints {
            color_primaries: 9,
            transfer_function: 16,
            matrix_coefficients: 0,
            is_video_full_range_image: true,
        };
        let deep = sips_like_png(&path, 64, 48, b"profile", Some(pq));
        optimize_heif_png(&path).unwrap();
        let info = png_info(&path);
        assert_eq!(info.bit_depth, png::BitDepth::Sixteen);
        assert_eq!(
            info.coding_independent_code_points
                .map(|c| c.transfer_function),
            Some(16)
        );
        assert_eq!(info.icc_profile.as_deref(), Some(&b"profile"[..]));
        assert_eq!(image::open(&path).unwrap().to_rgba16(), deep);
    }

    /// Whether [`optimize_heif_png`] takes a 16-bit PNG with these tags to
    /// 8 bits.
    fn goes_to_eight_bits(meta: &DeepPng) -> bool {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.png");
        deep_png(&path, 64, 48, meta);
        optimize_heif_png(&path).unwrap();
        png_info(&path).bit_depth == png::BitDepth::Eight
    }

    #[test]
    fn only_png_confirmed_as_sdr_goes_to_eight_bits() {
        let p3 = icc_named("Display P3");
        let srgb_icc = icc_named("sRGB IEC61966-2.1");
        let adobe = icc_named("Adobe RGB (1998)");
        let cicp = |transfer_function| png::CodingIndependentCodePoints {
            color_primaries: 12,
            transfer_function,
            matrix_coefficients: 0,
            is_video_full_range_image: true,
        };
        // An ICC 4.4 `cicp` tag with this transfer.
        let cicp_tag = |transfer| {
            (
                *b"cicp",
                [&b"cicp\0\0\0\0"[..], &[9, transfer, 0, 1]].concat(),
            )
        };
        let custom_icc = icc_named("Custom Camera Profile");
        let sdr_cicp_icc = icc_with("Custom Camera Profile", &[cicp_tag(13)]);
        let sdr = [
            (
                "sRGB chunk",
                DeepPng {
                    srgb: true,
                    ..DeepPng::default()
                },
            ),
            (
                "Display P3 ICC",
                DeepPng {
                    icc: Some(&p3),
                    ..DeepPng::default()
                },
            ),
            (
                "cICP sRGB",
                DeepPng {
                    cicp: Some(cicp(13)),
                    ..DeepPng::default()
                },
            ),
            (
                "cICP BT.709",
                DeepPng {
                    cicp: Some(cicp(1)),
                    ..DeepPng::default()
                },
            ),
            (
                "sRGB ICC",
                DeepPng {
                    icc: Some(&srgb_icc),
                    ..DeepPng::default()
                },
            ),
            (
                "Adobe RGB ICC",
                DeepPng {
                    icc: Some(&adobe),
                    ..DeepPng::default()
                },
            ),
            (
                "custom ICC with an sRGB cicp tag",
                DeepPng {
                    icc: Some(&sdr_cicp_icc),
                    ..DeepPng::default()
                },
            ),
        ];
        for (name, meta) in &sdr {
            assert!(goes_to_eight_bits(meta), "{name} should go to 8 bits");
        }
        let pq_icc = icc_named("Rec. ITU-R BT.2100 PQ");
        let hlg_icc = icc_named("Display P3 HLG");
        let linear_icc = icc_named("sRGB Linear");
        let scrgb_icc = icc_named("scRGB");
        // ICC 4.4 `cicp` tags saying PQ and linear, under a neutral name.
        let cicp_icc = icc_with("Display", &[cicp_tag(16)]);
        let linear_cicp_icc = icc_with("Display", &[cicp_tag(8)]);
        assert!(icc_signals_hdr(&cicp_icc));
        assert!(icc_signals_hdr(&icc_with("Display", &[cicp_tag(18)])));
        assert!(icc_signals_hdr(&linear_cicp_icc));
        assert!(icc_signals_hdr(&linear_icc));
        assert!(icc_signals_hdr(&scrgb_icc));
        assert!(!icc_signals_hdr(&icc_with("Display", &[cicp_tag(13)])));
        assert!(!icc_signals_hdr(&icc_named("Display P3")));
        assert!(!icc_signals_hdr(&icc_named("sRGB IEC61966-2.1")));
        assert!(!icc_signals_hdr(&icc_named("Adobe RGB (1998)")));
        assert!(!icc_signals_hdr(&custom_icc));
        assert!(icc_confirms_sdr(&icc_named("Display P3")));
        assert!(icc_confirms_sdr(&icc_named("Color LCD")));
        assert!(icc_confirms_sdr(&icc_named("ITU-R BT.709 Reference")));
        assert!(icc_confirms_sdr(&sdr_cicp_icc));
        assert!(!icc_confirms_sdr(&custom_icc));
        assert!(!icc_confirms_sdr(&icc_with("Display", &[cicp_tag(16)])));
        assert!(!icc_confirms_sdr(b"not a profile at all"));
        let kept = [
            ("no color tags", DeepPng::default()),
            (
                "custom ICC under a neutral name",
                DeepPng {
                    icc: Some(&custom_icc),
                    ..DeepPng::default()
                },
            ),
            (
                "unreadable ICC",
                DeepPng {
                    icc: Some(b"profile"),
                    ..DeepPng::default()
                },
            ),
            (
                "linear ICC",
                DeepPng {
                    icc: Some(&linear_icc),
                    ..DeepPng::default()
                },
            ),
            (
                "scRGB ICC",
                DeepPng {
                    icc: Some(&scrgb_icc),
                    ..DeepPng::default()
                },
            ),
            (
                "ICC with a linear cicp tag",
                DeepPng {
                    icc: Some(&linear_cicp_icc),
                    ..DeepPng::default()
                },
            ),
            (
                "cICP linear",
                DeepPng {
                    cicp: Some(cicp(8)),
                    ..DeepPng::default()
                },
            ),
            (
                "SDR cICP beside a PQ ICC",
                DeepPng {
                    cicp: Some(cicp(13)),
                    icc: Some(&pq_icc),
                    ..DeepPng::default()
                },
            ),
            (
                "cICP PQ",
                DeepPng {
                    cicp: Some(PQ),
                    ..DeepPng::default()
                },
            ),
            (
                "cICP HLG",
                DeepPng {
                    cicp: Some(cicp(18)),
                    ..DeepPng::default()
                },
            ),
            (
                "PQ ICC",
                DeepPng {
                    icc: Some(&pq_icc),
                    ..DeepPng::default()
                },
            ),
            (
                "HLG ICC",
                DeepPng {
                    icc: Some(&hlg_icc),
                    ..DeepPng::default()
                },
            ),
            (
                "ICC with a PQ cicp tag",
                DeepPng {
                    icc: Some(&cicp_icc),
                    ..DeepPng::default()
                },
            ),
            (
                "sRGB with mDCV and cLLI",
                DeepPng {
                    srgb: true,
                    chunks: hdr10_chunks(),
                    ..DeepPng::default()
                },
            ),
            (
                "SDR cICP with a draft-spelled cLLi",
                DeepPng {
                    cicp: Some(cicp(13)),
                    chunks: vec![(*b"cLLi", hdr10_chunks()[1].1.clone())],
                    ..DeepPng::default()
                },
            ),
        ];
        for (name, meta) in &kept {
            assert!(!goes_to_eight_bits(meta), "{name} should keep 16 bits");
        }
    }

    #[test]
    fn custom_neutral_icc_profile_keeps_sixteen_bits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("camera.png");
        let icc = icc_named("Custom Camera Profile");
        let meta = DeepPng {
            icc: Some(&icc),
            ..DeepPng::default()
        };
        let deep = deep_png(&path, 96, 64, &meta);
        assert!(!is_confirmed_sdr_png(&PngMeta::read(&path).unwrap()));
        optimize_heif_png(&path).unwrap();
        let info = png_info(&path);
        assert_eq!(info.bit_depth, png::BitDepth::Sixteen);
        assert_eq!(info.icc_profile.as_deref(), Some(&icc[..]));
        assert_eq!(
            image::open(&path).unwrap().to_rgb16(),
            DynamicImage::ImageRgba16(deep).to_rgb16()
        );
    }

    #[test]
    fn icc_tone_curves_must_be_sdr_gamma() {
        let gamma = |g: f64| move |x: f64| x.powf(g);
        let srgb = |x: f64| {
            if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        };
        let para = |kind: u16, g: f64, params: usize| {
            let mut tag = b"para\0\0\0\0".to_vec();
            tag.extend_from_slice(&kind.to_be_bytes());
            tag.extend_from_slice(&[0, 0]);
            tag.extend_from_slice(&((g * 65536.0) as i32).to_be_bytes());
            tag.resize(12 + 4 * params, 0);
            tag
        };
        // A rough PQ EOTF, normalized: 0.5 in gives about 0.009 out.
        let pq = |x: f64| {
            let (m1, m2) = (0.159_301_76, 78.843_75);
            let (c1, c2, c3) = (0.835_937_5, 18.851_562_5, 18.687_5);
            let p = x.powf(1.0 / m2);
            ((p - c1).max(0.0) / (c2 - c3 * p)).powf(1.0 / m1)
        };
        let sdr = [
            ("para sRGB", srgb_para()),
            ("para type 0, gamma 2.2", para(0, 2.2, 1)),
            ("para type 4, gamma 2.4", para(4, 2.4, 7)),
            ("curv gamma 2.2", curv(&[563])),
            ("curv gamma 1.8", curv(&[461])),
            ("curv sRGB table", curv_table(1024, srgb)),
            ("curv gamma 1.8 table", curv_table(256, gamma(1.8))),
            ("curv three-entry table at 2.4", curv_table(3, gamma(2.4))),
        ];
        for (name, tag) in &sdr {
            assert!(is_sdr_trc_tag(tag), "{name} should be SDR");
            let icc = icc_with_trc("Display P3", Some(tag.clone()), &[]);
            assert!(has_verified_sdr_trc(&icc), "{name} profile");
        }
        let mut falling = curv_table(1024, srgb);
        falling[12 + 2 * 600..12 + 2 * 601].copy_from_slice(&0u16.to_be_bytes());
        let mut short_table = curv_table(1024, srgb);
        short_table.truncate(400);
        let not_sdr = [
            ("curv with no entries (linear)", curv(&[])),
            ("curv gamma 1.0", curv(&[256])),
            ("curv gamma 3.0", curv(&[768])),
            ("curv linear table", curv_table(1024, |x| x)),
            ("curv PQ table", curv_table(1024, pq)),
            ("curv table that falls", falling),
            ("curv table cut short", short_table),
            ("curv table not reaching 65535", {
                curv_table(1024, |x| srgb(x) * 0.9)
            }),
            ("curv header only", b"curv\0\0\0\0".to_vec()),
            ("para gamma 1.0", para(3, 1.0, 5)),
            ("para gamma 2.4, type 5", para(5, 2.4, 7)),
            ("para type 3 missing parameters", para(3, 2.4, 2)),
            ("para header only", b"para\0\0\0\0\0\0".to_vec()),
            ("other tag type", b"sf32\0\0\0\0\0\0\0\0".to_vec()),
            ("empty", Vec::new()),
        ];
        for (name, tag) in &not_sdr {
            assert!(!is_sdr_trc_tag(tag), "{name} should not be SDR");
        }
        assert!(!has_verified_sdr_trc(&icc_with_trc(
            "Display P3",
            None,
            &[]
        )));
        // Only two of the three channels.
        let two = icc_with_trc(
            "Display P3",
            None,
            &[(*b"rTRC", srgb_para()), (*b"gTRC", srgb_para())],
        );
        assert!(!has_verified_sdr_trc(&two));
        // One channel linear.
        let mixed = icc_with_trc(
            "Display P3",
            None,
            &[
                (*b"rTRC", srgb_para()),
                (*b"gTRC", curv(&[])),
                (*b"bTRC", srgb_para()),
            ],
        );
        assert!(!has_verified_sdr_trc(&mixed));
        // A tag table entry pointing past the end of the profile.
        let mut past_end = icc_named("Display P3");
        past_end.truncate(past_end.len() - 10);
        assert!(!has_verified_sdr_trc(&past_end));
        assert!(!has_verified_sdr_trc(b"not a profile at all"));
    }

    /// A profile that calls itself Display P3 or sRGB but has a linear,
    /// missing or malformed tone curve is not SDR: the file keeps 16 bits
    /// and every pixel.
    #[test]
    fn sdr_named_icc_with_unverified_curves_keeps_sixteen_bits() {
        let p3 = |trc| icc_with_trc("Display P3", trc, &[]);
        let cases = [
            ("Display P3 with linear curv", p3(Some(curv(&[])))),
            ("Display P3 with gamma 1.0 curv", p3(Some(curv(&[256])))),
            (
                "sRGB IEC61966-2.1 with a linear table",
                icc_with_trc("sRGB IEC61966-2.1", Some(curv_table(1024, |x| x)), &[]),
            ),
            ("Display P3 without curves", p3(None)),
            (
                "Display P3 with filler for curves",
                p3(Some((0..512u32).map(|i| (i * 7 % 251) as u8).collect())),
            ),
        ];
        for (name, icc) in &cases {
            assert!(!icc_signals_hdr(icc), "{name}");
            assert!(!icc_confirms_sdr(icc), "{name}");
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("p3.png");
            let meta = DeepPng {
                icc: Some(icc),
                ..DeepPng::default()
            };
            let deep = deep_png(&path, 64, 48, &meta);
            assert!(
                !is_confirmed_sdr_png(&PngMeta::read(&path).unwrap()),
                "{name}"
            );
            optimize_heif_png(&path).unwrap();
            let info = png_info(&path);
            assert_eq!(info.bit_depth, png::BitDepth::Sixteen, "{name}");
            assert_eq!(info.icc_profile.as_deref(), Some(&icc[..]), "{name}");
            assert_eq!(
                image::open(&path).unwrap().to_rgb16(),
                DynamicImage::ImageRgba16(deep).to_rgb16(),
                "{name}"
            );
        }
        // The same name with real curves still goes to 8 bits.
        assert!(goes_to_eight_bits(&DeepPng {
            icc: Some(&p3(Some(curv(&[563])))),
            ..DeepPng::default()
        }));
    }

    /// Every chunk of a PNG after its image data has been read to `IEND`.
    fn full_png_info(path: &Path) -> png::Info<'static> {
        let mut reader = png::Decoder::new(std::io::BufReader::new(File::open(path).unwrap()))
            .read_info()
            .unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut buf).unwrap();
        reader.finish().unwrap();
        reader.info().clone()
    }

    /// The type of each chunk in a PNG, in order.
    fn chunk_order(path: &Path) -> Vec<String> {
        let data = std::fs::read(path).unwrap();
        let mut at = 8;
        let mut kinds = Vec::new();
        while at + 8 <= data.len() {
            let len = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
            kinds.push(String::from_utf8_lossy(&data[at + 4..at + 8]).into_owned());
            at += 12 + len;
        }
        kinds
    }

    #[test]
    fn text_and_exif_after_the_image_data_survive_rewrites() {
        let dir = tempfile::tempdir().unwrap();
        let p3 = icc_named("Display P3");
        let text =
            |key: &str, value: &str| (*b"tEXt", [key.as_bytes(), &[0], value.as_bytes()].concat());
        let itxt = (
            *b"iTXt",
            [&b"XML:com.adobe.xmp\0\0\0\0\0"[..], b"<x:xmpmeta/>"].concat(),
        );
        let exif = exif_with_orientation(1);
        let path = dir.path().join("trailing.png");
        let meta = DeepPng {
            icc: Some(&p3),
            trailing: vec![
                text("Comment", "after IDAT"),
                itxt.clone(),
                (*b"eXIf", exif.clone()),
            ],
            ..DeepPng::default()
        };
        deep_png(&path, 64, 48, &meta);
        let original = full_png_info(&path);
        assert_eq!(original.uncompressed_latin1_text.len(), 1);
        let source = PngMeta::read(&path).unwrap();
        assert!(source.info.uncompressed_latin1_text.is_empty());
        assert_eq!(source.trailing_chunks, meta.trailing);

        // A plain rewrite keeps them, and only with metadata.
        let img = image::open(&path).unwrap();
        let kept = dir.path().join("kept.png");
        write_png(&img, &source, &kept, true).unwrap();
        let stripped = dir.path().join("stripped.png");
        write_png(&img, &source, &stripped, false).unwrap();
        let info = full_png_info(&stripped);
        assert!(info.uncompressed_latin1_text.is_empty());
        assert!(info.utf8_text.is_empty());
        assert!(info.exif_metadata.is_none());

        // So does the HEIF optimization, which takes the file to 8 bits.
        optimize_heif_png(&path).unwrap();
        for out in [&kept, &path] {
            let info = full_png_info(out);
            assert_eq!(info.uncompressed_latin1_text.len(), 1, "{out:?}");
            let t = &info.uncompressed_latin1_text[0];
            assert_eq!((&t.keyword[..], &t.text[..]), ("Comment", "after IDAT"));
            assert_eq!(info.utf8_text.len(), 1, "{out:?}");
            assert_eq!(info.utf8_text[0].keyword, "XML:com.adobe.xmp");
            assert_eq!(info.exif_metadata.as_deref(), Some(&exif[..]), "{out:?}");
            let order = chunk_order(out);
            let idat = order.iter().position(|k| k == "IDAT").unwrap();
            assert!(order.iter().position(|k| k == "tEXt").unwrap() > idat);
            assert_eq!(order.last().map(String::as_str), Some("IEND"));
        }
        assert_eq!(png_info(&path).bit_depth, png::BitDepth::Eight);

        // A chunk with a bad CRC after the image data is dropped, not copied
        // with a fresh one.
        let mut data = std::fs::read(&kept).unwrap();
        let at = data.windows(10).position(|w| w == b"after IDAT").unwrap();
        data[at] ^= 0x20;
        let corrupt = dir.path().join("corrupt.png");
        std::fs::write(&corrupt, &data).unwrap();
        assert!(PngMeta::read(&corrupt).unwrap().trailing_chunks.is_empty());
    }

    #[test]
    fn hdr_chunks_after_the_image_data_keep_sixteen_bits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("late_hdr.png");
        let meta = DeepPng {
            srgb: true,
            trailing: hdr10_chunks(),
            ..DeepPng::default()
        };
        deep_png(&path, 64, 48, &meta);
        let source = PngMeta::read(&path).unwrap();
        assert_eq!(source.hdr_chunks, hdr10_chunks());
        assert!(!is_confirmed_sdr_png(&source));
        optimize_heif_png(&path).unwrap();
        assert_eq!(png_info(&path).bit_depth, png::BitDepth::Sixteen);
        // Rewritten before the image data, where PNG wants them.
        assert_eq!(PngMeta::read(&path).unwrap().hdr_chunks, hdr10_chunks());
        let info = png_info(&path);
        assert!(info.mastering_display_color_volume.is_some());
        assert!(info.content_light_level.is_some());
    }

    #[test]
    fn ambiguous_sixteen_bit_png_keeps_its_depth_and_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.png");
        let deep = deep_png(&path, 96, 64, &DeepPng::default());
        optimize_heif_png(&path).unwrap();
        let info = png_info(&path);
        assert_eq!(info.bit_depth, png::BitDepth::Sixteen);
        // The opaque alpha still goes.
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(
            image::open(&path).unwrap().to_rgb16(),
            DynamicImage::ImageRgba16(deep).to_rgb16()
        );
    }

    #[test]
    fn near_opaque_sixteen_bit_alpha_is_not_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let p3 = icc_named("Display P3");
        let cases = [
            ("one pixel at 65530", None, Some(((7, 5), 65530))),
            ("every pixel at 65530", Some(65530), None),
        ];
        for (name, alpha, alpha_at) in cases {
            let path = dir.path().join("alpha.png");
            let meta = DeepPng {
                icc: Some(&p3),
                alpha,
                alpha_at,
                ..DeepPng::default()
            };
            let deep = deep_png(&path, 64, 48, &meta);
            optimize_heif_png(&path).unwrap();
            let info = png_info(&path);
            assert_eq!(info.bit_depth, png::BitDepth::Eight, "{name}");
            assert_eq!(info.color_type, png::ColorType::Rgba, "{name}");
            let back = image::open(&path).unwrap().to_rgba8();
            for (d, b) in deep.pixels().zip(back.pixels()) {
                if d.0[3] == u16::MAX {
                    assert_eq!(b.0[3], 255, "{name}");
                } else {
                    assert_eq!(b.0[3], 254, "{name}");
                }
            }
        }
    }

    #[test]
    fn hdr_png_keeps_depth_and_every_hdr_tag() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hdr10.png");
        let icc = icc_named("Rec. ITU-R BT.2100 PQ");
        let meta = DeepPng {
            icc: Some(&icc),
            cicp: Some(PQ),
            chunks: hdr10_chunks(),
            ..DeepPng::default()
        };
        let deep = deep_png(&path, 64, 48, &meta);
        optimize_heif_png(&path).unwrap();
        let info = png_info(&path);
        // Rewritten (the opaque alpha went), still 16 bits.
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(info.bit_depth, png::BitDepth::Sixteen);
        assert_eq!(info.coding_independent_code_points, Some(PQ));
        assert_eq!(info.icc_profile.as_deref(), Some(&icc[..]));
        let mdcv = info.mastering_display_color_volume.expect("mDCV");
        assert_eq!(mdcv.max_luminance, 10_000_000);
        assert_eq!(mdcv.min_luminance, 50);
        let clli = info.content_light_level.expect("cLLI");
        assert_eq!(clli.max_content_light_level, 10_000_000);
        assert_eq!(clli.max_frame_average_light_level, 4_000_000);
        assert_eq!(PngMeta::read(&path).unwrap().hdr_chunks, hdr10_chunks());
        assert_eq!(
            image::open(&path).unwrap().to_rgb16(),
            DynamicImage::ImageRgba16(deep).to_rgb16()
        );
    }

    #[test]
    fn rotated_wide_gamut_heic_png_is_finished_with_its_profile() {
        use crate::orientation::{decoder_orientation, open_stored};
        use image::metadata::Orientation;
        for orientation in [6u16, 3] {
            let dir = tempfile::tempdir().unwrap();
            // Not a HEIF: what sips was given only matters for its irot,
            // and the orientation here comes from the EXIF sips copied.
            let source = dir.path().join("IMG_0001.heic");
            std::fs::write(&source, b"not a heif").unwrap();
            let path = dir.path().join("IMG_0001.png");
            let p3 = icc_named("Display P3");
            let meta = DeepPng {
                icc: Some(&p3),
                exif: Some(exif_with_orientation(orientation)),
                ..DeepPng::default()
            };
            let deep = deep_png(&path, 96, 64, &meta);
            let before = std::fs::metadata(&path).unwrap().len();
            crate::heic::finish_sips_png("heic", &source, &path, &Options::default()).unwrap();
            let after = std::fs::metadata(&path).unwrap().len();
            assert!(after < before, "{after} >= {before}");

            let info = png_info(&path);
            assert_eq!(info.bit_depth, png::BitDepth::Eight);
            assert_eq!(info.color_type, png::ColorType::Rgb);
            assert_eq!(info.icc_profile.as_deref(), Some(&p3[..]));
            // EXIF stays, with only the orientation reset.
            let mut want_exif = exif_with_orientation(orientation);
            want_exif[18..20].copy_from_slice(&1u16.to_be_bytes());
            assert_eq!(info.exif_metadata.as_deref(), Some(&want_exif[..]));
            assert_eq!(
                decoder_orientation(&path).unwrap(),
                Orientation::NoTransforms
            );
            let dims = info.pixel_dims.unwrap();
            if orientation == 6 {
                assert_eq!((info.width, info.height), (64, 96));
                assert_eq!((dims.xppu, dims.yppu), (5670, 2835));
            } else {
                assert_eq!((info.width, info.height), (96, 64));
                assert_eq!((dims.xppu, dims.yppu), (2835, 5670));
            }

            let mut want = DynamicImage::ImageRgba16(deep);
            want.apply_orientation(Orientation::from_exif(orientation as u8).unwrap());
            let want = want.to_rgb8();
            let back = open_stored(&path).unwrap().to_rgb8();
            assert_eq!(back.dimensions(), want.dimensions());
            for (a, b) in want.pixels().zip(back.pixels()) {
                for i in 0..3 {
                    assert!((a.0[i] as i32 - b.0[i] as i32).abs() <= 1, "{a:?} vs {b:?}");
                }
            }
            let names: Vec<_> = std::fs::read_dir(dir.path())
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect();
            assert_eq!(names.len(), 2, "{names:?}");
        }
    }

    #[test]
    fn a_failed_optimization_keeps_the_sips_png() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("in.heic");
        std::fs::write(&source, b"not a heif").unwrap();
        // A PNG whose image data is cut short: the header reads, the
        // pixels don't, so the optimizer fails partway.
        let path = dir.path().join("out.png");
        let p3 = icc_named("Display P3");
        deep_png(
            &path,
            64,
            48,
            &DeepPng {
                icc: Some(&p3),
                ..DeepPng::default()
            },
        );
        let full = std::fs::read(&path).unwrap();
        let cut = &full[..full.len() - 200];
        std::fs::write(&path, cut).unwrap();
        assert!(optimize_heif_png(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), cut);
        // The conversion still succeeds, with the file sips wrote.
        crate::heic::finish_sips_png("heic", &source, &path, &Options::default()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), cut);
        let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(names.len(), 2);
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
                // Resizing rounds each edge, so the ratio is 4:3 to a pixel.
                let (w, h) = (back.width(), back.height());
                assert!(w < 512 && (w * 3).abs_diff(h * 4) <= 4, "{w}x{h}");
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
    fn color_profiles_survive_compression_and_conversion() {
        use image::{ImageDecoder, ImageEncoder};
        let dir = tempfile::tempdir().unwrap();
        // Any bytes do: encoders embed the profile without parsing it.
        let icc: Vec<u8> = (0..600u32).map(|i| (i % 251) as u8).collect();
        let src = dir.path().join("p3.jpg");
        {
            let file = BufWriter::new(File::create(&src).unwrap());
            let mut enc = JpegEncoder::new_with_quality(file, 95);
            enc.set_icc_profile(icc.clone()).unwrap();
            let px = photo(64, 48).to_rgb8();
            enc.write_image(px.as_raw(), 64, 48, image::ExtendedColorType::Rgb8)
                .unwrap();
        }
        let jpeg = format_by_id("jpeg").unwrap();
        let cancel = Cancel::new();
        for (to, quality) in [
            ("jpeg", Some(60)),
            ("png", None),
            ("webp", None),
            ("webp", Some(100)),
            ("tiff", None),
        ] {
            let out = tempfile::tempdir().unwrap();
            let options = Options {
                quality,
                strip_metadata: true,
                ..Options::default()
            };
            let step = Step {
                from: jpeg,
                to: format_by_id(to).unwrap(),
            };
            let ctx = Ctx::new(step, &options, &|_| {}, &cancel);
            let files = ImageEngine.convert(&ctx, &src, out.path()).unwrap();
            let mut decoder = image::ImageReader::open(&files[0])
                .unwrap()
                .with_guessed_format()
                .unwrap()
                .into_decoder()
                .unwrap();
            if to == "tiff" {
                // The image crate's TIFF decoder doesn't read the tag back,
                // so look for the profile in the file itself.
                let raw = std::fs::read(&files[0]).unwrap();
                assert!(raw.windows(icc.len()).any(|w| w == &icc[..]), "{to}");
            } else {
                assert_eq!(
                    decoder.icc_profile().unwrap().as_deref(),
                    Some(&icc[..]),
                    "{to} {quality:?}"
                );
            }
            assert_eq!(image::open(&files[0]).unwrap().width(), 64, "{to}");
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
        let err = encode_with_meta(
            photo(256, 256),
            "jpeg",
            &options,
            &dir.path().join("a.jpg"),
            &Meta::default(),
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
    fn gif_palette_is_global_so_full_gray_ramps_stay_gray() {
        // Every gray level once, as a rendered page of anti-aliased text has.
        // Pillow reads a frame palette that is exactly this ramp as "no
        // palette" and used the padded black global table instead.
        let ramp = image::GrayImage::from_fn(256, 2, |x, _| image::Luma([x as u8]));
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("ramp.gif");
        encode(
            DynamicImage::ImageLuma8(ramp.clone()),
            "gif",
            &Options::default(),
            &out,
        )
        .unwrap();
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::Indexed);
        let mut decoder = options.read_info(File::open(&out).unwrap()).unwrap();
        let global = decoder.global_palette().unwrap().to_vec();
        let frame = decoder.read_next_frame().unwrap().unwrap();
        assert!(frame.palette.is_none(), "palette written on the frame");
        assert_eq!(global.len(), 256 * 3);
        assert!(
            frame
                .buffer
                .iter()
                .any(|&i| global[i as usize * 3..][..3] != [0, 0, 0]),
            "global table is padding"
        );
        assert_eq!(image::open(&out).unwrap().to_luma8(), ramp);
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
