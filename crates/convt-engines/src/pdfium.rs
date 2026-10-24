use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use convt_core::{Background, Ctx, Engine, Error, Result, Step};
use pdfium_render::prelude::*;

/// PDF work with PDFium, loaded from `$CONVT_PDFIUM_DIR`, next to the
/// executable, or the system library path. Debug builds also use the
/// repository vendor directory; release tests set the override explicitly.
///
/// - PDF to PNG or JPEG writes one image per rendered page; the registry
///   names them `doc.png`, `doc-2.png` and so on.
/// - PDF to TXT or HTML extracts the text layer of the selected pages into
///   one file. TXT separates pages with a form feed, as `pdftotext` does;
///   HTML has one `<section>` per page.
/// - An image to PDF makes a one-page PDF the size of the image at `dpi`
///   (96 unless chosen, the resolution SVG and screens use).
pub struct PdfiumEngine {
    pdfium: &'static std::result::Result<Pdfium, String>,
}

/// PDFium bindings are process-global: pdfium-render refuses to bind a second
/// time, so every engine and registry in the process shares one instance.
static PDFIUM: OnceLock<std::result::Result<Pdfium, String>> = OnceLock::new();

fn failed(e: impl std::fmt::Display) -> Error {
    Error::EngineFailed {
        engine: "pdfium",
        message: e.to_string(),
    }
}

/// Images PDFium embeds; everything the `image` crate decodes. HEIC, AVIF
/// and SVG reach PDF through PNG.
const IMAGE_INPUTS: &[&str] = crate::image::INPUTS;

/// Without `--dpi`, an image's pixels are laid out at this resolution.
const DEFAULT_IMAGE_DPI: u32 = 96;

/// The largest page PDF viewers accept: 200 inches. Larger images are
/// laid out at a higher resolution so the page fits.
const MAX_PAGE_POINTS: f32 = 14_400.0;

/// The smallest page side the PDF format allows. Tiny images are laid out
/// at a lower resolution so the page reaches it.
const MIN_PAGE_POINTS: f32 = 3.0;

fn shipped_dirs(override_dir: Option<PathBuf>, exe_dir: Option<PathBuf>) -> Vec<PathBuf> {
    override_dir.into_iter().chain(exe_dir).collect()
}

fn absolute_override() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("CONVT_PDFIUM_DIR")?);
    if dir.is_absolute() {
        Some(dir)
    } else {
        eprintln!(
            "convt: ignoring CONVT_PDFIUM_DIR={dir:?}: native library overrides must be absolute paths"
        );
        None
    }
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = shipped_dirs(absolute_override(), crate::paths::exe_dir());
    dirs.extend(crate::paths::bundle_dirs());
    #[cfg(debug_assertions)]
    let dirs = dirs
        .into_iter()
        .chain([Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium/lib")])
        .collect();
    dirs
}

fn load() -> std::result::Result<Pdfium, String> {
    for dir in candidate_dirs() {
        let path = Pdfium::pdfium_platform_library_name_at_path(&dir);
        if path.exists()
            && let Ok(bindings) = Pdfium::bind_to_library(&path)
        {
            return Ok(Pdfium::new(bindings));
        }
    }
    Pdfium::bind_to_system_library()
        .map(Pdfium::new)
        .map_err(|e| format!("PDFium not found ({e}); run scripts/fetch-pdfium.sh"))
}

impl PdfiumEngine {
    pub fn new() -> Self {
        Self {
            pdfium: PDFIUM.get_or_init(load),
        }
    }

    fn pdfium(&self) -> Result<&'static Pdfium> {
        self.pdfium.as_ref().map_err(failed)
    }
}

impl Default for PdfiumEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for PdfiumEngine {
    fn id(&self) -> &'static str {
        "pdfium"
    }

    fn priority(&self) -> i32 {
        10
    }

    fn unavailable_reason(&self) -> Option<String> {
        self.pdfium.as_ref().err().cloned()
    }

    fn steps(&self) -> Vec<Step> {
        crate::steps(&["pdf"], &["png", "jpeg", "txt", "html"])
            .chain(crate::steps(IMAGE_INPUTS, &["pdf"]))
            .collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let pdfium = self.pdfium()?;
        match (ctx.step.from.id, ctx.step.to.id) {
            ("pdf", "txt" | "html") => extract_text(pdfium, ctx, input, out_dir),
            ("pdf", _) => render(pdfium, ctx, input, out_dir),
            (_, "pdf") => image_to_pdf(pdfium, ctx, input, out_dir),
            (from, to) => Err(failed(format!("no step from {from} to {to}"))),
        }
    }
}

