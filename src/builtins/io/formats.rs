//! Format-specific Import/Export converters.
//!
//! This module provides:
//! - The `Format` enum for identifying file formats
//! - Format detection (from file extension or explicit name)
//! - Import converters: text → Value
//! - Export converters: Value → text/binary
//!
//! # Supported Formats
//!
//! | Format  | Extensions           | Import | Export |
//! |---------|----------------------|--------|--------|
//! | Text    | .txt                 | ✓      | ✓      |
//! | WL      | .m, .wl              | ✓      | ✓      |
//! | JSON    | .json                | ✓      | ✓      |
//! | NB      | .nb                  | ✓      | ✗      |
//! | CSV     | .csv                 | ✓      | ✓      |
//! | TSV     | .tsv                 | ✓      | ✓      |
//! | Table   | .dat, .txt, .data    | ✓      | ✓      |
//! | HTML    | .html, .htm          | ✓      | ✗      |
//! | PNG     | .png                 | ✓      | ✓      |
//! | SVG     | .svg                 | ✓      | ✓      |

use std::sync::Arc;

use crate::value::{EvalError, Value};

// ── Format Enum ──────────────────────────────────────────────────────────────

/// Known import/export formats.
#[derive(Debug, Clone, PartialEq)]
pub enum Format {
    Text,
    WL,
    JSON,
    RawJson,
    NB,
    CSV,
    TSV,
    Table,
    List,
    Lines,
    Words,
    HTML,
    XML,
    PNG,
    SVG,
    Binary,
}

impl Format {
    /// Returns `true` for formats that are stored as binary (not UTF-8 text).
    pub fn is_binary(&self) -> bool {
        matches!(self, Format::PNG | Format::Binary)
    }
}

/// Options controlling text import (applied on a line basis, before parsing).
#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    /// Number of leading lines to discard before parsing.
    pub skip_lines: usize,
    /// Number of leading lines treated as a header and discarded.
    pub header_lines: usize,
}

impl ImportOptions {
    /// Drop `SkipLines` then `HeaderLines` from the front of `text`.
    fn apply(&self, text: &str) -> String {
        let drop = self.skip_lines + self.header_lines;
        if drop == 0 {
            return text.to_string();
        }
        text.split_inclusive('\n').skip(drop).collect()
    }
}

// ── Export Output ────────────────────────────────────────────────────────────

/// The result of an export conversion.
pub enum ExportOutput {
    Text(String),
    Binary(Vec<u8>),
}

// ── Format Detection ─────────────────────────────────────────────────────────

/// Normalise a format name: uppercase, strip dashes and underscores.
fn normalise(name: &str) -> String {
    name.to_uppercase().replace(['-', '_'], "")
}

/// Parse a format name string into a `Format` (case-insensitive).
fn format_from_name(name: &str) -> Option<Format> {
    match normalise(name).as_str() {
        "TEXT" | "STRING" => Some(Format::Text),
        "WL" | "M" | "WOLFRAM" => Some(Format::WL),
        "JSON" => Some(Format::JSON),
        "RAWJSON" => Some(Format::RawJson),
        "NB" | "NOTEBOOK" => Some(Format::NB),
        "CSV" => Some(Format::CSV),
        "TSV" => Some(Format::TSV),
        "TABLE" | "DAT" => Some(Format::Table),
        "LIST" => Some(Format::List),
        "LINES" => Some(Format::Lines),
        "WORDS" => Some(Format::Words),
        "HTML" | "HTM" => Some(Format::HTML),
        "XML" => Some(Format::XML),
        "PNG" => Some(Format::PNG),
        "SVG" => Some(Format::SVG),
        "BINARY" | "BYTE" | "BYTES" => Some(Format::Binary),
        _ => None,
    }
}

