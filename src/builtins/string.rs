use crate::value::{EvalError, Value};
use rug::Integer;

pub fn builtin_string_join(args: &[Value]) -> Result<Value, EvalError> {
    let mut result = String::new();
    // If single list argument, join the list elements as strings.
    let items: &[Value] = if args.len() == 1 {
        match &args[0] {
            Value::List(list) => list.as_slice(),
            _ => args,
        }
    } else {
        args
    };
    for arg in items {
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

/// StringPart[s, n] — get the nth character (1-indexed). Negative n counts from end.
pub fn builtin_string_part(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringPart requires exactly 2 arguments".to_string(),
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
    let len = chars.len() as i64;
    let idx = if n >= 1 {
        n - 1
    } else if n < 0 {
        len + n
    } else {
        // n == 0 — invalid for 1-indexed
        return Err(EvalError::Error(format!(
            "StringPart: position {} is out of bounds (string length {})",
            n, len
        )));
    };
    if idx < 0 || idx >= len {
        return Err(EvalError::Error(format!(
            "StringPart: position {} is out of bounds (string length {})",
            n, len
        )));
    }
    Ok(Value::Str(chars[idx as usize].to_string()))
}

/// StringPosition[s, sub] — list of 1-indexed start positions where sub occurs in s.
pub fn builtin_string_position(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringPosition requires exactly 2 arguments".to_string(),
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
    let sub = match &args[1] {
        Value::Str(sub) => sub.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let positions: Vec<Value> = s
        .match_indices(&sub)
        .map(|(pos, _)| Value::Integer(Integer::from(pos as i64 + 1)))
        .collect();
    Ok(Value::List(positions))
}

/// StringCount[s, sub] — count non-overlapping occurrences of sub in s.
pub fn builtin_string_count(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringCount requires exactly 2 arguments".to_string(),
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
    let sub = match &args[1] {
        Value::Str(sub) => sub.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let count = s.match_indices(&sub).count();
    Ok(Value::Integer(Integer::from(count as i64)))
}

/// StringRepeat[s, n] — repeat string s n times.
pub fn builtin_string_repeat(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringRepeat requires exactly 2 arguments".to_string(),
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
    if n <= 0 {
        return Ok(Value::Str(String::new()));
    }
    Ok(Value::Str(s.repeat(n as usize)))
}

/// StringDelete[s, sub] — remove all occurrences of sub from s.
pub fn builtin_string_delete(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringDelete requires exactly 2 arguments".to_string(),
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
    let sub = match &args[1] {
        Value::Str(sub) => sub.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    Ok(Value::Str(s.replace(&sub, "")))
}

/// StringInsert[s, ins, n] — insert ins at position n in s (1-indexed). Negative n counts from end.
pub fn builtin_string_insert(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "StringInsert requires exactly 3 arguments: StringInsert[s, ins, n]".to_string(),
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
    let ins = match &args[1] {
        Value::Str(ins) => ins.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let n = args[2].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[2].type_name().to_string(),
    })?;
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;
    let idx = if n >= 1 {
        // n is 1-indexed; n=1 means before first character
        ((n - 1).min(len)) as usize
    } else if n < 0 {
        // n=-1 means before last character
        let pos = len + n;
        if pos < 0 {
            return Err(EvalError::Error(format!(
                "StringInsert: position {} is out of bounds (string length {})",
                n, len
            )));
        }
        pos as usize
    } else {
        return Err(EvalError::Error(format!(
            "StringInsert: position {} is out of bounds (string length {})",
            n, len
        )));
    };
    if idx > chars.len() {
        return Err(EvalError::Error(format!(
            "StringInsert: position {} is out of bounds (string length {})",
            n, len
        )));
    }
    let mut result: String = chars[..idx].iter().collect();
    result.push_str(&ins);
    result.extend(chars[idx..].iter());
    Ok(Value::Str(result))
}

/// StringRiffle[list, sep] — join a list of values with separator sep between each.
pub fn builtin_string_riffle(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringRiffle requires exactly 2 arguments".to_string(),
        ));
    }
    let list = match &args[0] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let sep = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let parts: Vec<String> = list
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.clone(),
            other => other.to_string(),
        })
        .collect();
    Ok(Value::Str(parts.join(&sep)))
}

/// StringFreeQ[s, sub] — True if s does NOT contain sub.
pub fn builtin_string_free_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringFreeQ requires exactly 2 arguments".to_string(),
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
        Value::Str(sub) => sub,
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    Ok(Value::Bool(!s.contains(sub.as_str())))
}

