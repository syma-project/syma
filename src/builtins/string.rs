use crate::value::{EvalError, Format, Value};
use rug::Integer;

use super::list::resolve_span;

/// Extract a `&str` from a `&Value`, returning TypeError if not a string.
fn str_of(v: &Value) -> Result<&str, EvalError> {
    match v {
        Value::Str(s) => Ok(s.as_str()),
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: v.type_name().to_string(),
        }),
    }
}

/// Extract a single string argument from args at the given index.
fn str_arg(args: &[Value], idx: usize) -> Result<&str, EvalError> {
    match args.get(idx) {
        Some(v) => str_of(v),
        None => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: "<missing>".to_string(),
        }),
    }
}

/// Extract two string arguments.
fn str_args(args: &[Value], a: usize, b: usize) -> Result<(&str, &str), EvalError> {
    Ok((str_arg(args, a)?, str_arg(args, b)?))
}

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
    let s = str_arg(args, 0)?;
    Ok(Value::Integer(Integer::from(s.chars().count() as i64)))
}

pub fn builtin_to_string(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ToString requires exactly 1 argument".to_string(),
        ));
    }
    let formatted = Value::Formatted {
        format: Format::InputForm,
        value: Box::new(args[0].clone()),
    };
    Ok(Value::Str(formatted.to_string()))
}

pub fn builtin_to_expression(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ToExpression requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
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
    let s = str_arg(args, 0)?;
    let delim = if args.len() == 2 {
        str_arg(args, 1)?
    } else {
        " "
    };
    // Splitting on the empty delimiter yields one part per character; Rust's
    // `split("")` otherwise produces spurious empty strings at both ends.
    let parts: Vec<Value> = if delim.is_empty() {
        s.chars().map(|c| Value::Str(c.to_string())).collect()
    } else {
        s.split(delim)
            .map(|part| Value::Str(part.to_string()))
            .collect()
    };
    Ok(Value::List(parts))
}

pub fn builtin_string_replace(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringReplace requires exactly 2 arguments".to_string(),
        ));
    }
    let s = str_arg(args, 0)?.to_string();
    match &args[1] {
        Value::Rule {
            lhs,
            rhs,
            delayed: false,
        } => {
            let old = str_of(lhs.as_ref())?;
            let new = str_of(rhs.as_ref())?;
            Ok(Value::Str(s.replacen(old, new, 1)))
        }
        Value::List(rules) => {
            let mut result = s;
            for rule in rules {
                if let Value::Rule {
                    lhs,
                    rhs,
                    delayed: false,
                } = rule
                    && let Ok(old) = str_of(lhs.as_ref())
                    && let Ok(new) = str_of(rhs.as_ref())
                {
                    result = result.replacen(old, new, 1);
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
    let s = str_arg(args, 0)?;
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    // {m, n} takes an inclusive character span; negative positions count from
    // the end.
    if let Value::List(range) = &args[1] {
        if range.len() != 2 {
            return Err(EvalError::Error(
                "StringTake: range spec must have 2 elements".to_string(),
            ));
        }
        let m = range[0].to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer".to_string(),
            got: range[0].type_name().to_string(),
        })?;
        let n = range[1].to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer".to_string(),
            got: range[1].type_name().to_string(),
        })?;
        let (start, end) = resolve_span(m, n, len);
        return Ok(Value::Str(chars[start..end].iter().collect()));
    }
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    // Positive n takes the first n characters; negative n takes the last |n|.
    let result: String = if n >= 0 {
        chars[..(n as usize).min(len)].iter().collect()
    } else {
        let count = ((-n) as usize).min(len);
        chars[len - count..].iter().collect()
    };
    Ok(Value::Str(result))
}

pub fn builtin_string_drop(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringDrop requires exactly 2 arguments".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    if let Value::List(range) = &args[1] {
        if range.len() != 2 {
            return Err(EvalError::Error(
                "StringDrop: range spec must have 2 elements".to_string(),
            ));
        }
        let m = range[0].to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer".to_string(),
            got: range[0].type_name().to_string(),
        })?;
        let n = range[1].to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer".to_string(),
            got: range[1].type_name().to_string(),
        })?;
        let (start, end) = resolve_span(m, n, len);
        let mut result: String = chars[..start].iter().collect();
        result.extend(chars[end..].iter());
        return Ok(Value::Str(result));
    }
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    // Positive n drops the first n characters; negative n drops the last |n|.
    let result: String = if n >= 0 {
        chars[(n as usize).min(len)..].iter().collect()
    } else {
        let count = ((-n) as usize).min(len);
        chars[..len - count].iter().collect()
    };
    Ok(Value::Str(result))
}