/// Infer a `Format` from a file path extension (lowercased).
fn format_from_extension(path: &str) -> Option<Format> {
    let ext = std::path::Path::new(path)
        .extension()?
        .to_str()?
        .to_lowercase();
    match ext.as_str() {
        "json" => Some(Format::JSON),
        "nb" => Some(Format::NB),
        "m" | "wl" => Some(Format::WL),
        "csv" => Some(Format::CSV),
        "tsv" => Some(Format::TSV),
        "dat" | "data" | "table" => Some(Format::Table),
        "list" => Some(Format::List),
        "lines" => Some(Format::Lines),
        "words" => Some(Format::Words),
        "html" | "htm" => Some(Format::HTML),
        "xml" => Some(Format::XML),
        "png" => Some(Format::PNG),
        "svg" => Some(Format::SVG),
        "bin" | "binary" => Some(Format::Binary),
        "txt" | "text" => Some(Format::Text),
        _ => None,
    }
}

/// Detect the format: if an explicit name is given, parse it; otherwise infer
/// from the file path extension.
pub fn detect_format(path: &str, format_name: Option<&str>) -> Result<Format, EvalError> {
    if let Some(name) = format_name {
        format_from_name(name).ok_or_else(|| EvalError::Error(format!("Unknown format '{}'", name)))
    } else {
        format_from_extension(path).ok_or_else(|| {
            EvalError::Error(format!(
                "Cannot determine format from path '{}'; provide an explicit format argument",
                path
            ))
        })
    }
}

// ── CSV / TSV helpers ────────────────────────────────────────────────────────

/// Parse delimited text (CSV/TSV) into records of fields.
///
/// Works on whole documents so that quoted fields may contain the delimiter
/// **and** newlines. Handles `""` as an escaped quote, normalises CRLF line
/// endings (also inside quoted fields), and skips blank lines. Whitespace
/// inside fields is preserved so that values survive a round-trip.
fn parse_delimited(text: &str, delimiter: char) -> Vec<Vec<String>> {
    // Iterate over chars (not bytes) so multi-byte UTF-8 content is preserved.
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut row_started = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        field.push('"');
                        chars.next();
                    } else {
                        in_quotes = false;
                    }
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    field.push('\n');
                }
                _ => field.push(c),
            }
            continue;
        }

        match c {
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                if row_started {
                    records.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
                row_started = false;
            }
            '"' => {
                in_quotes = true;
                row_started = true;
            }
            c if c == delimiter => {
                row.push(std::mem::take(&mut field));
                row_started = true;
            }
            _ => {
                field.push(c);
                row_started = true;
            }
        }
    }

    if in_quotes || row_started || !field.is_empty() {
        row.push(field);
        records.push(row);
    }
    records
}

/// Parse a single delimited line into its fields (no newline handling).
#[allow(dead_code)]
fn parse_delimited_line(line: &str, delimiter: u8) -> Vec<String> {
    parse_delimited(line, delimiter as char)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Convert records into a `Value::List` of `Value::List` rows.
fn records_to_value(records: Vec<Vec<String>>) -> Value {
    Value::List(
        records
            .into_iter()
            .map(|row| Value::List(row.into_iter().map(Value::Str).collect()))
            .collect(),
    )
}

/// Quote a field if it contains the delimiter, a quote, or a line break.
fn quote_field(s: &str, delimiter: char) -> String {
    if s.contains(delimiter) || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Export rows separated by `delimiter`, quoting fields as needed.
fn export_delimited(value: &Value, delimiter: char) -> Result<String, EvalError> {
    let rows = value_to_rows(value)?;
    let mut out = String::new();
    let sep = delimiter.to_string();
    for row in &rows {
        let cols: Vec<String> = row
            .iter()
            .map(|v| quote_field(&value_to_csv_field(v), delimiter))
            .collect();
        out.push_str(&cols.join(&sep));
        out.push('\n');
    }
    Ok(out)
}

// ── CSV ──────────────────────────────────────────────────────────────────────

fn import_csv(text: &str) -> Result<Value, EvalError> {
    Ok(records_to_value(parse_delimited(text, ',')))
}

fn export_csv(value: &Value) -> Result<String, EvalError> {
    export_delimited(value, ',')
}

// ── TSV ──────────────────────────────────────────────────────────────────────

fn import_tsv(text: &str) -> Result<Value, EvalError> {
    Ok(records_to_value(parse_delimited(text, '\t')))
}

fn export_tsv(value: &Value) -> Result<String, EvalError> {
    export_delimited(value, '\t')
}

// ── Table (whitespace-separated) ─────────────────────────────────────────────

fn import_table(text: &str) -> Result<Value, EvalError> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let fields: Vec<Value> = trimmed
            .split_whitespace()
            .map(|s| Value::Str(s.to_string()))
            .collect();
        rows.push(Value::List(fields));
    }
    Ok(Value::List(rows))
}

