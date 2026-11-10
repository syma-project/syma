//! string builtins closing gaps: Capitalize, TitleCase, CharacterRange, Hash, NumberString, StringTemplate, StringReplaceList.
//!
//! Gap-fill module; see the individual builtin docs for semantics.

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::Integer;

/// Extract a `&str` from a `&Value`, returning `None` when it is not a string.
fn get_str(v: &Value) -> Option<&str> {
    match v {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    }
}

/// Extract a `&str` or raise a `TypeError`.
fn str_arg<'a>(v: &'a Value, expected: &str) -> Result<&'a str, EvalError> {
    get_str(v).ok_or_else(|| EvalError::TypeError {
        expected: expected.to_string(),
        got: v.type_name().to_string(),
    })
}

// ── Capitalize / TitleCase ─────────────────────────────────────────────────

/// Capitalize["string"] — uppercase the first character, lowercase the rest.
pub fn builtin_capitalize(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Capitalize requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(&args[0], "String")?;
    let mut chars = s.chars();
    match chars.next() {
        None => Ok(Value::Str(String::new())),
        Some(first) => {
            let mut out: String = first.to_uppercase().collect();
            out.push_str(&chars.as_str().to_lowercase());
            Ok(Value::Str(out))
        }
    }
}

/// TitleCase["string"] — capitalize the first letter of each word, lowercase
/// the remaining letters. Non-letters delimit words.
pub fn builtin_title_case(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TitleCase requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(&args[0], "String")?;
    let mut out = String::with_capacity(s.len());
    let mut at_word_start = true;
    for c in s.chars() {
        if c.is_alphabetic() {
            if at_word_start {
                out.extend(c.to_uppercase());
                at_word_start = false;
            } else {
                out.extend(c.to_lowercase());
            }
        } else {
            out.push(c);
            at_word_start = true;
        }
    }
    Ok(Value::Str(out))
}

// ── CharacterRange ─────────────────────────────────────────────────────────

/// CharacterRange["a", "e"] or CharacterRange[97, 101] — inclusive list of
/// single-character strings between the endpoints (ascending or descending).
pub fn builtin_character_range(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "CharacterRange requires exactly 2 arguments".to_string(),
        ));
    }
    let (start, end) = match (&args[0], &args[1]) {
        (Value::Str(a), Value::Str(b)) => {
            let mut it = a.chars();
            let ca = it.next().filter(|_| it.next().is_none()).ok_or_else(|| {
                EvalError::Error(format!("CharacterRange: '{}' is not a single character", a))
            })?;
            let mut it = b.chars();
            let cb = it.next().filter(|_| it.next().is_none()).ok_or_else(|| {
                EvalError::Error(format!("CharacterRange: '{}' is not a single character", b))
            })?;
            (ca as u32, cb as u32)
        }
        (a, b) => {
            let ca = a.to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "String or Integer".to_string(),
                got: a.type_name().to_string(),
            })?;
            let cb = b.to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "String or Integer".to_string(),
                got: b.type_name().to_string(),
            })?;
            let ca = u32::try_from(ca).map_err(|_| {
                EvalError::Error("CharacterRange: character code out of range".to_string())
            })?;
            let cb = u32::try_from(cb).map_err(|_| {
                EvalError::Error("CharacterRange: character code out of range".to_string())
            })?;
            (ca, cb)
        }
    };
    let mut out = Vec::new();
    if start <= end {
        for c in start..=end {
            out.push(char_value(c)?);
        }
    } else {
        for c in (end..=start).rev() {
            out.push(char_value(c)?);
        }
    }
    Ok(Value::List(out))
}

fn char_value(code: u32) -> Result<Value, EvalError> {
    let c = char::from_u32(code)
        .ok_or_else(|| EvalError::Error(format!("CharacterRange: invalid code point {code}")))?;
    Ok(Value::Str(c.to_string()))
}

