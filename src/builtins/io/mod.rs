//! I/O builtins: Print, Input, Write, WriteLine, PrintF, WriteString, ReadString.
//!
//! Format-specific Import/Export lives in sibling modules:
//! - `export.rs` — Export dispatcher and format converters
//! - `import.rs` — Import dispatcher and format converters

pub mod export;
pub mod formats;
pub mod import;
pub mod nb;

pub use export::builtin_export;
pub use import::builtin_import;

use crate::env::Env;
use crate::value::EvalError;
use crate::value::Value;
use std::io::Write as IoWrite;

pub fn builtin_print(args: &[Value]) -> Result<Value, EvalError> {
    for arg in args {
        match arg {
            Value::Str(s) => print!("{}", s),
            other => print!("{}", other),
        }
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

/// Write[args...] — print concatenated args without a trailing newline.
pub fn builtin_write(args: &[Value]) -> Result<Value, EvalError> {
    for arg in args {
        match arg {
            Value::Str(s) => print!("{}", s),
            other => print!("{}", other),
        }
    }
    std::io::stdout().flush().ok();
    Ok(Value::Null)
}

/// WriteLine[args...] — print concatenated args followed by a newline.
pub fn builtin_write_line(args: &[Value]) -> Result<Value, EvalError> {
    for arg in args {
        match arg {
            Value::Str(s) => print!("{}", s),
            other => print!("{}", other),
        }
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

/// ImportString[data, "format"] — import from a string using the specified format.
pub fn builtin_import_string(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ImportString requires exactly 2 arguments: ImportString[data, \"format\"]".to_string(),
        ));
    }
    let data = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let format_name = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let format = formats::detect_format("", Some(&format_name))?;
    if format.is_binary() {
        return Err(EvalError::Error(format!(
            "ImportString does not support binary format '{}'",
            format_name
        )));
    }
    formats::format_import_text(&format, &data)
}

/// ExportString[data, "format"] — export to a string using the specified format.
pub fn builtin_export_string(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ExportString requires exactly 2 arguments: ExportString[data, \"format\"]".to_string(),
        ));
    }
    let data = &args[0];
    let format_name = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let format = formats::detect_format("", Some(&format_name))?;
    match formats::format_export(&format, data)? {
        formats::ExportOutput::Text(s) => Ok(Value::Str(s)),
        formats::ExportOutput::Binary(_) => Err(EvalError::Error(format!(
            "ExportString does not support binary format '{}'",
            format_name
        ))),
    }
}

/// ReadList[path] — read all lines from a file into a list of strings.
pub fn builtin_read_list(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ReadList requires exactly 1 argument: ReadList[path]".to_string(),
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
    let text = std::fs::read_to_string(&path)
        .map_err(|e| EvalError::Error(format!("ReadList failed: {}", e)))?;
    let lines: Vec<Value> = text.lines().map(|s| Value::Str(s.to_string())).collect();
    Ok(Value::List(lines))
}

// ── FileRead ───────────────────────────────────────────────────────────────────

/// FileRead[path] — Read entire file contents as a string.
pub fn builtin_file_read(args: &[Value]) -> Result<Value, EvalError> {
    let path = match args {
        [Value::Str(s)] => s.clone(),
        _ => {
            return Err(EvalError::NoMatch {
                head: "FileRead".to_string(),
                args: args.to_vec().into(),
            });
        }
    };
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| EvalError::Error(format!("FileRead failed: {}", e)))?;
    Ok(Value::Str(contents))
}

// ── FileWrite ──────────────────────────────────────────────────────────────────

/// FileWrite[path, content] — Write a string to a file.
pub fn builtin_file_write(args: &[Value]) -> Result<Value, EvalError> {
    let (path, content) = match args {
        [Value::Str(p), Value::Str(c)] => (p.clone(), c.clone()),
        [Value::Str(p), other] => (p.clone(), format!("{}", other)),
        _ => {
            return Err(EvalError::NoMatch {
                head: "FileWrite".to_string(),
                args: args.to_vec().into(),
            });
        }
    };
    std::fs::write(&path, &content)
        .map_err(|e| EvalError::Error(format!("FileWrite failed: {}", e)))?;
    Ok(Value::Null)
}

