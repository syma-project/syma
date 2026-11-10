//! Import builtin.
//!
//! Delegates format detection and conversion to the `formats` module.

use crate::value::{EvalError, Value};

/// Import[path] — import data from a file, detecting format from extension.
///
/// Import[path, "format"] — import using an explicit format name.
///
/// Supported formats:
///   JSON, RawJSON, CSV, TSV, Table, List, Lines, Words, HTML, XML, PNG, SVG,
///   WL, NB, Text, Binary
pub fn builtin_import(args: &[Value]) -> Result<Value, EvalError> {
    super::builtin_import_env(args, &crate::env::Env::new())
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