pub fn builtin_string_contains_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringContainsQ requires exactly 2 arguments".to_string(),
        ));
    }
    let (s, sub) = str_args(args, 0, 1)?;
    Ok(Value::Bool(s.contains(sub)))
}

pub fn builtin_string_reverse(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "StringReverse requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    Ok(Value::Str(s.chars().rev().collect()))
}

pub fn builtin_to_upper_case(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ToUpperCase requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    Ok(Value::Str(s.to_uppercase()))
}

pub fn builtin_to_lower_case(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ToLowerCase requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    Ok(Value::Str(s.to_lowercase()))
}

/// Characters["string"] — split string into a list of single-character strings.
pub fn builtin_characters(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Characters requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    Ok(Value::List(
        s.chars().map(|c| Value::Str(c.to_string())).collect(),
    ))
}

/// StringMatchQ["string", "pattern"] — check if string matches a glob pattern.
pub fn builtin_string_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringMatchQ requires exactly 2 arguments".to_string(),
        ));
    }
    let (s, pat) = str_args(args, 0, 1)?;
    Ok(Value::Bool(glob_match(pat, s)))
}

/// Simple glob-style pattern matching (* = any substring, ? = any single char).
pub(crate) fn glob_match(pattern: &str, text: &str) -> bool {
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

/// Number of leading/trailing characters produced by repeating `pad`.
///
/// `padding_prefix` returns the first `count` characters of the infinite
/// repetition of `pad`; `padding_suffix` returns the last `count` characters.
/// An empty `pad` falls back to a single space.
fn padding_prefix(pad: &str, count: usize) -> String {
    let pad = if pad.is_empty() { " " } else { pad };
    let plen = pad.chars().count();
    pad.repeat(count / plen + 1).chars().take(count).collect()
}

fn padding_suffix(pad: &str, count: usize) -> String {
    let pad = if pad.is_empty() { " " } else { pad };
    let plen = pad.chars().count();
    let repeated = pad.repeat(count / plen + 1);
    let chars: Vec<char> = repeated.chars().collect();
    chars[chars.len() - count..].iter().collect()
}

/// StringPadLeft["str", n] or StringPadLeft["str", n, "pad"] — left-pad a string.
pub fn builtin_string_pad_left(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "StringPadLeft requires 2 or 3 arguments".to_string(),
        ));
    }
    let s = str_arg(args, 0)?.to_string();
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    let pad = if args.len() == 3 {
        str_arg(args, 2)?.to_string()
    } else {
        ' '.to_string()
    };
    let width = n.unsigned_abs() as usize;
    let cur = s.chars().count();
    if width <= cur {
        return Ok(Value::Str(s));
    }
    let count = width - cur;
    if n >= 0 {
        let padding = padding_suffix(&pad, count);
        Ok(Value::Str(format!("{}{}", padding, s)))
    } else {
        // Negative width pads on the right.
        let padding = padding_prefix(&pad, count);
        Ok(Value::Str(format!("{}{}", s, padding)))
    }
}

/// StringPadRight["str", n] or StringPadRight["str", n, "pad"] — right-pad a string.
pub fn builtin_string_pad_right(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "StringPadRight requires 2 or 3 arguments".to_string(),
        ));
    }
    let s = str_arg(args, 0)?.to_string();
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    let pad = if args.len() == 3 {
        str_arg(args, 2)?.to_string()
    } else {
        ' '.to_string()
    };
    let width = n.unsigned_abs() as usize;
    let cur = s.chars().count();
    if width <= cur {
        return Ok(Value::Str(s));
    }
    let count = width - cur;
    if n >= 0 {
        let padding = padding_prefix(&pad, count);
        Ok(Value::Str(format!("{}{}", s, padding)))
    } else {
        // Negative width pads on the left.
        let padding = padding_suffix(&pad, count);
        Ok(Value::Str(format!("{}{}", padding, s)))
    }
}

