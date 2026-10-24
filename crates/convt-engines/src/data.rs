use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use convt_core::{Ctx, Engine, Error, Result, Step};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use serde_json::value::RawValue;

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
                "json" | "yaml" => write_tree(&rows_to_tree(rows)?, to)?,
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
    /// The number as written, so `12345678901234567890123` stays exact.
    Number(String),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    fn kind(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "text",
            Value::Array(_) => "list",
            Value::Object(_) => "mapping",
        }
    }

    /// The text a table cell shows: scalars as written, nesting as compact JSON.
    fn into_cell(self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n,
            Value::String(s) => s,
            nested => serde_json::to_string(&nested).expect("values always serialize"),
        }
    }
}

/// An object's members in the order they are read. A repeated key keeps its
/// first place and its last value, as `JSON.parse` and PyYAML do. Two
/// different keys that make the same text, like YAML's `1` and `"1"`, are
/// an error instead, since one would silently replace the other.
#[derive(Default)]
struct Members {
    entries: Vec<(String, Value)>,
    /// Each key's place in `entries`, and the kind of value it was written as.
    index: HashMap<String, (usize, &'static str)>,
}

impl Members {
    fn insert(&mut self, key: Value, value: Value) -> std::result::Result<(), String> {
        let kind = key.kind();
        match self.index.entry(key.into_cell()) {
            Entry::Occupied(slot) => {
                let (i, first) = *slot.get();
                if first != kind {
                    return Err(format!(
                        "a {first} key and a {kind} key both read as {:?}; rename one so neither is lost",
                        slot.key()
                    ));
                }
                self.entries[i].1 = value;
            }
            Entry::Vacant(slot) => {
                self.entries.push((slot.key().clone(), value));
                slot.insert((self.entries.len() - 1, kind));
            }
        }
        Ok(())
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
                Ok(Value::Number(n.to_string()))
            }
            fn visit_u64<E>(self, n: u64) -> std::result::Result<Value, E> {
                Ok(Value::Number(n.to_string()))
            }
            fn visit_i128<E>(self, n: i128) -> std::result::Result<Value, E> {
                Ok(Value::Number(n.to_string()))
            }
            fn visit_u128<E>(self, n: u128) -> std::result::Result<Value, E> {
                Ok(Value::Number(n.to_string()))
            }
            fn visit_f64<E>(self, n: f64) -> std::result::Result<Value, E> {
                // JSON has no NaN or infinity; YAML's `.nan` and `.inf` stay text.
                Ok(serde_json::Number::from_f64(n).map_or_else(
                    || Value::String(n.to_string()),
                    |n| Value::Number(n.to_string()),
                ))
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
                let mut members = Members::default();
                while let Some((key, value)) = map.next_entry::<Value, Value>()? {
                    members.insert(key, value).map_err(de::Error::custom)?;
                }
                Ok(Value::Object(members.entries))
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
            // Raw, so JSON gets the digits as written rather than an f64.
            Value::Number(n) => serde_json::value::RawValue::from_string(n.clone())
                .map_err(serde::ser::Error::custom)?
                .serialize(s),
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
        let invalid = |e: serde_json::Error| failed(format!("invalid JSON: {e}"));
        let raw: &RawValue = serde_json::from_str(source).map_err(invalid)?;
        return json_value(raw, 0).map_err(invalid);
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

/// How deep JSON may nest, as serde_json allows.
const MAX_DEPTH: usize = 128;

/// A JSON value with every number kept as written. serde_json without its
/// workspace-wide `arbitrary_precision` feature reads numbers as `f64`,
/// which rounds `12345678901234567890123` to `1.2345678901234568e22`.
fn json_value(raw: &RawValue, depth: usize) -> serde_json::Result<Value> {
    use serde::de::Error as _;
    if depth == MAX_DEPTH {
        return Err(serde_json::Error::custom("recursion limit exceeded"));
    }
    let text = raw.get();
    match text.as_bytes().first() {
        Some(b'{') => {
            let RawEntries(raw_entries) = serde_json::from_str(text)?;
            let mut members = Members::default();
            for (key, raw) in raw_entries {
                let value = json_value(raw, depth + 1)?;
                members
                    .insert(Value::String(key), value)
                    .map_err(serde_json::Error::custom)?;
            }
            Ok(Value::Object(members.entries))
        }
        Some(b'[') => {
            let items: Vec<&RawValue> = serde_json::from_str(text)?;
            items
                .into_iter()
                .map(|raw| json_value(raw, depth + 1))
                .collect::<serde_json::Result<_>>()
                .map(Value::Array)
        }
        Some(b'-' | b'0'..=b'9') => Ok(Value::Number(text.to_owned())),
        _ => serde_json::from_str(text),
    }
}

/// A JSON object's members in order, each value still unparsed.
struct RawEntries<'a>(Vec<(String, &'a RawValue)>);

impl<'de: 'a, 'a> Deserialize<'de> for RawEntries<'a> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = RawEntries<'de>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(RawEntries(entries))
            }
        }
        d.deserialize_map(V)
    }
}

