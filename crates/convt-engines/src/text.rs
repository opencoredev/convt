//! Helpers the text engines (Markdown, data, subtitles) share.

use std::path::Path;

use convt_core::{Error, Result};

/// Reads a UTF-8 text file without its byte order mark.
pub(crate) fn read(engine: &'static str, input: &Path) -> Result<String> {
    let bytes = std::fs::read(input)?;
    let text = String::from_utf8(bytes).map_err(|_| Error::EngineFailed {
        engine,
        message: "the file is not UTF-8 text".into(),
    })?;
    Ok(match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_owned(),
        None => text,
    })
}

/// Escapes text for HTML element content and attribute values.
pub(crate) fn escape_html(text: &str) -> String {
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

/// A standalone HTML page around `body`, with a small readable stylesheet.
pub(crate) fn html_document(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
body {{ max-width: 46rem; margin: 2rem auto; padding: 0 1rem; font: 16px/1.6 system-ui, -apple-system, "Segoe UI", sans-serif; color: #1f2328; }}
h1, h2, h3, h4, h5, h6 {{ line-height: 1.25; margin: 1.5em 0 0.5em; }}
pre, code {{ font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 0.9em; background: #f4f5f7; border-radius: 4px; }}
code {{ padding: 0.1em 0.3em; }}
pre {{ padding: 0.8em 1em; overflow-x: auto; }}
pre code {{ padding: 0; background: none; }}
blockquote {{ margin: 0; padding: 0 1em; color: #59636e; border-left: 4px solid #d1d9e0; }}
table {{ border-collapse: collapse; margin: 1em 0; }}
th, td {{ border: 1px solid #d1d9e0; padding: 0.3em 0.7em; text-align: left; vertical-align: top; }}
th {{ background: #f4f5f7; }}
img {{ max-width: 100%; }}
</style>
</head>
<body>
{body}</body>
</html>
"#,
        title = escape_html(title),
    )
}

/// Lays `rows` out in columns padded to the widest cell, two spaces apart.
/// Line breaks inside a cell become spaces.
pub(crate) fn aligned_columns(rows: &[Vec<String>]) -> String {
    use unicode_width::UnicodeWidthStr;
    let rows: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.split(['\r', '\n']).collect::<Vec<_>>().join(" "))
                .collect()
        })
        .collect();
    // One pass over the cells that exist: a ragged table can be far wider
    // than most of its rows.
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0usize; columns];
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.width());
        }
    }
    let mut out = String::new();
    for row in &rows {
        let mut line = String::new();
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            line.push_str(cell);
            line.extend(std::iter::repeat_n(' ', widths[i] - cell.width()));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// The file name without its extension, for page titles.
pub(crate) fn stem(input: &Path) -> String {
    input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}
