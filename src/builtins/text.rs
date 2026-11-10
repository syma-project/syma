//! string builtins closing gaps: Capitalize, TitleCase, CharacterRange, Hash, NumberString, StringTemplate, StringReplaceList.
//!
//! Gap-fill module; see the individual builtin docs for semantics.

use crate::ast::Expr;
use crate::env::Env;
use crate::eval::apply_function;
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

/// Decapitalize["string"] — lowercase the first character, leave the rest as-is.
pub fn builtin_decapitalize(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Decapitalize requires exactly 1 argument".to_string(),
        ));
    }
    let s = str_arg(&args[0], "String")?;
    let mut chars = s.chars();
    match chars.next() {
        None => Ok(Value::Str(String::new())),
        Some(first) => {
            let mut out: String = first.to_lowercase().collect();
            out.push_str(chars.as_str());
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

// ── Edit distances ─────────────────────────────────────────────────────────

/// Units of a string or list, so HammingDistance handles both argument shapes.
fn sequence_units(v: &Value) -> Option<Vec<Value>> {
    match v {
        Value::Str(s) => Some(s.chars().map(|c| Value::Str(c.to_string())).collect()),
        Value::List(items) => Some(items.clone()),
        _ => None,
    }
}

/// HammingDistance[s1, s2] — number of positions at which the two strings (or
/// lists) differ. The arguments must have equal length; otherwise the result is
/// left unevaluated, matching Wolfram Language.
pub fn builtin_hamming_distance(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "HammingDistance requires exactly 2 arguments".to_string(),
        ));
    }
    let (a, b) = match (sequence_units(&args[0]), sequence_units(&args[1])) {
        (Some(a), Some(b)) => (a, b),
        _ => return Ok(unevaluated("HammingDistance", args)),
    };
    if a.len() != b.len() {
        return Ok(unevaluated("HammingDistance", args));
    }
    let mut dist = 0i64;
    for (x, y) in a.iter().zip(b.iter()) {
        if !x.struct_eq(y) {
            dist += 1;
        }
    }
    Ok(Value::Integer(Integer::from(dist)))
}

/// DamerauLevenshteinDistance[s1, s2] — the minimum number of insertions,
/// deletions, substitutions and transpositions of adjacent characters needed
/// to transform `s1` into `s2` (optimal string alignment distance).
pub fn builtin_damerau_levenshtein_distance(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "DamerauLevenshteinDistance requires exactly 2 arguments".to_string(),
        ));
    }
    let s1 = str_arg(&args[0], "String")?;
    let s2 = str_arg(&args[1], "String")?;
    let a: Vec<char> = s1.chars().collect();
    let b: Vec<char> = s2.chars().collect();
    let m = a.len();
    let n = b.len();
    // True Damerau–Levenshtein distance (with an alphabet table so that
    // arbitrary transpositions of adjacent characters cost 1).
    let mut da: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
    let max = m + n;
    let mut h = vec![vec![0usize; n + 2]; m + 2];
    h[0][0] = max;
    for i in 0..=m {
        h[i + 1][0] = max;
        h[i + 1][1] = i;
    }
    for j in 0..=n {
        h[0][j + 1] = max;
        h[1][j + 1] = j;
    }
    for i in 1..=m {
        let mut db = 0usize;
        for j in 1..=n {
            let i1 = *da.get(&b[j - 1]).unwrap_or(&0);
            let j1 = db;
            let (cost, new_db) = if a[i - 1] == b[j - 1] {
                (0, j)
            } else {
                (1, db)
            };
            db = new_db;
            h[i + 1][j + 1] = (h[i][j] + cost).min(h[i + 1][j] + 1).min(h[i][j + 1] + 1);
            if i1 > 0 && j1 > 0 {
                h[i + 1][j + 1] = h[i + 1][j + 1].min(h[i1][j1] + (i - i1 - 1) + 1 + (j - j1 - 1));
            }
        }
        da.insert(a[i - 1], i);
    }
    Ok(Value::Integer(Integer::from(h[m + 1][n + 1] as i64)))
}

/// LongestCommonSubstring[s1, s2] — the longest contiguous substring common to
/// both strings, under its Wolfram-canonical name.
pub fn builtin_longest_common_substring(args: &[Value]) -> Result<Value, EvalError> {
    crate::builtins::string::builtin_longest_common_sub_string(args)
}

