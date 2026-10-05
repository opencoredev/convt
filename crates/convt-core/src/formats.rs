use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Image,
    Vector,
    Video,
    Audio,
    Pdf,
    Document,
    Presentation,
    Spreadsheet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Format {
    /// Stable identifier used by the CLI, API and presets, e.g. `"jpeg"`.
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    /// Extensions without the dot. The first one is used for output files.
    pub extensions: &'static [&'static str],
    pub mime: &'static str,
}

impl Format {
    pub fn extension(&self) -> &'static str {
        self.extensions[0]
    }
}

macro_rules! formats {
    ($($id:literal, $name:literal, $cat:ident, [$($ext:literal),+], $mime:literal;)+) => {
        pub static FORMATS: &[Format] = &[
            $(Format { id: $id, name: $name, category: Category::$cat, extensions: &[$($ext),+], mime: $mime },)+
        ];
    };
}

formats! {
    // Images
    "jpeg", "JPEG", Image, ["jpg", "jpeg", "jfif"], "image/jpeg";
    "png", "PNG", Image, ["png"], "image/png";
    "webp", "WebP", Image, ["webp"], "image/webp";
    "heic", "HEIC", Image, ["heic", "heif"], "image/heic";
    "avif", "AVIF", Image, ["avif"], "image/avif";
    "gif", "GIF", Image, ["gif"], "image/gif";
    "tiff", "TIFF", Image, ["tiff", "tif"], "image/tiff";
    "bmp", "BMP", Image, ["bmp"], "image/bmp";
    "ico", "ICO", Image, ["ico"], "image/x-icon";
    "tga", "TGA", Image, ["tga"], "image/x-tga";
    "ppm", "PPM", Image, ["ppm", "pgm", "pbm", "pnm"], "image/x-portable-anymap";
    "qoi", "QOI", Image, ["qoi"], "image/qoi";
    "exr", "OpenEXR", Image, ["exr"], "image/x-exr";
    // Vector
    "svg", "SVG", Vector, ["svg"], "image/svg+xml";
    // Video
    "mp4", "MP4", Video, ["mp4", "m4v"], "video/mp4";
    "mov", "MOV", Video, ["mov"], "video/quicktime";
    "webm", "WebM", Video, ["webm"], "video/webm";
    "mkv", "MKV", Video, ["mkv"], "video/x-matroska";
    "avi", "AVI", Video, ["avi"], "video/x-msvideo";
    // Audio
    "mp3", "MP3", Audio, ["mp3"], "audio/mpeg";
    "wav", "WAV", Audio, ["wav"], "audio/wav";
    "flac", "FLAC", Audio, ["flac"], "audio/flac";
    "aac", "AAC", Audio, ["aac"], "audio/aac";
    "m4a", "M4A", Audio, ["m4a"], "audio/mp4";
    "ogg", "OGG", Audio, ["ogg", "oga"], "audio/ogg";
    "opus", "Opus", Audio, ["opus"], "audio/opus";
    // PDF
    "pdf", "PDF", Pdf, ["pdf"], "application/pdf";
    // Office
    "docx", "DOCX", Document, ["docx"], "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
    "doc", "DOC", Document, ["doc"], "application/msword";
    "odt", "ODT", Document, ["odt"], "application/vnd.oasis.opendocument.text";
    "rtf", "RTF", Document, ["rtf"], "application/rtf";
    "txt", "Plain text", Document, ["txt"], "text/plain";
    "html", "HTML", Document, ["html", "htm"], "text/html";
    "pptx", "PPTX", Presentation, ["pptx"], "application/vnd.openxmlformats-officedocument.presentationml.presentation";
    "ppt", "PPT", Presentation, ["ppt"], "application/vnd.ms-powerpoint";
    "odp", "ODP", Presentation, ["odp"], "application/vnd.oasis.opendocument.presentation";
    "xlsx", "XLSX", Spreadsheet, ["xlsx"], "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
    "xls", "XLS", Spreadsheet, ["xls"], "application/vnd.ms-excel";
    "ods", "ODS", Spreadsheet, ["ods"], "application/vnd.oasis.opendocument.spreadsheet";
    "csv", "CSV", Spreadsheet, ["csv"], "text/csv";
}

pub fn format_by_id(id: &str) -> Option<&'static Format> {
    let id = id.to_ascii_lowercase();
    FORMATS
        .iter()
        .find(|f| f.id == id)
        .or_else(|| format_by_extension_str(&id))
}

/// Detects a file's format from its extension. Content sniffing comes later.
pub fn format_by_extension(path: &Path) -> Option<&'static Format> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    format_by_extension_str(&ext)
}

fn format_by_extension_str(ext: &str) -> Option<&'static Format> {
    FORMATS.iter().find(|f| f.extensions.contains(&ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = FORMATS.iter().map(|f| f.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), FORMATS.len());
    }

    #[test]
    fn lookups() {
        assert_eq!(format_by_id("JPG").unwrap().id, "jpeg");
        assert_eq!(
            format_by_extension(Path::new("a/IMG_1.HEIC")).unwrap().id,
            "heic"
        );
        assert!(format_by_extension(Path::new("noext")).is_none());
    }
}
