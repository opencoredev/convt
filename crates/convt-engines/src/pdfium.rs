use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use convt_core::{Background, Ctx, Engine, Error, Result, Step};
use pdfium_render::prelude::*;

/// Renders PDF pages with PDFium, loaded from `$CONVT_PDFIUM_DIR`, next to
/// the executable, or the system library path. Debug builds also use the
/// repository vendor directory; release tests set the override explicitly.
///
/// Writes one image per rendered page; the registry names them `doc.png`,
/// `doc-2.png` and so on.
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
    let mut dirs = shipped_dirs(
        absolute_override(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf)),
    );
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
        crate::steps(&["pdf"], &["png", "jpeg"]).collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let pdfium = self.pdfium()?;
        let doc = pdfium.load_pdf_from_file(input, None).map_err(failed)?;
        let pages = doc.pages();
        let count = pages.len() as u32;
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
            Some(Background::Color([r, g, b])) => {
                config.set_clear_color(PdfColor::new(r, g, b, 255))
            }
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
            shipped_dirs(
                absolute_override(),
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(Path::to_path_buf)),
            )
            .into_iter()
            .chain(crate::paths::bundle_dirs())
            .collect::<Vec<_>>()
        );
    }
}
