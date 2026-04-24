//! Import builtin.
//!
//! Delegates format detection and conversion to the `formats` module.

use crate::value::{EvalError, Value};

/// Import[path] — import data from a file, detecting format from extension.
///
/// Import[path, "format"] — import using an explicit format name.
///
/// Supported formats:
///   JSON, CSV, TSV, Table, HTML, PNG, SVG, WL, NB, Text
pub fn builtin_import(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Import requires 1 or 2 arguments: Import[path] or Import[path, \"format\"]"
                .to_string(),
        ));
    }
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };

    let format_name = args.get(1).and_then(|v| match v {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    });

    let format = super::formats::detect_format(&path, format_name)?;

    // Always read as bytes; binary formats decode directly, text formats
    // decode as UTF-8 inside format_import_binary.
    let data = std::fs::read(&path)
        .map_err(|e| EvalError::Error(format!("Import failed: {}", e)))?;
    super::formats::format_import_binary(&format, &data)
}