/// The 0-based pages `--pages` selects from a document of `count`, or an
/// error when it selects none.
fn selected_pages(count: u32, ctx: &Ctx) -> Result<Vec<u32>> {
    let range = ctx.options.pages;
    let wanted: Vec<u32> = (0..count)
        .filter(|&i| range.is_none_or(|r| r.contains(i as usize)))
        .collect();
    if wanted.is_empty() {
        let asked = range.map_or("any".into(), |r| r.to_string());
        let unit = if count == 1 { "page" } else { "pages" };
        return Err(Error::InvalidOption(format!(
            "the document has {count} {unit}, so pages {asked} select nothing"
        )));
    }
    Ok(wanted)
}

fn render(pdfium: &Pdfium, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
    let doc = pdfium.load_pdf_from_file(input, None).map_err(failed)?;
    let pages = doc.pages();
    let wanted = selected_pages(pages.len() as u32, ctx)?;
    let mut config = match ctx.options.dpi {
        Some(dpi) => PdfRenderConfig::new().scale_page_by_factor(dpi as f32 / 72.0),
        None => PdfRenderConfig::new()
            .set_target_width(2000)
            .set_maximum_height(4000),
    };
    // Pages render onto the chosen background. Transparent keeps the page
    // background clear where the output can store it; everything else,
    // and the default, is white, which is what PDF viewers show.
    config = match ctx.options.background {
        Some(Background::Color([r, g, b])) => config.set_clear_color(PdfColor::new(r, g, b, 255)),
        Some(Background::Transparent) if ctx.step.to.keeps_transparency() => {
            config.set_clear_color(PdfColor::new(255, 255, 255, 0))
        }
        Some(Background::Transparent) => {
            return Err(Error::InvalidOption(crate::image::no_transparency(
                ctx.step.to.id,
            )));
        }
        None => config,
    };
    if let Some(max) = ctx.options.max_size {
        let max = max.min(i32::MAX as u32) as i32;
        config = config.set_maximum_width(max).set_maximum_height(max);
    }
    let mut files = Vec::with_capacity(wanted.len());
    for (n, &i) in wanted.iter().enumerate() {
        ctx.check()?;
        let page = pages.get(i as _).map_err(failed)?;
        let img = page
            .render_with_config(&config)
            .map_err(failed)?
            .as_image()
            .map_err(failed)?;
        // Named by page number, so the registry can name outputs after
        // the pages they came from when --pages skips some.
        let target = ctx.artifact(out_dir, i as usize);
        crate::image::encode(img, ctx.step.to.id, ctx.options, &target)?;
        files.push(target);
        ctx.progress((n + 1) as f32 / wanted.len() as f32);
    }
    Ok(files)
}

