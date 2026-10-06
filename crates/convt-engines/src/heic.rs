//! HEIC and AVIF decoding and encoding through HEIF containers. On macOS the system's ImageIO (through `sips`)
//! handles it; elsewhere libheif is loaded at runtime from
//! `$CONVT_LIBHEIF_DIR`, next to the executable, or the system library path.
//! Decoders produce PNG; encoders accept PNG when the matching plugin is available.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::{Path, PathBuf};

use convt_core::{Ctx, Engine, Error, Result, Step};
use image::ImageDecoder;
use libloading::{Library, Symbol};

fn failed(e: impl std::fmt::Display) -> Error {
    Error::EngineFailed {
        engine: "libheif",
        message: e.to_string(),
    }
}

#[cfg(target_os = "linux")]
const LIB_NAMES: &[&str] = &["libheif.so.1", "libheif.so"];
#[cfg(target_os = "macos")]
const LIB_NAMES: &[&str] = &["libheif.1.dylib", "libheif.dylib"];
#[cfg(windows)]
const LIB_NAMES: &[&str] = &["heif.dll", "libheif.dll"];
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
const LIB_NAMES: &[&str] = &["libheif.so.1"];

// From libheif/heif.h. The enums are C ints.
const COLORSPACE_RGB: c_int = 1;
const CHROMA_INTERLEAVED_RGB: c_int = 10;
const CHROMA_INTERLEAVED_RGBA: c_int = 11;
const CHANNEL_INTERLEAVED: c_int = 10;
const CHROMA_INTERLEAVED_RGBA_16LE: c_int = 15;
const UNSUPPORTED_BIT_DEPTH: c_int = 4000;

#[repr(C)]
struct HeifError {
    code: c_int,
    subcode: c_int,
    message: *const c_char,
}

impl HeifError {
    fn check(self) -> Result<()> {
        if self.code == 0 {
            return Ok(());
        }
        let message = if self.message.is_null() {
            format!("libheif error {}", self.code)
        } else {
            // SAFETY: libheif returns a static, NUL-terminated string.
            unsafe { CStr::from_ptr(self.message) }
                .to_string_lossy()
                .into_owned()
        };
        Err(failed(message))
    }
}

type Opaque = c_void;

/// Decodes HEIC and AVIF with libheif, which carries its own HEVC and AV1 decoders.
pub struct LibheifEngine {
    lib: std::result::Result<Library, String>,
}

fn absolute_override(name: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os(name)?);
    if dir.is_absolute() {
        Some(dir)
    } else {
        eprintln!(
            "convt: ignoring {name}={dir:?}: native library overrides must be absolute paths"
        );
        None
    }
}

fn plugin_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<_> = crate::paths::bundle_dirs()
        .into_iter()
        .filter(|p| p.is_absolute())
        .map(|p| p.join("libheif/plugins"))
        .collect();
    if let Some(dir) = absolute_override("CONVT_LIBHEIF_PLUGIN_DIR") {
        dirs.push(dir);
    }
    dirs
}

fn initialize(lib: &Library) -> std::result::Result<(), String> {
    // No libheif API may run before this initializer. Upstream heif_init and
    // even context allocation can load plugins from the inherited environment.
    // SAFETY: the bundled patch exports struct heif_error fn(void).
    unsafe {
        let init = lib
            .get::<unsafe extern "C" fn() -> HeifError>(b"heif_convt_init_no_plugins\0")
            .map_err(|_| "libheif lacks heif_convt_init_no_plugins; unpatched system libraries are unsupported".to_owned())?;
        init().check().map_err(|e| e.to_string())?;
        for dir in plugin_dirs() {
            if let Ok(path) = CString::new(dir.as_os_str().as_encoded_bytes()) {
                // Signature from libheif 1.17 heif.h; path lives through the call.
                if let Ok(load) = lib.get::<unsafe extern "C" fn(
                    *const c_char,
                    *mut *mut Opaque,
                    *mut c_int,
                    c_int,
                ) -> HeifError>(b"heif_load_plugins\0")
                {
                    let _ = load(path.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), 0);
                }
            }
        }
    }
    Ok(())
}

impl LibheifEngine {
    pub fn new() -> Self {
        let lib = load().and_then(|lib| {
            initialize(&lib)?;
            Ok(lib)
        });
        Self { lib }
    }

    /// Probe an actual encoder instance, rather than trusting a registered descriptor.
    pub fn supports_output(&self, id: &str) -> bool {
        let format = match id {
            "heic" => 1,
            "avif" => 4,
            _ => return false,
        };
        let Ok(lib) = &self.lib else { return false };
        encoder(lib, format).is_ok()
    }

    /// Whether the loaded library has a decoder plugin for this input format.
    pub fn supports_input(&self, id: &str) -> bool {
        let format = match id {
            "heic" => 1,
            "avif" => 4,
            _ => return false,
        };
        let Ok(lib) = &self.lib else { return false };
        // SAFETY: matches int heif_have_decoder_for_format(enum heif_compression_format).
        unsafe {
            lib.get::<unsafe extern "C" fn(c_int) -> c_int>(b"heif_have_decoder_for_format\0")
                .is_ok_and(|available| available(format) != 0)
        }
    }
}

impl Drop for LibheifEngine {
    fn drop(&mut self) {
        if let Ok(lib) = &self.lib {
            // Balance the secure initializer's reference count while loaded.
            // SAFETY: matches void heif_deinit(void).
            unsafe {
                if let Ok(deinit) = lib.get::<unsafe extern "C" fn()>(b"heif_deinit\0") {
                    deinit();
                }
            }
        }
    }
}

impl Default for LibheifEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn load() -> std::result::Result<Library, String> {
    let mut dirs = Vec::new();
    if let Some(d) = absolute_override("CONVT_LIBHEIF_DIR") {
        dirs.push(d);
    }
    if let Some(d) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        dirs.push(d);
    }
    dirs.extend(crate::paths::bundle_dirs());
    let mut last = String::from("no candidates");
    let candidates = dirs
        .iter()
        .flat_map(|d| LIB_NAMES.iter().map(move |n| d.join(n)))
        .filter(|p| p.exists())
        .chain(LIB_NAMES.iter().map(PathBuf::from));
    for path in candidates {
        // SAFETY: loading libheif runs only its library initialisers.
        match unsafe { Library::new(&path) } {
            Ok(lib) => return Ok(lib),
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

/// Frees libheif objects when decoding returns early.
struct Guard<'l> {
    ptr: *mut Opaque,
    free: Symbol<'l, unsafe extern "C" fn(*mut Opaque)>,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: `ptr` came from the matching libheif allocator and is
            // freed exactly once.
            unsafe { (self.free)(self.ptr) }
        }
    }
}

/// Looks up a libheif function. The caller picks the signature.
unsafe fn sym<'l, T>(lib: &'l Library, name: &[u8]) -> Result<Symbol<'l, T>> {
    // SAFETY: forwarded to the caller, who states the C signature as `T`.
    unsafe { lib.get(name) }.map_err(failed)
}

