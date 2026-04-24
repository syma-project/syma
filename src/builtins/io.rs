use crate::value::EvalError;
use crate::value::Value;

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
    use std::io::Write as IoWrite;
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