fn export_table(value: &Value) -> Result<String, EvalError> {
    let rows = value_to_rows(value)?;
    let mut out = String::new();
    for row in &rows {
        let cols: Vec<String> = row.iter().map(value_to_csv_field).collect();
        out.push_str(&cols.join(" "));
        out.push('\n');
    }
    Ok(out)
}

/// Parse a whitespace token into a number when possible, else keep it a string.
fn parse_scalar(s: &str) -> Value {
    if let Ok(i) = s.parse::<rug::Integer>() {
        return Value::Integer(i);
    }
    if let Ok(f) = s.parse::<f64>() {
        return Value::Real(rug::Float::with_val(crate::value::DEFAULT_PRECISION, f));
    }
    Value::Str(s.to_string())
}

/// `"List"` import: like `Table` but numeric tokens become numbers.
fn import_list(text: &str) -> Result<Value, EvalError> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        rows.push(Value::List(
            trimmed.split_whitespace().map(parse_scalar).collect(),
        ));
    }
    Ok(Value::List(rows))
}

// ── Lines / Words ────────────────────────────────────────────────────────────

fn import_lines(text: &str) -> Result<Value, EvalError> {
    Ok(Value::List(
        text.lines()
            .map(|s| Value::Str(s.trim_end_matches('\r').to_string()))
            .collect(),
    ))
}

fn export_lines(value: &Value) -> Result<String, EvalError> {
    let mut out = match value {
        Value::Str(s) => s.clone(),
        Value::List(items) => items
            .iter()
            .map(value_to_csv_field)
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    };
    out.push('\n');
    Ok(out)
}

fn import_words(text: &str) -> Result<Value, EvalError> {
    Ok(Value::List(
        text.split_whitespace()
            .map(|s| Value::Str(s.to_string()))
            .collect(),
    ))
}

fn export_words(value: &Value) -> Result<String, EvalError> {
    let mut out = match value {
        Value::Str(s) => s.clone(),
        Value::List(items) => items
            .iter()
            .map(value_to_csv_field)
            .collect::<Vec<_>>()
            .join(" "),
        other => other.to_string(),
    };
    out.push('\n');
    Ok(out)
}

// ── Binary ───────────────────────────────────────────────────────────────────

fn import_binary(data: &[u8]) -> Value {
    Value::List(
        data.iter()
            .map(|b| Value::Integer(rug::Integer::from(*b)))
            .collect(),
    )
}

pub fn export_binary(value: &Value) -> Result<Vec<u8>, EvalError> {
    match value {
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for b in items {
                match b {
                    Value::Integer(n) => out.push(n.to_u8().unwrap_or(0)),
                    Value::Str(s) => out.extend_from_slice(s.as_bytes()),
                    other => {
                        return Err(EvalError::Error(format!(
                            "Cannot export {} as Binary: expected integer byte values",
                            other.type_name()
                        )));
                    }
                }
            }
            Ok(out)
        }
        Value::Str(s) => Ok(s.as_bytes().to_vec()),
        Value::Integer(n) => Ok(vec![n.to_u8().unwrap_or(0)]),
        other => Err(EvalError::Error(format!(
            "Cannot export {} as Binary",
            other.type_name()
        ))),
    }
}

