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
    let mut path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    // Expand leading ~ to home directory (mirrors Export).
    if (path.starts_with("~/") || path == "~")
        && let Ok(home) = std::env::var("HOME")
    {
        path = if path == "~" {
            home
        } else {
            format!("{}{}", home, &path[1..])
        };
    }

    let format_name = match args.get(1) {
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

    // Always read as bytes; binary formats decode directly, text formats
    // decode as UTF-8 inside format_import_binary.
    let data =
        std::fs::read(&path).map_err(|e| EvalError::Error(format!("Import failed: {}", e)))?;
    super::formats::format_import_binary(&format, &data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    #[test]
    fn test_import_non_string_format_errors() {
        let path = std::env::temp_dir().join(format!("syma_imp_ns_{}.csv", std::process::id()));
        std::fs::write(&path, "a,b").unwrap();
        let res = builtin_import(&[
            Value::Str(path.to_string_lossy().into_owned()),
            Value::Integer(Integer::from(1)),
        ]);
        assert!(matches!(res, Err(EvalError::TypeError { .. })));
        std::fs::remove_file(&path).ok();
    }
}