// ── FileExists ─────────────────────────────────────────────────────────────────

/// FileExists[path] — Return True if the file or directory exists.
pub fn builtin_file_exists(args: &[Value]) -> Result<Value, EvalError> {
    let path = match args {
        [Value::Str(s)] => s.clone(),
        _ => {
            return Err(EvalError::NoMatch {
                head: "FileExists".to_string(),
                args: args.to_vec().into(),
            });
        }
    };
    Ok(Value::Bool(std::path::Path::new(&path).exists()))
}

// ── DirectoryCreate ────────────────────────────────────────────────────────────

/// DirectoryCreate[path] — Create directory (and any missing parent directories).
pub fn builtin_directory_create(args: &[Value]) -> Result<Value, EvalError> {
    let path = match args {
        [Value::Str(s)] => s.clone(),
        _ => {
            return Err(EvalError::NoMatch {
                head: "DirectoryCreate".to_string(),
                args: args.to_vec().into(),
            });
        }
    };
    std::fs::create_dir_all(&path)
        .map_err(|e| EvalError::Error(format!("DirectoryCreate failed: {}", e)))?;
    Ok(Value::Str(path))
}

// ── CopyFile ───────────────────────────────────────────────────────────────────

/// CopyFile[src, dst] — Copy a file. Returns the number of bytes copied.
pub fn builtin_copy_file(args: &[Value]) -> Result<Value, EvalError> {
    let (src, dst) = match args {
        [Value::Str(s), Value::Str(d)] => (s.clone(), d.clone()),
        _ => {
            return Err(EvalError::NoMatch {
                head: "CopyFile".to_string(),
                args: args.to_vec().into(),
            });
        }
    };
    let bytes = std::fs::copy(&src, &dst)
        .map_err(|e| EvalError::Error(format!("CopyFile failed: {}", e)))?;
    Ok(Value::Integer(rug::Integer::from(bytes)))
}

// ── MoveFile ───────────────────────────────────────────────────────────────────

/// MoveFile[src, dst] — Move/rename a file.
pub fn builtin_move_file(args: &[Value]) -> Result<Value, EvalError> {
    let (src, dst) = match args {
        [Value::Str(s), Value::Str(d)] => (s.clone(), d.clone()),
        _ => {
            return Err(EvalError::NoMatch {
                head: "MoveFile".to_string(),
                args: args.to_vec().into(),
            });
        }
    };
    std::fs::rename(&src, &dst)
        .map_err(|e| EvalError::Error(format!("MoveFile failed: {}", e)))?;
    Ok(Value::Str(dst))
}

// ── DeleteFile ─────────────────────────────────────────────────────────────────

/// DeleteFile[path] — Delete a file.
pub fn builtin_delete_file(args: &[Value]) -> Result<Value, EvalError> {
    let path = match args {
        [Value::Str(s)] => s.clone(),
        _ => {
            return Err(EvalError::NoMatch {
                head: "DeleteFile".to_string(),
                args: args.to_vec().into(),
            });
        }
    };
    std::fs::remove_file(&path)
        .map_err(|e| EvalError::Error(format!("DeleteFile failed: {}", e)))?;
    Ok(Value::Bool(true))
}

// ── Timing ─────────────────────────────────────────────────────────────────────

/// Timing[expr] — Measure evaluation time.
/// Returns `{elapsed_seconds, result}`.
pub fn builtin_timing(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::NoMatch {
            head: "Timing".to_string(),
            args: args.to_vec().into(),
        });
    }
    let start = std::time::Instant::now();
    let result = crate::eval::apply_function(
        &Value::Symbol("Identity".to_string()),
        &[args[0].clone()],
        env,
    );
    let elapsed = start.elapsed();
    let evaluated = match result {
        Ok(v) => v,
        Err(_) => args[0].clone(),
    };
    let seconds = rug::Float::with_val(
        crate::value::DEFAULT_PRECISION,
        elapsed.as_secs_f64(),
    );
    Ok(Value::List(vec![Value::Real(seconds), evaluated]))
}

// ── Echo ───────────────────────────────────────────────────────────────────────