/// StringTrim["str"] — remove leading and trailing whitespace.
pub fn builtin_string_trim(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "StringTrim requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    Ok(Value::Str(s.trim().to_string()))
}

/// StringStartsQ["str", "prefix"] — check if string starts with prefix.
pub fn builtin_string_starts_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringStartsQ requires exactly 2 arguments".to_string(),
        ));
    }
    let (s, p) = str_args(args, 0, 1)?;
    Ok(Value::Bool(s.starts_with(p)))
}

/// StringEndsQ["str", "suffix"] — check if string ends with suffix.
pub fn builtin_string_ends_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringEndsQ requires exactly 2 arguments".to_string(),
        ));
    }
    let (s, p) = str_args(args, 0, 1)?;
    Ok(Value::Bool(s.ends_with(p)))
}

/// StringPart[s, n] — get the nth character (1-indexed). Negative n counts from end.
pub fn builtin_string_part(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringPart requires exactly 2 arguments".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
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
    let (s, sub) = str_args(args, 0, 1)?;
    // match_indices returns byte offsets; convert to char indices
    let positions: Vec<Value> = s
        .match_indices(sub)
        .map(|(byte_pos, _)| {
            // Report character positions, not byte offsets.
            let char_pos = s[..byte_pos].chars().count();
            Value::Integer(Integer::from(char_pos as i64 + 1))
        })
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
    let (s, sub) = str_args(args, 0, 1)?;
    let count = s.match_indices(sub).count();
    Ok(Value::Integer(Integer::from(count as i64)))
}

/// StringRepeat[s, n] — repeat string s n times.
pub fn builtin_string_repeat(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringRepeat requires exactly 2 arguments".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
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
    let (s, sub) = str_args(args, 0, 1)?;
    Ok(Value::Str(s.replace(sub, "")))
}

/// StringInsert[s, ins, n] — insert ins at position n in s (1-indexed). Negative n counts from end.
pub fn builtin_string_insert(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "StringInsert requires exactly 3 arguments: StringInsert[s, ins, n]".to_string(),
        ));
    }
    let s = str_arg(args, 0)?.to_string();
    let ins = str_arg(args, 1)?.to_string();
    let n = args[2].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[2].type_name().to_string(),
    })?;
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;
    let idx = if n >= 1 {
        ((n - 1).min(len)) as usize
    } else if n < 0 {
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
    let items = match &args[0] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let sep = str_arg(args, 1)?;
    let parts: Vec<String> = items
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.clone(),
            other => other.to_string(),
        })
        .collect();
    Ok(Value::Str(parts.join(sep)))
}

/// StringFreeQ[s, sub] — True if s does NOT contain sub.
pub fn builtin_string_free_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringFreeQ requires exactly 2 arguments".to_string(),
        ));
    }
    let (s, sub) = str_args(args, 0, 1)?;
    Ok(Value::Bool(!s.contains(sub)))
}

/// LetterQ[s] — True if s is non-empty and all characters are Unicode letters.
pub fn builtin_letter_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "LetterQ requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    Ok(Value::Bool(
        !s.is_empty() && s.chars().all(|c| c.is_alphabetic()),
    ))
}

/// DigitQ[s] — True if s is non-empty and all characters are ASCII digits.
pub fn builtin_digit_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "DigitQ requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    Ok(Value::Bool(
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()),
    ))
}

/// UpperCaseQ[s] — True if s is non-empty and all letters are uppercase.
/// Non-letter characters are ignored.
pub fn builtin_upper_case_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "UpperCaseQ requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    let letters: Vec<char> = s.chars().filter(|c| c.is_alphabetic()).collect();
    Ok(Value::Bool(
        !letters.is_empty() && letters.iter().all(|c| c.is_uppercase()),
    ))
}

/// LowerCaseQ[s] — True if s is non-empty and all letters are lowercase.
/// Non-letter characters are ignored.
pub fn builtin_lower_case_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "LowerCaseQ requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    let letters: Vec<char> = s.chars().filter(|c| c.is_alphabetic()).collect();
    Ok(Value::Bool(
        !letters.is_empty() && letters.iter().all(|c| c.is_lowercase()),
    ))
}