// ── Hash ───────────────────────────────────────────────────────────────────

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Stable FNV-1a 64-bit hash of a byte slice.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Hash[expr] or Hash[expr, "type"] — deterministic hash of the expression's
/// printed form. Supported types: "Integer" (default), "String", "Hexadecimal".
pub fn builtin_hash(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Hash requires 1 or 2 arguments".to_string(),
        ));
    }
    let ty = match args.get(1) {
        None => "Integer",
        Some(Value::Str(s)) => s.as_str(),
        Some(other) => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };
    // Mask to 63 bits so the integer result is always non-negative.
    let hash = fnv1a(args[0].to_string().as_bytes()) & 0x7fff_ffff_ffff_ffff;
    match ty {
        "Integer" => Ok(Value::Integer(Integer::from(hash as i64))),
        "String" => Ok(Value::Str(hash.to_string())),
        "Hexadecimal" | "Hex" => Ok(Value::Str(format!("{hash:016x}"))),
        other => Err(EvalError::NoMatch {
            head: format!("Hash[_, \"{other}\"]"),
            args: args.to_vec().into(),
        }),
    }
}

// ── NumberString ───────────────────────────────────────────────────────────

/// NumberString["string"] — leading numeric substring, or "" if none.
pub fn builtin_number_string(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "NumberString requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(&args[0], "String")?;
    Ok(Value::Str(leading_number(s)))
}

/// Scan the longest prefix of `s` that forms a Wolfram-style number literal.
fn leading_number(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let n = chars.len();

    if i < n && (chars[i] == '+' || chars[i] == '-') {
        i += 1;
    }
    let mut total_digits = 0usize;
    while i < n && chars[i].is_ascii_digit() {
        i += 1;
        total_digits += 1;
    }
    // Fractional part
    if i < n && chars[i] == '.' {
        let dot = i;
        i += 1;
        let mut frac_digits = 0usize;
        while i < n && chars[i].is_ascii_digit() {
            i += 1;
            frac_digits += 1;
        }
        total_digits += frac_digits;
        // A bare trailing dot is only valid if the mantissa had digits.
        if frac_digits == 0 && total_digits == 0 {
            i = dot;
        }
    }
    if total_digits == 0 {
        return String::new();
    }
    // Exponent: e/E with optional sign, or *^ with optional sign.
    let exp_start = i;
    let mut j = i;
    let mut has_exp = false;
    if j < n && (chars[j] == 'e' || chars[j] == 'E') {
        j += 1;
        has_exp = true;
    } else if j + 1 < n && chars[j] == '*' && chars[j + 1] == '^' {
        j += 2;
        has_exp = true;
    }
    if has_exp {
        if j < n && (chars[j] == '+' || chars[j] == '-') {
            j += 1;
        }
        let mut exp_digits = 0usize;
        while j < n && chars[j].is_ascii_digit() {
            j += 1;
            exp_digits += 1;
        }
        if exp_digits > 0 {
            i = j;
        } else {
            i = exp_start;
        }
    }
    chars[..i].iter().collect()
}

// ── StringTemplate ─────────────────────────────────────────────────────────

/// StringTemplate["a`1`b"] — return a `PureFunction` that substitutes arguments
/// for `` `1` ``, `` `2` ``, … placeholders. `` `` `` is a literal backtick.
pub fn builtin_string_template(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "StringTemplate requires exactly 1 argument".to_string(),
        ));
    }
    let template = str_arg(&args[0], "String")?;
    let (parts, slot_count, params) = parse_template(template);
    let body = if parts.is_empty() {
        Expr::Str(String::new())
    } else if parts.len() == 1 {
        parts.into_iter().next().unwrap()
    } else {
        Expr::Call {
            head: Box::new(Expr::Symbol("StringJoin".to_string())),
            args: parts,
        }
    };
    Ok(Value::PureFunction {
        body,
        slot_count,
        params,
        env: None,
    })
}

/// Parse a template into body parts and the maximum slot index used.
fn parse_template(t: &str) -> (Vec<Expr>, usize, Vec<String>) {
    let chars: Vec<char> = t.chars().collect();
    let mut parts: Vec<Expr> = Vec::new();
    let mut text = String::new();
    let mut slots = 0usize;
    let mut params: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '`' {
            if i + 1 < chars.len() && chars[i + 1] == '`' {
                text.push('`');
                i += 2;
                continue;
            }
            let mut j = i + 1;
            let mut name = String::new();
            while j < chars.len() && chars[j] != '`' {
                name.push(chars[j]);
                j += 1;
            }
            if !text.is_empty() {
                parts.push(Expr::Str(std::mem::take(&mut text)));
            }
            if let Ok(n) = name.parse::<usize>() {
                if n >= 1 {
                    parts.push(Expr::Slot(Some(n)));
                    slots = slots.max(n);
                } else {
                    parts.push(Expr::Str(name));
                }
            } else if name.is_empty() {
                parts.push(Expr::Str(String::new()));
            } else {
                if !params.contains(&name) {
                    params.push(name.clone());
                }
                parts.push(Expr::Symbol(name));
            }
            i = if j < chars.len() { j + 1 } else { j };
        } else {
            text.push(chars[i]);
            i += 1;
        }
    }
    if !text.is_empty() {
        parts.push(Expr::Str(text));
    }
    (parts, slots, params)
}

