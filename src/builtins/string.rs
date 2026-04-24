use crate::value::{EvalError, Value};
use rug::Integer;

pub fn builtin_string_join(args: &[Value]) -> Result<Value, EvalError> {
    let mut result = String::new();
    for arg in args {
        match arg {
            Value::Str(s) => result.push_str(s),
            _ => result.push_str(&arg.to_string()),
        }
    }
    Ok(Value::Str(result))
}

pub fn builtin_string_length(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "StringLength requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Str(s) => Ok(Value::Integer(Integer::from(s.len() as i64))),
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_to_string(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ToString requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Str(args[0].to_string()))
}

pub fn builtin_to_expression(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ToExpression requires exactly 1 argument".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s,
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let tokens = crate::lexer::tokenize(s)
        .map_err(|e| EvalError::Error(format!("ToExpression parse error: {}", e)))?;
    let ast = crate::parser::parse(tokens)
        .map_err(|e| EvalError::Error(format!("ToExpression parse error: {}", e)))?;
    let env = crate::env::Env::new();
    crate::builtins::register_builtins(&env);
    crate::eval::eval_program(&ast, &env)
}

pub fn builtin_string_split(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "StringSplit requires 1 or 2 arguments".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s,
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let delim = if args.len() == 2 {
        match &args[1] {
            Value::Str(d) => d.as_str(),
            _ => {
                return Err(EvalError::TypeError {
                    expected: "String".to_string(),
                    got: args[1].type_name().to_string(),
                });
            }
        }
    } else {
        " "
    };
    Ok(Value::List(
        s.split(delim)
            .map(|part| Value::Str(part.to_string()))
            .collect(),
    ))
}

pub fn builtin_string_replace(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringReplace requires exactly 2 arguments".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    match &args[1] {
        Value::Rule {
            lhs,
            rhs,
            delayed: false,
        } => {
            let old = match lhs.as_ref() {
                Value::Str(s) => s.clone(),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "String".to_string(),
                        got: lhs.type_name().to_string(),
                    });
                }
            };
            let new = match rhs.as_ref() {
                Value::Str(s) => s.clone(),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "String".to_string(),
                        got: rhs.type_name().to_string(),
                    });
                }
            };
            Ok(Value::Str(s.replace(&old, &new)))
        }
        Value::List(rules) => {
            let mut result = s;
            for rule in rules {
                if let Value::Rule {
                    lhs,
                    rhs,
                    delayed: false,
                } = rule
                    && let (Value::Str(old), Value::Str(new)) = (lhs.as_ref(), rhs.as_ref())
                {
                    result = result.replace(old, new);
                }
            }
            Ok(Value::Str(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "Rule or List of Rules".to_string(),
            got: args[1].type_name().to_string(),
        }),
    }
}

pub fn builtin_string_take(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringTake requires exactly 2 arguments".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s,
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    let chars: Vec<char> = s.chars().collect();
    let count = if n >= 0 {
        n as usize
    } else {
        chars.len().saturating_sub((-n) as usize)
    };
    Ok(Value::Str(chars[..count.min(chars.len())].iter().collect()))
}

pub fn builtin_string_drop(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringDrop requires exactly 2 arguments".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s,
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    let chars: Vec<char> = s.chars().collect();
    let count = if n >= 0 {
        n as usize
    } else {
        chars.len().saturating_sub((-n) as usize)
    };
    Ok(Value::Str(chars[count.min(chars.len())..].iter().collect()))
}

pub fn builtin_string_contains_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringContainsQ requires exactly 2 arguments".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s,
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let sub = match &args[1] {
        Value::Str(s) => s,
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    Ok(Value::Bool(s.contains(sub.as_str())))
}