/// Echo[expr] — Print the expression and return it.
pub fn builtin_echo(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::NoMatch {
            head: "Echo".to_string(),
            args: args.to_vec().into(),
        });
    }
    let mut last_result = args[0].clone();
    for arg in args {
        eprintln!("{}", arg);
        match crate::eval::apply_function(
            &Value::Symbol("Identity".to_string()),
            &[arg.clone()],
            env,
        ) {
            Ok(v) => {
                println!("{}", v);
                last_result = v;
            }
            Err(_) => {
                println!("{}", arg);
                last_result = arg.clone();
            }
        }
    }
    Ok(last_result)
}

// ── Quit ───────────────────────────────────────────────────────────────────────

/// Quit[] — Exit the Syma session.
pub fn builtin_quit(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Err(EvalError::NoMatch {
            head: "Quit".to_string(),
            args: args.to_vec().into(),
        });
    }
    std::process::exit(0)
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
        let wr =
            builtin_write_string(&[Value::Str(path.to_string()), Value::Str(data.to_string())]);
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
        let export_result = builtin_export(&[
            Value::Str(path.to_string()),
            Value::Str(data_str.to_string()),
        ]);
        assert!(export_result.is_ok());
        let import_result = builtin_import(&[Value::Str(path.to_string())]).unwrap();
        assert_eq!(import_result, Value::Str(data_str.to_string()));
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_import_nb_with_input_cells() {
        use std::fs;
        let path = "/tmp/test_syma_nb_import.nb";
        let nb_content = r#"Notebook[{
Cell[BoxData[RowBox[{"1", "+", "2"}]], "Input"],
Cell[BoxData[RowBox[{"x", "^", "2"}]], "Input"]
}]"#;
        fs::write(path, nb_content).ok();
        let result = builtin_import(&[Value::Str(path.to_string())]);
        assert!(result.is_ok(), "NB import failed: {:?}", result);
        let val = result.unwrap();
        match &val {
            Value::Str(s) => {
                assert!(
                    s.contains("1+2"),
                    "Expected code to contain '1+2', got: {}",
                    s
                );
                assert!(
                    s.contains("x^2"),
                    "Expected code to contain 'x^2', got: {}",
                    s
                );
            }
            _ => panic!("Expected Value::Str from NB import, got {:?}", val),
        }
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_import_nb_no_input_cells() {
        use std::fs;
        let path = "/tmp/test_syma_nb_empty.nb";
        let nb_content = r#"Notebook[{
Cell[BoxData[StyleBox["Title", FontSize->24]], "Title"]
}]"#;
        fs::write(path, nb_content).ok();
        let result = builtin_import(&[Value::Str(path.to_string())]);
        assert!(
            result.is_ok(),
            "NB import (no Input cells) failed: {:?}",
            result
        );
        let val = result.unwrap();
        assert_eq!(val, Value::Str(String::new()));
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_import_m_file() {
        use std::fs;
        let path = "/tmp/test_syma_m_import.m";
        let m_content = "(* WL source *)\nf[x_] := x^2\nf[5]";
        fs::write(path, m_content).ok();
        let result = builtin_import(&[Value::Str(path.to_string())]);
        assert!(result.is_ok(), ".m import failed: {:?}", result);
        let val = result.unwrap();
        assert_eq!(val, Value::Str(m_content.to_string()));
        fs::remove_file(path).ok();
    }

    // ── New Format Tests ───────────────────────────────────────────────

    #[test]
    fn test_import_csv_basic() {
        let csv = "a,b,c\n1,2,3\nx,y,z";
        let result = super::formats::format_import_text(&super::formats::Format::CSV, csv).unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::List(vec![
                    Value::Str("a".into()),
                    Value::Str("b".into()),
                    Value::Str("c".into())
                ]),
                Value::List(vec![
                    Value::Str("1".into()),
                    Value::Str("2".into()),
                    Value::Str("3".into())
                ]),
                Value::List(vec![
                    Value::Str("x".into()),
                    Value::Str("y".into()),
                    Value::Str("z".into())
                ]),
            ])
        );
    }

    #[test]
    fn test_import_csv_quoted() {
        let csv = r#""hello, world",foo,"a""b""#;
        let result = super::formats::format_import_text(&super::formats::Format::CSV, csv).unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::List(vec![
                Value::Str("hello, world".into()),
                Value::Str("foo".into()),
                Value::Str("a\"b".into()),
            ]),])
        );
    }

    #[test]
    fn test_export_csv_roundtrip() {
        use std::fs;
        let path = "/tmp/test_syma_csv_rt.csv";
        let data = Value::List(vec![
            Value::List(vec![Value::Str("x".into()), Value::Str("y".into())]),
            Value::List(vec![Value::Str("1".into()), Value::Str("2".into())]),
        ]);
        builtin_export(&[Value::Str(path.to_string()), data.clone()]).unwrap();
        let imported = builtin_import(&[Value::Str(path.to_string())]).unwrap();
        assert_eq!(imported, data);
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_import_tsv() {
        let tsv = "a\tb\tc\n1\t2\t3";
        let result = super::formats::format_import_text(&super::formats::Format::TSV, tsv).unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::List(vec![
                    Value::Str("a".into()),
                    Value::Str("b".into()),
                    Value::Str("c".into())
                ]),
                Value::List(vec![
                    Value::Str("1".into()),
                    Value::Str("2".into()),
                    Value::Str("3".into())
                ]),
            ])
        );
    }

    #[test]
    fn test_export_tsv_roundtrip() {
        use std::fs;
        let path = "/tmp/test_syma_tsv_rt.tsv";
        let data = Value::List(vec![Value::List(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
        ])]);
        builtin_export(&[Value::Str(path.to_string()), data.clone()]).unwrap();
        let imported = builtin_import(&[Value::Str(path.to_string())]).unwrap();
        assert_eq!(imported, data);
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_import_table() {
        let table = "1 2 3\n4 5 6\n\n7 8 9";
        let result =
            super::formats::format_import_text(&super::formats::Format::Table, table).unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::List(vec![
                    Value::Str("1".into()),
                    Value::Str("2".into()),
                    Value::Str("3".into())
                ]),
                Value::List(vec![
                    Value::Str("4".into()),
                    Value::Str("5".into()),
                    Value::Str("6".into())
                ]),
                Value::List(vec![
                    Value::Str("7".into()),
                    Value::Str("8".into()),
                    Value::Str("9".into())
                ]),
            ])
        );
    }

    #[test]
    fn test_import_html_strips_tags() {
        // No space between block elements — simple tag stripper removes tags only
        let html = "<p>Hello</p><div>World</div>";
        let result =
            super::formats::format_import_text(&super::formats::Format::HTML, html).unwrap();
        assert_eq!(result, Value::Str("HelloWorld".into()));
    }

    #[test]
    fn test_import_html_entities() {
        let html = "a &amp; b &lt; c &gt; d &quot; e";
        let result =
            super::formats::format_import_text(&super::formats::Format::HTML, html).unwrap();
        assert_eq!(result, Value::Str("a & b < c > d \" e".into()));
    }

    #[test]
    fn test_import_string_csv() {
        let result =
            builtin_import_string(&[Value::Str("a,b,c\n1,2,3".into()), Value::Str("CSV".into())])
                .unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::List(vec![
                    Value::Str("a".into()),
                    Value::Str("b".into()),
                    Value::Str("c".into())
                ]),
                Value::List(vec![
                    Value::Str("1".into()),
                    Value::Str("2".into()),
                    Value::Str("3".into())
                ]),
            ])
        );
    }

    #[test]
    fn test_export_string_json() {
        let data = Value::List(vec![
            Value::Integer(rug::Integer::from(1)),
            Value::Integer(rug::Integer::from(2)),
        ]);
        let result = builtin_export_string(&[data, Value::Str("JSON".into())]).unwrap();
        match result {
            Value::Str(s) => assert!(s.contains("1") && s.contains("2"), "JSON string: {}", s),
            _ => panic!("Expected Str, got {:?}", result),
        }
    }

    #[test]
    fn test_read_list() {
        use std::fs;
        let path = "/tmp/test_syma_readlist.txt";
        fs::write(path, "line1\nline2\nline3").ok();
        let result = builtin_read_list(&[Value::Str(path.to_string())]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::Str("line1".into()),
                Value::Str("line2".into()),
                Value::Str("line3".into()),
            ])
        );
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_import_explicit_format() {
        use std::fs;
        let path = "/tmp/test_syma_explicit.xyz";
        fs::write(path, "a,b,c\n1,2,3").ok();
        let result = builtin_import(&[Value::Str(path.to_string()), Value::Str("CSV".into())]);
        assert!(
            result.is_ok(),
            "Explicit format import failed: {:?}",
            result
        );
        let val = result.unwrap();
        assert_eq!(
            val,
            Value::List(vec![
                Value::List(vec![
                    Value::Str("a".into()),
                    Value::Str("b".into()),
                    Value::Str("c".into())
                ]),
                Value::List(vec![
                    Value::Str("1".into()),
                    Value::Str("2".into()),
                    Value::Str("3".into())
                ]),
            ])
        );
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_import_unknown_format() {
        let result = builtin_import(&[Value::Str("/tmp/test.unknown".into())]);
        assert!(result.is_err(), "Should fail for unknown extension");
    }

    #[test]
    fn test_import_string_binary_rejected() {
        let result = builtin_import_string(&[Value::Str("fake".into()), Value::Str("PNG".into())]);
        assert!(result.is_err(), "ImportString should reject PNG");
    }

    #[test]
    fn test_export_string_binary_rejected() {
        let result = builtin_export_string(&[
            Value::Integer(rug::Integer::from(1)),
            Value::Str("PNG".into()),
        ]);
        assert!(result.is_err(), "ExportString should reject PNG");
    }

    #[test]
    fn test_import_csv_multiline_field() {
        let csv = "a,\"b\nc\",d\ne,f,g";
        let result = super::formats::format_import_text(&super::formats::Format::CSV, csv).unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::List(vec![
                    Value::Str("a".into()),
                    Value::Str("b\nc".into()),
                    Value::Str("d".into()),
                ]),
                Value::List(vec![
                    Value::Str("e".into()),
                    Value::Str("f".into()),
                    Value::Str("g".into()),
                ]),
            ])
        );
    }

    #[test]
    fn test_import_png_image() {
        use std::fs;
        let path = "/tmp/test_syma_png_import.png";
        let img = image::DynamicImage::new_rgba8(2, 2);
        img.save(path).ok();
        let result = builtin_import(&[Value::Str(path.to_string())]);
        assert!(result.is_ok(), "PNG import failed: {:?}", result);
        match result.unwrap() {
            Value::Image(_) => {}
            other => panic!("Expected Image, got {:?}", other),
        }
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_export_png_image() {
        use std::fs;
        let path = "/tmp/test_syma_png_export.png";
        let img = image::DynamicImage::new_rgba8(4, 4);
        let data = Value::Image(std::sync::Arc::new(img));
        let result = builtin_export(&[Value::Str(path.to_string()), data]);
        assert!(result.is_ok(), "PNG export failed: {:?}", result);
        assert!(std::path::Path::new(path).exists(), "PNG file should exist");
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_export_text_roundtrip() {
        use std::fs;
        let path = "/tmp/test_syma_export_text.txt";
        let data = Value::Str("Hello world".into());
        builtin_export(&[Value::Str(path.to_string()), data.clone()]).unwrap();
        let imported = builtin_import(&[Value::Str(path.to_string())]).unwrap();
        assert_eq!(imported, data);
        fs::remove_file(path).ok();
    }

    // ── New builtin tests ────────────────────────────────────────────────

    #[test]
    fn test_file_read_write_roundtrip() {
        use std::fs;
        let path = "/tmp/test_syma_file_rw.txt";
        let content = "Hello, Syma file I/O!";
        let write_result = builtin_file_write(&[
            Value::Str(path.to_string()),
            Value::Str(content.to_string()),
        ]);
        assert!(write_result.is_ok(), "FileWrite should succeed: {:?}", write_result);
        let read_result = builtin_file_read(&[Value::Str(path.to_string())]);
        assert!(read_result.is_ok(), "FileRead should succeed: {:?}", read_result);
        let val = read_result.unwrap();
        assert_eq!(val, Value::Str(content.to_string()));
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_file_read_nonexistent() {
        let result = builtin_file_read(&[Value::Str("/nonexistent_file_xyz.txt".into())]);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_read_wrong_args() {
        // No args
        let result = builtin_file_read(&[]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));
        // Integer arg
        let result = builtin_file_read(&[Value::Integer(rug::Integer::from(42))]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));
        // Too many args
        let result = builtin_file_read(&[
            Value::Str("/tmp/x.txt".into()),
            Value::Str("extra".into()),
        ]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));
    }

    #[test]
    fn test_file_write_wrong_args() {
        // No args
        let result = builtin_file_write(&[]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));
        // Non-string path
        let result = builtin_file_write(&[
            Value::Integer(rug::Integer::from(1)),
            Value::Str("hello".into()),
        ]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));
    }

    #[test]
    fn test_file_write_non_string_content() {
        use std::fs;
        let path = "/tmp/test_syma_write_int.txt";
        let result = builtin_file_write(&[
            Value::Str(path.to_string()),
            Value::Integer(rug::Integer::from(42)),
        ]);
        assert!(result.is_ok());
        let contents = fs::read_to_string(path).unwrap();
        assert_eq!(contents, "42");
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_file_exists() {
        use std::fs;
        let path = "/tmp/test_syma_exists_check.txt";
        fs::write(path, "data").ok();
        let result = builtin_file_exists(&[Value::Str(path.to_string())]).unwrap();
        assert_eq!(result, Value::Bool(true));

        let result = builtin_file_exists(&[Value::Str("/nonexistent_file_abc.txt".into())]).unwrap();
        assert_eq!(result, Value::Bool(false));

        // Wrong arg count
        let result = builtin_file_exists(&[]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));

        fs::remove_file(path).ok();
    }

    #[test]
    fn test_file_exists_wrong_type() {
        let result = builtin_file_exists(&[Value::Integer(rug::Integer::from(1))]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));
    }

    #[test]
    fn test_directory_create() {
        use std::fs;
        let path = "/tmp/test_syma_dir_create/subdir";
        let result = builtin_directory_create(&[Value::Str(path.to_string())]);
        assert!(result.is_ok());
        assert!(std::path::Path::new(path).is_dir());
        fs::remove_dir_all("/tmp/test_syma_dir_create").ok();

        // Wrong args
        let result = builtin_directory_create(&[]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));
    }

    #[test]
    fn test_copy_file() {
        use std::fs;
        let src = "/tmp/test_syma_copy_src.txt";
        let dst = "/tmp/test_syma_copy_dst.txt";
        fs::write(src, "copy me").ok();
        let result = builtin_copy_file(&[
            Value::Str(src.to_string()),
            Value::Str(dst.to_string()),
        ]);
        assert!(result.is_ok());
        match result.unwrap() {
            Value::Integer(n) => {
                let bytes: u64 = n.to_u64().unwrap();
                assert_eq!(bytes, 7); // "copy me" is 7 bytes
            }
            other => panic!("Expected Integer, got {:?}", other),
        }
        let contents = fs::read_to_string(dst).unwrap();
        assert_eq!(contents, "copy me");
        fs::remove_file(src).ok();
        fs::remove_file(dst).ok();

        // Wrong args
        let result = builtin_copy_file(&[Value::Str(src.to_string())]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));
    }

    #[test]
    fn test_move_file() {
        use std::fs;
        let src = "/tmp/test_syma_move_src.txt";
        let dst = "/tmp/test_syma_move_dst.txt";
        fs::write(src, "move me").ok();
        let result = builtin_move_file(&[
            Value::Str(src.to_string()),
            Value::Str(dst.to_string()),
        ]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Str(dst.to_string()));
        assert!(!std::path::Path::new(src).exists());
        assert!(std::path::Path::new(dst).exists());
        fs::remove_file(dst).ok();

        // Wrong args
        let result = builtin_move_file(&[]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));
    }

    #[test]
    fn test_delete_file() {
        use std::fs;
        let path = "/tmp/test_syma_delete.txt";
        fs::write(path, "delete me").ok();
        let result = builtin_delete_file(&[Value::Str(path.to_string())]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Bool(true));
        assert!(!std::path::Path::new(path).exists());

        // Wrong args
        let result = builtin_delete_file(&[]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));

        // Non-existent file
        let result = builtin_delete_file(&[Value::Str("/nonexistent_file_xyz.txt".into())]);
        assert!(result.is_err());
    }

    #[test]
    fn test_quit_wrong_args() {
        let result = builtin_quit(&[Value::Str("arg".into())]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));
    }
}
