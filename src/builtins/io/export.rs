//! Export builtin and format-specific export converters.
//!
//! `builtin_export` detects the target format from the file extension and
//! dispatches to the appropriate converter. Adding a new format requires:
//! 1. Writing a converter function
//! 2. Adding a `path.ends_with(".ext")` arm in `builtin_export`

use crate::ffi::marshal::value_to_json;
use crate::value::{EvalError, Value};

/// Export[path, data] — export data to a file.
///
/// Format is detected by file extension:
/// - `.svg` — if data is a `Graphics` object, render to SVG
/// - `.json` — serialise Value to JSON
/// - everything else — write `data.to_string()` as text
pub fn builtin_export(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Export requires exactly 2 arguments".to_string(),
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

    let data = if path.ends_with(".svg")
        && let Value::Call {
            head,
            args: g_args,
        } = &args[1]
        && head == "Graphics"
        && !g_args.is_empty()
    {
        let primitives = &g_args[0];
        let options = if g_args.len() >= 2 {
            g_args[1].clone()
        } else {
            Value::Assoc(std::collections::HashMap::new())
        };
        crate::builtins::graphics::render_svg(primitives, &options)?
    } else if path.ends_with(".json") {
        let json_val = value_to_json(&args[1])
            .map_err(|e| EvalError::Error(format!("Export JSON error: {}", e)))?;
        serde_json::to_string_pretty(&json_val)
            .map_err(|e| EvalError::Error(format!("Export JSON serialisation: {}", e)))?
    } else {
        match &args[1] {
            Value::Str(s) => s.clone(),
            other => format!("{}", other),
        }
    };

    std::fs::write(&path, &data).map_err(|e| EvalError::Error(format!("Export failed: {}", e)))?;
    Ok(Value::Null)
}