// ── StringReplaceList ──────────────────────────────────────────────────────

/// StringReplaceList["string", rule] — one result per match, each replacing
/// exactly one occurrence. With a list of rules, results are concatenated per
/// rule in order.
pub fn builtin_string_replace_list(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StringReplaceList requires exactly 2 arguments".to_string(),
        ));
    }
    let s = str_arg(&args[0], "String")?;
    let mut out = Vec::new();
    match &args[1] {
        Value::List(rules) => {
            for rule in rules {
                apply_replace_list(s, rule, &mut out)?;
            }
        }
        rule => apply_replace_list(s, rule, &mut out)?,
    }
    Ok(Value::List(out))
}

fn apply_replace_list(s: &str, rule: &Value, out: &mut Vec<Value>) -> Result<(), EvalError> {
    let Value::Rule {
        lhs,
        rhs,
        delayed: false,
    } = rule
    else {
        return Err(EvalError::TypeError {
            expected: "Rule".to_string(),
            got: rule.type_name().to_string(),
        });
    };
    let pat = rule_pattern(lhs)?;
    let new = pattern_str(rhs)
        .map(str::to_string)
        .ok_or_else(|| EvalError::TypeError {
            expected: "String".to_string(),
            got: rhs.type_name().to_string(),
        })?;
    let chars: Vec<char> = s.chars().collect();
    for (a, b) in find_glob_matches(&chars, &pat) {
        let mut result: String = chars[..a].iter().collect();
        result.push_str(&new);
        result.extend(chars[b..].iter());
        out.push(Value::Str(result));
    }
    Ok(())
}

/// Convert a rule LHS into a glob pattern (StringExpression concatenates parts).
fn rule_pattern(v: &Value) -> Result<String, EvalError> {
    pattern_to_glob(v).ok_or_else(|| EvalError::TypeError {
        expected: "String or StringExpression".to_string(),
        got: v.type_name().to_string(),
    })
}

/// Extract a string from a string value, tolerating the `Pattern[Str]` wrapping
/// the parser applies to string literals inside rules.
fn pattern_str(v: &Value) -> Option<&str> {
    match v {
        Value::Str(s) => Some(s.as_str()),
        Value::Pattern(Expr::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

/// Convert a string pattern or StringExpression to a glob pattern.
fn pattern_to_glob(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        Value::Pattern(Expr::Str(s)) => Some(s.clone()),
        Value::Call { head, args } if head == "StringExpression" => {
            let mut glob = String::new();
            for a in args {
                glob.push_str(&pattern_to_glob(a)?);
            }
            Some(glob)
        }
        Value::Pattern(Expr::Call { head, args }) if matches!(head.as_ref(), Expr::Symbol(h) if h == "StringExpression") =>
        {
            let mut glob = String::new();
            for a in args {
                glob.push_str(&pattern_to_glob_ast(a)?);
            }
            Some(glob)
        }
        _ => None,
    }
}

/// Like [`pattern_to_glob`] but for AST nodes (used inside `Pattern` wrappers).
fn pattern_to_glob_ast(e: &Expr) -> Option<String> {
    match e {
        Expr::Str(s) => Some(s.clone()),
        Expr::Call { head, args } if matches!(head.as_ref(), Expr::Symbol(h) if h == "StringExpression") =>
        {
            let mut glob = String::new();
            for a in args {
                glob.push_str(&pattern_to_glob_ast(a)?);
            }
            Some(glob)
        }
        _ => None,
    }
}

/// Find non-overlapping leftmost-longest matches of `glob` in `chars` as
/// `(start, end)` character ranges.
fn find_glob_matches(chars: &[char], glob: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let mut best: Option<usize> = None;
        for j in (i + 1..=chars.len()).rev() {
            let sub: String = chars[i..j].iter().collect();
            if super::string::glob_match(glob, &sub) {
                best = Some(j);
                break;
            }
        }
        match best {
            Some(j) => {
                out.push((i, j));
                i = j;
            }
            None => i += 1,
        }
    }
    out
}

// ── StringExpression + pattern-aware overrides ─────────────────────────────

/// StringExpression[parts…] — keep the head symbolic.
pub fn builtin_string_expression(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Call {
        head: "StringExpression".to_string(),
        args: args.to_vec(),
    })
}

