use std::path::{Path, PathBuf};

use convt_core::{Ctx, Engine, Error, Result, Step};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::text;

/// Markdown to HTML and plain text with pulldown-cmark, and HTML back to
/// Markdown with htmd. Office formats go through LibreOffice from the HTML.
pub struct MarkdownEngine;

fn failed(e: impl std::fmt::Display) -> Error {
    Error::EngineFailed {
        engine: "markdown",
        message: e.to_string(),
    }
}

/// GitHub-flavored Markdown: tables, strikethrough, task lists, footnotes.
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
}

impl Engine for MarkdownEngine {
    fn id(&self) -> &'static str {
        "markdown"
    }

    // Ahead of any other engine that learns these steps.
    fn priority(&self) -> i32 {
        10
    }

    fn steps(&self) -> Vec<Step> {
        crate::steps(&["md"], &["html", "txt"])
            .chain(crate::steps(&["html"], &["md"]))
            .collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let source = text::read("markdown", input)?;
        let output = match (ctx.step.from.id, ctx.step.to.id) {
            // Relative images resolve only next to the Markdown. A middle
            // hop's page sits in a temporary folder and a final page may go
            // to another folder, so those pages carry their images inline
            // (LibreOffice embeds them from there). A final page next to the
            // Markdown keeps its paths relative.
            ("md", "html") => {
                let dir = input_dir(input)?;
                if ctx.is_last_hop() && same_dir(&dir, &result_dir(out_dir)) {
                    to_html(&source, &text::stem(input))
                } else {
                    to_html_with_images_from(&source, &text::stem(input), &dir)
                }
            }
            ("md", "txt") => to_plain(&source),
            ("html", "md") => from_html(&source)?,
            (from, to) => return Err(failed(format!("unsupported step {from} -> {to}"))),
        };
        let path = ctx.artifact(out_dir, 0);
        std::fs::write(&path, output)?;
        Ok(vec![path])
    }
}

/// The absolute folder holding `input`.
fn input_dir(input: &Path) -> Result<PathBuf> {
    let dir = input.parent().unwrap_or(Path::new("."));
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    Ok(std::path::absolute(dir)?)
}

/// The folder a step writing into `out_dir` publishes to: the parent of the
/// registry's hidden `.convt-*` staging folder, or `out_dir` itself.
fn result_dir(out_dir: &Path) -> PathBuf {
    out_dir
        .ancestors()
        .find(|dir| {
            dir.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".convt-"))
        })
        .and_then(Path::parent)
        .unwrap_or(out_dir)
        .to_path_buf()
}

/// Whether `a` and `b` are the same folder, after resolving links.
fn same_dir(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// A standalone page titled by the first heading, or else the file name.
pub fn to_html(markdown: &str, fallback_title: &str) -> String {
    page(markdown, fallback_title, None)
}

/// [`to_html`], with each relative image that is a file in `dir` inlined
/// as a data URI. Other images keep their paths.
pub fn to_html_with_images_from(markdown: &str, fallback_title: &str, dir: &Path) -> String {
    page(
        markdown,
        fallback_title,
        url::Url::from_directory_path(dir).ok(),
    )
}

fn page(markdown: &str, fallback_title: &str, images_base: Option<url::Url>) -> String {
    let events = Parser::new_ext(markdown, options()).map(|event| match (event, &images_base) {
        (
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }),
            Some(base),
        ) => {
            let dest_url = inline_image(base, &dest_url).map_or(dest_url, Into::into);
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            })
        }
        (event, _) => event,
    });
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, events);
    let title = first_heading(markdown).unwrap_or_else(|| fallback_title.to_owned());
    text::html_document(&title, &body)
}

/// A data URI of the image `dest` names relative to `base`. `None` for a URL
/// with a scheme, a fragment, or a file that is missing or not an image, so
/// only pictures are read and never a text file a link happens to name.
fn inline_image(base: &url::Url, dest: &str) -> Option<String> {
    use base64::Engine as _;
    if dest.is_empty() || dest.starts_with('#') {
        return None;
    }
    if url::Url::parse(dest) != Err(url::ParseError::RelativeUrlWithoutBase) {
        return None;
    }
    let path = base.join(dest).ok()?.to_file_path().ok()?;
    let bytes = std::fs::read(&path).ok()?;
    let mime = match image::guess_format(&bytes) {
        Ok(format) => format.to_mime_type(),
        Err(_)
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("svg")) =>
        {
            "image/svg+xml"
        }
        Err(_) => return None,
    };
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:{mime};base64,{data}"))
}