/// Writes the text of the selected pages as one TXT or HTML file.
fn extract_text(pdfium: &Pdfium, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
    let doc = pdfium.load_pdf_from_file(input, None).map_err(failed)?;
    let pages = doc.pages();
    let wanted = selected_pages(pages.len() as u32, ctx)?;
    let html = ctx.step.to.id == "html";
    let mut texts = Vec::with_capacity(wanted.len());
    for (n, &i) in wanted.iter().enumerate() {
        ctx.check()?;
        let page = pages.get(i as _).map_err(failed)?;
        let text = page.text().map_err(failed)?;
        texts.push(if html {
            paragraphs(&text).join("\n\n")
        } else {
            clean_text(&text.all())
        });
        ctx.progress((n + 1) as f32 / wanted.len() as f32);
    }
    // An empty file would look like success; a scan has nothing to extract.
    if texts.iter().all(|t| t.trim().is_empty()) {
        return Err(failed(
            "the selected pages have no text to extract; the PDF may be a scan",
        ));
    }
    let body = if html {
        let title = doc
            .metadata()
            .get(PdfDocumentMetadataTagType::Title)
            .map(|t| t.value().trim().to_string())
            .filter(|t| !t.is_empty())
            .or_else(|| input.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_default();
        text_html(&title, &wanted, &texts)
    } else {
        let mut out = texts.join("\n\x0c");
        out.push('\n');
        out
    };
    let target = ctx.artifact(out_dir, 0);
    std::fs::write(&target, body)?;
    Ok(vec![target])
}

/// PDFium's text with `\n` line ends, trailing spaces and control
/// characters (other than tabs) removed.
fn clean_text(raw: &str) -> String {
    raw.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(|line| {
            line.chars()
                .filter(|&c| c == '\t' || !(c.is_control() || matches!(c, '\u{fffe}' | '\u{ffff}')))
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_matches('\n')
        .to_string()
}

/// One line of page text: what it says, its baseline, and its largest font.
struct Line {
    text: String,
    baseline: f32,
    size: f32,
}

/// The page's text as paragraphs. PDFs store lines, not paragraphs, so a
/// new one starts where the next line sits more than 1.45 font sizes
/// lower (single-spaced text wraps at about 1.2), moves up (a new column),
/// or changes font size by a fifth (a heading). Lines of one paragraph are
/// joined with spaces so word processors can reflow them.
fn paragraphs(text: &PdfPageText) -> Vec<String> {
    let mut lines: Vec<Line> = Vec::new();
    let mut current = Line {
        text: String::new(),
        baseline: f32::NAN,
        size: 0.0,
    };
    let mut finish = |line: &mut Line| {
        let done = std::mem::replace(
            line,
            Line {
                text: String::new(),
                baseline: f32::NAN,
                size: 0.0,
            },
        );
        let cleaned = clean_text(&done.text);
        if !cleaned.trim().is_empty() {
            lines.push(Line {
                text: cleaned.trim().to_string(),
                ..done
            });
        }
    };
    for c in text.chars().iter() {
        match c.unicode_char() {
            Some('\n') => finish(&mut current),
            Some('\r') | None => {}
            Some(ch) => {
                current.text.push(ch);
                if !ch.is_whitespace() {
                    if current.baseline.is_nan()
                        && let Ok(y) = c.origin_y()
                    {
                        current.baseline = y.value;
                    }
                    current.size = current.size.max(c.scaled_font_size().value);
                }
            }
        }
    }
    finish(&mut current);
    let mut out: Vec<String> = Vec::new();
    let mut prev: Option<&Line> = None;
    for line in &lines {
        let joins = prev.is_some_and(|p| {
            let size = p.size.max(line.size).max(1.0);
            let drop = p.baseline - line.baseline;
            let similar = (p.size - line.size).abs() <= 0.2 * size;
            drop > 0.0 && drop <= 1.45 * size && similar
        });
        match out.last_mut() {
            Some(paragraph) if joins => {
                if !paragraph.ends_with('-') {
                    paragraph.push(' ');
                }
                paragraph.push_str(&line.text);
            }
            _ => out.push(line.text.clone()),
        }
        prev = Some(line);
    }
    out
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// A self-contained HTML document: one `<section>` per page and a `<p>`
/// per blank-line-separated block of `texts`.
fn text_html(title: &str, pages: &[u32], texts: &[String]) -> String {
    let mut out = format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n</head>\n<body>\n",
        escape_html(title)
    );
    for (page, text) in pages.iter().zip(texts) {
        out.push_str(&format!("<section id=\"page-{}\">\n", page + 1));
        for block in text.split("\n\n").map(str::trim).filter(|b| !b.is_empty()) {
            out.push_str(&format!("<p>{}</p>\n", escape_html(block)));
        }
        out.push_str("</section>\n");
    }
    out.push_str("</body>\n</html>\n");
    out
}

/// Writes a one-page PDF holding the image at its own aspect ratio.
fn image_to_pdf(pdfium: &Pdfium, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut doc = pdfium.create_new_pdf().map_err(failed)?;
    let (object, width, height) = match jpeg_as_is(ctx, input)? {
        // An upright JPEG that needs no scaling goes in byte for byte, so
        // the PDF is no bigger than the photo and loses nothing more.
        Some((w, h)) => (
            PdfPageImageObject::new_from_jpeg_file(&doc, input).map_err(failed)?,
            w,
            h,
        ),
        None => {
            let img = crate::image::fit(
                crate::orientation::open_oriented(input)?,
                ctx.options.max_size,
            );
            ctx.check()?;
            // A chosen color goes under transparent areas. Otherwise PDFium
            // keeps the alpha channel as a soft mask, and viewers show the
            // page, white, through it.
            let img = match ctx.options.background {
                Some(Background::Color(color)) => crate::image::flatten(img, color),
                Some(Background::Transparent) | None => img,
            };
            let (w, h) = (img.width(), img.height());
            (image_object(&doc, &img).map_err(failed)?, w, h)
        }
    };
    ctx.progress(0.5);
    let dpi = ctx.options.dpi.unwrap_or(DEFAULT_IMAGE_DPI) as f32;
    let mut scale = 72.0 / dpi;
    let shortest = width.min(height) as f32 * scale;
    if shortest < MIN_PAGE_POINTS {
        scale *= MIN_PAGE_POINTS / shortest;
    }
    // The longest side wins when an image is too narrow for both.
    let longest = width.max(height) as f32 * scale;
    if longest > MAX_PAGE_POINTS {
        scale *= MAX_PAGE_POINTS / longest;
    }
    let (w, h) = (width as f32 * scale, height as f32 * scale);
    let mut object = object;
    object.scale(w, h).map_err(failed)?;
    let mut page = doc
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::Custom(
            PdfPoints::new(w),
            PdfPoints::new(h),
        ))
        .map_err(failed)?;
    page.objects_mut()
        .add_image_object(object)
        .map_err(failed)?;
    ctx.check()?;
    let target = ctx.artifact(out_dir, 0);
    doc.save_to_file(&target).map_err(failed)?;
    Ok(vec![target])
}

/// An image object for `img`. Only images with transparency get PDFium's
/// RGBA path, which adds a soft mask; opaque ones are stored as gray or RGB,
/// at a third or less of the size.
fn image_object<'a>(
    doc: &PdfDocument<'a>,
    img: &image::DynamicImage,
) -> std::result::Result<PdfPageImageObject<'a>, PdfiumError> {
    let transparent = img.color().has_alpha() && img.to_rgba8().pixels().any(|p| p.0[3] < 255);
    if transparent {
        return PdfPageImageObject::new(doc, img);
    }
    let (width, height) = (img.width() as usize, img.height() as usize);
    let gray = !img.color().has_color();
    let (format, bytes) = if gray {
        (PdfBitmapFormat::Gray, 1)
    } else {
        (PdfBitmapFormat::BGR, 3)
    };
    // PDFium rows are padded to four bytes.
    let stride = (width * bytes).div_ceil(4) * 4;
    let mut buffer = vec![0u8; stride * height];
    if gray {
        for (row, src) in buffer
            .chunks_exact_mut(stride)
            .zip(img.to_luma8().as_raw().chunks_exact(width))
        {
            row[..width].copy_from_slice(src);
        }
    } else {
        for (row, src) in buffer
            .chunks_exact_mut(stride)
            .zip(img.to_rgb8().as_raw().chunks_exact(width * 3))
        {
            for (dst, px) in row.chunks_exact_mut(3).zip(src.chunks_exact(3)) {
                dst.copy_from_slice(&[px[2], px[1], px[0]]);
            }
        }
    }
    let to_pixels = |n: usize| Pixels::try_from(n).map_err(|_| PdfiumError::ImageSizeOutOfBounds);
    let bitmap = PdfBitmap::from_bytes(to_pixels(width)?, to_pixels(height)?, format, &mut buffer)?;
    // `new` needs pixels; the bitmap replaces this placeholder.
    let mut object = PdfPageImageObject::new(doc, &image::DynamicImage::new_luma8(1, 1))?;
    object.set_bitmap(&bitmap)?;
    Ok(object)
}

