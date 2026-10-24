//! What a finished conversion puts on the clipboard: the image itself
//! when there is one the clipboard can hold, otherwise the output paths.

use std::path::{Path, PathBuf};

use gpui_kit::{ClipboardItem, Image, ImageFormat};

pub fn clipboard_item(outputs: &[PathBuf]) -> ClipboardItem {
    if let [one] = outputs
        && let Some(format) = image_format(one)
        && let Ok(bytes) = std::fs::read(one)
    {
        return ClipboardItem::new_image(&Image::from_bytes(format, bytes));
    }
    let paths: Vec<String> = outputs.iter().map(|p| p.display().to_string()).collect();
    ClipboardItem::new_string(paths.join("\n"))
}

fn image_format(path: &Path) -> Option<ImageFormat> {
    Some(match convt_core::format_by_extension(path)?.id {
        "png" => ImageFormat::Png,
        "jpeg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::Webp,
        "gif" => ImageFormat::Gif,
        "svg" => ImageFormat::Svg,
        "bmp" => ImageFormat::Bmp,
        "tiff" => ImageFormat::Tiff,
        _ => return None,
    })
}