pub fn builtin_string_reverse(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "StringReverse requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Str(s) => Ok(Value::Str(s.chars().rev().collect())),
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_to_upper_case(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ToUpperCase requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Str(s) => Ok(Value::Str(s.to_uppercase())),
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_to_lower_case(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ToLowerCase requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Str(s) => Ok(Value::Str(s.to_lowercase())),
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// Characters["string"] — split string into a list of single-character strings.
pub fn builtin_characters(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Characters requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Str(s) => Ok(Value::List(
            s.chars().map(|c| Value::Str(c.to_string())).collect(),
        )),
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// StringMatchQ["string", "pattern"] — check if string matches a glob pattern.
pub fn builtin_string_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringMatchQ requires exactly 2 arguments".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s,
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let pat = match &args[1] {
        Value::Str(p) => p,
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    Ok(Value::Bool(glob_match(pat, s)))
}

/// Simple glob-style pattern matching (* = any substring, ? = any single char).
fn glob_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    glob_match_chars(&pat, &txt)
}

fn glob_match_chars(pat: &[char], txt: &[char]) -> bool {
    match (pat.first(), txt.first()) {
        (None, None) => true,
        (Some('*'), _) => {
            // * matches zero or more characters
            glob_match_chars(&pat[1..], txt)
                || (!txt.is_empty() && glob_match_chars(pat, &txt[1..]))
        }
        (Some('?'), Some(_)) => glob_match_chars(&pat[1..], &txt[1..]),
        (Some(p), Some(t)) if p == t => glob_match_chars(&pat[1..], &txt[1..]),
        _ => false,
    }
}

/// StringPadLeft["str", n] or StringPadLeft["str", n, "pad"] — left-pad a string.
pub fn builtin_string_pad_left(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "StringPadLeft requires 2 or 3 arguments".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    let pad = if args.len() == 3 {
        match &args[2] {
            Value::Str(p) => p.clone(),
            _ => {
                return Err(EvalError::TypeError {
                    expected: "String".to_string(),
                    got: args[2].type_name().to_string(),
                });
            }
        }
    } else {
        " ".to_string()
    };
    if n <= 0 || s.len() >= n as usize {
        return Ok(Value::Str(s));
    }
    let pad_char = pad.chars().next().unwrap_or(' ');
    let padding: String = std::iter::repeat_n(pad_char, n as usize - s.len()).collect();
    Ok(Value::Str(format!("{}{}", padding, s)))
}

/// StringPadRight["str", n] or StringPadRight["str", n, "pad"] — right-pad a string.
pub fn builtin_string_pad_right(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "StringPadRight requires 2 or 3 arguments".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    let pad = if args.len() == 3 {
        match &args[2] {
            Value::Str(p) => p.clone(),
            _ => {
                return Err(EvalError::TypeError {
                    expected: "String".to_string(),
                    got: args[2].type_name().to_string(),
                });
            }
        }
    } else {
        " ".to_string()
    };
    if n <= 0 || s.len() >= n as usize {
        return Ok(Value::Str(s));
    }
    let pad_char = pad.chars().next().unwrap_or(' ');
    let padding: String = std::iter::repeat_n(pad_char, n as usize - s.len()).collect();
    Ok(Value::Str(format!("{}{}", s, padding)))
}

/// StringTrim["str"] — remove leading and trailing whitespace.
pub fn builtin_string_trim(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "StringTrim requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Str(s) => Ok(Value::Str(s.trim().to_string())),
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// StringStartsQ["str", "prefix"] — check if string starts with prefix.
pub fn builtin_string_starts_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringStartsQ requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Str(s), Value::Str(p)) => Ok(Value::Bool(s.starts_with(p.as_str()))),
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// StringEndsQ["str", "suffix"] — check if string ends with suffix.
pub fn builtin_string_ends_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringEndsQ requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Str(s), Value::Str(p)) => Ok(Value::Bool(s.ends_with(p.as_str()))),
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn string(s: &str) -> Value {
        Value::Str(s.to_string())
    }

    #[test]
    fn test_string_join() {
        let result = builtin_string_join(&[string("hello"), string(" world")]).unwrap();
        assert_eq!(result, string("hello world"));
    }

    #[test]
    fn test_string_length() {
        assert_eq!(builtin_string_length(&[string("hello")]).unwrap(), int(5));
        assert_eq!(builtin_string_length(&[string("")]).unwrap(), int(0));
    }

    #[test]
    fn test_to_string() {
        assert_eq!(builtin_to_string(&[int(42)]).unwrap(), string("42"));
        assert_eq!(
            builtin_to_string(&[Value::Bool(true)]).unwrap(),
            string("True")
        );
    }

    #[test]
    fn test_characters() {
        let result = builtin_characters(&[Value::Str("abc".to_string())]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::Str("a".to_string()),
                Value::Str("b".to_string()),
                Value::Str("c".to_string()),
            ])
        );
    }

    #[test]
    fn test_characters_empty() {
        let result = builtin_characters(&[Value::Str("".to_string())]).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_string_match_q_exact() {
        let result = builtin_string_match_q(&[
            Value::Str("hello".to_string()),
            Value::Str("hello".to_string()),
        ])
        .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_string_match_q_wildcard() {
        let result = builtin_string_match_q(&[
            Value::Str("hello".to_string()),
            Value::Str("h*".to_string()),
        ])
        .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_string_match_q_no_match() {
        let result = builtin_string_match_q(&[
            Value::Str("hello".to_string()),
            Value::Str("world".to_string()),
        ])
        .unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_string_pad_left() {
        let result = builtin_string_pad_left(&[Value::Str("42".to_string()), int(5)]).unwrap();
        assert_eq!(result, Value::Str("   42".to_string()));
    }

    #[test]
    fn test_string_pad_right() {
        let result = builtin_string_pad_right(&[Value::Str("hi".to_string()), int(5)]).unwrap();
        assert_eq!(result, Value::Str("hi   ".to_string()));
    }

    #[test]
    fn test_string_trim() {
        let result = builtin_string_trim(&[Value::Str("  hello  ".to_string())]).unwrap();
        assert_eq!(result, Value::Str("hello".to_string()));
    }

    #[test]
    fn test_string_starts_q() {
        assert_eq!(
            builtin_string_starts_q(&[
                Value::Str("hello".to_string()),
                Value::Str("hel".to_string())
            ])
            .unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_starts_q(&[
                Value::Str("hello".to_string()),
                Value::Str("ell".to_string())
            ])
            .unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_string_ends_q() {
        assert_eq!(
            builtin_string_ends_q(&[
                Value::Str("hello".to_string()),
                Value::Str("llo".to_string())
            ])
            .unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_ends_q(&[
                Value::Str("hello".to_string()),
                Value::Str("hel".to_string())
            ])
            .unwrap(),
            Value::Bool(false)
        );
    }
}