/// TextWords[s] — split string into a list of words (by whitespace).
pub fn builtin_text_words(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TextWords requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    let words: Vec<Value> = s
        .split_whitespace()
        .map(|w| Value::Str(w.to_string()))
        .collect();
    Ok(Value::List(words))
}

/// CharacterCounts[s] — return a list of {char, count} pairs sorted by character.
pub fn builtin_character_counts(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "CharacterCounts requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
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

// ── New string functions ──

pub fn builtin_to_character_code(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ToCharacterCode requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0).map_err(|_| EvalError::NoMatch {
        head: "ToCharacterCode".to_string(),
        args: args.to_vec().into(),
    })?;
    let codes: Vec<Value> = s
        .chars()
        .map(|c| Value::Integer(Integer::from(c as u32)))
        .collect();
    Ok(Value::List(codes))
}

pub fn builtin_from_character_code(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FromCharacterCode requires exactly 1 argument".to_string(),
        ));
    }
    if let Value::List(codes) = &args[0] {
        let mut s = String::new();
        for code in codes {
            if let Value::Integer(n) = code
                && let Some(val) = n.to_u32()
                && let Some(c) = char::from_u32(val)
            {
                s.push(c);
            }
        }
        return Ok(Value::Str(s));
    }
    Err(EvalError::NoMatch {
        head: "FromCharacterCode".to_string(),
        args: args.to_vec().into(),
    })
}

pub fn builtin_edit_distance(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "EditDistance requires exactly 2 arguments".to_string(),
        ));
    }
    let (s1, s2) = str_args(args, 0, 1).map_err(|_| EvalError::NoMatch {
        head: "EditDistance".to_string(),
        args: args.to_vec().into(),
    })?;
    let a: Vec<char> = s1.chars().collect();
    let b: Vec<char> = s2.chars().collect();
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, v) in dp[0].iter_mut().enumerate() {
        *v = j;
    }
    #[allow(clippy::needless_range_loop)]
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    Ok(Value::Integer(Integer::from(dp[m][n] as i64)))
}

pub fn builtin_longest_common_subsequence(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LongestCommonSubsequence requires exactly 2 arguments".to_string(),
        ));
    }
    let (s1, s2) = str_args(args, 0, 1).map_err(|_| EvalError::NoMatch {
        head: "LongestCommonSubsequence".to_string(),
        args: args.to_vec().into(),
    })?;
    let a: Vec<char> = s1.chars().collect();
    let b: Vec<char> = s2.chars().collect();
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }
    let mut result = String::new();
    let (mut i, mut j) = (m, n);
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            result.push(a[i - 1]);
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    let result: String = result.chars().rev().collect();
    Ok(Value::Str(result))
}

pub fn builtin_longest_common_sub_string(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LongestCommonSubString requires exactly 2 arguments".to_string(),
        ));
    }
    let (s1, s2) = str_args(args, 0, 1).map_err(|_| EvalError::NoMatch {
        head: "LongestCommonSubString".to_string(),
        args: args.to_vec().into(),
    })?;
    let a: Vec<char> = s1.chars().collect();
    let b: Vec<char> = s2.chars().collect();
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    let mut max_len = 0usize;
    let mut max_end = 0usize;
    for i in 1..=m {
        for j in 1..=n {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
                if dp[i][j] > max_len {
                    max_len = dp[i][j];
                    max_end = i;
                }
            }
        }
    }
    if max_len == 0 {
        return Ok(Value::Str(String::new()));
    }
    let result: String = a[max_end - max_len..max_end].iter().collect();
    Ok(Value::Str(result))
}

pub fn builtin_word_count(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "WordCount requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0).map_err(|_| EvalError::NoMatch {
        head: "WordCount".to_string(),
        args: args.to_vec().into(),
    })?;
    let count = s.split_whitespace().count();
    Ok(Value::Integer(Integer::from(count as i64)))
}

