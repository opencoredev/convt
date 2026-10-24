use std::fmt;
use std::path::{Path, PathBuf};

use convt_core::{Ctx, Engine, Error, Result, Step};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

use crate::text;

/// Converts between CSV, TSV, JSON and YAML, and renders tables as HTML,
/// Markdown and aligned plain text. A table's first row is its header:
/// JSON and YAML hold an array with one object per later row.
pub struct DataEngine;

const TABLES: &[&str] = &["csv", "tsv"];
const TREES: &[&str] = &["json", "yaml"];

fn failed(message: impl fmt::Display) -> Error {
    Error::EngineFailed {
        engine: "data",
        message: message.to_string(),
    }
}

impl Engine for DataEngine {
    fn id(&self) -> &'static str {
        "data"
    }

    // CSV to HTML and text beat a spreadsheet round trip through LibreOffice.
    fn priority(&self) -> i32 {
        10
    }

    fn steps(&self) -> Vec<Step> {
        let all = [TABLES, TREES].concat();
        crate::steps(&all, &all)
            .chain(crate::steps(TABLES, &["html", "md", "txt"]))
            .collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let source = text::read("data", input)?;
        let (from, to) = (ctx.step.from.id, ctx.step.to.id);
        let output = if TREES.contains(&from) && TREES.contains(&to) {
            write_tree(&read_tree(from, &source)?, to)?
        } else {
            let rows = match from {
                "csv" | "tsv" => read_table(&source, delimiter(from))?,
                _ => tree_to_rows(read_tree(from, &source)?)?,
            };
            ctx.check()?;
            match to {
                "csv" | "tsv" => write_table(&rows, delimiter(to))?,
                "json" | "yaml" => write_tree(&rows_to_tree(rows), to)?,
                "html" => table_html(&rows, &title(input)),
                "md" => table_markdown(&rows),
                "txt" => text::aligned_columns(&rows),
                _ => return Err(failed(format!("unsupported step {from} -> {to}"))),
            }
        };
        let path = ctx.artifact(out_dir, 0);
        std::fs::write(&path, output)?;
        Ok(vec![path])
    }
}

fn delimiter(id: &str) -> u8 {
    if id == "tsv" { b'\t' } else { b',' }
}

/// The file name, or a generic title for an intermediate file of a route.
fn title(input: &Path) -> String {
    let stem = text::stem(input);
    if stem.is_empty() || stem.bytes().all(|b| b.is_ascii_digit()) {
        "Table".into()
    } else {
        stem
    }
}