pub fn write_tree(value: &Value, format: &str) -> Result<String> {
    if format == "json" {
        let mut out = serde_json::to_string_pretty(value).map_err(failed)?;
        out.push('\n');
        Ok(out)
    } else {
        let mut out = String::new();
        yaml_node(&mut out, value, 0);
        Ok(out)
    }
}

/// Writes `value` as block YAML where the cursor stands: at the start of a
/// line already indented to `indent`, or after `- `.
///
/// serde_norway quotes only what a YAML 1.2 reader would misread, so `on`,
/// `NO`, `1_000` or `2024-01-01` came out bare and PyYAML, a YAML 1.1
/// reader, turned them into booleans, numbers and dates. Strings here are
/// bare only when no YAML version reads them as anything else.
fn yaml_node(out: &mut String, value: &Value, indent: usize) {
    match value {
        Value::Object(entries) if !entries.is_empty() => {
            for (i, (key, value)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push_str(&" ".repeat(indent));
                }
                out.push_str(&yaml_string(key, indent));
                out.push(':');
                match value {
                    Value::Object(e) if !e.is_empty() => {
                        out.push('\n');
                        out.push_str(&" ".repeat(indent + 2));
                        yaml_node(out, value, indent + 2);
                    }
                    // A list under a key starts at the key's own indent.
                    Value::Array(a) if !a.is_empty() => {
                        out.push('\n');
                        out.push_str(&" ".repeat(indent));
                        yaml_node(out, value, indent);
                    }
                    _ => {
                        out.push(' ');
                        yaml_node(out, value, indent);
                    }
                }
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(&" ".repeat(indent));
                }
                out.push_str("- ");
                yaml_node(out, item, indent + 2);
            }
        }
        scalar => {
            out.push_str(&match scalar {
                Value::Null => "null".into(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => yaml_number(n),
                Value::String(s) => yaml_string(s, indent),
                Value::Array(_) => "[]".into(),
                Value::Object(_) => "{}".into(),
            });
            out.push('\n');
        }
    }
}

/// A JSON number as a number to YAML 1.1 too, which needs a `.` and a signed
/// exponent in a float: `1e5` becomes `1.0e+5`.
fn yaml_number(text: &str) -> String {
    let Some(e) = text.find(['e', 'E']) else {
        return text.to_owned();
    };
    let (mantissa, e, exponent) = (&text[..e], &text[e..=e], &text[e + 1..]);
    let point = if mantissa.contains('.') { "" } else { ".0" };
    let sign = if exponent.starts_with(['+', '-']) {
        ""
    } else {
        "+"
    };
    format!("{mantissa}{point}{e}{sign}{exponent}")
}