/// SequenceAlignment[s1, s2] — a global alignment of two strings, given as a
/// list of successive matching runs (plain strings) and differing runs, where a
/// differing run is a pair `{u1, u2}` (with `""` used for a gap).
pub fn builtin_sequence_alignment(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "SequenceAlignment requires exactly 2 arguments".to_string(),
        ));
    }
    let s1 = str_arg(&args[0], "String")?;
    let s2 = str_arg(&args[1], "String")?;
    let a: Vec<char> = s1.chars().collect();
    let b: Vec<char> = s2.chars().collect();
    let m = a.len();
    let n = b.len();
    // Longest-common-subsequence dynamic program.
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }
    // Backtrack into aligned columns; `None` marks a gap.
    let (mut i, mut j) = (m, n);
    let mut left: Vec<Option<char>> = Vec::new();
    let mut right: Vec<Option<char>> = Vec::new();
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            left.push(Some(a[i - 1]));
            right.push(Some(b[j - 1]));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            left.push(Some(a[i - 1]));
            right.push(None);
            i -= 1;
        } else {
            left.push(None);
            right.push(Some(b[j - 1]));
            j -= 1;
        }
    }
    while i > 0 {
        left.push(Some(a[i - 1]));
        right.push(None);
        i -= 1;
    }
    while j > 0 {
        left.push(None);
        right.push(Some(b[j - 1]));
        j -= 1;
    }
    left.reverse();
    right.reverse();
    // Group aligned columns into matching runs and merged differing pairs.
    let mut out: Vec<Value> = Vec::new();
    let mut run = String::new();
    let mut diff_l = String::new();
    let mut diff_r = String::new();
    fn flush_run(run: &mut String, out: &mut Vec<Value>) {
        if !run.is_empty() {
            out.push(Value::Str(std::mem::take(run)));
        }
    }
    for (x, y) in left.iter().zip(right.iter()) {
        match (x, y) {
            (Some(cx), Some(cy)) if cx == cy => {
                if !diff_l.is_empty() || !diff_r.is_empty() {
                    out.push(Value::List(vec![
                        Value::Str(std::mem::take(&mut diff_l)),
                        Value::Str(std::mem::take(&mut diff_r)),
                    ]));
                }
                run.push(*cx);
            }
            _ => {
                flush_run(&mut run, &mut out);
                if let Some(c) = x {
                    diff_l.push(*c);
                }
                if let Some(c) = y {
                    diff_r.push(*c);
                }
            }
        }
    }
    flush_run(&mut run, &mut out);
    if !diff_l.is_empty() || !diff_r.is_empty() {
        out.push(Value::List(vec![Value::Str(diff_l), Value::Str(diff_r)]));
    }
    Ok(Value::List(out))
}

// ── TemplateApply ──────────────────────────────────────────────────────────