/// A JSON or YAML value that keeps the order of object keys as written.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The text a table cell shows: scalars as written, nesting as compact JSON.
    fn into_cell(self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => s,
            nested => serde_json::to_string(&nested).expect("values always serialize"),
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Value;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON or YAML value")
            }
            fn visit_bool<E>(self, b: bool) -> std::result::Result<Value, E> {
                Ok(Value::Bool(b))
            }
            fn visit_i64<E>(self, n: i64) -> std::result::Result<Value, E> {
                Ok(Value::Number(n.into()))
            }
            fn visit_u64<E>(self, n: u64) -> std::result::Result<Value, E> {
                Ok(Value::Number(n.into()))
            }
            fn visit_f64<E>(self, n: f64) -> std::result::Result<Value, E> {
                // JSON has no NaN or infinity; YAML's `.nan` and `.inf` stay text.
                Ok(serde_json::Number::from_f64(n)
                    .map_or_else(|| Value::String(n.to_string()), Value::Number))
            }
            fn visit_str<E>(self, s: &str) -> std::result::Result<Value, E> {
                Ok(Value::String(s.to_owned()))
            }
            fn visit_string<E>(self, s: String) -> std::result::Result<Value, E> {
                Ok(Value::String(s))
            }
            fn visit_none<E>(self) -> std::result::Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_unit<E>(self) -> std::result::Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_some<D: Deserializer<'de>>(
                self,
                d: D,
            ) -> std::result::Result<Value, D::Error> {
                Value::deserialize(d)
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Value, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element()? {
                    items.push(item);
                }
                Ok(Value::Array(items))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Value, A::Error> {
                let mut entries: Vec<(String, Value)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<Value, Value>()? {
                    let key = key.into_cell();
                    // A repeated key keeps its first place and its last value.
                    match entries.iter_mut().find(|(k, _)| *k == key) {
                        Some(entry) => entry.1 = value,
                        None => entries.push((key, value)),
                    }
                }
                Ok(Value::Object(entries))
            }
            fn visit_enum<A: de::EnumAccess<'de>>(
                self,
                _: A,
            ) -> std::result::Result<Value, A::Error> {
                Err(de::Error::custom("YAML tags are not supported"))
            }
        }
        d.deserialize_any(V)
    }
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Value::Null => s.serialize_unit(),
            Value::Bool(b) => s.serialize_bool(*b),
            Value::Number(n) => n.serialize(s),
            Value::String(v) => s.serialize_str(v),
            Value::Array(items) => {
                let mut seq = s.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Value::Object(entries) => {
                let mut map = s.serialize_map(Some(entries.len()))?;
                for (k, v) in entries {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

/// Parses JSON, or YAML. A YAML file with several documents becomes an
/// array of them.
pub fn read_tree(format: &str, source: &str) -> Result<Value> {
    if source.trim().is_empty() {
        return Err(failed(format!(
            "the {} file is empty",
            format.to_uppercase()
        )));
    }
    if format == "json" {
        return serde_json::from_str(source).map_err(|e| failed(format!("invalid JSON: {e}")));
    }
    let mut documents = Vec::new();
    for document in serde_norway::Deserializer::from_str(source) {
        documents
            .push(Value::deserialize(document).map_err(|e| failed(format!("invalid YAML: {e}")))?);
    }
    match documents.len() {
        0 => Err(failed("the YAML file is empty")),
        1 => Ok(documents.pop().expect("one document")),
        _ => Ok(Value::Array(documents)),
    }
}

pub fn write_tree(value: &Value, format: &str) -> Result<String> {
    if format == "json" {
        let mut out = serde_json::to_string_pretty(value).map_err(failed)?;
        out.push('\n');
        Ok(out)
    } else {
        serde_norway::to_string(value).map_err(failed)
    }
}

/// Every row of a CSV or TSV file. Rows may differ in length. CSV fields may
/// be quoted. TSV (a tab `delimiter`) has no quoting: a quote is text, and
/// `\t`, `\n`, `\r` and `\\` stand for a tab, line breaks and a backslash.
/// Any other backslash stays as written.
pub fn read_table(source: &str, delimiter: u8) -> Result<Vec<Vec<String>>> {
    let tsv = delimiter == b'\t';
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .quoting(!tsv)
        .delimiter(delimiter)
        .from_reader(source.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| failed(format!("invalid table: {e}")))?;
        let cell = |s: &str| if tsv { tsv_unescape(s) } else { s.to_owned() };
        rows.push(record.iter().map(cell).collect());
    }
    Ok(rows)
}

/// CSV with fields quoted where needed, or TSV with tabs, line breaks and
/// backslashes inside cells escaped, as [`read_table`] reads them.
pub fn write_table(rows: &[Vec<String>], delimiter: u8) -> Result<String> {
    let tsv = delimiter == b'\t';
    let mut writer = csv::WriterBuilder::new()
        .flexible(true)
        .quote_style(if tsv {
            csv::QuoteStyle::Never
        } else {
            csv::QuoteStyle::Necessary
        })
        .delimiter(delimiter)
        .from_writer(Vec::new());
    for row in rows {
        if tsv {
            writer.write_record(row.iter().map(|c| tsv_escape(c)))
        } else {
            writer.write_record(row)
        }
        .map_err(failed)?;
    }
    let bytes = writer.into_inner().map_err(failed)?;
    String::from_utf8(bytes).map_err(failed)
}

fn tsv_escape(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    for c in cell.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

fn tsv_unescape(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    let mut chars = cell.chars().peekable();
    while let Some(c) = chars.next() {
        let escaped = match (c, chars.peek()) {
            ('\\', Some('\\')) => '\\',
            ('\\', Some('t')) => '\t',
            ('\\', Some('n')) => '\n',
            ('\\', Some('r')) => '\r',
            _ => {
                out.push(c);
                continue;
            }
        };
        chars.next();
        out.push(escaped);
    }
    out
}

/// The header row, padded so every column has a distinct, non-empty name.
fn header(rows: &[Vec<String>]) -> Vec<String> {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut names: Vec<String> = Vec::with_capacity(width);
    for i in 0..width {
        let given = rows[0].get(i).map(|s| s.trim()).unwrap_or_default();
        let base = if given.is_empty() {
            format!("column {}", i + 1)
        } else {
            given.to_owned()
        };
        let mut name = base.clone();
        let mut n = 2;
        while names.contains(&name) {
            name = format!("{base} {n}");
            n += 1;
        }
        names.push(name);
    }
    names
}

/// One object per row after the header, keyed by the header. Short rows
/// leave their missing cells out.
pub fn rows_to_tree(rows: Vec<Vec<String>>) -> Value {
    if rows.is_empty() {
        return Value::Array(Vec::new());
    }
    let keys = header(&rows);
    Value::Array(
        rows.into_iter()
            .skip(1)
            .map(|row| {
                Value::Object(
                    keys.iter()
                        .cloned()
                        .zip(row.into_iter().map(Value::String))
                        .collect(),
                )
            })
            .collect(),
    )
}

/// A table from an array of objects (their keys in first-seen order make the
/// header), an array of arrays (rows as they are), or a single object (one row).
pub fn tree_to_rows(value: Value) -> Result<Vec<Vec<String>>> {
    let items = match value {
        Value::Array(items) => items,
        object @ Value::Object(_) => vec![object],
        _ => {
            return Err(failed(
                "a table needs an array of objects or an array of arrays, not a single value",
            ));
        }
    };
    if items.is_empty() {
        return Ok(Vec::new());
    }
    if items.iter().all(|v| matches!(v, Value::Object(_))) {
        let mut keys: Vec<String> = Vec::new();
        for item in &items {
            if let Value::Object(entries) = item {
                for (k, _) in entries {
                    if !keys.contains(k) {
                        keys.push(k.clone());
                    }
                }
            }
        }
        if keys.is_empty() {
            return Err(failed("the objects have no fields to make columns from"));
        }
        let mut rows = vec![keys.clone()];
        for item in items {
            let Value::Object(mut entries) = item else {
                unreachable!()
            };
            rows.push(
                keys.iter()
                    .map(|k| {
                        entries
                            .iter()
                            .position(|(key, _)| key == k)
                            .map(|i| entries.swap_remove(i).1.into_cell())
                            .unwrap_or_default()
                    })
                    .collect(),
            );
        }
        Ok(rows)
    } else if items.iter().all(|v| matches!(v, Value::Array(_))) {
        Ok(items
            .into_iter()
            .map(|row| match row {
                Value::Array(cells) => cells.into_iter().map(Value::into_cell).collect(),
                _ => unreachable!(),
            })
            .collect())
    } else {
        Err(failed(
            "a table needs an array of objects or an array of arrays; this array mixes other values in",
        ))
    }
}

/// A standalone page with the table, its first row as the header.
pub fn table_html(rows: &[Vec<String>], title: &str) -> String {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let cell = |tag: &str, s: Option<&String>| {
        let s = text::escape_html(s.map_or("", String::as_str)).replace('\n', "<br>");
        format!("<{tag}>{s}</{tag}>")
    };
    let row = |tag: &str, r: &Vec<String>| {
        let cells: String = (0..width).map(|i| cell(tag, r.get(i))).collect();
        format!("<tr>{cells}</tr>\n")
    };
    let mut body = String::from("<table>\n");
    if let Some((head, rest)) = rows.split_first() {
        body.push_str(&format!("<thead>\n{}</thead>\n<tbody>\n", row("th", head)));
        for r in rest {
            body.push_str(&row("td", r));
        }
        body.push_str("</tbody>\n");
    }
    body.push_str("</table>\n");
    text::html_document(title, &body)
}

/// A GitHub-flavored Markdown pipe table, its first row as the header.
pub fn table_markdown(rows: &[Vec<String>]) -> String {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return String::new();
    }
    let escape = |s: &str| {
        s.replace('\\', "\\\\")
            .replace('|', "\\|")
            .replace("\r\n", "<br>")
            .replace(['\n', '\r'], "<br>")
    };
    let line = |r: &Vec<String>| {
        let cells: Vec<String> = (0..width)
            .map(|i| escape(r.get(i).map_or("", String::as_str)))
            .collect();
        format!("| {} |\n", cells.join(" | "))
    };
    let mut out = line(&rows[0]);
    out.push_str(&format!("|{}\n", " --- |".repeat(width)));
    for r in &rows[1..] {
        out.push_str(&line(r));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(s: &str) -> Vec<Vec<String>> {
        read_table(s, b',').unwrap()
    }

    #[test]
    fn csv_to_json_keys_rows_by_header_in_order() {
        let rows = table("zeta,alpha,mid\n1,2,3\n4,5,6\n");
        let json = write_tree(&rows_to_tree(rows), "json").unwrap();
        assert_eq!(
            json,
            "[\n  {\n    \"zeta\": \"1\",\n    \"alpha\": \"2\",\n    \"mid\": \"3\"\n  },\n  {\n    \"zeta\": \"4\",\n    \"alpha\": \"5\",\n    \"mid\": \"6\"\n  }\n]\n"
        );
    }

    #[test]
    fn quoted_fields_keep_commas_quotes_and_newlines() {
        let csv = "name,note\n\"Smith, J\",\"said \"\"hi\"\"\nthen left\"\n";
        let rows = table(csv);
        assert_eq!(rows[1], ["Smith, J", "said \"hi\"\nthen left"]);
        let back = write_table(&tree_to_rows(rows_to_tree(rows.clone())).unwrap(), b',').unwrap();
        assert_eq!(back, csv);
        let tsv = write_table(&rows, b'\t').unwrap();
        assert_eq!(read_table(&tsv, b'\t').unwrap(), rows);
    }

    #[test]
    fn tsv_keeps_quotes_and_escapes_tabs_and_breaks() {
        let tsv = "note\tpath\n\"Hello\" she said\tC:\\data\n";
        let rows = read_table(tsv, b'\t').unwrap();
        assert_eq!(rows[1], ["\"Hello\" she said", "C:\\data"]);
        let cells = vec![
            vec!["a\tb".to_owned(), "line\nbreak\r".into()],
            vec![r"back\slash \t not a tab".into(), "\"".into()],
        ];
        let written = write_table(&cells, b'\t').unwrap();
        assert_eq!(
            written,
            "a\\tb\tline\\nbreak\\r\nback\\\\slash \\\\t not a tab\t\"\n"
        );
        assert_eq!(read_table(&written, b'\t').unwrap(), cells);
    }

    #[test]
    fn unicode_survives_every_format() {
        let rows = table("名前,emoji\nÅsa,🎉\n");
        for format in ["json", "yaml"] {
            let text = write_tree(&rows_to_tree(rows.clone()), format).unwrap();
            assert!(text.contains("Åsa") && text.contains("🎉"), "{text}");
            let back = tree_to_rows(read_tree(format, &text).unwrap()).unwrap();
            assert_eq!(back, rows, "{format}");
        }
    }

    #[test]
    fn a_bom_is_not_part_of_the_first_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.csv");
        std::fs::write(&path, "\u{feff}id,name\n1,x\n").unwrap();
        let rows = read_table(&text::read("data", &path).unwrap(), b',').unwrap();
        assert_eq!(rows[0], ["id", "name"]);
    }

    #[test]
    fn ragged_rows_get_named_columns() {
        let rows = table("a,b\n1\n2,3,4\n");
        let tree = rows_to_tree(rows);
        let json = serde_json::to_string(&tree).unwrap();
        assert_eq!(json, r#"[{"a":"1"},{"a":"2","b":"3","column 3":"4"}]"#);
        let back = tree_to_rows(tree).unwrap();
        assert_eq!(
            back,
            [
                vec!["a", "b", "column 3"],
                vec!["1", "", ""],
                vec!["2", "3", "4"]
            ]
        );
    }

    #[test]
    fn duplicate_and_blank_headers_stay_distinct() {
        let json = serde_json::to_string(&rows_to_tree(table("a,a,\n1,2,3\n"))).unwrap();
        assert_eq!(json, r#"[{"a":"1","a 2":"2","column 3":"3"}]"#);
    }

    #[test]
    fn empty_input_is_an_empty_table() {
        assert!(table("").is_empty());
        assert_eq!(
            write_tree(&rows_to_tree(Vec::new()), "json").unwrap(),
            "[]\n"
        );
        assert!(
            tree_to_rows(read_tree("json", "[]").unwrap())
                .unwrap()
                .is_empty()
        );
        assert_eq!(write_table(&[], b',').unwrap(), "");
        assert_eq!(table_markdown(&[]), "");
        assert!(read_tree("json", "  \n").is_err());
        assert!(read_tree("yaml", "").is_err());
    }

    #[test]
    fn objects_union_their_keys_in_first_seen_order() {
        let json = r#"[{"b":1,"a":true},{"c":null,"a":2.5},{"b":{"x":[1,2]},"d":"s"}]"#;
        let rows = tree_to_rows(read_tree("json", json).unwrap()).unwrap();
        assert_eq!(
            rows,
            [
                vec!["b", "a", "c", "d"],
                vec!["1", "true", "", ""],
                vec!["", "2.5", "", ""],
                vec![r#"{"x":[1,2]}"#, "", "", "s"],
            ]
        );
    }

    #[test]
    fn arrays_of_arrays_are_rows() {
        let rows = tree_to_rows(read_tree("yaml", "- [a, b]\n- [1, 2]\n").unwrap()).unwrap();
        assert_eq!(rows, [vec!["a", "b"], vec!["1", "2"]]);
    }

    #[test]
    fn shapes_that_are_not_tables_fail_clearly() {
        for (format, source) in [
            ("json", "42"),
            ("json", "\"text\""),
            ("json", "[1, 2]"),
            ("json", "[{\"a\":1}, [1]]"),
            ("json", "[{}]"),
            ("yaml", "just a string"),
        ] {
            let error = tree_to_rows(read_tree(format, source).unwrap()).unwrap_err();
            assert!(
                matches!(&error, Error::EngineFailed { engine: "data", message } if message.contains("table") || message.contains("columns")),
                "{source}: {error}"
            );
        }
        let error = read_tree("json", "{oops").unwrap_err();
        assert!(error.to_string().contains("invalid JSON"), "{error}");
    }

    #[test]
    fn json_and_yaml_convert_any_shape_in_order() {
        let json = r#"{"z":1,"a":{"nested":[true,null,"x"]},"m":-3.25}"#;
        let yaml = write_tree(&read_tree("json", json).unwrap(), "yaml").unwrap();
        assert_eq!(
            yaml,
            "z: 1\na:\n  nested:\n  - true\n  - null\n  - x\nm: -3.25\n"
        );
        let back = write_tree(&read_tree("yaml", &yaml).unwrap(), "json").unwrap();
        assert_eq!(
            serde_json::to_string(&read_tree("json", &back).unwrap()).unwrap(),
            json
        );
    }

    #[test]
    fn several_yaml_documents_become_an_array() {
        let value = read_tree("yaml", "a: 1\n---\na: 2\n").unwrap();
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            r#"[{"a":1},{"a":2}]"#
        );
    }

    #[test]
    fn strings_that_look_like_numbers_stay_strings_in_yaml() {
        let yaml = write_tree(&rows_to_tree(table("zip,flag\n02134,yes\n")), "yaml").unwrap();
        let back = tree_to_rows(read_tree("yaml", &yaml).unwrap()).unwrap();
        assert_eq!(back[1], ["02134", "yes"], "{yaml}");
    }

    #[test]
    fn markdown_tables_escape_pipes() {
        let md = table_markdown(&table("a|b,c\n\"x\ny\",\\\n"));
        assert_eq!(md, "| a\\|b | c |\n| --- | --- |\n| x<br>y | \\\\ |\n");
    }

    #[test]
    fn html_tables_escape_cells() {
        let html = table_html(&table("h\n<b>&\n"), "t");
        assert!(
            html.contains("<th>h</th>") && html.contains("<td>&lt;b&gt;&amp;</td>"),
            "{html}"
        );
        assert!(
            html.contains("<thead>") && html.contains("<tbody>"),
            "{html}"
        );
    }

    #[test]
    fn text_aligns_columns_by_display_width() {
        let txt = text::aligned_columns(&table("name,n\n日本,1\nab,22\n"));
        assert_eq!(txt, "name  n\n日本  1\nab    22\n");
    }
}