fn decode(lib: &Library, input: &Path) -> Result<image::DynamicImage> {
    let path = CString::new(input.as_os_str().as_encoded_bytes())
        .map_err(|_| failed("path contains a NUL byte"))?;
    // SAFETY: every signature below matches libheif/heif.h. Pointers come
    // from libheif and are released by the guards in reverse order.
    unsafe {
        let ctx_alloc: Symbol<unsafe extern "C" fn() -> *mut Opaque> =
            sym(lib, b"heif_context_alloc\0")?;
        let ctx = Guard {
            ptr: ctx_alloc(),
            free: sym(lib, b"heif_context_free\0")?,
        };
        if ctx.ptr.is_null() {
            return Err(failed("could not allocate a context"));
        }
        let read: Symbol<
            unsafe extern "C" fn(*mut Opaque, *const c_char, *const Opaque) -> HeifError,
        > = sym(lib, b"heif_context_read_from_file\0")?;
        read(ctx.ptr, path.as_ptr(), std::ptr::null()).check()?;

        let primary: Symbol<unsafe extern "C" fn(*mut Opaque, *mut *mut Opaque) -> HeifError> =
            sym(lib, b"heif_context_get_primary_image_handle\0")?;
        let mut handle = Guard {
            ptr: std::ptr::null_mut(),
            free: sym(lib, b"heif_image_handle_release\0")?,
        };
        primary(ctx.ptr, &mut handle.ptr).check()?;

        let has_alpha: Symbol<unsafe extern "C" fn(*const Opaque) -> c_int> =
            sym(lib, b"heif_image_handle_has_alpha_channel\0")?;
        let alpha = has_alpha(handle.ptr) != 0;
        let chroma = if alpha {
            CHROMA_INTERLEAVED_RGBA
        } else {
            CHROMA_INTERLEAVED_RGB
        };
        // Default decoding options apply the rotation and mirroring stored in
        // the file, so photos come out upright.
        let decode: Symbol<
            unsafe extern "C" fn(
                *const Opaque,
                *mut *mut Opaque,
                c_int,
                c_int,
                *const Opaque,
            ) -> HeifError,
        > = sym(lib, b"heif_decode_image\0")?;
        let mut img = Guard {
            ptr: std::ptr::null_mut(),
            free: sym(lib, b"heif_image_release\0")?,
        };
        decode(
            handle.ptr,
            &mut img.ptr,
            COLORSPACE_RGB,
            chroma,
            std::ptr::null(),
        )
        .check()?;

        let dim: Symbol<unsafe extern "C" fn(*const Opaque, c_int) -> c_int> =
            sym(lib, b"heif_image_get_width\0")?;
        let width = dim(img.ptr, CHANNEL_INTERLEAVED);
        let dim: Symbol<unsafe extern "C" fn(*const Opaque, c_int) -> c_int> =
            sym(lib, b"heif_image_get_height\0")?;
        let height = dim(img.ptr, CHANNEL_INTERLEAVED);
        let plane: Symbol<unsafe extern "C" fn(*const Opaque, c_int, *mut c_int) -> *const u8> =
            sym(lib, b"heif_image_get_plane_readonly\0")?;
        let mut stride: c_int = 0;
        let data = plane(img.ptr, CHANNEL_INTERLEAVED, &mut stride);
        let channels = if alpha { 4 } else { 3 };
        if data.is_null() || width <= 0 || height <= 0 || (stride as i64) < width as i64 * channels
        {
            return Err(failed("decoded image has no pixel data"));
        }
        let (w, h, stride) = (width as usize, height as usize, stride as usize);
        let row = w * channels as usize;
        let mut pixels = Vec::with_capacity(row * h);
        for y in 0..h {
            // SAFETY: libheif guarantees `height` rows of `stride` bytes.
            pixels.extend_from_slice(std::slice::from_raw_parts(data.add(y * stride), row));
        }
        let (w, h) = (w as u32, h as u32);
        Ok(if alpha {
            image::RgbaImage::from_raw(w, h, pixels).map(image::DynamicImage::ImageRgba8)
        } else {
            image::RgbImage::from_raw(w, h, pixels).map(image::DynamicImage::ImageRgb8)
        }
        .expect("buffer matches dimensions"))
    }
}

// SAFETY in these helpers: signatures match libheif/heif.h, and guards own
// each pointer from its matching allocator for no longer than the Library.
fn encoder(lib: &Library, format: c_int) -> Result<(Guard<'_>, Guard<'_>)> {
    unsafe {
        let alloc: Symbol<unsafe extern "C" fn() -> *mut Opaque> =
            sym(lib, b"heif_context_alloc\0")?;
        let ctx = Guard {
            ptr: alloc(),
            free: sym(lib, b"heif_context_free\0")?,
        };
        if ctx.ptr.is_null() {
            return Err(failed("could not allocate context"));
        }
        let mut enc = Guard {
            ptr: std::ptr::null_mut(),
            free: sym(lib, b"heif_encoder_release\0")?,
        };
        let get: Symbol<unsafe extern "C" fn(*mut Opaque, c_int, *mut *mut Opaque) -> HeifError> =
            sym(lib, b"heif_context_get_encoder_for_format\0")?;
        get(ctx.ptr, format, &mut enc.ptr).check()?;
        if enc.ptr.is_null() {
            return Err(failed("encoder returned no instance"));
        }
        Ok((ctx, enc))
    }
}

#[repr(C)]
struct Nclx {
    version: u8,
    primaries: c_int,
    transfer: c_int,
    matrix: c_int,
    full_range: u8,
    chromaticities: [f32; 8],
}

// Prefix through version 4 of heif_encoding_options. Allocate the full
// versioned object in libheif and leave later fields at their library defaults.
#[repr(C)]
struct EncodingOptions {
    version: u8,
    save_alpha: u8,
    macos_workaround: u8,
    save_two_profiles: u8,
    output_nclx: *mut Nclx,
    omit_nclx: u8,
}

#[derive(Debug, PartialEq)]
enum EncodeAttempt {
    Written,
    UnsupportedBitDepth,
}

#[derive(Clone, Copy)]
struct Colour {
    primaries: u16,
    transfer: u16,
    gamma: Option<f32>,
    to_srgb: Option<[[f64; 3]; 3]>,
}

// ImageDecoder exposes ICC but not PNG cICP/cHRM. Read only bounded metadata
// before IDAT; image's PNG decoder still validates/decodes the actual image.
fn png_colour(input: &Path) -> Result<Option<Colour>> {
    use std::io::Read;
    let mut file = std::fs::File::open(input)?;
    let mut signature = [0; 8];
    if file.read_exact(&mut signature).is_err() || &signature != b"\x89PNG\r\n\x1a\n" {
        return Ok(None);
    }
    let mut colour = Colour {
        primaries: 1,
        transfer: 13,
        gamma: None,
        to_srgb: None,
    };
    let (mut cicp, mut srgb, mut gamma, mut chroma) = (None, false, None, None);
    loop {
        let mut header = [0; 8];
        file.read_exact(&mut header)?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let kind = &header[4..];
        if kind == b"IDAT" || kind == b"IEND" {
            break;
        }
        if matches!(kind, b"cICP" | b"sRGB" | b"gAMA" | b"cHRM") {
            if length > 32 {
                return Err(failed("invalid PNG colour metadata length"));
            }
            let mut data = vec![0; length];
            file.read_exact(&mut data)?;
            match kind {
                b"cICP" if length == 4 => {
                    if data[2] != 0 || data[3] != 1 {
                        return Err(failed("PNG cICP must describe full-range RGB"));
                    }
                    cicp = Some((u16::from(data[0]), u16::from(data[1])));
                }
                b"sRGB" if length == 1 => srgb = true,
                b"gAMA" if length == 4 => {
                    gamma = Some(u32::from_be_bytes(data.try_into().unwrap()))
                }
                b"cHRM" if length == 32 => chroma = Some(data),
                _ => return Err(failed("invalid PNG colour metadata")),
            }
            let mut crc = [0; 4];
            file.read_exact(&mut crc)?;
        } else {
            // Skip through a bounded stack buffer, without allocating chunk length.
            let copied = std::io::copy(
                &mut (&mut file).take(length as u64 + 4),
                &mut std::io::sink(),
            )?;
            if copied != length as u64 + 4 {
                return Err(failed("truncated PNG chunk"));
            }
        }
    }
    if let Some((primaries, transfer)) = cicp {
        colour.primaries = primaries;
        colour.transfer = transfer;
    } else if !srgb {
        if let Some(chroma) = chroma {
            let coordinates: Vec<u32> = chroma
                .chunks_exact(4)
                .map(|c| u32::from_be_bytes(c.try_into().unwrap()))
                .collect();
            colour.primaries = match coordinates.as_slice() {
                [31270, 32900, 64000, 33000, 30000, 60000, 15000, 6000] => 1,
                [31000, 31600, 67000, 33000, 21000, 71000, 14000, 8000] => 4,
                [31270, 32900, 64000, 33000, 29000, 60000, 15000, 6000] => 5,
                // BT.601 and SMPTE 240M share coordinates; cHRM alone cannot
                // distinguish codes 6 and 7. Use BT.601, retaining the transfer.
                [31270, 32900, 63000, 34000, 31000, 59500, 15500, 7000] => 6,
                [31000, 31600, 68100, 31900, 24300, 69200, 14500, 4900] => 8,
                [31270, 32900, 70800, 29200, 17000, 79700, 13100, 4600] => 9,
                [33333, 33333, 100000, 0, 0, 100000, 0, 0] => 10,
                [31400, 35100, 68000, 32000, 26500, 69000, 15000, 6000] => 11,
                [31270, 32900, 68000, 32000, 26500, 69000, 15000, 6000] => 12,
                [31270, 32900, 63000, 34000, 29500, 60500, 15500, 7700] => 22,
                _ => {
                    let xy: [f64; 8] =
                        std::array::from_fn(|i| f64::from(coordinates[i]) / 100000.0);
                    colour.to_srgb = Some(rgb_to_srgb_matrix(xy)?);
                    1
                }
            };
        }
        match gamma {
            Some(100000) => colour.transfer = 8,
            Some(45455) => colour.transfer = 4, // BT.470M: gamma 2.2.
            Some(35714) => colour.transfer = 5, // BT.470BG: gamma 2.8.
            None => {}
            Some(0) => return Err(failed("PNG gamma must be positive")),
            Some(value) => colour.gamma = Some(value as f32 / 100000.0),
        }
    }
    Ok(Some(colour))
}