/// TemplateApply[template, args] — substitute into a `` ` ``-template.
///
/// `template` is either a template string or a `PureFunction` produced by
/// `StringTemplate`; `args` is a list (positional), an association (named) or a
/// single value. The result is always a string.
pub fn builtin_template_apply(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "TemplateApply requires exactly 2 arguments".to_string(),
        ));
    }
    let pf = match &args[0] {
        Value::PureFunction { .. } => args[0].clone(),
        Value::Str(_) => builtin_string_template(std::slice::from_ref(&args[0]))?,
        other => {
            return Err(EvalError::TypeError {
                expected: "String or PureFunction".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };
    let params = match &pf {
        Value::PureFunction { params, .. } => params.clone(),
        _ => Vec::new(),
    };
    let call_args: Vec<Value> = match &args[1] {
        Value::List(items) => items.clone(),
        Value::Assoc(map) => params
            .iter()
            .map(|p| map.get(p).cloned().unwrap_or(Value::Null))
            .collect(),
        other => vec![other.clone()],
    };
    let result = apply_function(&pf, &call_args, env)?;
    Ok(match result {
        Value::Str(_) => result,
        other => Value::Str(other.to_string()),
    })
}

/// Build an unevaluated `head[args…]` call.
fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

// ── Registration ───────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "Capitalize", builtin_capitalize);
    super::register_builtin(env, "Decapitalize", builtin_decapitalize);
    super::register_builtin(env, "TitleCase", builtin_title_case);
    super::register_builtin(env, "CharacterRange", builtin_character_range);
    super::register_builtin(env, "Hash", builtin_hash);
    super::register_builtin(env, "NumberString", builtin_number_string);
    super::register_builtin(env, "StringTemplate", builtin_string_template);
    super::register_builtin(env, "StringReplaceList", builtin_string_replace_list);
    super::register_builtin(env, "StringExpression", builtin_string_expression);
    super::register_builtin(env, "HammingDistance", builtin_hamming_distance);
    super::register_builtin(
        env,
        "DamerauLevenshteinDistance",
        builtin_damerau_levenshtein_distance,
    );
    super::register_builtin(
        env,
        "LongestCommonSubstring",
        builtin_longest_common_substring,
    );
    super::register_builtin(env, "SequenceAlignment", builtin_sequence_alignment);
    super::register_builtin_env(env, "TemplateApply", builtin_template_apply);
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

    // ── Tests via the evaluator (covers registration + parsing) ──

    fn eval_str(input: &str) -> Value {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let tokens = crate::lexer::tokenize(input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        crate::eval::eval_program(&ast, &env).unwrap()
    }

    #[test]
    fn test_new_text_builtins_registered() {
        let env = env_with_builtins();
        for name in [
            "Decapitalize",
            "HammingDistance",
            "DamerauLevenshteinDistance",
            "LongestCommonSubstring",
            "SequenceAlignment",
            "TemplateApply",
        ] {
            assert!(
                matches!(env.get(name), Some(Value::Builtin(..))),
                "{name} not registered as builtin"
            );
        }
    }

    // ── Decapitalize ──

    #[test]
    fn test_decapitalize() {
        assert_eq!(builtin_decapitalize(&[s("ABC")]).unwrap(), s("aBC"));
        assert_eq!(builtin_decapitalize(&[s("abc")]).unwrap(), s("abc"));
        assert_eq!(builtin_decapitalize(&[s("")]).unwrap(), s(""));
        assert_eq!(eval_str("Decapitalize[\"HELLO\"]"), s("hELLO"));
    }

    // ── Characters ──

    #[test]
    fn test_characters() {
        assert_eq!(
            eval_str("Characters[\"abc\"]"),
            Value::List(vec![s("a"), s("b"), s("c")])
        );
        assert_eq!(eval_str("Characters[\"\"]"), Value::List(vec![]));
    }

    // ── ToCharacterCode / FromCharacterCode ──

    #[test]
    fn test_character_code_roundtrip() {
        assert_eq!(
            eval_str("ToCharacterCode[\"ab\"]"),
            Value::List(vec![int(97), int(98)])
        );
        assert_eq!(eval_str("FromCharacterCode[{97, 98}]"), s("ab"));
    }

    // ── IntegerString / FromDigits string forms ──

    #[test]
    fn test_integer_string_and_from_digits_string() {
        assert_eq!(eval_str("IntegerString[255, 16]"), s("ff"));
        assert_eq!(eval_str("IntegerString[10, 2]"), s("1010"));
        assert_eq!(eval_str("IntegerString[0]"), s("0"));
        assert_eq!(eval_str("FromDigits[\"ff\", 16]"), int(255));
        assert_eq!(eval_str("FromDigits[\"1010\", 2]"), int(10));
        assert_eq!(eval_str("FromDigits[\"-123\"]"), int(-123));
    }

    // ── StringRiffle / padding / repeat / alphabet ──

    #[test]
    fn test_string_riffle() {
        assert_eq!(eval_str("StringRiffle[{\"a\", \"b\", \"c\"}]"), s("a b c"));
        assert_eq!(
            eval_str("StringRiffle[{\"a\", \"b\", \"c\"}, \"-\"]"),
            s("a-b-c")
        );
    }

    #[test]
    fn test_string_pad() {
        assert_eq!(eval_str("StringPadLeft[\"ab\", 5]"), s("   ab"));
        assert_eq!(eval_str("StringPadRight[\"ab\", 5]"), s("ab   "));
        assert_eq!(eval_str("StringPadLeft[\"ab\", 6, \"xy\"]"), s("xyxyab"));
        assert_eq!(eval_str("StringPadRight[\"x\", 4, \"-\"]"), s("x---"));
    }

    #[test]
    fn test_string_repeat() {
        assert_eq!(eval_str("StringRepeat[\"ab\", 3]"), s("ababab"));
        assert_eq!(eval_str("StringRepeat[\"a\", 0]"), s(""));
    }

    #[test]
    fn test_alphabet() {
        let out = eval_str("Alphabet[]");
        let Value::List(letters) = out else {
            panic!("Alphabet[] should be a list");
        };
        assert_eq!(letters.len(), 26);
        assert_eq!(letters[0], s("a"));
        assert_eq!(letters[25], s("z"));
    }

    #[test]
    fn test_to_title_case_alias() {
        assert_eq!(eval_str("ToTitleCase[\"hello world\"]"), s("Hello World"));
    }

    // ── Edit distances ──

    #[test]
    fn test_edit_distance() {
        assert_eq!(eval_str("EditDistance[\"kitten\", \"sitting\"]"), int(3));
        assert_eq!(eval_str("EditDistance[\"abc\", \"abc\"]"), int(0));
    }

    #[test]
    fn test_hamming_distance() {
        assert_eq!(eval_str("HammingDistance[\"abc\", \"abd\"]"), int(1));
        assert_eq!(eval_str("HammingDistance[\"abc\", \"abc\"]"), int(0));
        assert_eq!(
            eval_str("HammingDistance[\"bccbbcb\", \"ccbbccb\"]"),
            int(3)
        );
        // Unequal lengths stay unevaluated.
        assert_eq!(
            eval_str("HammingDistance[\"abc\", \"ab\"]"),
            Value::Call {
                head: "HammingDistance".to_string(),
                args: vec![s("abc"), s("ab")],
            }
        );
        // Lists work too.
        assert_eq!(eval_str("HammingDistance[{1, 2, 3}, {1, 5, 3}]"), int(1));
    }

    #[test]
    fn test_damerau_levenshtein_distance() {
        // A transposition costs 1, unlike plain LevenshteinDistance.
        assert_eq!(
            eval_str("DamerauLevenshteinDistance[\"ab\", \"ba\"]"),
            int(1)
        );
        assert_eq!(
            eval_str("DamerauLevenshteinDistance[\"ca\", \"abc\"]"),
            int(2)
        );
        assert_eq!(
            eval_str("DamerauLevenshteinDistance[\"abc\", \"abc\"]"),
            int(0)
        );
    }

    // ── Longest common subsequence / substring ──

    #[test]
    fn test_longest_common_subsequence() {
        assert_eq!(
            eval_str("LongestCommonSubsequence[\"abcdef\", \"acf\"]"),
            s("acf")
        );
    }

    #[test]
    fn test_longest_common_substring() {
        assert_eq!(
            eval_str("LongestCommonSubstring[\"abcdef\", \"xbcdy\"]"),
            s("bcd")
        );
        assert_eq!(eval_str("LongestCommonSubstring[\"abc\", \"xyz\"]"), s(""));
    }

    // ── SequenceAlignment ──

    #[test]
    fn test_sequence_alignment_identical() {
        assert_eq!(
            eval_str("SequenceAlignment[\"abc\", \"abc\"]"),
            Value::List(vec![s("abc")])
        );
    }

    #[test]
    fn test_sequence_alignment_substitution() {
        assert_eq!(
            eval_str("SequenceAlignment[\"abcXabcXabc\", \"abcYabcYabc\"]"),
            Value::List(vec![
                s("abc"),
                Value::List(vec![s("X"), s("Y")]),
                s("abc"),
                Value::List(vec![s("X"), s("Y")]),
                s("abc"),
            ])
        );
    }

    #[test]
    fn test_sequence_alignment_gap() {
        // A gap is represented by an empty string on the gapped side.
        let out = eval_str("SequenceAlignment[\"ac\", \"abcd\"]");
        let Value::List(segs) = out else {
            panic!("SequenceAlignment should yield a list");
        };
        // Every segment is either a matching string or a differing pair.
        for seg in &segs {
            match seg {
                Value::Str(_) => {}
                Value::List(pair) => assert_eq!(pair.len(), 2),
                other => panic!("unexpected segment {other:?}"),
            }
        }
        assert_eq!(segs.first(), Some(&s("a")));
    }

    // ── StringTemplate / TemplateApply ──

    #[test]
    fn test_template_apply_positional() {
        assert_eq!(
            eval_str("TemplateApply[\"`1` + `2`\", {\"a\", \"b\"}]"),
            s("a + b")
        );
        assert_eq!(eval_str("TemplateApply[\"v=`1`\", 5]"), s("v=5"));
    }

    #[test]
    fn test_template_apply_named() {
        assert_eq!(
            eval_str("TemplateApply[\"`x`-`y`\", <|\"x\" -> \"a\", \"y\" -> \"b\"|>]"),
            s("a-b")
        );
    }

    #[test]
    fn test_template_apply_from_string_template() {
        assert_eq!(
            eval_str("TemplateApply[StringTemplate[\"[`1`]\"], {\"go\"}]"),
            s("[go]")
        );
    }
}
