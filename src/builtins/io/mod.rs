//! I/O builtins: Print, Input, Write, WriteLine, PrintF, WriteString, ReadString.
//!
//! Format-specific Import/Export lives in sibling modules:
//! - `export.rs` — Export dispatcher and format converters
//! - `import.rs` — Import dispatcher and format converters

pub mod export;
pub mod import;

pub use export::builtin_export;
pub use import::builtin_import;

use crate::value::EvalError;
use crate::value::Value;
use std::io::Write as IoWrite;

pub fn builtin_print(args: &[Value]) -> Result<Value, EvalError> {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            print!(" ");
        }
        print!("{}", arg);
    }
    println!();
    Ok(Value::Null)
}

pub fn builtin_input(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() > 1 {
        return Err(EvalError::Error(
            "Input requires 0 or 1 arguments".to_string(),
        ));
    }
    if args.len() == 1
        && let Value::Str(prompt) = &args[0]
    {
        eprint!("{}", prompt);
    }
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| EvalError::Error(format!("Input error: {}", e)))?;
    Ok(Value::Str(input.trim().to_string()))
}

/// Write[args...] — print space-separated args without a trailing newline.
pub fn builtin_write(args: &[Value]) -> Result<Value, EvalError> {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            print!(" ");
        }
        print!("{}", arg);
    }
    std::io::stdout().flush().ok();
    Ok(Value::Null)
}

/// WriteLine[args...] — print space-separated args followed by a newline.
pub fn builtin_write_line(args: &[Value]) -> Result<Value, EvalError> {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            print!(" ");
        }
        print!("{}", arg);
    }
    println!();
    Ok(Value::Null)
}

/// PrintF["fmt", args...] — formatted print.
/// Replaces `~1~`, `~2~`, ... with the corresponding arguments.
pub fn builtin_printf(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "PrintF requires at least 1 argument".to_string(),
        ));
    }
    let template = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let mut result = template;
    for (i, arg) in args[1..].iter().enumerate() {
        result = result.replace(&format!("~{}~", i + 1), &format!("{}", arg));
    }
    print!("{}", result);
    Ok(Value::Null)
}

/// WriteString[path, data] — write a string to a file. Creates or overwrites.
pub fn builtin_write_string(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "WriteString requires exactly 2 arguments".to_string(),
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
    let data = match &args[1] {
        Value::Str(s) => s.clone(),
        other => format!("{}", other),
    };
    std::fs::write(&path, &data)
        .map_err(|e| EvalError::Error(format!("WriteString failed: {}", e)))?;
    Ok(Value::Null)
}

/// ReadString[path] — read entire file contents as a string.
pub fn builtin_read_string(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ReadString requires exactly 1 argument".to_string(),
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
        .map_err(|e| EvalError::Error(format!("ReadString failed: {}", e)))?;
    Ok(Value::Str(contents))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    #[test]
    fn test_write_string_errors() {
        let result = builtin_write_string(&[]);
        assert!(result.is_err());
        let result = builtin_write_string(&[Value::Str("/tmp/test.txt".into())]);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_string_errors() {
        let result = builtin_read_string(&[]);
        assert!(result.is_err());
        let result = builtin_read_string(&[Value::Str("/nonexistent/file".into())]);
        assert!(result.is_err());
    }

    #[test]
    fn test_write_read_string_roundtrip() {
        use std::fs;
        let path = "/tmp/test_syma_io_roundtrip.txt";
        let data = "Hello, Syma!";
        let wr = builtin_write_string(&[
            Value::Str(path.to_string()),
            Value::Str(data.to_string()),
        ]);
        assert!(wr.is_ok());
        let rd = builtin_read_string(&[Value::Str(path.to_string())]).unwrap();
        assert_eq!(rd, Value::Str(data.to_string()));
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_export_errors() {
        let result = builtin_export(&[]);
        assert!(result.is_err());
        let result = builtin_export(&[Value::Str("/tmp/x.txt".into())]);
        assert!(result.is_err());
    }

    #[test]
    fn test_import_errors() {
        let result = builtin_import(&[]);
        assert!(result.is_err());
        let result = builtin_import(&[Value::Str("/nonexistent/file.json".into())]);
        assert!(result.is_err());
    }

    #[test]
    fn test_export_import_json_roundtrip() {
        use std::fs;
        let path = "/tmp/test_syma_json_rt.json";
        let data = Value::List(vec![
            Value::Integer(rug::Integer::from(1)),
            Value::Integer(rug::Integer::from(2)),
            Value::Integer(rug::Integer::from(3)),
        ]);
        let export_result = builtin_export(&[Value::Str(path.to_string()), data.clone()]);
        assert!(export_result.is_ok(), "Export failed: {:?}", export_result);
        let import_result = builtin_import(&[Value::Str(path.to_string())]);
        assert!(import_result.is_ok(), "Import failed: {:?}", import_result);
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_export_import_text_roundtrip() {
        use std::fs;
        let path = "/tmp/test_syma_text_rt.txt";
        let data_str = "Hello world";
        let export_result =
            builtin_export(&[Value::Str(path.to_string()), Value::Str(data_str.to_string())]);
        assert!(export_result.is_ok());
        let import_result = builtin_import(&[Value::Str(path.to_string())]).unwrap();
        assert_eq!(import_result, Value::Str(data_str.to_string()));
        fs::remove_file(path).ok();
    }
}