/// StringPartition[s, n] — splits string s into substrings of length n.
///
/// The last substring may be shorter if the string length is not a multiple of n.
/// StringPartition["abcde", 2] → {"ab", "cd", "e"}
/// StringPartition["abcde", 3] → {"abc", "de"}
pub fn builtin_string_partition(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringPartition requires exactly 2 arguments".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if n <= 0 {
        return Err(EvalError::Error(
            "StringPartition: partition size must be positive".to_string(),
        ));
    }
    let n = n as usize;
    let chars: Vec<char> = s.chars().collect();
    let result: Vec<Value> = chars
        .chunks(n)
        .map(|chunk| Value::Str(chunk.iter().collect()))
        .collect();
    Ok(Value::List(result))
}

pub fn builtin_sentence_count(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "SentenceCount requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0).map_err(|_| EvalError::NoMatch {
        head: "SentenceCount".to_string(),
        args: args.to_vec().into(),
    })?;
    let sentences: Vec<&str> = s
        .split(['.', '!', '?'])
        .filter(|s| !s.trim().is_empty())
        .collect();
    Ok(Value::Integer(Integer::from(sentences.len() as i64)))
}

// ── StringToByteArray ─────────────────────────────────────────────────────

/// StringToByteArray[s] — convert a string to a list of byte values (UTF-8 encoding).
///
/// StringToByteArray["abc"] → {97, 98, 99}
pub fn builtin_string_to_byte_array(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "StringToByteArray requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(args, 0)?;
    let bytes: Vec<Value> = s
        .bytes()
        .map(|b| Value::Integer(Integer::from(b as i64)))
        .collect();
    Ok(Value::List(bytes))
}

// ── IntegerString ─────────────────────────────────────────────────────────