type Matrix = [[f64; 3]; 3];

fn matrix_vector(matrix: Matrix, vector: [f64; 3]) -> [f64; 3] {
    matrix.map(|row| row.into_iter().zip(vector).map(|(a, b)| a * b).sum())
}

fn matrix_product(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

fn inverse(matrix: Matrix) -> Result<Matrix> {
    let [[a, b, c], [d, e, f], [g, h, i]] = matrix;
    let adjugate = [
        [e * i - f * h, c * h - b * i, b * f - c * e],
        [f * g - d * i, a * i - c * g, c * d - a * f],
        [d * h - e * g, b * g - a * h, a * e - b * d],
    ];
    let determinant = a * adjugate[0][0] + b * adjugate[1][0] + c * adjugate[2][0];
    if !determinant.is_finite() || determinant.abs() < 1e-12 {
        return Err(failed("degenerate PNG chromaticities"));
    }
    Ok(adjugate.map(|row| row.map(|v| v / determinant)))
}

// cHRM is quantized to 1/100000. Reject invalid or degenerate coordinates
// before division. All work is on fixed-size matrices, independent of image size.
fn rgb_to_srgb_matrix(xy: [f64; 8]) -> Result<Matrix> {
    let xyz = |x: f64, y: f64| -> Result<[f64; 3]> {
        if !x.is_finite() || !y.is_finite() || x < 0.0 || y <= 0.0 || x + y > 1.0 {
            return Err(failed("invalid PNG chromaticities"));
        }
        Ok([x / y, 1.0, (1.0 - x - y) / y])
    };
    let white = xyz(xy[0], xy[1])?;
    // Primary luminance may be zero (XYZ primaries). Use homogeneous
    // chromaticity vectors; normalize only the white, whose Y must be positive.
    let primary = |x: f64, y: f64| -> Result<[f64; 3]> {
        if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 || x + y > 1.0 {
            return Err(failed("invalid PNG chromaticities"));
        }
        Ok([x, y, 1.0 - x - y])
    };
    let primaries = [
        primary(xy[2], xy[3])?,
        primary(xy[4], xy[5])?,
        primary(xy[6], xy[7])?,
    ];
    let basis = std::array::from_fn(|i| std::array::from_fn(|j| primaries[j][i]));
    let scale = matrix_vector(inverse(basis)?, white);
    if scale.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err(failed("PNG white point lies outside its RGB primaries"));
    }
    let rgb_xyz = std::array::from_fn(|i| std::array::from_fn(|j| basis[i][j] * scale[j]));
    // Bradford adaptation moves the source white to sRGB's D65 white.
    let bradford = [
        [0.8951, 0.2664, -0.1614],
        [-0.7502, 1.7135, 0.0367],
        [0.0389, -0.0685, 1.0296],
    ];
    let source_cones = matrix_vector(bradford, white);
    let target_cones = matrix_vector(bradford, xyz(0.3127, 0.3290)?);
    if source_cones.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err(failed("invalid PNG white point"));
    }
    let adaptation = matrix_product(
        inverse(bradford)?,
        std::array::from_fn(|i| bradford[i].map(|v| v * target_cones[i] / source_cones[i])),
    );
    // Derive the destination matrix from the same quantized sRGB coordinates,
    // so a white/neutral input stays neutral without rounded XYZ constants.
    let srgb_basis = [
        [0.64 / 0.33, 0.30 / 0.60, 0.15 / 0.06],
        [1.0, 1.0, 1.0],
        [
            (1.0 - 0.64 - 0.33) / 0.33,
            (1.0 - 0.30 - 0.60) / 0.60,
            (1.0 - 0.15 - 0.06) / 0.06,
        ],
    ];
    let srgb_scale = matrix_vector(inverse(srgb_basis)?, xyz(0.3127, 0.3290)?);
    let srgb_xyz =
        std::array::from_fn(|i| std::array::from_fn(|j| srgb_basis[i][j] * srgb_scale[j]));
    let transform = matrix_product(inverse(srgb_xyz)?, matrix_product(adaptation, rgb_xyz));
    if transform.iter().flatten().any(|v| !v.is_finite()) {
        return Err(failed("invalid PNG colour transform"));
    }
    Ok(transform)
}

fn convert_colour(pixels: image::DynamicImage, colour: &mut Colour) -> image::DynamicImage {
    if colour.gamma.is_none() && colour.to_srgb.is_none() {
        return pixels;
    }
    let mut converted = pixels.to_rgba32f();
    for pixel in converted.pixels_mut() {
        let linear = std::array::from_fn(|i| {
            let value = f64::from(pixel.0[i]);
            if let Some(gamma) = colour.gamma {
                value.powf(1.0 / f64::from(gamma))
            } else {
                match colour.transfer {
                    8 => value,
                    4 => value.powf(2.2),
                    5 => value.powf(2.8),
                    _ if value <= 0.04045 => value / 12.92,
                    _ => ((value + 0.055) / 1.055).powf(2.4),
                }
            }
        });
        let linear = colour
            .to_srgb
            .map_or(linear, |matrix| matrix_vector(matrix, linear));
        for (channel, value) in pixel.0[..3].iter_mut().zip(linear) {
            // Clip out-of-gamut colours to sRGB; alpha never participates.
            let value = value.clamp(0.0, 1.0);
            *channel = if value <= 0.0031308 {
                (12.92 * value) as f32
            } else {
                (1.055 * value.powf(1.0 / 2.4) - 0.055) as f32
            };
        }
    }
    colour.transfer = 13;
    if colour.to_srgb.is_some() {
        colour.primaries = 1;
    }
    image::DynamicImage::ImageRgba16(image::DynamicImage::ImageRgba32F(converted).to_rgba16())
}

fn encode(lib: &Library, input: &Path, ctx: &Ctx, output: &Path) -> Result<()> {
    match encode_depth(lib, input, ctx, output, false)? {
        EncodeAttempt::Written => Ok(()),
        EncodeAttempt::UnsupportedBitDepth => {
            // Retry HEIC only for the encoder's explicit unsupported-depth error.
            // The first attempt has released every object before the retry.
            encode_depth(lib, input, ctx, output, true).map(|_| ())
        }
    }
}

