use crate::ffi::marshal::{json_to_value, value_to_json};
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
    if args.len() == 1 {
        if let Value::Str(prompt) = &args[0] {
            eprint!("{}", prompt);
        }
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

/// WriteLine[args...] — print space-separated args followed by a newline (same as Print).
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
/// Example: PrintF["Hello ~1~, you are ~2~ years old.", "Alice", 30]
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

/// Export[path, data] — export data to a file.
/// Format is detected by extension:
///   `.json` — serialise Value to JSON
///   everything else — write data.to_string() as text
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
    let data = if path.ends_with(".json") {
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

/// Import[path] — import data from a file.
/// Format is detected by extension:
///   `.json` — parse JSON into Value
///   everything else — return as Value::Str
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
        json_to_value(&contents).map_err(|e| EvalError::Error(format!("Import JSON parse: {}", e)))
    } else {
        Ok(Value::Str(contents))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn str_val(s: &str) -> Value {
        Value::Str(s.to_string())
    }

    #[test]
    fn test_write_read_string_roundtrip() {
        let path = "/tmp/syma_test_io_roundtrip.txt";
        let data = "hello world\nline two";
        builtin_write_string(&[str_val(path), str_val(data)]).unwrap();
        let result = builtin_read_string(&[str_val(path)]).unwrap();
        assert_eq!(result, str_val(data));
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_write_string_errors() {
        assert!(builtin_write_string(&[str_val("x")]).is_err()); // too few args
        assert!(
            builtin_write_string(&[Value::Integer(rug::Integer::from(1)), str_val("x")]).is_err()
        ); // wrong type
    }

    #[test]
    fn test_read_string_errors() {
        assert!(builtin_read_string(&[]).is_err()); // no args
        assert!(builtin_read_string(&[str_val("/tmp/syma_nonexistent_file_xyz")]).is_err()); // file not found
    }

    #[test]
    fn test_export_import_json_roundtrip() {
        let path = "/tmp/syma_test_export.json";
        let data = Value::List(vec![
            Value::Integer(rug::Integer::from(1)),
            Value::Integer(rug::Integer::from(2)),
            Value::Integer(rug::Integer::from(3)),
        ]);
        builtin_export(&[str_val(path), data.clone()]).unwrap();
        let result = builtin_import(&[str_val(path)]).unwrap();
        assert_eq!(result, data);
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_export_import_text_roundtrip() {
        let path = "/tmp/syma_test_export.txt";
        let data = str_val("some text content");
        builtin_export(&[str_val(path), data.clone()]).unwrap();
        let result = builtin_import(&[str_val(path)]).unwrap();
        assert_eq!(result, data);
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_export_errors() {
        assert!(builtin_export(&[str_val("x")]).is_err()); // too few args
    }

    #[test]
    fn test_import_errors() {
        assert!(builtin_import(&[]).is_err()); // no args
    }
}
