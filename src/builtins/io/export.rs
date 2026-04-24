//! Export builtin.
//!
//! Delegates format detection and conversion to the `formats` module.

use super::formats::ExportOutput;
use crate::value::{EvalError, Value};

/// Export[path, data] — export data to a file, detecting format from extension.
///
/// Export[path, data, "format"] — export using an explicit format name.
///
/// Supported formats:
///   JSON, CSV, TSV, Table, SVG, PNG, Text, WL
pub fn builtin_export(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Export requires 2 or 3 arguments: Export[path, data] or Export[path, data, \"format\"]"
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
    let data = &args[1];

    let format_name = args.get(2).and_then(|v| match v {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    });

    let format = super::formats::detect_format(&path, format_name)?;

    match super::formats::format_export(&format, data)? {
        ExportOutput::Text(s) => std::fs::write(&path, &s)
            .map_err(|e| EvalError::Error(format!("Export failed: {}", e)))?,
        ExportOutput::Binary(bytes) => std::fs::write(&path, &bytes)
            .map_err(|e| EvalError::Error(format!("Export failed: {}", e)))?,
    }
    Ok(Value::Null)
}
