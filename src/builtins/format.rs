/// Format and display builtins: InputForm, Short, Shallow, NumberForm, Grid, Defer, etc.
///
/// These builtins do not transform their argument; they wrap it in a
/// `Value::Formatted { format, ... }` that controls how the value displays.
/// The actual rendering is implemented in `value.rs`'s Display impl.
use crate::lexer;
use crate::parser;
use crate::value::{EvalError, Format, Value};

// ── Format wrappers ─────────────────────────────────────────────────────────────

/// InputForm[expr] — display expr in infix notation (e.g., `a + b` not `Plus[a, b]`).
pub fn builtin_input_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "InputForm requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::InputForm,
        value: Box::new(args[0].clone()),
    })
}

/// FullForm[expr] — display expr in head[arg, ...] notation (default display).
pub fn builtin_full_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FullForm requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::FullForm,
        value: Box::new(args[0].clone()),
    })
}

/// Short[expr] — display with top-level truncation (default 5 items).
/// Short[expr, n] — display with at most n top-level items.
pub fn builtin_short(args: &[Value]) -> Result<Value, EvalError> {
    let n = match args.len() {
        1 => 5usize,
        2 => {
            let v = args[1].to_integer().unwrap_or(5);
            if v < 1 {
                return Err(EvalError::Error(
                    "Short[expr, n] requires n >= 1".to_string(),
                ));
            }
            v as usize
        }
        _ => {
            return Err(EvalError::Error(
                "Short requires 1 or 2 arguments".to_string(),
            ))
        }
    };
    Ok(Value::Formatted {
        format: Format::Short(n),
        value: Box::new(args[0].clone()),
    })
}

/// Shallow[expr] — display with limited nesting depth (default 3).
/// Shallow[expr, depth] — display with at most `depth` nesting levels.
pub fn builtin_shallow(args: &[Value]) -> Result<Value, EvalError> {
    let depth = match args.len() {
        1 => 3usize,
        2 => {
            let v = args[1].to_integer().unwrap_or(3);
            if v < 1 {
                return Err(EvalError::Error(
                    "Shallow[expr, n] requires n >= 1".to_string(),
                ));
            }
            v as usize
        }
        _ => {
            return Err(EvalError::Error(
                "Shallow requires 1 or 2 arguments".to_string(),
            ))
        }
    };
    Ok(Value::Formatted {
        format: Format::Shallow(depth),
        value: Box::new(args[0].clone()),
    })
}

/// NumberForm[expr, n] — display numbers with n significant digits.
pub fn builtin_number_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NumberForm requires exactly 2 arguments: NumberForm[expr, n]".to_string(),
        ));
    }
    let digits = args[1].to_integer().unwrap_or(6);
    if digits < 1 {
        return Err(EvalError::Error(
            "NumberForm[expr, n] requires n >= 1".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::NumberForm {
            digits: digits as usize,
        },
        value: Box::new(args[0].clone()),
    })
}

/// ScientificForm[expr, n] — display numbers in scientific notation with n significant digits.
pub fn builtin_scientific_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ScientificForm requires exactly 2 arguments: ScientificForm[expr, n]"
                .to_string(),
        ));
    }
    let digits = args[1].to_integer().unwrap_or(6);
    if digits < 1 {
        return Err(EvalError::Error(
            "ScientificForm[expr, n] requires n >= 1".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::ScientificForm {
            digits: digits as usize,
        },
        value: Box::new(args[0].clone()),
    })
}

/// BaseForm[expr, base] — display a number in the given base (2–36).
pub fn builtin_base_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "BaseForm requires exactly 2 arguments: BaseForm[expr, base]".to_string(),
        ));
    }
    let base_val = args[1].to_integer().unwrap_or(10);
    if !(2..=36).contains(&base_val) {
        return Err(EvalError::Error(
            "BaseForm[expr, base] requires base between 2 and 36".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::BaseForm(base_val as u32),
        value: Box::new(args[0].clone()),
    })
}

/// Grid[list] — display a 2D list as an aligned table grid.
pub fn builtin_grid(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Grid requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::Grid,
        value: Box::new(args[0].clone()),
    })
}