/// `s` bare if that is safe, else quoted. Bare strings start with a letter
/// or `_` and hold only letters, digits, spaces and `_-./()+@`, so no
/// number, date, time, `~`, `.inf` or indicator can be mistaken for one,
/// and are none of the YAML 1.1 and 1.2 words for booleans and null.
fn yaml_string(s: &str, indent: usize) -> String {
    const WORDS: &[&str] = &["y", "n", "yes", "no", "on", "off", "true", "false", "null"];
    let mut chars = s.chars();
    let bare = chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || " _-./()+@".contains(c))
        && !s.ends_with(' ')
        && !WORDS.iter().any(|w| s.eq_ignore_ascii_case(w));
    if bare {
        return s.to_owned();
    }
    let printable = |c: char| !c.is_control() && !matches!(c, '\u{2028}' | '\u{2029}' | '\u{feff}');
    if s.chars().all(printable) {
        return format!("'{}'", s.replace('\'', "''"));
    }
    if let Some(block) = yaml_literal(s, indent) {
        return block;
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if !printable(c) => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A multi-line string as a literal block, when its lines need no escaping:
/// no control characters but line breaks, at most one trailing break, and
/// no leading space that would set the block's indentation.
fn yaml_literal(s: &str, indent: usize) -> Option<String> {
    let (body, chomp) = match s.strip_suffix('\n') {
        Some(body) => (body, ""),
        None => (s, "-"),
    };
    let plain = |c: char| {
        c == '\t' || !c.is_control() && !matches!(c, '\u{2028}' | '\u{2029}' | '\u{feff}')
    };
    if !body.contains('\n')
        || body.ends_with('\n')
        || body.starts_with([' ', '\n'])
        || !body.chars().all(|c| c == '\n' || plain(c))
    {
        return None;
    }
    let pad = " ".repeat(indent + 2);
    let mut out = format!("|{chomp}");
    for line in body.split('\n') {
        out.push('\n');
        if !line.is_empty() {
            out.push_str(&pad);
            out.push_str(line);
        }
    }
    Some(out)
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
    let mut taken: HashSet<String> = HashSet::with_capacity(width);
    // The next suffix to try for each name, so a thousand `a` columns don't
    // each count up from 2.
    let mut next: HashMap<String, usize> = HashMap::new();
    for i in 0..width {
        let given = rows[0].get(i).map(|s| s.trim()).unwrap_or_default();
        let base = if given.is_empty() {
            format!("column {}", i + 1)
        } else {
            given.to_owned()
        };
        let mut name = base.clone();
        if taken.contains(&name) {
            let n = next.entry(base.clone()).or_insert(2);
            loop {
                name = format!("{base} {n}");
                *n += 1;
                if !taken.contains(&name) {
                    break;
                }
            }
        }
        taken.insert(name.clone());
        names.push(name);
    }
    names
}

/// One object per row after the header, keyed by the header. Short rows
/// leave their missing cells out. A header with no rows is an error, since
/// an empty array would lose it.
pub fn rows_to_tree(rows: Vec<Vec<String>>) -> Result<Value> {
    if rows.is_empty() {
        return Ok(Value::Array(Vec::new()));
    }
    if rows.len() == 1 {
        return Err(failed("the table has a header but no rows"));
    }
    let keys = header(&rows);
    Ok(Value::Array(
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
    ))
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
        let mut seen: HashSet<&str> = HashSet::new();
        for item in &items {
            if let Value::Object(entries) = item {
                for (k, _) in entries {
                    if seen.insert(k) {
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
            let Value::Object(entries) = item else {
                unreachable!()
            };
            let mut cells: HashMap<String, Value> = entries.into_iter().collect();
            rows.push(
                keys.iter()
                    .map(|k| cells.remove(k).map(Value::into_cell).unwrap_or_default())
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
    let line = |r: &Vec<String>| {
        let cells: Vec<String> = (0..width)
            .map(|i| markdown_cell(r.get(i).map_or("", String::as_str)))
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

/// A cell's text for a pipe table, shown as written: inline Markdown
/// characters are backslash-escaped, `&`, `<` and `>` become entities so no
/// HTML or entity comes alive, and line breaks become `<br>`. Block markers
/// like a leading `#` or `-` need nothing, since a cell holds inline text only.
fn markdown_cell(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '|' | '~' => {
                out.push('\\');
                out.push(c);
            }
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => {
                chars.next_if_eq(&'\n');
                out.push_str("<br>");
            }
            '\n' => out.push_str("<br>"),
            c => out.push(c),
        }
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
        let json = write_tree(&rows_to_tree(rows).unwrap(), "json").unwrap();
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
        let back = write_table(
            &tree_to_rows(rows_to_tree(rows.clone()).unwrap()).unwrap(),
            b',',
        )
        .unwrap();
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
            let text = write_tree(&rows_to_tree(rows.clone()).unwrap(), format).unwrap();
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
    fn a_header_without_rows_is_an_error() {
        for csv in ["id,name\n", "id,name"] {
            let error = rows_to_tree(table(csv)).unwrap_err().to_string();
            assert!(
                error.contains("the table has a header but no rows"),
                "{error}"
            );
        }
    }

    #[test]
    fn ragged_rows_get_named_columns() {
        let rows = table("a,b\n1\n2,3,4\n");
        let tree = rows_to_tree(rows).unwrap();
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
        let json = serde_json::to_string(&rows_to_tree(table("a,a,\n1,2,3\n")).unwrap()).unwrap();
        assert_eq!(json, r#"[{"a":"1","a 2":"2","column 3":"3"}]"#);
    }

    #[test]
    fn empty_input_is_an_empty_table() {
        assert!(table("").is_empty());
        assert_eq!(
            write_tree(&rows_to_tree(Vec::new()).unwrap(), "json").unwrap(),
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
    fn objects_with_many_keys_stay_in_order() {
        // Each key once looked through every key before it: 100,000 keys
        // took half a minute.
        let n = 100_000;
        let json = format!(
            "{{{}}}",
            (0..n)
                .map(|i| format!("\"k{i}\":{i}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let value = read_tree("json", &json).unwrap();
        let yaml = write_tree(&value, "yaml").unwrap();
        let again = read_tree("yaml", &yaml).unwrap();
        assert_eq!(again, value);
        let rows = tree_to_rows(again).unwrap();
        assert_eq!(rows[0].len(), n);
        assert_eq!(
            (rows[0][n - 1].as_str(), rows[1][n - 1].as_str()),
            ("k99999", "99999")
        );
        let csv = format!("{}\n", vec!["a"; n].join(","));
        let names = header(&table(&csv));
        assert_eq!(
            (names[1].as_str(), names[n - 1].as_str()),
            ("a 2", "a 100000")
        );
    }

    #[test]
    fn keys_that_read_the_same_are_an_error_not_a_loss() {
        for (yaml, kinds, key) in [
            (
                "1: numeric\n\"1\": text\n",
                "a number key and a text key",
                "\"1\"",
            ),
            (
                "\"true\": a\ntrue: b\n",
                "a text key and a boolean key",
                "\"true\"",
            ),
            ("~: a\n\"\": b\n", "a null key and a text key", "\"\""),
            (
                "[1, 2]: a\n\"[1,2]\": b\n",
                "a list key and a text key",
                "\"[1,2]\"",
            ),
        ] {
            let error = read_tree("yaml", yaml).unwrap_err().to_string();
            assert!(error.contains("invalid YAML"), "{error}");
            assert!(error.contains(kinds) && error.contains(key), "{error}");
        }
        // The same key twice keeps its first place and its last value.
        for (format, source) in [
            ("yaml", "1: a\nb: x\n1: c\n"),
            ("json", r#"{"1":"a","b":"x","1":"c"}"#),
        ] {
            let value = read_tree(format, source).unwrap();
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                r#"{"1":"c","b":"x"}"#,
                "{format}"
            );
        }
        // Keys that only look alike stay apart.
        let value = read_tree("yaml", "1: a\n1.0: b\n\"01\": c\n").unwrap();
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            r#"{"1":"a","1.0":"b","01":"c"}"#
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
        let yaml = write_tree(
            &rows_to_tree(table("zip,flag\n02134,yes\n")).unwrap(),
            "yaml",
        )
        .unwrap();
        let back = tree_to_rows(read_tree("yaml", &yaml).unwrap()).unwrap();
        assert_eq!(back[1], ["02134", "yes"], "{yaml}");
    }

    #[test]
    fn yaml_quotes_every_string_a_yaml_1_1_reader_would_misread() {
        let words = [
            "on",
            "Off",
            "NO",
            "yes",
            "Y",
            "n",
            "True",
            "null",
            "NULL",
            "~",
            "",
            "1_000",
            "017",
            "0o17",
            "0x1F",
            "1e3",
            ".inf",
            "-.Inf",
            ".NaN",
            "+1",
            "2024-01-01",
            "2024-01-01 10:00:00",
            "12:30",
            "190:20:30",
            "=",
            "<<",
            "-",
            "a: b",
            "#x",
            "x #y",
            "null ",
            " lead",
            "it's",
            "tab\there",
            "bell\u{7}",
            "\u{2028}",
        ];
        let value = Value::Object(
            words
                .iter()
                .map(|w| (w.to_string(), Value::String(w.to_string())))
                .collect(),
        );
        let yaml = write_tree(&value, "yaml").unwrap();
        for w in words {
            assert!(
                !yaml.contains(&format!("\n{w}:")),
                "bare key {w:?}:\n{yaml}"
            );
            assert!(
                !yaml.contains(&format!(": {w}\n")),
                "bare value {w:?}:\n{yaml}"
            );
        }
        assert_eq!(read_tree("yaml", &yaml).unwrap(), value, "{yaml}");
        // Ordinary words stay bare.
        let plain = Value::Array(vec![
            Value::String("Åsa Berg".into()),
            Value::String("v1.2-beta".into()),
            Value::String("note (draft)".into()),
        ]);
        assert_eq!(
            write_tree(&plain, "yaml").unwrap(),
            "- Åsa Berg\n- v1.2-beta\n- note (draft)\n"
        );
    }

    #[test]
    fn yaml_keeps_numbers_numbers_and_breaks_readable() {
        let json = r#"{"nums":[1e5,2.5E-3,-7,0.5],"text":"one\ntwo\n","more":"a\n\n b","deep":[[1,[]],{}]}"#;
        let value = read_tree("json", json).unwrap();
        let yaml = write_tree(&value, "yaml").unwrap();
        assert_eq!(
            yaml,
            "nums:\n- 1.0e+5\n- 2.5E-3\n- -7\n- 0.5\ntext: |\n  one\n  two\nmore: |-\n  a\n\n   b\ndeep:\n- - 1\n  - []\n- {}\n"
        );
        // The same values, though floats come back in their shortest form.
        let parsed = |v: &Value| {
            serde_json::from_str::<serde_json::Value>(&serde_json::to_string(v).unwrap()).unwrap()
        };
        assert_eq!(parsed(&read_tree("yaml", &yaml).unwrap()), parsed(&value));
        assert_eq!(yaml_number("1e5"), "1.0e+5");
        assert_eq!(yaml_number("-2.5E-3"), "-2.5E-3");
    }

    #[test]
    fn json_numbers_keep_every_digit() {
        let json = r#"[{"big":12345678901234567890123,"neg":-98765432109876543210,"f":0.1000000000000000055511151231257827,"e":1E400}]"#;
        let value = read_tree("json", json).unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), json);
        let yaml = write_tree(&value, "yaml").unwrap();
        assert_eq!(
            yaml,
            "- big: 12345678901234567890123\n  neg: -98765432109876543210\n  f: 0.1000000000000000055511151231257827\n  e: 1.0E+400\n"
        );
        let back = read_tree(
            "yaml",
            "- big: 12345678901234567890123\n  neg: -98765432109876543210\n",
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&back).unwrap(),
            r#"[{"big":12345678901234567890123,"neg":-98765432109876543210}]"#
        );
        let rows = tree_to_rows(value).unwrap();
        assert_eq!(rows[1][0], "12345678901234567890123");
        let nested = format!("{}1{}", "[".repeat(200), "]".repeat(200));
        assert!(read_tree("json", &nested).is_err());
        let deep = format!("{}1{}", "[".repeat(100), "]".repeat(100));
        assert_eq!(
            serde_json::to_string(&read_tree("json", &deep).unwrap()).unwrap(),
            deep
        );
    }

    #[test]
    fn markdown_tables_escape_pipes() {
        let md = table_markdown(&table("a|b,c\n\"x\ny\",\\\n"));
        assert_eq!(md, "| a\\|b | c |\n| --- | --- |\n| x<br>y | \\\\ |\n");
    }

    #[test]
    fn markdown_tables_show_markup_as_written() {
        let rows = table(
            "h\n<b>literal</b>\n**value**\n&amp;\n[x](y) `c` _u_ ~~s~~\n# not a heading\n\"a\r\nb\rc\"\n",
        );
        let md = table_markdown(&rows);
        assert!(md.contains("| &lt;b&gt;literal&lt;/b&gt; |"), "{md}");
        assert!(md.contains("| \\*\\*value\\*\\* |"), "{md}");
        assert!(md.contains("| a<br>b<br>c |"), "{md}");
        // Rendered, every cell reads exactly as the CSV wrote it.
        let html = crate::markdown::to_html(&md, "t");
        for cell in [
            "<td>&lt;b&gt;literal&lt;/b&gt;</td>",
            "<td>**value**</td>",
            "<td>&amp;amp;</td>",
            "<td>[x](y) `c` _u_ ~~s~~</td>",
            "<td># not a heading</td>",
            "<td>a<br>b<br>c</td>",
        ] {
            assert!(html.contains(cell), "{cell}\n{html}");
        }
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
