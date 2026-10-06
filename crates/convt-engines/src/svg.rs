use std::path::{Path, PathBuf};

use convt_core::{Background, Ctx, Engine, Error, Result, Step};
use resvg::{tiny_skia, usvg};

/// Renders SVG to PNG with resvg. Other raster targets go through the image
/// engine as a second hop.
pub struct SvgEngine;

fn failed(e: impl std::fmt::Display) -> Error {
    Error::EngineFailed {
        engine: "svg",
        message: e.to_string(),
    }
}

impl Engine for SvgEngine {
    fn id(&self) -> &'static str {
        "svg"
    }

    fn steps(&self) -> Vec<Step> {
        crate::steps(&["svg"], &["png"]).collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut opt = usvg::Options::default();
        opt.fontdb_mut().load_system_fonts();
        let tree = usvg::Tree::from_data(&std::fs::read(input)?, &opt).map_err(failed)?;
        ctx.check()?;
        // SVG user units are CSS pixels, 96 per inch.
        let mut scale = ctx.options.dpi.map_or(1.0, |d| d as f32 / 96.0);
        let size = tree.size();
        if let Some(max) = ctx.options.max_size {
            let longest = size.width().max(size.height()) * scale;
            if longest > max as f32 {
                scale *= max as f32 / longest;
            }
        }
        let (w, h) = (
            (size.width() * scale).ceil() as u32,
            (size.height() * scale).ceil() as u32,
        );
        let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or_else(|| failed("SVG has no size"))?;
        // A chosen background color goes under the drawing; otherwise the
        // canvas stays transparent, and a later step to a format without
        // transparency flattens it (white unless chosen).
        if let Some(Background::Color([r, g, b])) = ctx.options.background {
            pixmap.fill(tiny_skia::Color::from_rgba8(r, g, b, 255));
        }
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let output = ctx.artifact(out_dir, 0);
        pixmap.save_png(&output).map_err(failed)?;
        Ok(vec![output])
    }
}