fn encode_depth(
    lib: &Library,
    input: &Path,
    ctx: &Ctx,
    output: &Path,
    fallback: bool,
) -> Result<EncodeAttempt> {
    let mut decoder = image::ImageReader::open(input)?
        .with_guessed_format()?
        .into_decoder()
        .map_err(failed)?;
    let orientation = decoder.orientation().map_err(failed)?;
    let profile = decoder.icc_profile().map_err(failed)?;
    let mut pixels = image::DynamicImage::from_decoder(decoder).map_err(failed)?;
    pixels.apply_orientation(orientation);
    let source_space = pixels.color_space();
    let mut colour = if profile.is_none() {
        png_colour(input)?.unwrap_or(Colour {
            primaries: source_space.primaries as u16,
            transfer: source_space.transfer as u16,
            gamma: None,
            to_srgb: None,
        })
    } else {
        Colour {
            primaries: 1,
            transfer: 13,
            gamma: None,
            to_srgb: None,
        }
    };
    pixels = convert_colour(pixels, &mut colour);
    let deep = !fallback
        && matches!(
            pixels.color(),
            image::ColorType::L16
                | image::ColorType::La16
                | image::ColorType::Rgb16
                | image::ColorType::Rgba16
                | image::ColorType::Rgb32F
                | image::ColorType::Rgba32F
        );
    let pixels = crate::image::fit(pixels, ctx.options.max_size);
    let (width, height) = (pixels.width(), pixels.height());
    let bytes = if deep {
        pixels
            .to_rgba16()
            .as_raw()
            .iter()
            .flat_map(|&v| (((u32::from(v) * 1023 + 32767) / 65535) as u16).to_le_bytes())
            .collect::<Vec<_>>()
    } else {
        pixels.to_rgba8().into_raw()
    };
    let width = c_int::try_from(width).map_err(failed)?;
    let height = c_int::try_from(height).map_err(failed)?;
    let (context, enc) = encoder(lib, if ctx.step.to.id == "heic" { 1 } else { 4 })?;
    unsafe {
        let quality: Symbol<unsafe extern "C" fn(*mut Opaque, c_int) -> HeifError> =
            sym(lib, b"heif_encoder_set_lossy_quality\0")?;
        quality(enc.ptr, ctx.options.quality.unwrap_or(90).into()).check()?;
        let param: Symbol<
            unsafe extern "C" fn(*mut Opaque, *const c_char, *const c_char) -> HeifError,
        > = sym(lib, b"heif_encoder_set_parameter_string\0")?;
        // Ignored by encoders other than x265. A still image has one frame,
        // so frame threads add memory but no speed; x265's default thread
        // pool still runs wavefront rows in parallel. The bundled x265 is
        // built without libnuma, so its pool never calls set_mempolicy.
        let _ = param(enc.ptr, c"x265:frame-threads".as_ptr(), c"1".as_ptr());
        let mut img = Guard {
            ptr: std::ptr::null_mut(),
            free: sym(lib, b"heif_image_release\0")?,
        };
        let create: Symbol<
            unsafe extern "C" fn(c_int, c_int, c_int, c_int, *mut *mut Opaque) -> HeifError,
        > = sym(lib, b"heif_image_create\0")?;
        create(
            width,
            height,
            COLORSPACE_RGB,
            if deep {
                CHROMA_INTERLEAVED_RGBA_16LE
            } else {
                CHROMA_INTERLEAVED_RGBA
            },
            &mut img.ptr,
        )
        .check()?;
        let add: Symbol<
            unsafe extern "C" fn(*mut Opaque, c_int, c_int, c_int, c_int) -> HeifError,
        > = sym(lib, b"heif_image_add_plane\0")?;
        add(
            img.ptr,
            CHANNEL_INTERLEAVED,
            width,
            height,
            if deep { 10 } else { 8 },
        )
        .check()?;
        let plane: Symbol<unsafe extern "C" fn(*mut Opaque, c_int, *mut c_int) -> *mut u8> =
            sym(lib, b"heif_image_get_plane\0")?;
        let mut stride = 0;
        let data = plane(img.ptr, CHANNEL_INTERLEAVED, &mut stride);
        let row = width as usize * if deep { 8 } else { 4 };
        if data.is_null() || (stride as i64) < row as i64 {
            return Err(failed("invalid encoder plane"));
        }
        for y in 0..height as usize {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr().add(y * row),
                data.add(y * stride as usize),
                row,
            );
        }
        if let Some(profile) = &profile {
            let set: Symbol<
                unsafe extern "C" fn(*mut Opaque, *const c_char, *const c_void, usize) -> HeifError,
            > = sym(lib, b"heif_image_set_raw_color_profile\0")?;
            set(
                img.ptr,
                c"prof".as_ptr(),
                profile.as_ptr().cast(),
                profile.len(),
            )
            .check()?;
        }
        let mut output_nclx = None;
        if profile.is_none() {
            let alloc: Symbol<unsafe extern "C" fn() -> *mut Nclx> =
                sym(lib, b"heif_nclx_color_profile_alloc\0")?;
            let nclx = Guard {
                ptr: alloc().cast(),
                free: sym(lib, b"heif_nclx_color_profile_free\0")?,
            };
            if nclx.ptr.is_null() {
                return Err(failed("could not allocate NCLX"));
            }
            for (name, value) in [
                (
                    &b"heif_nclx_color_profile_set_color_primaries\0"[..],
                    colour.primaries,
                ),
                (
                    &b"heif_nclx_color_profile_set_transfer_characteristics\0"[..],
                    colour.transfer,
                ),
                // Encoders consume YCbCr; identity RGB would be invalid for
                // subsampled AV1 and can abort libaom. BT.601 is libheif's default.
                (&b"heif_nclx_color_profile_set_matrix_coefficients\0"[..], 6),
            ] {
                let set: Symbol<unsafe extern "C" fn(*mut Opaque, u16) -> HeifError> =
                    sym(lib, name)?;
                set(nclx.ptr, value).check()?;
            }
            (*(nclx.ptr.cast::<Nclx>())).full_range = 1;
            let set: Symbol<unsafe extern "C" fn(*mut Opaque, *const Opaque) -> HeifError> =
                sym(lib, b"heif_image_set_nclx_color_profile\0")?;
            set(img.ptr, nclx.ptr).check()?;
            output_nclx = Some(nclx);
        }
        let alloc: Symbol<unsafe extern "C" fn() -> *mut Opaque> =
            sym(lib, b"heif_encoding_options_alloc\0")?;
        let options = Guard {
            ptr: alloc(),
            free: sym(lib, b"heif_encoding_options_free\0")?,
        };
        if options.ptr.is_null() {
            return Err(failed("could not allocate encoding options"));
        }
        let prefix = &mut *options.ptr.cast::<EncodingOptions>();
        if prefix.version < 4 {
            return Err(failed("libheif encoding options lack NCLX support"));
        }
        prefix.output_nclx = output_nclx
            .as_ref()
            .map_or(std::ptr::null_mut(), |p| p.ptr.cast());
        prefix.omit_nclx = 0;
        ctx.check()?;
        let mut handle = Guard {
            ptr: std::ptr::null_mut(),
            free: sym(lib, b"heif_image_handle_release\0")?,
        };
        let encode: Symbol<
            unsafe extern "C" fn(
                *mut Opaque,
                *const Opaque,
                *mut Opaque,
                *const Opaque,
                *mut *mut Opaque,
            ) -> HeifError,
        > = sym(lib, b"heif_context_encode_image\0")?;
        let result = encode(context.ptr, img.ptr, enc.ptr, options.ptr, &mut handle.ptr);
        if deep
            && ctx.step.to.id == "heic"
            && result.code != 0
            && result.subcode == UNSUPPORTED_BIT_DEPTH
        {
            return Ok(EncodeAttempt::UnsupportedBitDepth);
        }
        result.check()?;
        ctx.check()?;
        let path = CString::new(output.as_os_str().as_encoded_bytes()).map_err(failed)?;
        let write: Symbol<unsafe extern "C" fn(*mut Opaque, *const c_char) -> HeifError> =
            sym(lib, b"heif_context_write_to_file\0")?;
        write(context.ptr, path.as_ptr()).check()?;
    }
    Ok(EncodeAttempt::Written)
}

fn decoder_steps(hevc: bool, av1: bool) -> Vec<Step> {
    let inputs: Vec<_> = [("heic", hevc), ("avif", av1)]
        .into_iter()
        .filter_map(|(id, available)| available.then_some(id))
        .collect();
    crate::steps(&inputs, &["png"]).collect()
}