// ── XML (basic) ──────────────────────────────────────────────────────────────

/// Minimal XML parser producing `XMLElement["tag", {attrs}, {children}]` trees.
struct XmlParser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> XmlParser<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            src: s.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn starts_with(&self, pat: &str) -> bool {
        self.src[self.pos..].starts_with(pat.as_bytes())
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.pos += 1;
        }
    }

    /// Skip whitespace, comments, processing instructions and DOCTYPE.
    fn skip_misc(&mut self) {
        loop {
            self.skip_ws();
            if self.starts_with("<!--") {
                self.pos += 4;
                while self.pos < self.src.len() && !self.starts_with("-->") {
                    self.pos += 1;
                }
                self.pos = (self.pos + 3).min(self.src.len());
            } else if self.starts_with("<?") {
                self.pos += 2;
                while self.pos < self.src.len() && !self.starts_with("?>") {
                    self.pos += 1;
                }
                self.pos = (self.pos + 2).min(self.src.len());
            } else if self.starts_with("<!") {
                while self.pos < self.src.len() && self.peek() != Some(b'>') {
                    self.pos += 1;
                }
                self.pos = (self.pos + 1).min(self.src.len());
            } else {
                break;
            }
        }
    }

    fn read_name(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b':' | b'.') {
                self.pos += 1;
            } else {
                break;
            }
        }
        String::from_utf8_lossy(&self.src[start..self.pos]).into_owned()
    }

    fn parse_attrs(&mut self) -> Vec<Value> {
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'/') | Some(b'>') | None => break,
                _ => {
                    let name = self.read_name();
                    if name.is_empty() {
                        self.pos += 1;
                        continue;
                    }
                    self.skip_ws();
                    let value = if self.peek() == Some(b'=') {
                        self.pos += 1;
                        self.skip_ws();
                        self.read_quoted()
                    } else {
                        String::new()
                    };
                    attrs.push(Value::Rule {
                        lhs: Box::new(Value::Str(name)),
                        rhs: Box::new(Value::Str(value)),
                        delayed: false,
                    });
                }
            }
        }
        attrs
    }

    fn read_quoted(&mut self) -> String {
        let quote = match self.peek() {
            Some(q @ (b'"' | b'\'')) => {
                self.pos += 1;
                q
            }
            _ => return String::new(),
        };
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c == quote {
                self.pos += 1;
                break;
            }
            if c == b'&' {
                out.push_str(&self.read_entity());
            } else {
                let start = self.pos;
                self.pos += 1;
                out.push_str(&String::from_utf8_lossy(&self.src[start..self.pos]));
            }
        }
        out
    }

    fn read_entity(&mut self) -> String {
        let start = self.pos;
        self.pos += 1; // '&'
        while self.pos < self.src.len() && self.peek() != Some(b';') && self.pos - start < 12 {
            self.pos += 1;
        }
        if self.peek() == Some(b';') {
            self.pos += 1;
        }
        let entity =
            String::from_utf8_lossy(&self.src[start + 1..self.pos.saturating_sub(1)]).into_owned();
        decode_entity(&entity)
    }

    fn parse_element(&mut self) -> Result<Value, EvalError> {
        // Expect '<'
        if self.peek() != Some(b'<') {
            return Err(EvalError::Error("XML: expected element".to_string()));
        }
        self.pos += 1;
        let tag = self.read_name();
        let attrs = self.parse_attrs();
        self.skip_ws();
        let mut children: Vec<Value> = Vec::new();
        if self.peek() == Some(b'/') {
            // self-closing
            self.pos += 1;
            if self.peek() == Some(b'>') {
                self.pos += 1;
            }
        } else {
            if self.peek() == Some(b'>') {
                self.pos += 1;
            }
            // Parse content until the matching close tag.
            loop {
                if self.pos >= self.src.len() {
                    break;
                }
                if self.starts_with("</") {
                    self.pos += 2;
                    let _close = self.read_name();
                    self.skip_ws();
                    if self.peek() == Some(b'>') {
                        self.pos += 1;
                    }
                    break;
                }
                if self.starts_with("<!--") || self.starts_with("<?") || self.starts_with("<!") {
                    self.skip_misc();
                    continue;
                }
                if self.peek() == Some(b'<') {
                    children.push(self.parse_element()?);
                } else {
                    children.push(self.read_text());
                }
            }
        }
        Ok(Value::Call {
            head: "XMLElement".to_string(),
            args: vec![Value::Str(tag), Value::List(attrs), Value::List(children)],
        })
    }

    fn read_text(&mut self) -> Value {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c == b'<' {
                break;
            }
            if c == b'&' {
                out.push_str(&self.read_entity());
            } else {
                let start = self.pos;
                self.pos += 1;
                out.push_str(&String::from_utf8_lossy(&self.src[start..self.pos]));
            }
        }
        Value::Str(out.trim().to_string())
    }
}