/// LetterQ[s] — True if s is non-empty and all characters are Unicode letters.
pub fn builtin_letter_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "LetterQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Str(s) => Ok(Value::Bool(!s.is_empty() && s.chars().all(|c| c.is_alphabetic()))),
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// DigitQ[s] — True if s is non-empty and all characters are ASCII digits.
pub fn builtin_digit_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "DigitQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Str(s) => Ok(Value::Bool(!s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))),
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// UpperCaseQ[s] — True if s is non-empty and all letters are uppercase.
/// Non-letter characters are ignored.
pub fn builtin_upper_case_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "UpperCaseQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Str(s) => {
            let letters: Vec<char> = s.chars().filter(|c| c.is_alphabetic()).collect();
            Ok(Value::Bool(!letters.is_empty() && letters.iter().all(|c| c.is_uppercase())))
        }
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// LowerCaseQ[s] — True if s is non-empty and all letters are lowercase.
/// Non-letter characters are ignored.
pub fn builtin_lower_case_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "LowerCaseQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Str(s) => {
            let letters: Vec<char> = s.chars().filter(|c| c.is_alphabetic()).collect();
            Ok(Value::Bool(!letters.is_empty() && letters.iter().all(|c| c.is_lowercase())))
        }
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// TextWords[s] — split string into a list of words (by whitespace).
pub fn builtin_text_words(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TextWords requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Str(s) => {
            let words: Vec<Value> = s
                .split_whitespace()
                .map(|w| Value::Str(w.to_string()))
                .collect();
            Ok(Value::List(words))
        }
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// CharacterCounts[s] — return a list of {char, count} pairs sorted by character.
pub fn builtin_character_counts(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "CharacterCounts requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Str(s) => {
            let mut counts = std::collections::HashMap::new();
            for c in s.chars() {
                *counts.entry(c).or_insert(0) += 1;
            }
            let mut pairs: Vec<(char, usize)> = counts.into_iter().collect();
            pairs.sort_by_key(|a| a.0);
            let list: Vec<Value> = pairs
                .into_iter()
                .map(|(ch, count)| {
                    Value::List(vec![
                        Value::Str(ch.to_string()),
                        Value::Integer(Integer::from(count as i64)),
                    ])
                })
                .collect();
            Ok(Value::List(list))
        }
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// Alphabet[] — list of lowercase Latin letters.
/// Alphabet["Latin"] — same.
pub fn builtin_alphabet(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() > 1 {
        return Err(EvalError::Error(
            "Alphabet takes 0 or 1 arguments".to_string(),
        ));
    }
    if args.len() == 1 {
        match &args[0] {
            Value::Str(s) if s == "Latin" => {}
            _ => {
                return Err(EvalError::Error(format!(
                    "Alphabet: unknown alphabet '{}'. Supported: Latin",
                    match &args[0] {
                        Value::Str(s) => s,
                        other => other.type_name(),
                    }
                )));
            }
        }
    }
    let letters: Vec<Value> = ('a'..='z').map(|c| Value::Str(c.to_string())).collect();
    Ok(Value::List(letters))
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

    // ── StringPart ──

    #[test]
    fn test_string_part() {
        assert_eq!(
            builtin_string_part(&[string("hello"), int(1)]).unwrap(),
            string("h")
        );
        assert_eq!(
            builtin_string_part(&[string("hello"), int(5)]).unwrap(),
            string("o")
        );
    }

    #[test]
    fn test_string_part_negative() {
        assert_eq!(
            builtin_string_part(&[string("hello"), int(-1)]).unwrap(),
            string("o")
        );
        assert_eq!(
            builtin_string_part(&[string("hello"), int(-5)]).unwrap(),
            string("h")
        );
    }

    #[test]
    fn test_string_part_out_of_bounds() {
        assert!(builtin_string_part(&[string("hi"), int(10)]).is_err());
        assert!(builtin_string_part(&[string("hi"), int(-10)]).is_err());
        assert!(builtin_string_part(&[string("hi"), int(0)]).is_err());
    }

    // ── StringPosition ──

    #[test]
    fn test_string_position() {
        let result = builtin_string_position(&[string("hello"), string("l")]).unwrap();
        assert_eq!(result, Value::List(vec![int(3), int(4)]));
    }

    #[test]
    fn test_string_position_no_match() {
        let result = builtin_string_position(&[string("hello"), string("x")]).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_string_position_overlap() {
        // match_indices finds non-overlapping matches
        let result = builtin_string_position(&[string("aaa"), string("aa")]).unwrap();
        assert_eq!(result, Value::List(vec![int(1)]));
    }

    // ── StringCount ──

    #[test]
    fn test_string_count() {
        assert_eq!(
            builtin_string_count(&[string("hello"), string("l")]).unwrap(),
            int(2)
        );
        assert_eq!(
            builtin_string_count(&[string("hello"), string("x")]).unwrap(),
            int(0)
        );
    }

    #[test]
    fn test_string_count_empty_sub() {
        // match_indices on empty string matches between every character
        assert_eq!(
            builtin_string_count(&[string("abc"), string("")]).unwrap(),
            int(4)
        );
    }

    // ── StringRepeat ──

    #[test]
    fn test_string_repeat() {
        assert_eq!(
            builtin_string_repeat(&[string("ab"), int(3)]).unwrap(),
            string("ababab")
        );
    }

    #[test]
    fn test_string_repeat_zero() {
        assert_eq!(
            builtin_string_repeat(&[string("ab"), int(0)]).unwrap(),
            string("")
        );
    }

    #[test]
    fn test_string_repeat_negative() {
        assert_eq!(
            builtin_string_repeat(&[string("ab"), int(-1)]).unwrap(),
            string("")
        );
    }

    // ── StringDelete ──

    #[test]
    fn test_string_delete() {
        assert_eq!(
            builtin_string_delete(&[string("hello world"), string("l")]).unwrap(),
            string("heo word")
        );
    }

    #[test]
    fn test_string_delete_no_match() {
        assert_eq!(
            builtin_string_delete(&[string("hello"), string("z")]).unwrap(),
            string("hello")
        );
    }

    // ── StringInsert ──

    #[test]
    fn test_string_insert_middle() {
        assert_eq!(
            builtin_string_insert(&[string("HelloWorld"), string("**"), int(6)]).unwrap(),
            string("Hello**World")
        );
    }

    #[test]
    fn test_string_insert_beginning() {
        assert_eq!(
            builtin_string_insert(&[string("world"), string("hello "), int(1)]).unwrap(),
            string("hello world")
        );
    }

    #[test]
    fn test_string_insert_end() {
        assert_eq!(
            builtin_string_insert(&[string("hello"), string(" world"), int(6)]).unwrap(),
            string("hello world")
        );
    }

    #[test]
    fn test_string_insert_negative() {
        assert_eq!(
            builtin_string_insert(&[string("HelWord"), string("lo"), int(-4)]).unwrap(),
            string("HelloWord")
        );
    }

    // ── StringRiffle ──

    #[test]
    fn test_string_riffle() {
        let result = builtin_string_riffle(&[
            Value::List(vec![string("a"), string("b"), string("c")]),
            string(", "),
        ])
        .unwrap();
        assert_eq!(result, string("a, b, c"));
    }

    #[test]
    fn test_string_riffle_single() {
        let result =
            builtin_string_riffle(&[Value::List(vec![string("x")]), string(",")]).unwrap();
        assert_eq!(result, string("x"));
    }

    #[test]
    fn test_string_riffle_empty() {
        let result =
            builtin_string_riffle(&[Value::List(vec![]), string(",")]).unwrap();
        assert_eq!(result, string(""));
    }

    // ── StringFreeQ ──

    #[test]
    fn test_string_free_q() {
        assert_eq!(
            builtin_string_free_q(&[string("hello"), string("x")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_free_q(&[string("hello"), string("ell")]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── LetterQ ──

    #[test]
    fn test_letter_q() {
        assert_eq!(builtin_letter_q(&[string("abc")]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_letter_q(&[string("abc123")]).unwrap(), Value::Bool(false));
        assert_eq!(builtin_letter_q(&[string("")]).unwrap(), Value::Bool(false));
    }

    // ── DigitQ ──

    #[test]
    fn test_digit_q() {
        assert_eq!(builtin_digit_q(&[string("123")]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_digit_q(&[string("12a")]).unwrap(), Value::Bool(false));
        assert_eq!(builtin_digit_q(&[string("")]).unwrap(), Value::Bool(false));
    }

    // ── UpperCaseQ ──

    #[test]
    fn test_upper_case_q() {
        assert_eq!(builtin_upper_case_q(&[string("ABC")]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_upper_case_q(&[string("AbC")]).unwrap(), Value::Bool(false));
        // Non-letters are ignored
        assert_eq!(builtin_upper_case_q(&[string("A B C")]).unwrap(), Value::Bool(true));
    }

    // ── LowerCaseQ ──

    #[test]
    fn test_lower_case_q() {
        assert_eq!(builtin_lower_case_q(&[string("abc")]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_lower_case_q(&[string("aBc")]).unwrap(), Value::Bool(false));
        assert_eq!(builtin_lower_case_q(&[string("a b c")]).unwrap(), Value::Bool(true));
    }

    // ── TextWords ──

    #[test]
    fn test_text_words() {
        let result = builtin_text_words(&[string("hello world syma")]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![string("hello"), string("world"), string("syma")])
        );
    }

    #[test]
    fn test_text_words_empty() {
        let result = builtin_text_words(&[string("")]).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_text_words_multi_space() {
        let result = builtin_text_words(&[string("a   b   c")]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![string("a"), string("b"), string("c")])
        );
    }

    // ── CharacterCounts ──

    #[test]
    fn test_character_counts() {
        let result = builtin_character_counts(&[string("abbccc")]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::List(vec![string("a"), int(1)]),
                Value::List(vec![string("b"), int(2)]),
                Value::List(vec![string("c"), int(3)]),
            ])
        );
    }

    #[test]
    fn test_character_counts_empty() {
        let result = builtin_character_counts(&[string("")]).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    // ── Alphabet ──

    #[test]
    fn test_alphabet() {
        let result = builtin_alphabet(&[]).unwrap();
        if let Value::List(letters) = &result {
            assert_eq!(letters.len(), 26);
            assert_eq!(letters[0], string("a"));
            assert_eq!(letters[25], string("z"));
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn test_alphabet_latin() {
        let result = builtin_alphabet(&[string("Latin")]).unwrap();
        if let Value::List(letters) = &result {
            assert_eq!(letters.len(), 26);
        } else {
            panic!("expected List");
        }
    }
}