impl Engine for LibheifEngine {
    fn id(&self) -> &'static str {
        "libheif"
    }

    fn priority(&self) -> i32 {
        10
    }

    fn unavailable_reason(&self) -> Option<String> {
        self.lib.as_ref().err().cloned().or_else(|| {
            (!self.supports_input("heic")
                && !self.supports_input("avif")
                && !self.supports_output("heic")
                && !self.supports_output("avif"))
            .then(|| "libheif has no HEVC or AV1 decoder or encoder plugin".into())
        })
    }

    fn steps(&self) -> Vec<Step> {
        let mut steps = decoder_steps(self.supports_input("heic"), self.supports_input("avif"));
        for target in ["heic", "avif"] {
            if self.supports_output(target) {
                steps.extend(crate::steps(crate::image::INPUTS, &[target]));
            }
        }
        steps
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let lib = self
            .lib
            .as_ref()
            .map_err(|reason| Error::EngineUnavailable {
                engine: "libheif",
                reason: reason.clone(),
            })?;
        ctx.check()?;
        if ctx.step.to.id != "png" {
            let output = ctx.artifact(out_dir, 0);
            encode(lib, input, ctx, &output)?;
            return Ok(vec![output]);
        }
        let img = decode(lib, input)?;
        ctx.check()?;
        ctx.progress(0.7);
        let output = ctx.artifact(out_dir, 0);
        let img = crate::image::fit(img, ctx.options.max_size);
        crate::image::encode(img, "png", ctx.options, &output)?;
        Ok(vec![output])
    }
}

/// Converts HEIC with macOS's own codecs through `sips`, and decodes AVIF,
/// which libheif would otherwise need on macOS. PNG is the only format on
/// the other side: sips flattens alpha onto black when it writes JPEG, and
/// reads float TIFF (what the `image` engine writes for EXR) as linear light,
/// so routes through either would change the picture. The `image` engine
/// converts between PNG and everything else.
#[cfg(target_os = "macos")]
pub struct ImageIoEngine {
    sips: Option<PathBuf>,
}

#[cfg(target_os = "macos")]
impl ImageIoEngine {
    pub fn new() -> Self {
        let sips = PathBuf::from("/usr/bin/sips");
        Self {
            sips: sips.exists().then_some(sips),
        }
    }
}