fn import_xml(text: &str) -> Result<Value, EvalError> {
    let mut parser = XmlParser::new(text);
    parser.skip_misc();
    if parser.peek() != Some(b'<') {
        return Ok(Value::Str(text.to_string()));
    }
    parser.parse_element()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn serialize_xml(value: &Value, out: &mut String) {
    match value {
        Value::Call { head, args } if head == "XMLElement" && !args.is_empty() => {
            let tag = match &args[0] {
                Value::Str(s) => s.clone(),
                other => other.to_string(),
            };
            out.push('<');
            out.push_str(&tag);
            if let Some(Value::List(attrs)) = args.get(1) {
                for attr in attrs {
                    if let Value::Rule { lhs, rhs, .. } = attr {
                        out.push(' ');
                        out.push_str(&xml_escape(lhs.to_string().trim_matches('"')));
                        out.push_str("=\"");
                        out.push_str(&xml_escape(rhs.to_string().trim_matches('"')));
                        out.push('"');
                    }
                }
            }
            let children = args.get(2);
            let empty = matches!(children, Some(Value::List(c)) if c.is_empty());
            if empty {
                out.push_str("/>");
            } else {
                out.push('>');
                if let Some(Value::List(children)) = children {
                    for child in children {
                        serialize_xml(child, out);
                    }
                }
                out.push_str("</");
                out.push_str(&tag);
                out.push('>');
            }
        }
        Value::List(items) => {
            for item in items {
                serialize_xml(item, out);
            }
        }
        Value::Str(s) => out.push_str(&xml_escape(s)),
        other => out.push_str(&xml_escape(&other.to_string())),
    }
}

fn export_xml(value: &Value) -> Result<String, EvalError> {
    match value {
        Value::Str(s) => Ok(s.clone()),
        Value::Call { head, .. } if head == "XMLElement" => {
            let mut out = String::new();
            serialize_xml(value, &mut out);
            Ok(out)
        }
        other => Ok(other.to_string()),
    }
}

// ── HTML ─────────────────────────────────────────────────────────────────────

fn decode_entity(entity: &str) -> String {
    match entity {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        "nbsp" => " ".to_string(),
        _ => {
            // Numeric character references: &#233; or &#xE9;
            if let Some(rest) = entity.strip_prefix('#') {
                let code =
                    if let Some(hex) = rest.strip_prefix('x').or_else(|| rest.strip_prefix('X')) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        rest.parse::<u32>().ok()
                    };
                if let Some(c) = code.and_then(char::from_u32) {
                    return c.to_string();
                }
            }
            // Unknown named entity — keep it verbatim rather than dropping it.
            format!("&{};", entity)
        }
    }
}