fn first_heading(markdown: &str) -> Option<String> {
    let mut title: Option<String> = None;
    for event in Parser::new_ext(markdown, options()) {
        match (&mut title, event) {
            (None, Event::Start(Tag::Heading { .. })) => title = Some(String::new()),
            (Some(t), Event::Text(s) | Event::Code(s)) => t.push_str(&s),
            (Some(t), Event::SoftBreak | Event::HardBreak) => t.push(' '),
            (Some(t), Event::End(TagEnd::Heading(_))) => {
                let t = t.trim().to_owned();
                return (!t.is_empty()).then_some(t);
            }
            _ => {}
        }
    }
    None
}

/// The text of a Markdown document without its markup. Lists keep their
/// bullets and numbers, code blocks are indented, tables become columns.
pub fn to_plain(markdown: &str) -> String {
    let mut w = Plain::default();
    for event in Parser::new_ext(markdown, options()) {
        w.event(event);
    }
    let mut out = w.out.trim_end().to_owned();
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

#[derive(Default)]
struct Plain {
    out: String,
    /// The next number of each open list, `None` for bullets.
    lists: Vec<Option<u64>>,
    links: Vec<String>,
    code: Option<String>,
    table: Option<Vec<Vec<String>>>,
    /// The text of each open link, to tell if it shows its URL.
    link_text: Vec<String>,
}

impl Plain {
    fn push(&mut self, s: &str) {
        for text in &mut self.link_text {
            text.push_str(s);
        }
        if let Some(code) = &mut self.code {
            code.push_str(s);
        } else if let Some(cell) = self
            .table
            .as_mut()
            .and_then(|t| t.last_mut())
            .and_then(|r| r.last_mut())
        {
            cell.push_str(s);
        } else {
            self.out.push_str(s);
        }
    }

    /// Ends a block: a blank line after it, or one line break inside a list.
    fn end_block(&mut self) {
        let trimmed = self.out.trim_end_matches([' ', '\n']).len();
        if trimmed == 0 {
            self.out.clear();
            return;
        }
        self.out.truncate(trimmed);
        self.out
            .push_str(if self.lists.is_empty() { "\n\n" } else { "\n" });
    }

    fn start_line(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    fn event(&mut self, event: Event) {
        match event {
            Event::Start(Tag::Item) => {
                self.start_line();
                let depth = self.lists.len().saturating_sub(1);
                self.out.push_str(&"  ".repeat(depth));
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        format!("{}. ", *n - 1)
                    }
                    _ => "- ".into(),
                };
                self.out.push_str(&marker);
            }
            Event::Start(Tag::List(start)) => {
                self.start_line();
                self.lists.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                self.lists.pop();
                self.end_block();
            }
            Event::End(TagEnd::Item) => self.start_line(),
            Event::Start(Tag::CodeBlock(_)) => {
                self.start_line();
                self.code = Some(String::new());
            }
            Event::End(TagEnd::CodeBlock) => {
                let code = self.code.take().unwrap_or_default();
                let indent = "  ".repeat(self.lists.len());
                for line in code.trim_end_matches('\n').lines() {
                    if line.is_empty() {
                        self.out.push('\n');
                    } else {
                        self.out.push_str(&format!("{indent}    {line}\n"));
                    }
                }
                self.end_block();
            }
            Event::Start(Tag::Table(_)) => self.table = Some(Vec::new()),
            Event::Start(Tag::TableHead | Tag::TableRow) => {
                if let Some(t) = &mut self.table {
                    t.push(Vec::new());
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(row) = self.table.as_mut().and_then(|t| t.last_mut()) {
                    row.push(String::new());
                }
            }
            Event::End(TagEnd::Table) => {
                let rows = self.table.take().unwrap_or_default();
                self.start_line();
                self.out.push_str(&text::aligned_columns(&rows));
                self.end_block();
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                self.links.push(dest_url.into_string());
                self.link_text.push(String::new());
            }
            Event::End(TagEnd::Link) => {
                let dest = self.links.pop().unwrap_or_default();
                let shown = self.link_text.pop().unwrap_or_default();
                if !dest.is_empty() && !dest.starts_with('#') && shown.trim() != dest {
                    self.push(&format!(" ({dest})"));
                }
            }
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::BlockQuote(_)
                | TagEnd::FootnoteDefinition,
            ) if self.table.is_none() => self.end_block(),
            Event::Start(Tag::FootnoteDefinition(name)) => {
                self.start_line();
                self.out.push_str(&format!("[{name}] "));
            }
            Event::FootnoteReference(name) => self.push(&format!("[{name}]")),
            Event::Text(s) | Event::Code(s) => self.push(&s),
            Event::SoftBreak => self.push(" "),
            Event::HardBreak => {
                if self.table.is_some() {
                    self.push(" ");
                } else {
                    self.push("\n");
                }
            }
            Event::Rule => {
                self.start_line();
                self.out.push_str("----");
                self.end_block();
            }
            Event::TaskListMarker(done) => self.push(if done { "[x] " } else { "[ ] " }),
            // Raw HTML is markup, not text.
            _ => {}
        }
    }
}