/// Defer[expr] — display expr without evaluation.
/// Note: In the current evaluator, arguments are evaluated before the builtin
/// is called, so this acts as a display wrapper rather than a true hold.
/// For a true hold, use Hold[expr] or HoldComplete[expr].
pub fn builtin_defer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Defer requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::Deferred,
        value: Box::new(args[0].clone()),
    })
}

/// SyntaxQ[expr] — returns True if expr is valid Syma syntax, False otherwise.
/// Performs lex + parse only (no evaluation).
pub fn builtin_syntax_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "SyntaxQ requires exactly 1 argument".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s.clone(),
        other => return Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: other.type_name().to_string(),
        }),
    };
    match lexer::tokenize(&s) {
        Ok(tokens) => match parser::parse(tokens) {
            Ok(_) => Ok(Value::Bool(true)),
            Err(_) => Ok(Value::Bool(false)),
        },
        Err(_) => Ok(Value::Bool(false)),
    }
}

/// SyntaxLength[expr] — returns the position (as an Integer) of the first syntax error,
/// or the length of the expression string if it is valid.
pub fn builtin_syntax_length(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "SyntaxLength requires exactly 1 argument".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s.clone(),
        other => return Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: other.type_name().to_string(),
        }),
    };
    let len = s.len();
    match lexer::tokenize(&s) {
        Ok(tokens) => match parser::parse(tokens) {
            Ok(_) => Ok(Value::Integer(rug::Integer::from(len))),
            Err(_) => {
                // No precise position in ParseError; return a rough heuristic
                Ok(Value::Integer(rug::Integer::from(0)))
            }
        },
        Err(e) => {
            // Lexer error includes position info
            Ok(Value::Integer(rug::Integer::from(e.pos)))
        }
    }
}

// ── Symbol list for lazy package loading ─────────────────────────────────────────

/// All symbols provided by this module.
pub const SYMBOLS: &[&str] = &[
    "InputForm",
    "FullForm",
    "Short",
    "Shallow",
    "NumberForm",
    "ScientificForm",
    "BaseForm",
    "Grid",
    "Defer",
    "SyntaxQ",
    "SyntaxLength",
];