fn strip_html_tags(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let mut in_entity = false;
    let mut entity = String::new();

    for c in text.chars() {
        if in_tag {
            if c == '>' {
                in_tag = false;
            }
            continue;
        }
        if in_entity {
            if c == ';' {
                in_entity = false;
                out.push_str(&decode_entity(&entity));
                entity.clear();
            } else if c.is_ascii_alphanumeric() || c == '#' {
                entity.push(c);
            } else {
                // Not a well-formed entity — flush it literally and reprocess.
                in_entity = false;
                out.push('&');
                out.push_str(&entity);
                entity.clear();
                if c == '<' {
                    in_tag = true;
                } else {
                    out.push(c);
                }
            }
            continue;
        }
        match c {
            '<' => in_tag = true,
            '&' => {
                in_entity = true;
                entity.clear();
            }
            _ => out.push(c),
        }
    }
    // Flush a dangling entity (e.g. text ending in "&")
    if in_entity {
        out.push('&');
        out.push_str(&entity);
    }

    // Normalise whitespace runs to single spaces
    let mut result = String::new();
    let mut prev_space = false;
    for c in out.chars() {
        if c.is_whitespace() {
            if !prev_space {
                result.push(' ');
                prev_space = true;
            }
        } else {
            result.push(c);
            prev_space = false;
        }
    }
    result.trim().to_string()
}

fn import_html(text: &str) -> Result<Value, EvalError> {
    Ok(Value::Str(strip_html_tags(text)))
}

// ── PNG ──────────────────────────────────────────────────────────────────────

fn import_png(data: &[u8]) -> Result<Value, EvalError> {
    let img = image::load_from_memory(data)
        .map_err(|e| EvalError::Error(format!("PNG import failed: {}", e)))?;
    Ok(Value::Image(Arc::new(img)))
}

fn export_png(value: &Value) -> Result<Vec<u8>, EvalError> {
    match value {
        Value::Image(img) => {
            let mut buf = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
                .map_err(|e| EvalError::Error(format!("PNG export failed: {}", e)))?;
            Ok(buf)
        }
        other => Err(EvalError::Error(format!(
            "Cannot export {} as PNG: expected an Image value",
            other.type_name()
        ))),
    }
}

// ── SVG ──────────────────────────────────────────────────────────────────────

fn export_svg(value: &Value) -> Result<String, EvalError> {
    match value {
        // Graphics Call object — render via graphics module
        Value::Call { head, args } if head == "Graphics" && !args.is_empty() => {
            let primitives = &args[0];
            let options = if args.len() >= 2 {
                args[1].clone()
            } else {
                Value::Assoc(std::collections::HashMap::new())
            };
            crate::builtins::graphics::render_svg(primitives, &options)
        }
        // Already an SVG string — pass through
        Value::Str(s) => Ok(s.clone()),
        // Fallback: use Display
        other => Ok(other.to_string()),
    }
}

// ── Public Import API ────────────────────────────────────────────────────────

/// Import from a string of data using the given format.
/// Returns an error for binary formats (PNG).
pub fn format_import_text(format: &Format, text: &str) -> Result<Value, EvalError> {
    format_import_text_opts(format, text, &ImportOptions::default())
}

/// Import from a string of data using the given format and line options.
pub fn format_import_text_opts(
    format: &Format,
    text: &str,
    opts: &ImportOptions,
) -> Result<Value, EvalError> {
    // `Binary` and `XML` are handled on the raw bytes/text, everything else
    // gets the line-based `SkipLines`/`HeaderLines` treatment first.
    match format {
        Format::Binary => return Ok(import_binary(text.as_bytes())),
        Format::XML => return import_xml(text),
        _ => {}
    }
    let text = opts.apply(text);
    let text = text.as_str();
    match format {
        Format::Text | Format::WL => Ok(Value::Str(text.to_string())),
        Format::JSON | Format::RawJson => crate::ffi::marshal::json_to_value(text)
            .map_err(|e| EvalError::Error(format!("Import JSON error: {}", e))),
        Format::NB => super::nb::notebook_to_code(text)
            .map(Value::Str)
            .map_err(|e| EvalError::Error(format!("Import notebook error: {}", e))),
        Format::CSV => import_csv(text),
        Format::TSV => import_tsv(text),
        Format::Table => import_table(text),
        Format::List => import_list(text),
        Format::Lines => import_lines(text),
        Format::Words => import_words(text),
        Format::HTML => import_html(text),
        Format::SVG => Ok(Value::Str(text.to_string())),
        Format::Binary | Format::XML => unreachable!(),
        Format::PNG => Err(EvalError::Error(
            "Cannot import PNG from a string source; use Import[path] instead".to_string(),
        )),
    }
}