// ── Registration ───────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "Capitalize", builtin_capitalize);
    super::register_builtin(env, "TitleCase", builtin_title_case);
    super::register_builtin(env, "CharacterRange", builtin_character_range);
    super::register_builtin(env, "Hash", builtin_hash);
    super::register_builtin(env, "NumberString", builtin_number_string);
    super::register_builtin(env, "StringTemplate", builtin_string_template);
    super::register_builtin(env, "StringReplaceList", builtin_string_replace_list);
    super::register_builtin(env, "StringExpression", builtin_string_expression);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> Value {
        Value::Str(x.to_string())
    }
    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn rule(a: &str, b: &str) -> Value {
        Value::Rule {
            lhs: Box::new(s(a)),
            rhs: Box::new(s(b)),
            delayed: false,
        }
    }
    fn expr(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.to_string(),
            args,
        }
    }
    fn env_with_builtins() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    #[test]
    fn test_registered_in_env() {
        let env = env_with_builtins();
        assert!(env.get("Capitalize").is_some(), "Capitalize not registered");
        assert!(
            matches!(env.get("Hash"), Some(Value::Builtin(..))),
            "Hash not registered as builtin"
        );
    }

    // ── Capitalize / TitleCase ──

    #[test]
    fn test_capitalize() {
        assert_eq!(builtin_capitalize(&[s("abcDEF")]).unwrap(), s("Abcdef"));
        assert_eq!(builtin_capitalize(&[s("hello")]).unwrap(), s("Hello"));
        assert_eq!(builtin_capitalize(&[s("")]).unwrap(), s(""));
    }

    #[test]
    fn test_title_case() {
        assert_eq!(
            builtin_title_case(&[s("hello world")]).unwrap(),
            s("Hello World")
        );
        assert_eq!(
            builtin_title_case(&[s("HELLO wORLD")]).unwrap(),
            s("Hello World")
        );
        assert_eq!(builtin_title_case(&[s("a-b c")]).unwrap(), s("A-B C"));
    }

    // ── CharacterRange ──

    #[test]
    fn test_character_range_chars() {
        assert_eq!(
            builtin_character_range(&[s("a"), s("e")]).unwrap(),
            Value::List(vec![s("a"), s("b"), s("c"), s("d"), s("e")])
        );
    }

    #[test]
    fn test_character_range_codes() {
        assert_eq!(
            builtin_character_range(&[int(97), int(99)]).unwrap(),
            Value::List(vec![s("a"), s("b"), s("c")])
        );
    }

    #[test]
    fn test_character_range_descending() {
        assert_eq!(
            builtin_character_range(&[s("c"), s("a")]).unwrap(),
            Value::List(vec![s("c"), s("b"), s("a")])
        );
    }

    // ── Hash ──

    #[test]
    fn test_hash_deterministic() {
        assert_eq!(
            builtin_hash(&[int(1)]).unwrap(),
            builtin_hash(&[int(1)]).unwrap()
        );
        assert_ne!(
            builtin_hash(&[int(1)]).unwrap(),
            builtin_hash(&[int(2)]).unwrap()
        );
    }

    #[test]
    fn test_hash_known_value() {
        // FNV-1a of "1" masked to 63 bits, for stability of the algorithm.
        assert_eq!(
            builtin_hash(&[int(1)]).unwrap(),
            Value::Integer(Integer::from(3414762387142712060i64))
        );
    }

    #[test]
    fn test_hash_types() {
        assert_eq!(
            builtin_hash(&[int(1), s("String")]).unwrap(),
            s("3414762387142712060")
        );
        assert_eq!(
            builtin_hash(&[int(1), s("Hexadecimal")]).unwrap(),
            s("2f63ac4c86019afc")
        );
    }

    // ── NumberString ──

    #[test]
    fn test_number_string() {
        assert_eq!(builtin_number_string(&[s("123abc")]).unwrap(), s("123"));
        assert_eq!(
            builtin_number_string(&[s("-1.5e3xyz")]).unwrap(),
            s("-1.5e3")
        );
        assert_eq!(builtin_number_string(&[s("abc")]).unwrap(), s(""));
        assert_eq!(builtin_number_string(&[s("3.14")]).unwrap(), s("3.14"));
    }

    // ── StringTemplate ──

    #[test]
    fn test_string_template() {
        let env = env_with_builtins();
        let pf = builtin_string_template(&[s("a`1`b")]).unwrap();
        let out = crate::eval::apply_function(&pf, &[s("x")], &env).unwrap();
        assert_eq!(out, s("axb"));
    }

    #[test]
    fn test_string_template_multiple_slots() {
        let env = env_with_builtins();
        let pf = builtin_string_template(&[s("`1`-`2`")]).unwrap();
        let out = crate::eval::apply_function(&pf, &[s("a"), s("b")], &env).unwrap();
        assert_eq!(out, s("a-b"));
    }

    #[test]
    fn test_string_template_escaped_backtick() {
        let env = env_with_builtins();
        let pf = builtin_string_template(&[s("a``b")]).unwrap();
        let out = crate::eval::apply_function(&pf, &[], &env).unwrap();
        assert_eq!(out, s("a`b"));
    }

    #[test]
    fn test_string_template_non_string_arg() {
        let env = env_with_builtins();
        let pf = builtin_string_template(&[s("v=`1`")]).unwrap();
        let out = crate::eval::apply_function(&pf, &[int(5)], &env).unwrap();
        assert_eq!(out, s("v=5"));
    }

    // ── StringReplaceList ──

    #[test]
    fn test_string_replace_list() {
        let out = builtin_string_replace_list(&[s("abab"), rule("a", "X")]).unwrap();
        assert_eq!(out, Value::List(vec![s("Xbab"), s("abXb")]));
    }

    #[test]
    fn test_string_replace_list_no_match() {
        let out = builtin_string_replace_list(&[s("abc"), rule("z", "X")]).unwrap();
        assert_eq!(out, Value::List(vec![]));
    }

    #[test]
    fn test_string_replace_list_wrapped_rule() {
        // The parser wraps string literals inside rules as Pattern[Str].
        let wrapped = Value::Rule {
            lhs: Box::new(Value::Pattern(Expr::Str("a".to_string()))),
            rhs: Box::new(Value::Pattern(Expr::Str("X".to_string()))),
            delayed: false,
        };
        let out = builtin_string_replace_list(&[s("abab"), wrapped]).unwrap();
        assert_eq!(out, Value::List(vec![s("Xbab"), s("abXb")]));
    }

    #[test]
    fn test_string_replace_list_rules() {
        let out = builtin_string_replace_list(&[
            s("ab"),
            Value::List(vec![rule("a", "1"), rule("b", "2")]),
        ])
        .unwrap();
        assert_eq!(out, Value::List(vec![s("1b"), s("a2")]));
    }

    // ── StringExpression ──

    #[test]
    fn test_string_expression_head() {
        let out = builtin_string_expression(&[s("a"), s("b")]).unwrap();
        assert_eq!(out, expr("StringExpression", vec![s("a"), s("b")]));
    }

    #[test]
    fn test_string_expression_string_match_q() {
        // `stringpatterns` owns StringMatchQ and understands StringExpression.
        let pat = expr("StringExpression", vec![s("a"), s("b")]);
        assert_eq!(
            crate::builtins::stringpatterns::builtin_string_match_q(&[s("ab"), pat.clone()])
                .unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            crate::builtins::stringpatterns::builtin_string_match_q(&[s("ac"), pat]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_string_expression_string_cases() {
        let pat = expr("StringExpression", vec![s("a"), s("b")]);
        let out =
            crate::builtins::stringpatterns::builtin_string_cases(&[s("abcabc"), pat]).unwrap();
        assert_eq!(out, Value::List(vec![s("ab"), s("ab")]));
    }
}
