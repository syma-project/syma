//! Export builtin.
//!
//! Delegates format detection and conversion to the `formats` module.

use crate::value::{EvalError, Value};

/// Export[path, data] — export data to a file, detecting format from extension.
///
/// Export[path, data, "format"] — export using an explicit format name.
///
/// Supported formats:
///   JSON, RawJSON, CSV, TSV, Table, List, Lines, Words, XML, SVG, PNG, Text,
///   WL, Binary
pub fn builtin_export(args: &[Value]) -> Result<Value, EvalError> {
    super::builtin_export_env(args, &crate::env::Env::new())
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