/// IntegerString[n, base] — convert integer n to its string representation in the given base (2-36).
///
/// IntegerString[255, 16] → "ff"
/// IntegerString[10, 2] → "1010"
pub fn builtin_integer_string(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "IntegerString requires 1 or 2 arguments".to_string(),
        ));
    }
    let n = match &args[0] {
        Value::Integer(n) => n.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let base = if args.len() == 2 {
        match &args[1] {
            Value::Integer(b) if *b >= 2 && *b <= 36 => b.to_u32().unwrap_or(10),
            _ => {
                return Err(EvalError::Error(
                    "IntegerString: base must be an integer between 2 and 36".to_string(),
                ));
            }
        }
    } else {
        10
    };
    if n.is_zero() {
        return Ok(Value::Str("0".to_string()));
    }
    let negative = n.is_negative();
    let mut m = n.clone().abs();
    let base_int = Integer::from(base);
    let mut digits: Vec<char> = Vec::new();
    while m > 0 {
        let remainder_val: Integer = (&m % &base_int).into();
        let remainder = remainder_val.to_u32().unwrap_or(0);
        let c = if remainder < 10 {
            (b'0' + remainder as u8) as char
        } else {
            (b'a' + (remainder - 10) as u8) as char
        };
        digits.push(c);
        m /= &base_int;
    }
    digits.reverse();
    let mut result: String = digits.into_iter().collect();
    if negative {
        result.insert(0, '-');
    }
    Ok(Value::Str(result))
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
        let result = builtin_string_riffle(&[Value::List(vec![string("x")]), string(",")]).unwrap();
        assert_eq!(result, string("x"));
    }

    #[test]
    fn test_string_riffle_empty() {
        let result = builtin_string_riffle(&[Value::List(vec![]), string(",")]).unwrap();
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
        assert_eq!(
            builtin_letter_q(&[string("abc")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_letter_q(&[string("abc123")]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(builtin_letter_q(&[string("")]).unwrap(), Value::Bool(false));
    }

    // ── DigitQ ──

    #[test]
    fn test_digit_q() {
        assert_eq!(
            builtin_digit_q(&[string("123")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_digit_q(&[string("12a")]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(builtin_digit_q(&[string("")]).unwrap(), Value::Bool(false));
    }

    // ── UpperCaseQ ──

    #[test]
    fn test_upper_case_q() {
        assert_eq!(
            builtin_upper_case_q(&[string("ABC")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_upper_case_q(&[string("AbC")]).unwrap(),
            Value::Bool(false)
        );
        // Non-letters are ignored
        assert_eq!(
            builtin_upper_case_q(&[string("A B C")]).unwrap(),
            Value::Bool(true)
        );
    }

    // ── LowerCaseQ ──

    #[test]
    fn test_lower_case_q() {
        assert_eq!(
            builtin_lower_case_q(&[string("abc")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_lower_case_q(&[string("aBc")]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_lower_case_q(&[string("a b c")]).unwrap(),
            Value::Bool(true)
        );
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

    // ── StringSplit ──

    #[test]
    fn test_string_split_basic() {
        let result = builtin_string_split(&[string("a,b,c"), string(",")]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![string("a"), string("b"), string("c")])
        );
    }

    #[test]
    fn test_string_split_no_match() {
        let result = builtin_string_split(&[string("hello"), string(",")]).unwrap();
        assert_eq!(result, Value::List(vec![string("hello")]));
    }

    #[test]
    fn test_string_split_empty() {
        let result = builtin_string_split(&[string(""), string(",")]).unwrap();
        assert_eq!(result, Value::List(vec![string("")]));
    }

    // ── Regression: char-based length / positions on non-ASCII ──

    #[test]
    fn test_string_length_unicode() {
        // "héllo" is 5 characters but 6 bytes.
        assert_eq!(builtin_string_length(&[string("héllo")]).unwrap(), int(5));
    }

    #[test]
    fn test_string_position_unicode() {
        // Byte offsets would report position 10; character position is 4.
        let result = builtin_string_position(&[string("日本語x"), string("x")]).unwrap();
        assert_eq!(result, Value::List(vec![int(4)]));
    }

    // ── Regression: StringPad uses character width and repeated padding ──

    #[test]
    fn test_string_pad_left_multichar() {
        let result = builtin_string_pad_left(&[string("x"), int(5), string("ab")]).unwrap();
        assert_eq!(result, string("ababx"));
    }

    #[test]
    fn test_string_pad_right_multichar() {
        let result = builtin_string_pad_right(&[string("x"), int(5), string("ab")]).unwrap();
        assert_eq!(result, string("xabab"));
    }

    #[test]
    fn test_string_pad_left_unicode_width() {
        // "é" is one character; pad to width 3 using single-char padding.
        let result = builtin_string_pad_left(&[string("é"), int(3)]).unwrap();
        assert_eq!(result, string("  é"));
    }

    #[test]
    fn test_string_pad_negative_pads_opposite_side() {
        // Negative width pads on the opposite side, matching PadLeft/PadRight.
        assert_eq!(
            builtin_string_pad_left(&[string("x"), int(-3)]).unwrap(),
            string("x  ")
        );
        assert_eq!(
            builtin_string_pad_right(&[string("x"), int(-3)]).unwrap(),
            string("  x")
        );
    }

    // ── Regression: empty delimiter splits into characters ──

    #[test]
    fn test_string_split_empty_delim() {
        let result = builtin_string_split(&[string("abc"), string("")]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![string("a"), string("b"), string("c")])
        );
    }

    // ── Regression: StringTake/StringDrop negative + range semantics ──

    #[test]
    fn test_string_take_negative_returns_last_n() {
        assert_eq!(
            builtin_string_take(&[string("hello"), int(-2)]).unwrap(),
            string("lo")
        );
        // |n| larger than the string yields the whole string.
        assert_eq!(
            builtin_string_take(&[string("hi"), int(-5)]).unwrap(),
            string("hi")
        );
    }

    #[test]
    fn test_string_drop_negative_removes_last_n() {
        assert_eq!(
            builtin_string_drop(&[string("hello"), int(-2)]).unwrap(),
            string("hel")
        );
        assert_eq!(
            builtin_string_drop(&[string("hi"), int(-5)]).unwrap(),
            string("")
        );
    }

    #[test]
    fn test_string_take_drop_range() {
        assert_eq!(
            builtin_string_take(&[string("hello"), Value::List(vec![int(2), int(4)])]).unwrap(),
            string("ell")
        );
        assert_eq!(
            builtin_string_drop(&[string("hello"), Value::List(vec![int(2), int(4)])]).unwrap(),
            string("ho")
        );
        assert_eq!(
            builtin_string_take(&[string("hello"), Value::List(vec![int(-2), int(-1)])]).unwrap(),
            string("lo")
        );
    }

    // ── StringPartition tests ──

    #[test]
    fn test_string_partition_even() {
        // StringPartition["abcde", 2] → {"ab", "cd", "e"}
        let result = builtin_string_partition(&[string("abcde"), int(2)]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![string("ab"), string("cd"), string("e")])
        );
    }

    #[test]
    fn test_string_partition_exact() {
        // StringPartition["abcdef", 3] → {"abc", "def"}
        let result = builtin_string_partition(&[string("abcdef"), int(3)]).unwrap();
        assert_eq!(result, Value::List(vec![string("abc"), string("def")]));
    }

    #[test]
    fn test_string_partition_uneven() {
        // StringPartition["abcde", 3] → {"abc", "de"}
        let result = builtin_string_partition(&[string("abcde"), int(3)]).unwrap();
        assert_eq!(result, Value::List(vec![string("abc"), string("de")]));
    }

    #[test]
    fn test_string_partition_size_one() {
        // StringPartition["abc", 1] → {"a", "b", "c"}
        let result = builtin_string_partition(&[string("abc"), int(1)]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![string("a"), string("b"), string("c")])
        );
    }

    #[test]
    fn test_string_partition_size_larger_than_string() {
        // StringPartition["ab", 5] → {"ab"}
        let result = builtin_string_partition(&[string("ab"), int(5)]).unwrap();
        assert_eq!(result, Value::List(vec![string("ab")]));
    }

    #[test]
    fn test_string_partition_empty_string() {
        // StringPartition["", 3] → {}
        let result = builtin_string_partition(&[string(""), int(3)]).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_string_partition_wrong_count() {
        assert!(builtin_string_partition(&[string("abc")]).is_err());
    }

    #[test]
    fn test_string_partition_zero_size() {
        assert!(builtin_string_partition(&[string("abc"), int(0)]).is_err());
    }

    #[test]
    fn test_string_partition_negative_size() {
        assert!(builtin_string_partition(&[string("abc"), int(-1)]).is_err());
    }

    // ── StringToByteArray ───────────────────────────────────────────────

    #[test]
    fn test_string_to_byte_array() {
        // "abc" → {97, 98, 99}
        let result = builtin_string_to_byte_array(&[string("abc")]).unwrap();
        assert_eq!(result, Value::List(vec![int(97), int(98), int(99)]));
    }

    #[test]
    fn test_string_to_byte_array_empty() {
        // "" → {}
        let result = builtin_string_to_byte_array(&[string("")]).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_string_to_byte_array_special_chars() {
        // "A" → {65}
        let result = builtin_string_to_byte_array(&[string("A")]).unwrap();
        assert_eq!(result, Value::List(vec![int(65)]));
    }

    // ── IntegerString ───────────────────────────────────────────────────

    #[test]
    fn test_integer_string_hex() {
        // IntegerString[255, 16] → "ff"
        let result = builtin_integer_string(&[int(255), int(16)]).unwrap();
        assert_eq!(result, string("ff"));
    }

    #[test]
    fn test_integer_string_binary() {
        // IntegerString[10, 2] → "1010"
        let result = builtin_integer_string(&[int(10), int(2)]).unwrap();
        assert_eq!(result, string("1010"));
    }

    #[test]
    fn test_integer_string_decimal() {
        // IntegerString[42] → "42"
        let result = builtin_integer_string(&[int(42)]).unwrap();
        assert_eq!(result, string("42"));
    }

    #[test]
    fn test_integer_string_zero() {
        // IntegerString[0] → "0"
        let result = builtin_integer_string(&[int(0)]).unwrap();
        assert_eq!(result, string("0"));
    }

    #[test]
    fn test_integer_string_negative() {
        // IntegerString[-42] → "-42"
        let result = builtin_integer_string(&[int(-42)]).unwrap();
        assert_eq!(result, string("-42"));
    }

    #[test]
    fn test_integer_string_octal() {
        // IntegerString[8, 8] → "10"
        let result = builtin_integer_string(&[int(8), int(8)]).unwrap();
        assert_eq!(result, string("10"));
    }
}
