/// Pattern matching engine for Syma language.
///
/// Matches values against patterns, binding variables as needed.
/// Supports:
/// - Blanks: _, x_, _Integer, x_Integer
/// - Sequences: __, x__, ___, x___
/// - List patterns: {x_, y_}
/// - Literal patterns: 0, "hello", True
/// - Compound patterns: x_^n_, a_ + b_
/// - Guards: pattern /; condition
/// - Alternatives: (pat1 | pat2)

use std::collections::HashMap;

use rug::Float;

use crate::ast::Expr;
use crate::value::Value;

/// A set of variable bindings produced by pattern matching.
pub type Bindings = HashMap<String, Value>;

/// Result of a pattern match attempt.
#[derive(Debug)]
pub enum MatchResult {
    /// Match succeeded with bindings.
    Match(Bindings),
    /// Match failed.
    NoMatch,
}

/// Try to match a value against a pattern.
///
/// Returns Match(bindings) on success, NoMatch on failure.
pub fn match_pattern(pattern: &Expr, value: &Value) -> MatchResult {
    match pattern {
        // ── Blank patterns ──
        Expr::Blank { type_constraint } => {
            if let Some(tc) = type_constraint {
                if value.matches_type(tc) {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else {
                MatchResult::Match(HashMap::new())
            }
        }

        Expr::NamedBlank { name, type_constraint } => {
            if let Some(tc) = type_constraint {
                if value.matches_type(tc) {
                    let mut bindings = HashMap::new();
                    bindings.insert(name.clone(), value.clone());
                    MatchResult::Match(bindings)
                } else {
                    MatchResult::NoMatch
                }
            } else {
                let mut bindings = HashMap::new();
                bindings.insert(name.clone(), value.clone());
                MatchResult::Match(bindings)
            }
        }

        // ── Literal patterns ──
        Expr::Integer(n) => {
            if let Value::Integer(v) = value {
                if n == v {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else {
                MatchResult::NoMatch
            }
        }

        Expr::Real(r) => {
            if let Value::Real(v) = value {
                if r == v {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else if let Value::Integer(n) = value {
                let n_f = Float::with_val(crate::value::DEFAULT_PRECISION, n);
                if *r == n_f {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else {
                MatchResult::NoMatch
            }
        }

        Expr::Bool(b) => {
            if let Value::Bool(v) = value {
                if b == v {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else {
                MatchResult::NoMatch
            }
        }

        Expr::Str(s) => {
            if let Value::Str(v) = value {
                if s == v {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else {
                MatchResult::NoMatch
            }
        }

        Expr::Null => {
            if matches!(value, Value::Null) {
                MatchResult::Match(HashMap::new())
            } else {
                MatchResult::NoMatch
            }
        }

        // ── Symbol (matches exact symbol or named blank) ──
        Expr::Symbol(s) => {
            // Handle x_ as a named blank pattern (parsed as Symbol("x_"))
            if s.ends_with('_') && s.len() > 1 {
                let name = &s[..s.len() - 1];
                let mut bindings = HashMap::new();
                bindings.insert(name.to_string(), value.clone());
                MatchResult::Match(bindings)
            } else if let Value::Symbol(v) = value {
                if s == v {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else {
                MatchResult::NoMatch
            }
        }

        // ── List pattern ──
        Expr::List(patterns) => {
            if let Value::List(items) = value {
                match_list_pattern(patterns, items)
            } else {
                MatchResult::NoMatch
            }
        }

        // ── Guard pattern: pattern /; condition ──
        Expr::PatternGuard { pattern, condition: _ } => {
            // For now, just match the inner pattern.
            // Guard evaluation requires the evaluator, so it's handled
            // in the evaluator's dispatch logic.
            match_pattern(pattern, value)
        }

        // ── Call pattern: f[x_, y_], negated -expr, and alternatives (pat1 | pat2) ──
        Expr::Call { head, args } => {
            // Check for negated pattern: Times[-1, expr]
            if matches!(head.as_ref(), Expr::Symbol(s) if s == "Times")
                && args.len() == 2
                && matches!(&args[0], Expr::Integer(n) if *n == -1)
            {
                if let Value::Call { head: h, args: a } = value {
                    if h == "Times" && a.len() == 2 {
                        if let Value::Integer(n) = &a[0] {
                            if *n == -1 {
                                return match_pattern(&args[1], &a[1]);
                            }
                        }
                    }
                }
                return MatchResult::NoMatch;
            }

            // Check for alternatives: Alternatives[pat1, pat2, ...]
            if matches!(head.as_ref(), Expr::Symbol(s) if s == "Alternatives") {
                for alt in args {
                    if let MatchResult::Match(bindings) = match_pattern(alt, value) {
                        return MatchResult::Match(bindings);
                    }
                }
                return MatchResult::NoMatch;
            }

            // Regular call pattern: f[x_, y_]
            match_call_pattern(head, args, value)
        }

        // ── Rule patterns ──
        Expr::Rule { lhs, rhs } => {
            if let Value::Rule { lhs: vl, rhs: vr, delayed } = value {
                if !delayed {
                    let left_match = match_pattern(lhs, vl);
                    if let MatchResult::Match(mut bindings) = left_match {
                        if let MatchResult::Match(right_bindings) = match_pattern(rhs, vr) {
                            bindings.extend(right_bindings);
                            return MatchResult::Match(bindings);
                        }
                    }
                }
                MatchResult::NoMatch
            } else {
                MatchResult::NoMatch
            }
        }

        Expr::RuleDelayed { lhs, rhs } => {
            if let Value::Rule { lhs: vl, rhs: vr, delayed } = value {
                if *delayed {
                    let left_match = match_pattern(lhs, vl);
                    if let MatchResult::Match(mut bindings) = left_match {
                        if let MatchResult::Match(right_bindings) = match_pattern(rhs, vr) {
                            bindings.extend(right_bindings);
                            return MatchResult::Match(bindings);
                        }
                    }
                }
                MatchResult::NoMatch
            } else {
                MatchResult::NoMatch
            }
        }

        // ── Default: no match ──
        _ => MatchResult::NoMatch,
    }
}

/// Match a list pattern against a list value.
fn match_list_pattern(patterns: &[Expr], items: &[Value]) -> MatchResult {
    let mut bindings = HashMap::new();
    let mut pat_idx = 0;
    let mut val_idx = 0;

    // First pass: handle non-sequence patterns
    while pat_idx < patterns.len() && val_idx < items.len() {
        match &patterns[pat_idx] {
            // Sequence blank: matches 1+ remaining elements
            Expr::BlankSequence { name, type_constraint: _ } => {
                let remaining = items.len() - val_idx;
                if remaining == 0 {
                    return MatchResult::NoMatch; // __ needs at least 1
                }
                // For simplicity, match all remaining elements
                // TODO: backtrack for trailing patterns
                let seq = Value::List(items[val_idx..].to_vec());
                if let Some(n) = name {
                    bindings.insert(n.clone(), seq);
                }
                val_idx = items.len();
                pat_idx += 1;
            }

            // Optional sequence blank: matches 0+ remaining elements
            Expr::BlankNullSequence { name, type_constraint: _ } => {
                let seq = Value::List(items[val_idx..].to_vec());
                if let Some(n) = name {
                    bindings.insert(n.clone(), seq);
                }
                val_idx = items.len();
                pat_idx += 1;
            }

            // Regular pattern: match one element
            _ => {
                if val_idx >= items.len() {
                    return MatchResult::NoMatch;
                }
                match match_pattern(&patterns[pat_idx], &items[val_idx]) {
                    MatchResult::Match(b) => {
                        bindings.extend(b);
                        pat_idx += 1;
                        val_idx += 1;
                    }
                    MatchResult::NoMatch => return MatchResult::NoMatch,
                }
            }
        }
    }

    // Check if all patterns and values were consumed
    if pat_idx == patterns.len() && val_idx == items.len() {
        MatchResult::Match(bindings)
    } else {
        MatchResult::NoMatch
    }
}

/// Match a call pattern (e.g., f[x_, y_]) against a value.
fn match_call_pattern(head: &Expr, args: &[Expr], value: &Value) -> MatchResult {
    match value {
        Value::Call { head: vhead, args: vargs } => {
            // Match head
            let head_match = match head {
                Expr::Symbol(s) => {
                    if s == vhead {
                        MatchResult::Match(HashMap::new())
                    } else {
                        MatchResult::NoMatch
                    }
                }
                _ => MatchResult::NoMatch,
            };

            if let MatchResult::Match(mut bindings) = head_match {
                // Match args
                if args.len() != vargs.len() {
                    return MatchResult::NoMatch;
                }
                for (pat, val) in args.iter().zip(vargs.iter()) {
                    match match_pattern(pat, val) {
                        MatchResult::Match(b) => bindings.extend(b),
                        MatchResult::NoMatch => return MatchResult::NoMatch,
                    }
                }
                MatchResult::Match(bindings)
            } else {
                MatchResult::NoMatch
            }
        }
        _ => MatchResult::NoMatch,
    }
}

/// Try to match a value against a list of patterns and return the first match.
///
/// Returns (index, bindings) for the first matching pattern, or None.
#[allow(dead_code)]
pub fn match_first(patterns: &[Expr], value: &Value) -> Option<(usize, Bindings)> {
    for (i, pattern) in patterns.iter().enumerate() {
        if let MatchResult::Match(bindings) = match_pattern(pattern, value) {
            return Some((i, bindings));
        }
    }
    None
}

/// Apply a set of rules to a value.
///
/// Returns the first matching rule's RHS with bindings substituted, or None.
#[allow(dead_code)]
pub fn apply_rules(rules: &[(Expr, Expr)], value: &Value) -> Option<Value> {
    for (lhs, rhs) in rules {
        if let MatchResult::Match(bindings) = match_pattern(lhs, value) {
            return Some(substitute(rhs, &bindings));
        }
    }
    None
}

/// Substitute bindings into an expression.
#[allow(dead_code)]
fn substitute(expr: &Expr, bindings: &Bindings) -> Value {
    match expr {
        Expr::Symbol(s) => {
            if let Some(val) = bindings.get(s) {
                val.clone()
            } else {
                Value::Symbol(s.clone())
            }
        }
        Expr::Integer(n) => Value::Integer(n.clone()),
        Expr::Real(r) => Value::Real(r.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Null => Value::Null,
        Expr::List(items) => {
            Value::List(items.iter().map(|item| substitute(item, bindings)).collect())
        }
        Expr::Call { head, args } => {
            let h = substitute(head, bindings);
            let a: Vec<Value> = args.iter().map(|arg| substitute(arg, bindings)).collect();
            match h {
                Value::Symbol(name) => Value::Call { head: name, args: a },
                _ => Value::Call {
                    head: h.to_string(),
                    args: a,
                },
            }
        }
        _ => Value::Pattern(expr.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    #[test]
    fn test_match_integer() {
        let pattern = Expr::Integer(Integer::from(42));
        let value = Value::Integer(Integer::from(42));
        assert!(matches!(match_pattern(&pattern, &value), MatchResult::Match(_)));

        let value = Value::Integer(Integer::from(43));
        assert!(matches!(match_pattern(&pattern, &value), MatchResult::NoMatch));
    }

    #[test]
    fn test_match_blank() {
        let pattern = Expr::Blank { type_constraint: None };
        let value = Value::Integer(Integer::from(42));
        assert!(matches!(match_pattern(&pattern, &value), MatchResult::Match(_)));
    }

    #[test]
    fn test_match_named_blank() {
        let pattern = Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: None,
        };
        let value = Value::Integer(Integer::from(42));
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value) {
            assert_eq!(bindings.get("x"), Some(&Value::Integer(Integer::from(42))));
        } else {
            panic!("Expected match");
        }
    }

    #[test]
    fn test_match_typed_blank() {
        let pattern = Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: Some("Integer".to_string()),
        };

        let value = Value::Integer(Integer::from(42));
        assert!(matches!(match_pattern(&pattern, &value), MatchResult::Match(_)));

        let value = Value::Str("hello".to_string());
        assert!(matches!(match_pattern(&pattern, &value), MatchResult::NoMatch));
    }

    #[test]
    fn test_match_list() {
        let pattern = Expr::List(vec![
            Expr::NamedBlank { name: "a".to_string(), type_constraint: None },
            Expr::NamedBlank { name: "b".to_string(), type_constraint: None },
        ]);
        let value = Value::List(vec![Value::Integer(Integer::from(1)), Value::Integer(Integer::from(2))]);
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value) {
            assert_eq!(bindings.get("a"), Some(&Value::Integer(Integer::from(1))));
            assert_eq!(bindings.get("b"), Some(&Value::Integer(Integer::from(2))));
        } else {
            panic!("Expected match");
        }
    }

    #[test]
    fn test_match_call() {
        let pattern = Expr::Call {
            head: Box::new(Expr::Symbol("f".to_string())),
            args: vec![
                Expr::NamedBlank { name: "x".to_string(), type_constraint: None },
            ],
        };
        let value = Value::Call {
            head: "f".to_string(),
            args: vec![Value::Integer(Integer::from(42))],
        };
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value) {
            assert_eq!(bindings.get("x"), Some(&Value::Integer(Integer::from(42))));
        } else {
            panic!("Expected match");
        }
    }
}