/// HTML to Markdown. The `<head>`, scripts and styles are left out;
/// strikethrough and task-list checkboxes become their GFM forms.
pub fn from_html(html: &str) -> Result<String> {
    use htmd::Element;
    use htmd::element_handler::Handlers;
    let converter = htmd::HtmlToMarkdown::builder()
        .skip_tags(vec!["head", "script", "style", "noscript", "template"])
        .add_handler(
            vec!["del", "s", "strike"],
            |handlers: &dyn Handlers, element: Element| {
                let inner = handlers.walk_children(element.node).content;
                let trimmed = inner.trim();
                if trimmed.is_empty() {
                    return Some(inner.into());
                }
                // Spaces at the edges stay outside the delimiters, so
                // `keep<del> old </del>new` keeps its words apart.
                let start = inner.len() - inner.trim_start().len();
                let end = start + trimmed.len();
                Some(format!("{}~~{trimmed}~~{}", &inner[..start], &inner[end..]).into())
            },
        )
        .add_handler(vec!["input"], |_: &dyn Handlers, element: Element| {
            let attr = |name: &str| element.attrs.iter().find(|a| &*a.name.local == name);
            let checkbox = attr("type").is_some_and(|a| a.value.eq_ignore_ascii_case("checkbox"));
            checkbox.then(|| {
                if attr("checked").is_some() {
                    "[x] "
                } else {
                    "[ ] "
                }
                .to_owned()
                .into()
            })
        })
        .build();
    let mut md = converter.convert(html).map_err(failed)?;
    let end = md.trim_end().len();
    md.truncate(end);
    if !md.is_empty() {
        md.push('\n');
    }
    Ok(md)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_is_a_standalone_page_with_gfm() {
        let md = "# Notes & `<ideas>`\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n~~old~~\n\n- [x] done\n- [ ] todo\n";
        let html = to_html(md, "fallback");
        assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
        assert!(html.contains(r#"<meta charset="utf-8">"#));
        assert!(
            html.contains("<title>Notes &amp; &lt;ideas&gt;</title>"),
            "{html}"
        );
        assert!(
            html.contains("<table>") && html.contains("<td>1</td>"),
            "{html}"
        );
        assert!(html.contains("<del>old</del>"), "{html}");
        assert!(html.contains(r#"type="checkbox""#), "{html}");
        assert!(html.contains("<style>") && html.trim_end().ends_with("</html>"));
    }

    #[test]
    fn middle_hop_images_are_inlined_from_the_input_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        std::fs::create_dir(dir.path().join("my img")).unwrap();
        std::fs::write(dir.path().join("my img/red.png"), png.get_ref()).unwrap();
        std::fs::write(dir.path().join("notes.png"), "not a picture").unwrap();
        let md =
            "![a](my%20img/red.png) ![b](notes.png) ![c](gone.png) ![d](https://x.test/d.png)\n";
        let html = to_html_with_images_from(md, "x", dir.path());
        assert!(
            html.contains(r#"<img src="data:image/png;base64,iVBOR"#),
            "{html}"
        );
        assert!(html.contains(r#"src="notes.png""#), "{html}");
        assert!(html.contains(r#"src="gone.png""#), "{html}");
        assert!(html.contains(r#"src="https://x.test/d.png""#), "{html}");
        // A page the user keeps next to the Markdown stays relative.
        assert!(to_html(md, "x").contains(r#"src="my%20img/red.png""#));
    }

    #[test]
    fn a_final_page_in_another_folder_inlines_its_images() {
        use convt_core::{Cancel, Options, format_by_id};
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        std::fs::write(src.join("red.png"), png.get_ref()).unwrap();
        let input = src.join("notes.md");
        std::fs::write(&input, "![a](red.png)\n").unwrap();
        let convert = |dest: &Path| {
            let out = dest.join(".convt-test/0-0");
            std::fs::create_dir_all(&out).unwrap();
            let step = Step {
                from: format_by_id("md").unwrap(),
                to: format_by_id("html").unwrap(),
            };
            let (options, cancel) = (Options::default(), Cancel::new());
            let ctx = Ctx::new(step, &options, &|_| {}, &cancel);
            let files = MarkdownEngine.convert(&ctx, &input, &out).unwrap();
            std::fs::read_to_string(&files[0]).unwrap()
        };
        // Next to the Markdown, the path still resolves.
        let html = convert(&src);
        assert!(html.contains(r#"src="red.png""#), "{html}");
        // Through a differently spelled path to the same folder, too.
        let html = convert(&src.join("."));
        assert!(html.contains(r#"src="red.png""#), "{html}");
        // Anywhere else it would break, so the image goes inline.
        let elsewhere = dir.path().join("elsewhere");
        let html = convert(&elsewhere);
        assert!(html.contains(r#"src="data:image/png;base64,"#), "{html}");
    }

    #[test]
    fn html_title_falls_back_to_the_file_name() {
        assert!(to_html("just text\n", "notes").contains("<title>notes</title>"));
        assert!(to_html("", "empty").contains("<title>empty</title>"));
    }

    #[test]
    fn plain_text_keeps_lists_code_and_tables() {
        let md = "\
# Title

Some *emphasis* and `code` with a [link](https://convt.app).
Soft wrapped line.

1. one
2. two
   - nested
- [x] task

```rust
fn main() {}

let x = 1;
```

| Name | Qty |
|------|----:|
| Äpfel | 3 |
| b | 10 |

> quoted

---

<div>raw html</div>
";
        let plain = to_plain(md);
        assert_eq!(
            plain,
            "\
Title

Some emphasis and code with a link (https://convt.app). Soft wrapped line.

1. one
2. two
  - nested

- [x] task

    fn main() {}

    let x = 1;

Name   Qty
Äpfel  3
b      10

quoted

----
"
        );
    }

    #[test]
    fn plain_text_of_nothing_is_empty() {
        assert_eq!(to_plain(""), "");
        assert_eq!(to_plain("\n\n<!-- comment -->\n"), "");
    }

    #[test]
    fn a_link_showing_its_url_is_not_repeated() {
        assert_eq!(to_plain("<https://convt.app>\n"), "https://convt.app\n");
    }

    #[test]
    fn html_to_markdown_skips_the_head() {
        let html = "<html><head><title>T</title><style>p{}</style></head><body><h1>Hi</h1><p>A <strong>bold</strong> <a href=\"https://x.test\">link</a>.</p><ul><li>one</li><li>two</li></ul><script>alert(1)</script></body></html>";
        let md = from_html(html).unwrap();
        assert!(md.starts_with("# Hi\n"), "{md}");
        assert!(md.contains("**bold**"), "{md}");
        assert!(md.contains("[link](https://x.test)"), "{md}");
        assert!(md.contains("one") && md.contains("two"), "{md}");
        assert!(!md.contains("alert") && !md.contains("p{}"), "{md}");
        assert!(
            !md.lines().any(|l| l.trim() == "T"),
            "the title leaked: {md}"
        );
    }

    #[test]
    fn strikethrough_and_task_lists_come_back_from_html() {
        let md = from_html(&to_html("~~old~~ new\n\n- [x] done\n- [ ] todo\n", "x")).unwrap();
        assert!(md.contains("~~old~~ new"), "{md}");
        assert!(md.contains("[x] done") && md.contains("[ ] todo"), "{md}");
        let plain = from_html("<p><input type=text value=v>field</p>").unwrap();
        assert_eq!(plain, "field\n");
    }

    #[test]
    fn strikethrough_keeps_its_edge_spaces_outside_the_tildes() {
        assert_eq!(
            from_html("<p>keep<del> old </del>new</p>").unwrap(),
            "keep ~~old~~ new\n"
        );
        assert_eq!(
            from_html("<p>a<s>gone </s>b <strike> x</strike></p>").unwrap(),
            "a~~gone~~ b ~~x~~\n"
        );
        // The words stay apart once the Markdown renders again.
        let back = to_plain(&from_html("<p>keep<del> old </del>new</p>").unwrap());
        assert_eq!(back, "keep old new\n");
        let md = from_html(
            "<ul><li><input type=checkbox> done</li><li>x <input type=checkbox checked>y</li></ul>",
        )
        .unwrap();
        assert!(md.contains("[ ] done") && md.contains("x [x] y"), "{md}");
    }

    #[test]
    fn markdown_survives_a_round_trip_through_html() {
        let md = "# Heading\n\nText with **bold** and *italic*.\n";
        let back = from_html(&to_html(md, "x")).unwrap();
        assert!(back.contains("# Heading"), "{back}");
        assert!(back.contains("**bold**"), "{back}");
        assert!(
            back.contains("*italic*") || back.contains("_italic_"),
            "{back}"
        );
        assert!(!back.contains("max-width"), "the stylesheet leaked: {back}");
    }
}