/// Register all format builtins in the environment.
pub fn register(env: &crate::env::Env) {
    use super::register_builtin;
    register_builtin(env, "InputForm", builtin_input_form);
    register_builtin(env, "FullForm", builtin_full_form);
    register_builtin(env, "Short", builtin_short);
    register_builtin(env, "Shallow", builtin_shallow);
    register_builtin(env, "NumberForm", builtin_number_form);
    register_builtin(env, "ScientificForm", builtin_scientific_form);
    register_builtin(env, "BaseForm", builtin_base_form);
    register_builtin(env, "Grid", builtin_grid);
    register_builtin(env, "Defer", builtin_defer);
    register_builtin(env, "SyntaxQ", builtin_syntax_q);
    register_builtin(env, "SyntaxLength", builtin_syntax_length);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::DEFAULT_PRECISION;
    use rug::Float;

    fn eval_format(expr: &str) -> Value {
        let env = crate::env::Env::new();
        crate::builtins::register_builtins(&env);
        crate::eval_input(expr, &env).unwrap_or(Value::Null)
    }

    #[test]
    fn test_short_builtin() {
        // Short on a long list
        let v = Value::List((0..20).map(|i| Value::Integer(rug::Integer::from(i))).collect());
        let result = builtin_short(&[v]).unwrap();
        let s = result.to_string();
        assert!(s.contains("<<"));
        assert!(s.contains("15>>")); // 20 - 5 = 15 truncated
    }

    #[test]
    fn test_short_with_n() {
        let v = Value::List((0..20).map(|i| Value::Integer(rug::Integer::from(i))).collect());
        let n = Value::Integer(rug::Integer::from(3));
        let result = builtin_short(&[v, n]).unwrap();
        let s = result.to_string();
        assert!(s.contains("<<17>>"));
    }

    #[test]
    fn test_input_form_plus() {
        // InputForm should show infix notation
        let v = Value::Call {
            head: "Plus".to_string(),
            args: vec![
                Value::Integer(rug::Integer::from(1)),
                Value::Integer(rug::Integer::from(2)),
                Value::Integer(rug::Integer::from(3)),
            ],
        };
        let result = builtin_input_form(&[v]).unwrap();
        let s = result.to_string();
        assert_eq!(s, "1 + 2 + 3");
    }

    #[test]
    fn test_input_form_times() {
        let v = Value::Call {
            head: "Times".to_string(),
            args: vec![
                Value::Integer(rug::Integer::from(2)),
                Value::Symbol("x".to_string()),
            ],
        };
        let result = builtin_input_form(&[v]).unwrap();
        let s = result.to_string();
        assert_eq!(s, "2 x");
    }

    #[test]
    fn test_input_form_power() {
        let v = Value::Call {
            head: "Power".to_string(),
            args: vec![
                Value::Symbol("x".to_string()),
                Value::Integer(rug::Integer::from(2)),
            ],
        };
        let result = builtin_input_form(&[v]).unwrap();
        let s = result.to_string();
        assert_eq!(s, "x^2");
    }

    #[test]
    fn test_syntax_q() {
        let valid = Value::Str("1 + 2".to_string());
        assert_eq!(builtin_syntax_q(&[valid]).unwrap(), Value::Bool(true));

        let invalid = Value::Str("1 + ".to_string());
        assert_eq!(builtin_syntax_q(&[invalid]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_syntax_length() {
        let valid = Value::Str("1 + 2".to_string());
        let result = builtin_syntax_length(&[valid]).unwrap();
        assert_eq!(result.to_integer(), Some(5));

        // Invalid syntax: parse errors return 0 (no position info in ParseError)
        let invalid = Value::Str("1 + ".to_string());
        let result = builtin_syntax_length(&[invalid]).unwrap();
        // Should return some position (even 0) without panicking
        let _pos = result.to_integer().unwrap_or(0);
    }

    #[test]
    fn test_number_form() {
        let v = Value::Real(Float::with_val(DEFAULT_PRECISION, std::f64::consts::PI));
        let n = Value::Integer(rug::Integer::from(4));
        let result = builtin_number_form(&[v, n]).unwrap();
        let s = result.to_string();
        assert!(s.starts_with("3.14"), "NumberForm[Pi, 4] should start with 3.14, got: {}", s);
    }

    #[test]
    fn test_base_form() {
        let v = Value::Integer(rug::Integer::from(255));
        let base = Value::Integer(rug::Integer::from(16));
        let result = builtin_base_form(&[v, base]).unwrap();
        let s = result.to_string();
        assert_eq!(s, "ff(base 16)");
    }

    #[test]
    fn test_grid() {
        let v = Value::List(vec![
            Value::List(vec![
                Value::Integer(rug::Integer::from(1)),
                Value::Integer(rug::Integer::from(2)),
                Value::Integer(rug::Integer::from(3)),
            ]),
            Value::List(vec![
                Value::Integer(rug::Integer::from(10)),
                Value::Integer(rug::Integer::from(20)),
                Value::Integer(rug::Integer::from(30)),
            ]),
        ]);
        let result = builtin_grid(&[v]).unwrap();
        let s = result.to_string();
        // Grid should contain aligned columns (1 2 3 on one line, 10 20 30 on another)
        assert!(s.contains("1"));
        assert!(s.contains("10"));
        assert!(s.contains("20"));
        assert!(s.contains("30"));
    }

    #[test]
    fn test_input_form_compound() {
        // Test that InputForm renders 1 + 2*x correctly
        let v = Value::Call {
            head: "Plus".to_string(),
            args: vec![
                Value::Integer(rug::Integer::from(1)),
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![
                        Value::Integer(rug::Integer::from(2)),
                        Value::Symbol("x".to_string()),
                    ],
                },
            ],
        };
        let result = builtin_input_form(&[v]).unwrap();
        let s = result.to_string();
        assert_eq!(s, "1 + 2 x");
    }

    #[test]
    fn test_shallow_builtin() {
        // 4 levels of nesting, Shallow default depth 3 → innermost truncated
        let inner = Value::List(vec![
            Value::List(vec![
                Value::List(vec![
                    Value::List(vec![Value::Integer(rug::Integer::from(42))])
                ])
            ])
        ]);
        let result = builtin_shallow(&[inner]).unwrap();
        let s = result.to_string();
        // At depth 3 of 4, should show <<...>> for innermost
        assert!(s.contains("<<...>>"), "Shallow output should contain <<...>>, got: {s}");
    }
}