/// The size of a JPEG that can be embedded unchanged: upright, and within
/// `max_size`.
fn jpeg_as_is(ctx: &Ctx, input: &Path) -> Result<Option<(u32, u32)>> {
    if ctx.step.from.id != "jpeg" {
        return Ok(None);
    }
    let mut decoder = image::ImageReader::open(input)?
        .with_guessed_format()?
        .into_decoder()
        .map_err(failed)?;
    use image::ImageDecoder;
    let upright =
        decoder.orientation().map_err(failed)? == image::metadata::Orientation::NoTransforms;
    let (w, h) = decoder.dimensions();
    let fits = ctx.options.max_size.is_none_or(|m| w <= m && h <= m);
    Ok((upright && fits).then_some((w, h)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn worker_entry() {
        let _ = PdfiumEngine::new();
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn relative_override_never_runs_library_constructor() {
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
                .arg(dir.path().join("libpdfium.so"))
                .status()
                .unwrap()
                .success()
        );
        std::fs::copy(
            dir.path().join("libpdfium.so"),
            relative.join("libpdfium.so"),
        )
        .unwrap();
        for value in ["", ".", "relative"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "pdfium::tests::worker_entry",
                    "--ignored",
                    "--nocapture",
                ])
                .current_dir(dir.path())
                .env("CONVT_PDFIUM_DIR", value)
                .env("CONVT_TEST_NATIVE_MARKER", &marker)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(
                !marker.exists(),
                "relative PDFium override ran native code: {value:?}"
            );
            assert!(String::from_utf8_lossy(&output.stderr).contains("CONVT_PDFIUM_DIR"));
            assert!(String::from_utf8_lossy(&output.stderr).contains("must be absolute"));
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "pdfium::tests::worker_entry",
                "--ignored",
                "--nocapture",
            ])
            .env("CONVT_PDFIUM_DIR", dir.path())
            .env("CONVT_TEST_NATIVE_MARKER", &marker)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(marker.exists(), "absolute PDFium override was not loaded");
    }

    #[test]
    fn text_is_cleaned_and_escaped() {
        assert_eq!(
            clean_text("\r\n a\u{2}b  \r\nc\td\u{fffe}\r\n\r\n"),
            " ab\nc\td"
        );
        assert_eq!(
            text_html("<T&>", &[1], &["one\n\ntwo \"'".into()]),
            "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>&lt;T&amp;&gt;</title>\n\
             </head>\n<body>\n<section id=\"page-2\">\n<p>one</p>\n<p>two &quot;&#39;</p>\n</section>\n\
             </body>\n</html>\n"
        );
    }

    #[test]
    fn discovery_starts_with_the_override_and_the_resolved_executable_directory() {
        // `paths::exe_dir` resolves a symlinked CLI on macOS, so
        // /usr/local/bin/convt finds Contents/Frameworks/libpdfium.dylib.
        let dirs = candidate_dirs();
        let shipped = shipped_dirs(absolute_override(), crate::paths::exe_dir());
        assert_eq!(dirs[..shipped.len()], shipped[..]);
        assert_eq!(
            dirs[shipped.len()..shipped.len() + crate::paths::bundle_dirs().len()],
            crate::paths::bundle_dirs()[..]
        );
    }

    #[test]
    fn shipped_discovery_never_searches_executable_ancestors() {
        let exe_dir = PathBuf::from("/home/user/bin");
        assert_eq!(
            shipped_dirs(None, Some(exe_dir.clone())),
            vec![exe_dir.clone()]
        );
        let explicit = PathBuf::from("/opt/convt/pdfium");
        assert_eq!(
            shipped_dirs(Some(explicit.clone()), Some(exe_dir.clone())),
            vec![explicit, exe_dir]
        );
        assert!(shipped_dirs(None, None).is_empty());
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn release_discovery_contains_only_override_and_fixed_bundle_directories() {
        assert_eq!(
            candidate_dirs(),
            shipped_dirs(absolute_override(), crate::paths::exe_dir())
                .into_iter()
                .chain(crate::paths::bundle_dirs())
                .collect::<Vec<_>>()
        );
    }
}
