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
    let mut path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    // Expand leading ~ to home directory
    if (path.starts_with("~/") || path == "~")
        && let Ok(home) = std::env::var("HOME")
    {
        if path == "~" {
            path = home;
        } else {
            path = format!("{}{}", home, &path[1..]);
        }
    }
    let data = &args[1];

    let format_name = match args.get(2) {
        None => None,
        Some(Value::Str(s)) => Some(s.as_str()),
        Some(other) => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };

    let format = super::formats::detect_format(&path, format_name)?;

    match super::formats::format_export(&format, data)? {
        ExportOutput::Text(s) => std::fs::write(&path, &s)
            .map_err(|e| EvalError::Error(format!("Export failed: {}", e)))?,
        ExportOutput::Binary(bytes) => std::fs::write(&path, &bytes)
            .map_err(|e| EvalError::Error(format!("Export failed: {}", e)))?,
    }
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    #[test]
    fn test_export_non_string_format_errors() {
        let path = std::env::temp_dir().join(format!("syma_exp_ns_{}.csv", std::process::id()));
        let res = builtin_export(&[
            Value::Str(path.to_string_lossy().into_owned()),
            Value::List(vec![Value::Str("a".into())]),
            Value::Integer(Integer::from(1)),
        ]);
        assert!(matches!(res, Err(EvalError::TypeError { .. })));
        std::fs::remove_file(&path).ok();
    }
}