#[cfg(target_os = "macos")]
impl Default for ImageIoEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl Engine for ImageIoEngine {
    fn id(&self) -> &'static str {
        "imageio"
    }

    fn priority(&self) -> i32 {
        20
    }

    fn unavailable_reason(&self) -> Option<String> {
        self.sips
            .is_none()
            .then(|| "/usr/bin/sips not found".into())
    }

    fn steps(&self) -> Vec<Step> {
        crate::steps(&["heic", "avif"], &["png"])
            .chain(crate::steps(&["png"], &["heic"]))
            .collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let sips = self.sips.as_ref().ok_or(Error::EngineUnavailable {
            engine: "imageio",
            reason: "sips not found".into(),
        })?;
        ctx.indeterminate();
        let output = ctx.artifact(out_dir, 0);
        let format = ctx.step.to.id;
        let mut cmd = std::process::Command::new(sips);
        cmd.args(["-s", "format", format]);
        if matches!(format, "jpeg" | "heic") {
            let q = ctx.options.quality.unwrap_or(90).to_string();
            cmd.args(["-s", "formatOptions", &q]);
        }
        if let Some(max) = ctx.options.max_size {
            cmd.args(["-Z", &max.to_string()]);
        }
        cmd.arg(input).arg("--out").arg(&output);
        crate::run_tool("imageio", cmd, ctx, |_| {})?;
        Ok(vec![output])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use convt_core::{Cancel, Options, format_by_id};

    fn decoded(engine: &LibheifEngine, input: &Path) -> Result<image::DynamicImage> {
        let dir = tempfile::tempdir()?;
        let options = Options::default();
        let cancel = Cancel::new();
        let step = Step {
            from: format_by_id("heic").unwrap(),
            to: format_by_id("png").unwrap(),
        };
        let files = engine.convert(
            &Ctx::new(step, &options, &|_| {}, &cancel),
            input,
            dir.path(),
        )?;
        image::open(&files[0]).map_err(failed)
    }

    fn encoded_fixture(input: &Path, output: &Path, target: &str) -> bool {
        let engine = LibheifEngine::new();
        if !engine.supports_output(target) {
            assert!(
                std::env::var_os("CONVT_TEST_REQUIRE_HEIF").is_none(),
                "required {target} encoder missing: {:?}",
                engine.unavailable_reason()
            );
            eprintln!(
                "skipping {target} encoding: {:?}",
                engine.unavailable_reason()
            );
            return false;
        }
        let step = Step {
            from: format_by_id("png").unwrap(),
            to: format_by_id(target).unwrap(),
        };
        let options = Options {
            quality: Some(100),
            ..Options::default()
        };
        let cancel = Cancel::new();
        engine
            .convert(&Ctx::new(step, &options, &|_| {}, &cancel), input, output)
            .unwrap();
        true
    }

    fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc = !0u32;
        for &byte in &out[4..] {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        out.extend_from_slice(&(!crc).to_be_bytes());
        out
    }

    #[test]
    fn linear_gamma_png_keeps_linear_transfer() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("linear.png");
        image::RgbImage::from_pixel(32, 32, image::Rgb([128, 128, 128]))
            .save(&input)
            .unwrap();
        let mut png = std::fs::read(&input).unwrap();
        png.splice(33..33, png_chunk(b"gAMA", &100000u32.to_be_bytes()));
        std::fs::write(&input, png).unwrap();
        assert_eq!(png_colour(&input).unwrap().unwrap().transfer, 8);
        if !encoded_fixture(&input, dir.path(), "heic") {
            return;
        }
        let heic = std::fs::read(dir.path().join("1.heic")).unwrap();
        let nclx = heic.windows(4).position(|p| p == b"nclx").unwrap();
        assert_eq!(u16::from_be_bytes([heic[nclx + 6], heic[nclx + 7]]), 8);
    }

    #[test]
    fn sixteen_bit_png_uses_ten_bit_avif() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("deep.png");
        image::ImageBuffer::<image::Rgb<u16>, _>::from_pixel(
            32,
            32,
            image::Rgb([32768, 12345, 54321]),
        )
        .save(&input)
        .unwrap();
        if !encoded_fixture(&input, dir.path(), "avif") {
            return;
        }
        let bytes = std::fs::read(dir.path().join("1.avif")).unwrap();
        let pixi = bytes
            .windows(4)
            .enumerate()
            .find_map(|(i, p)| (p == b"pixi" && bytes.get(i + 8) == Some(&3)).then_some(i))
            .unwrap();
        assert_eq!(&bytes[pixi + 9..pixi + 12], &[10, 10, 10]);
    }

    fn colour_pixi(bytes: &[u8]) -> &[u8] {
        let pixi = bytes
            .windows(4)
            .enumerate()
            .find_map(|(i, p)| (p == b"pixi" && bytes.get(i + 8) == Some(&3)).then_some(i))
            .expect("missing colour pixi box");
        &bytes[pixi + 9..pixi + 12]
    }

    #[test]
    fn sixteen_bit_png_heic_uses_ten_bits_or_explicit_fallback() {
        let engine = LibheifEngine::new();
        if !engine.supports_output("heic") {
            assert!(
                std::env::var_os("CONVT_TEST_REQUIRE_HEIF").is_none(),
                "required HEVC encoder missing: {:?}",
                engine.unavailable_reason()
            );
            eprintln!("skipping: {:?}", engine.unavailable_reason());
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("deep.png");
        image::ImageBuffer::<image::Rgb<u16>, _>::from_pixel(
            32,
            32,
            image::Rgb([32768, 12345, 54321]),
        )
        .save(&input)
        .unwrap();
        let step = Step {
            from: format_by_id("png").unwrap(),
            to: format_by_id("heic").unwrap(),
        };
        let options = Options {
            quality: Some(100),
            ..Options::default()
        };
        let cancel = Cancel::new();
        let ctx = Ctx::new(step, &options, &|_| {}, &cancel);
        let probe = dir.path().join("probe.heic");
        let attempt =
            encode_depth(engine.lib.as_ref().unwrap(), &input, &ctx, &probe, false).unwrap();
        let expected = match attempt {
            EncodeAttempt::Written => {
                assert_eq!(colour_pixi(&std::fs::read(&probe).unwrap()), &[10, 10, 10]);
                10
            }
            EncodeAttempt::UnsupportedBitDepth => {
                assert!(!probe.exists(), "failed attempt must not write output");
                8
            }
        };
        if let Ok(required) = std::env::var("CONVT_TEST_HEIC_DEPTH") {
            assert_eq!(
                expected,
                required.parse::<u8>().unwrap(),
                "wrong test encoder capability"
            );
        }
        let files = engine.convert(&ctx, &input, dir.path()).unwrap();
        assert_eq!(
            colour_pixi(&std::fs::read(&files[0]).unwrap()),
            &[expected; 3]
        );
        eprintln!("HEIC: attempted 10-bit; output {expected}-bit ({attempt:?})");
    }

    #[test]
    fn png_colour_metadata_precedence_and_primaries() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("colour.png");
        image::RgbImage::from_pixel(32, 32, image::Rgb([128, 64, 192]))
            .save(&input)
            .unwrap();
        let original = std::fs::read(&input).unwrap();
        let chroma: Vec<_> = [31270u32, 32900, 68000, 32000, 26500, 69000, 15000, 6000]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect();
        for (chunks, primaries, transfer) in [
            (vec![], 1, 13),
            (vec![png_chunk(b"gAMA", &45455u32.to_be_bytes())], 1, 4),
            (
                vec![
                    png_chunk(b"cHRM", &chroma),
                    png_chunk(b"gAMA", &100000u32.to_be_bytes()),
                ],
                12,
                8,
            ),
            (
                vec![
                    png_chunk(b"cHRM", &chroma),
                    png_chunk(b"gAMA", &100000u32.to_be_bytes()),
                    png_chunk(b"sRGB", &[0]),
                ],
                1,
                13,
            ),
            (
                vec![png_chunk(b"cICP", &[9, 16, 0, 1]), png_chunk(b"sRGB", &[0])],
                9,
                16,
            ),
        ] {
            let mut png = original.clone();
            png.splice(33..33, chunks.concat());
            std::fs::write(&input, png).unwrap();
            let colour = png_colour(&input).unwrap().unwrap();
            assert_eq!((colour.primaries, colour.transfer), (primaries, transfer));
            if !encoded_fixture(&input, dir.path(), "heic") {
                continue;
            }
            let bytes = std::fs::read(dir.path().join("1.heic")).unwrap();
            let nclx = bytes.windows(4).position(|p| p == b"nclx").unwrap();
            assert_eq!(
                &bytes[nclx + 4..nclx + 8],
                &[primaries.to_be_bytes(), transfer.to_be_bytes()].concat()
            );
        }
    }

    fn write_chromaticity_png(input: &Path, xy: [u32; 8], gamma: Option<u32>) {
        image::RgbaImage::from_pixel(32, 32, image::Rgba([128, 64, 192, 128]))
            .save(input)
            .unwrap();
        let mut png = std::fs::read(input).unwrap();
        let coordinates: Vec<_> = xy.into_iter().flat_map(u32::to_be_bytes).collect();
        let mut chunks = png_chunk(b"cHRM", &coordinates);
        if let Some(gamma) = gamma {
            chunks.extend(png_chunk(b"gAMA", &gamma.to_be_bytes()));
        }
        png.splice(33..33, chunks);
        std::fs::write(input, png).unwrap();
    }

    #[test]
    fn all_representable_png_primaries_map_to_nclx() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("primaries.png");
        for (expected, xy) in [
            (
                1u16,
                [31270, 32900, 64000, 33000, 30000, 60000, 15000, 6000],
            ),
            (4, [31000, 31600, 67000, 33000, 21000, 71000, 14000, 8000]),
            (5, [31270, 32900, 64000, 33000, 29000, 60000, 15000, 6000]),
            (6, [31270, 32900, 63000, 34000, 31000, 59500, 15500, 7000]),
            (8, [31000, 31600, 68100, 31900, 24300, 69200, 14500, 4900]),
            (9, [31270, 32900, 70800, 29200, 17000, 79700, 13100, 4600]),
            (10, [33333, 33333, 100000, 0, 0, 100000, 0, 0]),
            (11, [31400, 35100, 68000, 32000, 26500, 69000, 15000, 6000]),
            (12, [31270, 32900, 68000, 32000, 26500, 69000, 15000, 6000]),
            (22, [31270, 32900, 63000, 34000, 29500, 60500, 15500, 7700]),
        ] {
            write_chromaticity_png(&input, xy, Some(100000));
            assert_eq!(png_colour(&input).unwrap().unwrap().primaries, expected);
            // Bundled x265 rejects EBU primary code 22; libaom supports it.
            let target = if expected == 22 { "avif" } else { "heic" };
            if !encoded_fixture(&input, dir.path(), target) {
                continue;
            }
            let bytes = std::fs::read(dir.path().join(format!("1.{target}"))).unwrap();
            let nclx = bytes.windows(4).position(|p| p == b"nclx").unwrap();
            assert_eq!(&bytes[nclx + 4..nclx + 6], &expected.to_be_bytes());
        }
    }

    #[test]
    fn custom_chromaticities_convert_pixels_and_tag_srgb() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("adobe-rgb.png");
        // Adobe RGB has no NCLX primary code. The reference values below use
        // its published linear RGB-to-sRGB matrix, with gamma 2.2 input.
        write_chromaticity_png(
            &input,
            [31270, 32900, 64000, 33000, 21000, 71000, 15000, 6000],
            Some(45455),
        );
        let expected = [0.5729982, 0.2421134, 0.7714638];
        let mut colour = png_colour(&input).unwrap().unwrap();
        let converted = convert_colour(image::open(&input).unwrap(), &mut colour).to_rgba32f();
        let pixel = converted.get_pixel(0, 0).0;
        for (actual, expected) in pixel[..3].iter().zip(expected) {
            assert!((actual - expected).abs() < 0.0002, "{pixel:?}");
        }
        assert!((pixel[3] - 128.0 / 255.0).abs() < 0.00002);
        assert_eq!((colour.primaries, colour.transfer), (1, 13));
        if !encoded_fixture(&input, dir.path(), "heic") {
            return;
        }
        let bytes = std::fs::read(dir.path().join("1.heic")).unwrap();
        let nclx = bytes.windows(4).position(|p| p == b"nclx").unwrap();
        assert_eq!(&bytes[nclx + 4..nclx + 8], &[0, 1, 0, 13]);
        let engine = LibheifEngine::new();
        if engine.supports_input("heic") {
            let pixel = decoded(&engine, &dir.path().join("1.heic"))
                .unwrap()
                .to_rgba32f()
                .get_pixel(16, 16)
                .0;
            for (actual, expected) in pixel[..3].iter().zip(expected) {
                assert!((actual - expected).abs() < 0.03, "{pixel:?}");
            }
            assert!((pixel[3] - 128.0 / 255.0).abs() < 0.02);
        }
    }

    #[test]
    fn custom_zero_y_primaries_are_valid_and_neutral() {
        let matrix = rgb_to_srgb_matrix([0.3127, 0.329, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0]).unwrap();
        for value in matrix_vector(matrix, [1.0; 3]) {
            assert!((value - 1.0).abs() < 1e-10);
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("xyz.png");
        write_chromaticity_png(
            &input,
            [31270, 32900, 100000, 0, 0, 100000, 0, 0],
            Some(100000),
        );
        let colour = png_colour(&input).unwrap().unwrap();
        assert!(colour.to_srgb.is_some());
        if !encoded_fixture(&input, dir.path(), "heic") {
            return;
        }
        let bytes = std::fs::read(dir.path().join("1.heic")).unwrap();
        let nclx = bytes.windows(4).position(|p| p == b"nclx").unwrap();
        assert_eq!(&bytes[nclx + 4..nclx + 8], &[0, 1, 0, 13]);
    }

    #[test]
    fn custom_chromaticity_white_adaptation_and_transfer() {
        let matrix =
            rgb_to_srgb_matrix([0.34567, 0.35850, 0.64, 0.33, 0.30, 0.60, 0.15, 0.06]).unwrap();
        for value in matrix_vector(matrix, [1.0; 3]) {
            assert!((value - 1.0).abs() < 1e-10);
        }
        for (transfer, gamma, expected) in [
            (13, None, 0.5),
            (8, None, 0.735357),
            (4, None, 0.503867),
            (5, None, 0.414),
        ] {
            let input = image::DynamicImage::ImageRgba32F(image::Rgba32FImage::from_pixel(
                1,
                1,
                image::Rgba([0.5, 0.5, 0.5, 0.25]),
            ));
            let mut colour = Colour {
                primaries: 1,
                transfer,
                gamma,
                to_srgb: Some(matrix),
            };
            let result = convert_colour(input, &mut colour).to_rgba32f();
            let pixel = result.get_pixel(0, 0).0;
            for channel in &pixel[..3] {
                assert!((channel - expected).abs() < 0.001, "{transfer}: {pixel:?}");
            }
            assert!((pixel[3] - 0.25).abs() < 0.00002);
        }
        assert!(rgb_to_srgb_matrix([0.3127, 0.329, 0.64, 0.33, 0.64, 0.33, 0.15, 0.06]).is_err());
        assert!(rgb_to_srgb_matrix([0.3127, 0.329, 0.64, -0.01, 0.30, 0.60, 0.15, 0.06]).is_err());
        assert!(rgb_to_srgb_matrix([0.3127, 0.329, 0.90, 0.33, 0.30, 0.60, 0.15, 0.06]).is_err());
    }

    #[test]
    #[ignore]
    fn worker_entry() {
        let _ = LibheifEngine::new();
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn relative_overrides_never_run_library_constructor() {
        let dir = tempfile::tempdir().unwrap();
        let relative = dir.path().join("relative");
        std::fs::create_dir(&relative).unwrap();
        let source = dir.path().join("sentinel.c");
        let marker = dir.path().join("loaded");
        std::fs::write(
            &source,
            r#"#include <stdio.h>
#include <stdlib.h>
__attribute__((constructor)) static void loaded(void) {
  FILE *f = fopen(getenv("CONVT_TEST_NATIVE_MARKER"), "w");
  if (f) { fputs("loaded", f); fclose(f); }
}
"#,
        )
        .unwrap();
        assert!(
            std::process::Command::new("cc")
                .args(["-shared", "-fPIC"])
                .arg(&source)
                .arg("-o")
                .arg(dir.path().join("libheif.so.1"))
                .status()
                .unwrap()
                .success()
        );
        std::fs::copy(
            dir.path().join("libheif.so.1"),
            relative.join("libheif.so.1"),
        )
        .unwrap();
        for value in ["", ".", "relative"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "heic::tests::worker_entry",
                    "--ignored",
                    "--nocapture",
                ])
                .current_dir(dir.path())
                .env("CONVT_LIBHEIF_DIR", value)
                .env("CONVT_LIBHEIF_PLUGIN_DIR", value)
                .env("CONVT_TEST_NATIVE_MARKER", &marker)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(
                !marker.exists(),
                "relative libheif override ran native code: {value:?}"
            );
            assert!(String::from_utf8_lossy(&output.stderr).contains("CONVT_LIBHEIF_DIR"));
            assert!(String::from_utf8_lossy(&output.stderr).contains("must be absolute"));
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "heic::tests::worker_entry",
                "--ignored",
                "--nocapture",
            ])
            .env("CONVT_LIBHEIF_DIR", dir.path())
            .env("CONVT_TEST_NATIVE_MARKER", &marker)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(marker.exists(), "absolute libheif override was not loaded");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn relative_plugin_override_never_runs_constructor() {
        let dir = tempfile::tempdir().unwrap();
        let relative = dir.path().join("relative");
        std::fs::create_dir(&relative).unwrap();
        let source = dir.path().join("plugin.c");
        let marker = dir.path().join("loaded");
        std::fs::write(
            &source,
            r#"#include <stdio.h>
#include <stdlib.h>
__attribute__((constructor)) static void loaded(void) {
  FILE *f = fopen(getenv("CONVT_TEST_NATIVE_MARKER"), "w");
  if (f) { fputs("loaded", f); fclose(f); }
}
"#,
        )
        .unwrap();
        assert!(
            std::process::Command::new("cc")
                .args(["-shared", "-fPIC"])
                .arg(&source)
                .arg("-o")
                .arg(dir.path().join("sentinel.so"))
                .status()
                .unwrap()
                .success()
        );
        std::fs::copy(dir.path().join("sentinel.so"), relative.join("sentinel.so")).unwrap();
        let library_source = dir.path().join("secure.c");
        std::fs::write(
            &library_source,
            r#"#include <dlfcn.h>
#include <stdio.h>
struct error { int code, subcode; const char *message; };
struct error heif_convt_init_no_plugins(void) { return (struct error){0, 0, "ok"}; }
struct error heif_load_plugins(const char *path, void **plugins, int *count, int size) {
  char file[4096]; snprintf(file, sizeof(file), "%s/sentinel.so", path);
  dlopen(file, RTLD_NOW);
  return (struct error){0, 0, "ok"};
}
"#,
        )
        .unwrap();
        assert!(
            std::process::Command::new("cc")
                .args(["-shared", "-fPIC"])
                .arg(&library_source)
                .args(["-ldl", "-o"])
                .arg(dir.path().join("libheif.so.1"))
                .status()
                .unwrap()
                .success()
        );
        for value in [
            std::ffi::OsStr::new(""),
            std::ffi::OsStr::new("."),
            std::ffi::OsStr::new("relative"),
            dir.path().as_os_str(),
        ] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "heic::tests::worker_entry",
                    "--ignored",
                    "--nocapture",
                ])
                .current_dir(dir.path())
                .env("CONVT_LIBHEIF_DIR", dir.path())
                .env("CONVT_LIBHEIF_PLUGIN_DIR", value)
                .env("CONVT_TEST_NATIVE_MARKER", &marker)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let absolute = Path::new(value).is_absolute();
            assert_eq!(marker.exists(), absolute, "plugin override: {value:?}");
            if !absolute {
                let warnings = String::from_utf8_lossy(&output.stderr);
                assert!(warnings.contains("CONVT_LIBHEIF_PLUGIN_DIR"));
                assert!(warnings.contains("must be absolute"));
            }
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn inherited_plugin_path_never_loads_code() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("sentinel.c");
        let plugin = dir.path().join("sentinel.so");
        let marker = dir.path().join("loaded");
        std::fs::write(
            &source,
            r#"#include <stdio.h>
#include <stdlib.h>
__attribute__((constructor)) static void loaded(void) {
  FILE *f = fopen(getenv("CONVT_TEST_PLUGIN_MARKER"), "w");
  if (f) { fputs("loaded", f); fclose(f); }
}
"#,
        )
        .unwrap();
        assert!(
            std::process::Command::new("cc")
                .args(["-shared", "-fPIC"])
                .arg(&source)
                .arg("-o")
                .arg(&plugin)
                .status()
                .unwrap()
                .success()
        );
        // Model an unpatched library whose ordinary initializer would load
        // the sentinel. A missing secure symbol must reject it before that call.
        let unpatched = dir.path().join("unpatched");
        std::fs::create_dir(&unpatched).unwrap();
        let init_source = dir.path().join("unpatched.c");
        std::fs::write(
            &init_source,
            r#"#include <dlfcn.h>
#include <stdlib.h>
#include <stdio.h>
struct error { int code, subcode; const char *message; };
struct error heif_init(void *params) {
  char path[4096];
  snprintf(path, sizeof(path), "%s/sentinel.so", getenv("LIBHEIF_PLUGIN_PATH"));
  dlopen(path, RTLD_NOW);
  return (struct error){0, 0, "ok"};
}
"#,
        )
        .unwrap();
        assert!(
            std::process::Command::new("cc")
                .args(["-shared", "-fPIC"])
                .arg(&init_source)
                .args(["-ldl", "-o"])
                .arg(unpatched.join("libheif.so.1"))
                .status()
                .unwrap()
                .success()
        );
        for lib_dir in [None, Some(&unpatched)] {
            for inherited in [dir.path(), Path::new(".")] {
                let mut command = std::process::Command::new(std::env::current_exe().unwrap());
                command
                    .args([
                        "--exact",
                        "heic::tests::worker_entry",
                        "--ignored",
                        "--nocapture",
                    ])
                    .current_dir(dir.path())
                    .env("LIBHEIF_PLUGIN_PATH", inherited)
                    .env("CONVT_LIBHEIF_PLUGIN_DIR", ".")
                    .env("CONVT_TEST_PLUGIN_MARKER", &marker);
                if let Some(lib_dir) = lib_dir {
                    command.env("CONVT_LIBHEIF_DIR", lib_dir);
                }
                let result = command.output().unwrap();
                assert!(result.status.success(), "{result:?}");
                assert!(
                    !marker.exists(),
                    "libheif loaded plugin code: inherited={inherited:?}, library={lib_dir:?}"
                );
            }
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn secure_initializer_precedes_explicit_absolute_plugin_loading() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("secure.c");
        let library = dir.path().join("secure.so");
        std::fs::write(
            &source,
            r#"#include <stdlib.h>
struct error { int code, subcode; const char *message; };
static int initialized, loaded, invalid;
struct error heif_init(void *params) { abort(); }
struct error heif_convt_init_no_plugins(void) {
  initialized = 1;
  return (struct error){0, 0, "ok"};
}
struct error heif_load_plugins(const char *path, void **plugins, int *count, int size) {
  if (!initialized || path[0] != '/') invalid = 1;
  loaded++;
  return (struct error){0, 0, "ok"};
}
int test_invalid(void) { return invalid; }
int test_loaded(void) { return loaded; }
"#,
        )
        .unwrap();
        assert!(
            std::process::Command::new("cc")
                .args(["-shared", "-fPIC"])
                .arg(&source)
                .arg("-o")
                .arg(&library)
                .status()
                .unwrap()
                .success()
        );
        // SAFETY: the test library exports the exact signatures above.
        unsafe {
            let lib = Library::new(&library).unwrap();
            initialize(&lib).unwrap();
            let invalid: Symbol<unsafe extern "C" fn() -> c_int> =
                lib.get(b"test_invalid\0").unwrap();
            let loaded: Symbol<unsafe extern "C" fn() -> c_int> =
                lib.get(b"test_loaded\0").unwrap();
            assert_eq!(invalid(), 0);
            assert_eq!(loaded() as usize, plugin_dirs().len());
        }
    }

    #[test]
    fn offers_only_inputs_with_decoder_plugins() {
        for (hevc, av1, expected) in [
            (false, false, vec![]),
            (true, false, vec!["heic"]),
            (false, true, vec!["avif"]),
            (true, true, vec!["heic", "avif"]),
        ] {
            let steps = decoder_steps(hevc, av1);
            assert_eq!(
                steps.iter().map(|s| s.from.id).collect::<Vec<_>>(),
                expected
            );
            assert!(steps.iter().all(|s| s.to.id == "png"));
        }
        let engine = LibheifEngine::new();
        for id in ["heic", "avif"] {
            assert_eq!(
                engine.steps().iter().any(|s| s.from.id == id),
                engine.supports_input(id)
            );
        }
    }

    #[test]
    fn encodes_heic_with_alpha_quality_and_size() {
        let engine = LibheifEngine::new();
        if !engine.supports_output("heic") || !engine.supports_input("heic") {
            eprintln!("skipping: HEVC encoder or decoder unavailable");
            assert_eq!(
                engine.steps().iter().any(|s| s.to.id == "heic"),
                engine.supports_output("heic")
            );
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.png");
        image::RgbaImage::from_pixel(64, 48, image::Rgba([240, 24, 24, 128]))
            .save(&input)
            .unwrap();
        let step = Step {
            from: format_by_id("png").unwrap(),
            to: format_by_id("heic").unwrap(),
        };
        let cancel = Cancel::new();
        let mut encoded = Vec::new();
        for quality in [20, 95] {
            let out = dir.path().join(quality.to_string());
            std::fs::create_dir(&out).unwrap();
            let options = Options {
                quality: Some(quality),
                max_size: Some(32),
                ..Options::default()
            };
            let files = engine
                .convert(&Ctx::new(step, &options, &|_| {}, &cancel), &input, &out)
                .unwrap();
            let pixels = decoded(&engine, &files[0]).unwrap().to_rgba8();
            assert_eq!(pixels.dimensions(), (32, 24));
            let p = pixels.get_pixel(16, 12).0;
            assert!(
                p[0].abs_diff(240) < 20 && p[1].abs_diff(24) < 20 && p[2].abs_diff(24) < 20,
                "{p:?}"
            );
            assert!(p[3].abs_diff(128) <= 5, "{p:?}");
            encoded.push(std::fs::read(&files[0]).unwrap());
        }
        assert_ne!(encoded[0], encoded[1], "quality must affect encoded data");
    }

    #[test]
    fn jpeg_orientation_is_applied_before_heic_encoding() {
        let engine = LibheifEngine::new();
        if !engine.supports_output("heic") || !engine.supports_input("heic") {
            eprintln!("skipping: HEVC encoder or decoder unavailable");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("oriented.jpeg");
        image::RgbImage::from_fn(64, 48, |x, _| {
            image::Rgb(if x < 32 { [240, 24, 24] } else { [24, 24, 240] })
        })
        .save(&input)
        .unwrap();
        let jpeg = std::fs::read(&input).unwrap();
        // Exif big-endian TIFF, one SHORT orientation entry: 6 (90 degrees).
        let exif = b"Exif\0\0MM\0*\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01\0\x06\0\0\0\0\0\0";
        let mut oriented = jpeg[..2].to_vec();
        oriented.extend_from_slice(&[0xff, 0xe1]);
        oriented.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        oriented.extend_from_slice(exif);
        oriented.extend_from_slice(&jpeg[2..]);
        std::fs::write(&input, oriented).unwrap();
        let step = Step {
            from: format_by_id("jpeg").unwrap(),
            to: format_by_id("heic").unwrap(),
        };
        assert!(engine.steps().contains(&step));
        let options = Options::default();
        let cancel = Cancel::new();
        let files = engine
            .convert(
                &Ctx::new(step, &options, &|_| {}, &cancel),
                &input,
                dir.path(),
            )
            .unwrap();
        let pixels = decoded(&engine, &files[0]).unwrap().to_rgb8();
        assert_eq!(pixels.dimensions(), (48, 64));
        assert!(pixels.get_pixel(24, 16).0[0] > 200);
        assert!(pixels.get_pixel(24, 48).0[2] > 200);
    }

    /// Needs libheif and a HEIC file: set `CONVT_TEST_HEIC=/path/to/photo.heic`.
    /// HEIC encoders are rare and the sample photos are not ours to commit.
    #[test]
    fn decodes_a_real_heic() {
        let Some(src) = std::env::var_os("CONVT_TEST_HEIC").map(PathBuf::from) else {
            eprintln!("skipping: CONVT_TEST_HEIC is not set");
            return;
        };
        let engine = LibheifEngine::new();
        if !engine.supports_input("heic") {
            eprintln!("skipping: HEVC decoder unavailable");
            return;
        }
        let out = tempfile::tempdir().unwrap();
        let step = Step {
            from: format_by_id("heic").unwrap(),
            to: format_by_id("png").unwrap(),
        };
        let o = Options {
            max_size: Some(300),
            ..Options::default()
        };
        let cancel = Cancel::new();
        let files = engine
            .convert(&Ctx::new(step, &o, &|_| {}, &cancel), &src, out.path())
            .unwrap();
        let img = image::open(&files[0]).unwrap();
        assert_eq!(img.width().max(img.height()), 300);
    }

    #[test]
    fn garbage_input_is_an_error_not_a_crash() {
        let engine = LibheifEngine::new();
        if engine.unavailable_reason().is_some() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("fake.heic");
        std::fs::write(&src, b"not a heic file").unwrap();
        let step = Step {
            from: format_by_id("heic").unwrap(),
            to: format_by_id("png").unwrap(),
        };
        let cancel = Cancel::new();
        let o = Options::default();
        let err = engine
            .convert(&Ctx::new(step, &o, &|_| {}, &cancel), &src, dir.path())
            .unwrap_err();
        assert_eq!(err.kind(), "engine_failed");
    }
}