/// Import from binary data using the given format.
/// For non-binary formats, decodes as UTF-8 and delegates to `format_import_text`.
pub fn format_import_binary(format: &Format, data: &[u8]) -> Result<Value, EvalError> {
    format_import_binary_opts(format, data, &ImportOptions::default())
}

/// Import from binary data with line options applied to text formats.
pub fn format_import_binary_opts(
    format: &Format,
    data: &[u8],
    opts: &ImportOptions,
) -> Result<Value, EvalError> {
    match format {
        Format::PNG => import_png(data),
        Format::Binary => Ok(import_binary(data)),
        _ => {
            let text = std::str::from_utf8(data)
                .map_err(|e| EvalError::Error(format!("File is not valid UTF-8: {}", e)))?;
            format_import_text_opts(format, text, opts)
        }
    }
}

// ── Public Export API ────────────────────────────────────────────────────────

/// Export a value to the given format.
pub fn format_export(format: &Format, value: &Value) -> Result<ExportOutput, EvalError> {
    match format {
        Format::JSON | Format::RawJson => {
            let jv = crate::ffi::marshal::value_to_json(value)
                .map_err(|e| EvalError::Error(format!("Export JSON error: {}", e)))?;
            let s = if *format == Format::RawJson {
                serde_json::to_string(&jv)
            } else {
                serde_json::to_string_pretty(&jv)
            }
            .map_err(|e| EvalError::Error(format!("Export JSON serialization: {}", e)))?;
            Ok(ExportOutput::Text(s))
        }
        Format::CSV => export_csv(value).map(ExportOutput::Text),
        Format::TSV => export_tsv(value).map(ExportOutput::Text),
        Format::Table | Format::List => export_table(value).map(ExportOutput::Text),
        Format::Lines => export_lines(value).map(ExportOutput::Text),
        Format::Words => export_words(value).map(ExportOutput::Text),
        Format::XML => export_xml(value).map(ExportOutput::Text),
        Format::SVG => export_svg(value).map(ExportOutput::Text),
        Format::PNG => export_png(value).map(ExportOutput::Binary),
        Format::Binary => export_binary(value).map(ExportOutput::Binary),
        Format::Text | Format::WL => {
            let s = match value {
                Value::Str(s) => s.clone(),
                other => other.to_string(),
            };
            Ok(ExportOutput::Text(s))
        }
        Format::NB => Err(EvalError::Error(
            "Export to Notebook format is not supported".to_string(),
        )),
        Format::HTML => Err(EvalError::Error(
            "Export to HTML format is not supported".to_string(),
        )),
    }
}

// ── Shared helpers ───────────────────────────────────────────────────────────

/// Extract a `Vec<Vec<Value>>` from a Value, expecting a list of lists.
fn value_to_rows(value: &Value) -> Result<Vec<Vec<Value>>, EvalError> {
    match value {
        Value::List(rows) => {
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                match row {
                    Value::List(cols) => out.push(cols.clone()),
                    other => out.push(vec![other.clone()]),
                }
            }
            Ok(out)
        }
        other => Ok(vec![vec![other.clone()]]),
    }
}

