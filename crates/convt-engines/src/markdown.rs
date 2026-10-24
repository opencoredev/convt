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
            ("md", "html") => to_html(&source, &text::stem(input)),
            ("md", "txt") => to_plain(&source),
            ("html", "md") => from_html(&source)?,
            (from, to) => return Err(failed(format!("unsupported step {from} -> {to}"))),
        };
        let path = ctx.artifact(out_dir, 0);
        std::fs::write(&path, output)?;
        Ok(vec![path])
    }
}

/// A standalone page titled by the first heading, or else the file name.
pub fn to_html(markdown: &str, fallback_title: &str) -> String {
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, Parser::new_ext(markdown, options()));
    let title = first_heading(markdown).unwrap_or_else(|| fallback_title.to_owned());
    text::html_document(&title, &body)
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
                Some(if trimmed.is_empty() {
                    inner.into()
                } else {
                    format!("~~{trimmed}~~").into()
                })
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
