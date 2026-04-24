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
    NB,
    CSV,
    TSV,
    Table,
    HTML,
    PNG,
    SVG,
}

impl Format {
    /// Returns `true` for formats that are stored as binary (not UTF-8 text).
    pub fn is_binary(&self) -> bool {
        matches!(self, Format::PNG)
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
        "TEXT" => Some(Format::Text),
        "WL" | "M" | "WOLFRAM" => Some(Format::WL),
        "JSON" => Some(Format::JSON),
        "NB" | "NOTEBOOK" => Some(Format::NB),
        "CSV" => Some(Format::CSV),
        "TSV" => Some(Format::TSV),
        "TABLE" | "DAT" => Some(Format::Table),
        "HTML" | "HTM" => Some(Format::HTML),
        "PNG" => Some(Format::PNG),
        "SVG" => Some(Format::SVG),
        _ => None,
    }
}

/// Infer a `Format` from a file path extension (lowercased).
fn format_from_extension(path: &str) -> Option<Format> {
    let ext = path.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "json" => Some(Format::JSON),
        "nb" => Some(Format::NB),
        "m" | "wl" => Some(Format::WL),
        "csv" => Some(Format::CSV),
        "tsv" => Some(Format::TSV),
        "dat" | "data" | "table" => Some(Format::Table),
        "html" | "htm" => Some(Format::HTML),
        "png" => Some(Format::PNG),
        "svg" => Some(Format::SVG),
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

/// Parse a single delimited line (CSV with `,` or TSV with `\t`), handling
/// double-quoted fields where commas/tabs are allowed.  A pair of `""` inside
/// a quoted field is a literal `"`.
fn parse_delimited_line(line: &str, delimiter: u8) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if in_quotes {
            if b == b'"' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                    current.push('"');
                    i += 2;
                    continue;
                }
                in_quotes = false;
            } else {
                current.push(b as char);
            }
        } else if b == b'"' {
            in_quotes = true;
        } else if b == delimiter {
            fields.push(current);
            current = String::new();
        } else {
            current.push(b as char);
        }
        i += 1;
    }
    fields.push(current);
    fields
}

/// Check if a string has an unclosed double-quote (odd number of `"` chars).
fn has_unclosed_quote(s: &str) -> bool {
    let mut count = 0u32;
    for &b in s.as_bytes() {
        if b == b'"' {
            count += 1;
        }
    }
    count % 2 == 1
}

// ── CSV ──────────────────────────────────────────────────────────────────────

fn import_csv(text: &str) -> Result<Value, EvalError> {
    let mut rows = Vec::new();
    let mut carry = String::new();

    for raw_line in text.split('\n') {
        if carry.is_empty() {
            carry = raw_line.to_string();
        } else {
            carry.push('\n');
            carry.push_str(raw_line);
        }

        if !has_unclosed_quote(&carry) {
            let trimmed = carry.trim();
            if !trimmed.is_empty() {
                let fields = parse_delimited_line(trimmed, b',');
                let row: Vec<Value> = fields.into_iter().map(Value::Str).collect();
                rows.push(Value::List(row));
            }
            carry.clear();
        }
    }
    // Trailing unclosed quote — include remaining carry as a row
    if !carry.is_empty() {
        let trimmed = carry.trim();
        if !trimmed.is_empty() {
            let fields = parse_delimited_line(trimmed, b',');
            let row: Vec<Value> = fields.into_iter().map(Value::Str).collect();
            rows.push(Value::List(row));
        }
    }
    Ok(Value::List(rows))
}

fn export_csv(value: &Value) -> Result<String, EvalError> {
    let rows = value_to_rows(value)?;
    let mut out = String::new();
    for row in &rows {
        let cols: Vec<String> = row
            .iter()
            .map(|v| {
                let s = value_to_csv_field(v);
                if s.contains(',') || s.contains('"') || s.contains('\n') {
                    format!("\"{}\"", s.replace('"', "\"\""))
                } else {
                    s
                }
            })
            .collect();
        out.push_str(&cols.join(","));
        out.push('\n');
    }
    Ok(out)
}

// ── TSV ──────────────────────────────────────────────────────────────────────

fn import_tsv(text: &str) -> Result<Value, EvalError> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let fields = parse_delimited_line(trimmed, b'\t');
        let row: Vec<Value> = fields.into_iter().map(Value::Str).collect();
        rows.push(Value::List(row));
    }
    Ok(Value::List(rows))
}

fn export_tsv(value: &Value) -> Result<String, EvalError> {
    let rows = value_to_rows(value)?;
    let mut out = String::new();
    for row in &rows {
        let cols: Vec<String> = row.iter().map(value_to_csv_field).collect();
        out.push_str(&cols.join("\t"));
        out.push('\n');
    }
    Ok(out)
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

// ── HTML ─────────────────────────────────────────────────────────────────────

fn strip_html_tags(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let mut in_entity = false;
    let mut entity = String::new();

    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            '&' if !in_tag => {
                in_entity = true;
                entity.clear();
            }
            ';' if in_entity => {
                in_entity = false;
                match entity.as_str() {
                    "amp" => out.push('&'),
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    "quot" => out.push('"'),
                    "nbsp" => out.push(' '),
                    _ => {}
                }
            }
            _ if in_entity => entity.push(c),
            _ if !in_tag && !in_entity => out.push(c),
            _ => {}
        }
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
    match format {
        Format::Text | Format::WL => Ok(Value::Str(text.to_string())),
        Format::JSON => crate::ffi::marshal::json_to_value(text)
            .map_err(|e| EvalError::Error(format!("Import JSON error: {}", e))),
        Format::NB => super::nb::notebook_to_code(text)
            .map(Value::Str)
            .map_err(|e| EvalError::Error(format!("Import notebook error: {}", e))),
        Format::CSV => import_csv(text),
        Format::TSV => import_tsv(text),
        Format::Table => import_table(text),
        Format::HTML => import_html(text),
        Format::SVG => Ok(Value::Str(text.to_string())),
        Format::PNG => Err(EvalError::Error(
            "Cannot import PNG from a string source; use Import[path] instead".to_string(),
        )),
    }
}

/// Import from binary data using the given format.
/// For non-binary formats, decodes as UTF-8 and delegates to `format_import_text`.
pub fn format_import_binary(format: &Format, data: &[u8]) -> Result<Value, EvalError> {
    match format {
        Format::PNG => import_png(data),
        _ => {
            let text = std::str::from_utf8(data)
                .map_err(|e| EvalError::Error(format!("File is not valid UTF-8: {}", e)))?;
            format_import_text(format, text)
        }
    }
}

// ── Public Export API ────────────────────────────────────────────────────────

/// Export a value to the given format.
pub fn format_export(format: &Format, value: &Value) -> Result<ExportOutput, EvalError> {
    match format {
        Format::JSON => {
            let jv = crate::ffi::marshal::value_to_json(value)
                .map_err(|e| EvalError::Error(format!("Export JSON error: {}", e)))?;
            serde_json::to_string_pretty(&jv)
                .map(ExportOutput::Text)
                .map_err(|e| EvalError::Error(format!("Export JSON serialization: {}", e)))
        }
        Format::CSV => export_csv(value).map(ExportOutput::Text),
        Format::TSV => export_tsv(value).map(ExportOutput::Text),
        Format::Table => export_table(value).map(ExportOutput::Text),
        Format::SVG => export_svg(value).map(ExportOutput::Text),
        Format::PNG => export_png(value).map(ExportOutput::Binary),
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