/// Convert a Value to its string representation for CSV/TSV output.
fn value_to_csv_field(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_delimited_line_preserves_utf8() {
        assert_eq!(
            parse_delimited_line("café,résumé", b','),
            vec!["café", "résumé"]
        );
        // Quoted field with multi-byte content and an embedded delimiter
        assert_eq!(
            parse_delimited_line("\"naïve, 日本語\",x", b','),
            vec!["naïve, 日本語", "x"]
        );
        // Escaped quotes still work
        assert_eq!(
            parse_delimited_line("\"a\"\"b\",c", b','),
            vec!["a\"b", "c"]
        );
    }

    #[test]
    fn test_import_csv_utf8() {
        let v = import_csv("café,résumé\nα,β").unwrap();
        assert_eq!(
            v,
            Value::List(vec![
                Value::List(vec![Value::Str("café".into()), Value::Str("résumé".into()),]),
                Value::List(vec![Value::Str("α".into()), Value::Str("β".into())]),
            ])
        );
    }

    #[test]
    fn test_parse_delimited_crlf_and_blank_lines() {
        // CRLF endings and blank lines are normalised away.
        let rows = parse_delimited("a,b\r\nc,d\r\n\r\ne,f\r\n", ',');
        assert_eq!(
            rows,
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string(), "d".to_string()],
                vec!["e".to_string(), "f".to_string()],
            ]
        );
    }

    #[test]
    fn test_parse_delimited_multiline_quoted_crlf() {
        // A quoted field may span lines; CRLF inside it becomes LF.
        let rows = parse_delimited("a,\"b\r\nc\",d", ',');
        assert_eq!(
            rows,
            vec![vec!["a".to_string(), "b\nc".to_string(), "d".to_string()]]
        );
    }

    #[test]
    fn test_parse_delimited_preserves_spaces_and_empty_fields() {
        let rows = parse_delimited(" a , ,c", ',');
        assert_eq!(
            rows,
            vec![vec![" a ".to_string(), " ".to_string(), "c".to_string()]]
        );
    }

    #[test]
    fn test_tsv_export_quotes_embedded_tab_and_newline() {
        let data = Value::List(vec![Value::List(vec![
            Value::Str("a\tb".into()),
            Value::Str("c\nd".into()),
            Value::Str("e\"f".into()),
        ])]);
        let out = export_tsv(&data).unwrap();
        // Every special field must be quoted so it round-trips.
        assert!(out.contains("\"a\tb\""), "out = {:?}", out);
        assert!(out.contains("\"c\nd\""), "out = {:?}", out);
        assert!(out.contains("\"e\"\"f\""), "out = {:?}", out);
        assert_eq!(import_tsv(&out).unwrap(), data);
    }

    #[test]
    fn test_csv_roundtrip_special_fields() {
        let data = Value::List(vec![
            Value::List(vec![
                Value::Str(" leading".into()),
                Value::Str("trailing ".into()),
                Value::Str("a,b".into()),
                Value::Str("q\"q".into()),
                Value::Str("nl\nnl".into()),
                Value::Str(String::new()),
            ]),
            Value::List(vec![
                Value::Str("café".into()),
                Value::Str("日本語".into()),
                Value::Str("x".into()),
                Value::Str("y".into()),
                Value::Str("z".into()),
                Value::Str("w".into()),
            ]),
        ]);
        let out = export_csv(&data).unwrap();
        assert_eq!(import_csv(&out).unwrap(), data);
    }

    #[test]
    fn test_csv_export_remains_simple_for_plain_fields() {
        let data = Value::List(vec![Value::List(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
        ])]);
        assert_eq!(export_csv(&data).unwrap(), "a,b\n");
    }

    #[test]
    fn test_html_bare_ampersand_preserved() {
        // A bare `&` used to swallow the remainder of the text.
        assert_eq!(
            import_html("<p>a & b</p>").unwrap(),
            Value::Str("a & b".into())
        );
    }

    #[test]
    fn test_html_unknown_and_numeric_entities() {
        assert_eq!(
            import_html("&copy; &#233; &#xE9;").unwrap(),
            Value::Str("&copy; é é".into())
        );
    }

    #[test]
    fn test_format_from_extension_uses_path_extension() {
        assert_eq!(format_from_extension("dir.d/foo.csv"), Some(Format::CSV));
        assert_eq!(format_from_extension("noext"), None);
        assert_eq!(format_from_extension("archive.JSON"), Some(Format::JSON));
    }
}
