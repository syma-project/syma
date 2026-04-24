//! Import builtin and format-specific import converters.
//!
//! `builtin_import` detects the source format from the file extension and
//! dispatches to the appropriate converter.

use crate::ffi::marshal::json_to_value;
use crate::value::{EvalError, Value};

/// Import[path] — import data from a file.
///
/// Format is detected by file extension:
/// - `.json` — parse JSON into Value
/// - everything else — return as `Value::Str`
pub fn builtin_import(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Import requires exactly 1 argument".to_string(),
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
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| EvalError::Error(format!("Import failed: {}", e)))?;
    if path.ends_with(".json") {
        let parsed = json_to_value(&contents)
            .map_err(|e| EvalError::Error(format!("Import JSON error: {}", e)))?;
        Ok(parsed)
    } else {
        Ok(Value::Str(contents))
    }
}
