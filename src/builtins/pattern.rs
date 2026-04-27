use crate::ast::Expr;
use crate::env::Env;
use crate::eval::eval;
use crate::pattern::{AttributeChecker, MatchResult, match_pattern};
use crate::value::EvalError;
use crate::value::Value;
use std::collections::HashMap;

/// Convert a value back to an Expr for pattern matching.
/// Handles nested Pattern values inside lists/calls.
fn value_to_pattern_expr(val: &Value) -> Expr {
    match val {
        Value::Pattern(expr) => expr.clone(),
        Value::List(items) => Expr::List(items.iter().map(value_to_pattern_expr).collect()),
        Value::Call { head, args } => Expr::Call {
            head: Box::new(Expr::Symbol(head.clone())),
            args: args.iter().map(value_to_pattern_expr).collect(),
        },
        Value::Integer(n) => Expr::Integer(n.clone()),
        Value::Real(r) => Expr::Real(r.clone()),
        Value::Str(s) => Expr::Str(s.clone()),
        Value::Bool(b) => Expr::Bool(*b),
        Value::Null => Expr::Null,
        Value::Symbol(s) => Expr::Symbol(s.clone()),
        _ => Expr::Symbol(val.to_string()),
    }
}

/// Check if a value tree contains any Pattern nodes.
fn contains_pattern_value(val: &Value) -> bool {
    match val {
        Value::Pattern(_) => true,
        Value::List(items) => items.iter().any(contains_pattern_value),
        Value::Call { args, .. } => args.iter().any(contains_pattern_value),
        _ => false,
    }
}

pub fn builtin_match_q(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MatchQ requires exactly 2 arguments".to_string(),
        ));
    }
    let attr_checker = AttributeChecker::new(env.attributes.clone());
    let result = match &args[1] {
        Value::Pattern(pat_expr) => {
            // Handle PatternGuard — need to evaluate the guard condition
            if let Expr::PatternGuard { pattern, condition } = pat_expr {
                if matches!(
                    match_pattern(pattern, &args[0], Some(&attr_checker)),
                    MatchResult::Match(_)
                ) {
                    // Evaluate guard condition with # bound to the value
                    let guard_env = env.child();
                    guard_env.set("#".to_string(), args[0].clone());
                    eval(condition, &guard_env)?.to_bool()
                } else {
                    false
                }
            } else {
                matches!(
                    match_pattern(pat_expr, &args[0], Some(&attr_checker)),
                    MatchResult::Match(_)
                )
            }
        }
        // Compound values containing Pattern elements → convert to pattern expr
        _ if contains_pattern_value(&args[1]) => {
            let pat_expr = value_to_pattern_expr(&args[1]);
            matches!(
                match_pattern(&pat_expr, &args[0], Some(&attr_checker)),
                MatchResult::Match(_)
            )
        }
        _ => args[0].struct_eq(&args[1]),
    };
    Ok(Value::Bool(result))
}

pub fn builtin_head(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Head requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Symbol(args[0].type_name().to_string()))
}

pub fn builtin_type_of(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TypeOf requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Symbol(args[0].type_name().to_string()))
}

pub fn builtin_free_q(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FreeQ requires exactly 2 arguments".to_string(),
        ));
    }
    let attr_checker = AttributeChecker::new(env.attributes.clone());
    #[inline]
    fn check_match(
        value: &Value,
        pattern: &Value,
        attr_checker: &AttributeChecker,
        env: Option<&Env>,
    ) -> bool {
        match pattern {
            Value::Pattern(pat_expr) => {
                if let Expr::PatternGuard { pattern, condition } = pat_expr {
                    if matches!(
                        match_pattern(pattern, value, Some(attr_checker)),
                        MatchResult::Match(_)
                    ) {
                        if let Some(env) = env {
                            let guard_env = env.child();
                            guard_env.set("#".to_string(), value.clone());
                            eval(condition, &guard_env)
                                .map(|v| v.to_bool())
                                .unwrap_or(false)
                        } else {
                            true
                        }
                    } else {
                        false
                    }
                } else {
                    matches!(
                        match_pattern(pat_expr, value, Some(attr_checker)),
                        MatchResult::Match(_)
                    )
                }
            }
            _ if contains_pattern_value(pattern) => {
                let pat_expr = value_to_pattern_expr(pattern);
                matches!(
                    match_pattern(&pat_expr, value, Some(attr_checker)),
                    MatchResult::Match(_)
                )
            }
            _ => value.struct_eq(pattern),
        }
    }
    fn contains_pattern(
        value: &Value,
        pattern: &Value,
        attr_checker: &AttributeChecker,
        env: Option<&Env>,
    ) -> bool {
        if check_match(value, pattern, attr_checker, env) {
            return true;
        }
        match value {
            Value::List(items) => items
                .iter()
                .any(|item| contains_pattern(item, pattern, attr_checker, env)),
            Value::Call { args, .. } => args
                .iter()
                .any(|arg| contains_pattern(arg, pattern, attr_checker, env)),
            _ => false,
        }
    }
    Ok(Value::Bool(!contains_pattern(
        &args[0],
        &args[1],
        &attr_checker,
        Some(env),
    )))
}

/// Check if a value matches a pattern.
/// If pattern is a Value::Pattern, use the pattern engine; otherwise use structural equality.
/// If env is provided, PatternGuard conditions are evaluated.
fn matches_value(
    value: &Value,
    pattern: &Value,
    attr_checker: Option<&AttributeChecker>,
    env: Option<&Env>,
) -> bool {
    match pattern {
        Value::Pattern(pat_expr) => {
            // Handle PatternGuard with guard evaluation
            if let Expr::PatternGuard { pattern, condition } = pat_expr {
                if matches!(
                    match_pattern(pattern, value, attr_checker),
                    MatchResult::Match(_)
                ) {
                    if let Some(env) = env {
                        let guard_env = env.child();
                        guard_env.set("#".to_string(), value.clone());
                        eval(condition, &guard_env)
                            .map(|v| v.to_bool())
                            .unwrap_or(false)
                    } else {
                        true // no env means we can't evaluate the guard
                    }
                } else {
                    false
                }
            } else {
                matches!(
                    match_pattern(pat_expr, value, attr_checker),
                    MatchResult::Match(_)
                )
            }
        }
        _ if contains_pattern_value(pattern) => {
            let pat_expr = value_to_pattern_expr(pattern);
            matches!(
                match_pattern(&pat_expr, value, attr_checker),
                MatchResult::Match(_)
            )
        }
        _ => value.struct_eq(pattern),
    }
}

/// Cases[list, pattern] — select elements matching a pattern.
pub fn builtin_cases(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Cases requires 2 or 3 arguments".to_string(),
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
    let attr_checker = AttributeChecker::new(env.attributes.clone());
    let pattern = &args[1];
    let result: Vec<Value> = items
        .iter()
        .filter(|item| matches_value(item, pattern, Some(&attr_checker), Some(env)))
        .cloned()
        .collect();
    Ok(Value::List(result))
}

/// DeleteCases[list, pattern] — remove elements matching a pattern.
pub fn builtin_delete_cases(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "DeleteCases requires 2 or 3 arguments".to_string(),
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
    let attr_checker = AttributeChecker::new(env.attributes.clone());
    let pattern = &args[1];
    let result: Vec<Value> = items
        .iter()
        .filter(|item| !matches_value(item, pattern, Some(&attr_checker), Some(env)))
        .cloned()
        .collect();
    Ok(Value::List(result))
}

// ── Dispatch ──

/// Extract a dispatch key from an Expr pattern.
fn extract_dispatch_key_expr(expr: &Expr) -> Option<(String, Vec<Option<String>>)> {
    let inner = match expr {
        Expr::PatternGuard { pattern, .. } => pattern.as_ref(),
        _ => expr,
    };
    match inner {
        Expr::Call { head, args } => {
            let head_name = match head.as_ref() {
                Expr::Symbol(s) => s.clone(),
                _ => return None,
            };
            let arg_keys = args.iter().map(|arg| get_arg_dispatch_key(arg)).collect();
            Some((head_name, arg_keys))
        }
        Expr::Symbol(s) => Some((s.clone(), vec![])),
        Expr::List(_) => Some(("List".to_string(), vec![Some("List".())])),
        _ => None,
    }
}

fn extract_dispatch_key_value(val: &Value) -> Option<(String, Vec<Option<String>>)> {
    match val {
        Value::Pattern(expr) => extract_dispatch_key_expr(expr),
        _ => None,
    }
}

fn get_arg_dispatch_key(arg: &Expr) -> Option<String> {
    let inner = match arg {
        Expr::PatternGuard { pattern, .. } => pattern.as_ref(),
        _ => arg,
    };
    match inner {
        Expr::Blank { type_constraint: None } => Some("Blank".to_string()),
        Expr::Blank { type_constraint: Some(tc) } => Some(tc.clone()),
        Expr::NamedBlank { type_constraint: None, .. } => Some("Blank".to_string()),
        Expr::NamedBlank { type_constraint: Some(tc), .. } => Some(tc.clone()),
        Expr::BlankSequence { type_constraint: None, .. } => Some("BlankSequence".to_string()),
        Expr::BlankSequence { type_constraint: Some(tc), .. } => Some(tc.clone()),
        Expr::BlankNullSequence { type_constraint: None, .. } => Some("BlankNullSequence".to_string()),
        Expr::BlankNullSequence { type_constraint: Some(tc), .. } => Some(tc.clone()),
        Expr::OptionalBlank { type_constraint: None, .. } => Some("OptionalBlank".to_string()),
        Expr::OptionalBlank { type_constraint: Some(tc), .. } => Some(tc.clone()),
        Expr::OptionalNamedBlank { type_constraint: None, .. } => Some("OptionalBlank".to_string()),
        Expr::OptionalNamedBlank { type_constraint: Some(tc), .. } => Some(tc.clone()),
        Expr::Integer(_) => Some("Integer".to_string()),
        Expr::Real(_) => Some("Real".to_string()),
        Expr::Str(_) => Some("String".to_string()),
        Expr::Bool(_) => Some("Boolean".to_string()),
        Expr::Null => Some("Null".to_string()),
        Expr::Symbol(s) if s.ends_with('_') => Some("Blank".to_string()),
        Expr::Symbol(s) => Some(s.clone()),
        Expr::Call { head, .. } => match head.as_ref() {
            Expr::Symbol(s) => Some(s.clone()),
            _ => None,
        },
        Expr::List(_) => Some("List".to_string()),
        _ => None,
    }
}

fn extract_rules_value(val: &Value) -> Vec<(Value, Value)> {
    match val {
        Value::RuleSet { rules, .. } => rules.clone(),
        Value::List(items) => {
            let mut rules = Vec::new();
            for item in items {
                match item {
                    Value::Rule { lhs, rhs, .. } => {
                        rules.push((lhs.as_ref().clone(), rhs.as_ref().clone()));
                    }
                    Value::Pattern(pat_expr) => {
                        rules.push((Value::Pattern(pat_expr.clone()), Value::Null));
                    }
                    _ => {}
                }
            }
            rules
        }
        Value::Rule { lhs, rhs, .. } => vec![(lhs.as_ref().clone(), rhs.as_ref().clone())],
        _ => vec![],
    }
}

/// `Dispatch[rules]` — build a dispatch-indexed rule set for O(1) lookup.
pub fn builtin_dispatch(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Dispatch requires exactly 1 argument".to_string(),
        ));
    }
    let rules = extract_rules_value(&args[0]);
    let mut index: HashMap<String, HashMap<Vec<Option<String>>, Vec<usize>>> = HashMap::new();
    for (idx, (lhs, _rhs)) in rules.iter().enumerate() {
        if let Some((head_name, arg_keys)) = extract_dispatch_key_value(lhs) {
            index
                .entry(head_name)
                .or_default()
                .entry(arg_keys)
                .or_default()
                .push(idx);
        }
    }
    Ok(Value::DispatchedRules { index, rules })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn list(vals: Vec<Value>) -> Value {
        Value::List(vals)
    }
    fn string(s: &str) -> Value {
        Value::Str(s.to_string())
    }
    fn boolean(b: bool) -> Value {
        Value::Bool(b)
    }

    #[test]
    fn test_head() {
        assert_eq!(
            builtin_head(&[int(42)]).unwrap(),
            Value::Symbol("Integer".to_string())
        );
        assert_eq!(
            builtin_head(&[list(vec![])]).unwrap(),
            Value::Symbol("List".to_string())
        );
    }

    #[test]
    fn test_type_of() {
        assert_eq!(
            builtin_type_of(&[string("hello")]).unwrap(),
            Value::Symbol("String".to_string())
        );
        assert_eq!(
            builtin_type_of(&[boolean(true)]).unwrap(),
            Value::Symbol("Boolean".to_string())
        );
    }

    fn test_env() -> crate::env::Env {
        crate::env::Env::new()
    }

    fn pat_blank() -> Value {
        Value::Pattern(crate::ast::Expr::Blank {
            type_constraint: None,
        })
    }
    fn pat_int(n: i64) -> Value {
        Value::Pattern(crate::ast::Expr::Integer(n.into()))
    }
    fn pat_string(s: &str) -> Value {
        Value::Pattern(crate::ast::Expr::Str(s.to_string()))
    }

    #[test]
    fn test_cases_basic() {
        let items = list(vec![int(1), int(2), string("x"), int(3)]);
        // Cases with a literal pattern (structural equality via Value::Pattern)
        let result = builtin_cases(&[items.clone(), pat_int(2)], &test_env()).unwrap();
        assert_eq!(result, list(vec![int(2)]));
    }

    #[test]
    fn test_cases_blank() {
        let items = list(vec![int(1), string("x"), int(3)]);
        // Cases with blank pattern matches everything
        let result = builtin_cases(&[items.clone(), pat_blank()], &test_env()).unwrap();
        assert_eq!(result, items);
    }

    #[test]
    fn test_cases_no_match() {
        let items = list(vec![int(1), int(2), int(3)]);
        let result = builtin_cases(&[items, pat_string("x")], &test_env()).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_cases_non_list_error() {
        let result = builtin_cases(&[int(42), pat_blank()], &test_env());
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_cases_basic() {
        let items = list(vec![int(1), int(2), string("x"), int(3)]);
        // DeleteCases removes matching elements
        let result = builtin_delete_cases(&[items, pat_int(2)], &test_env()).unwrap();
        assert_eq!(result, list(vec![int(1), string("x"), int(3)]));
    }

    #[test]
    fn test_delete_cases_blank() {
        let items = list(vec![int(1), int(2), int(3)]);
        // DeleteCases with blank removes everything
        let result = builtin_delete_cases(&[items, pat_blank()], &test_env()).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_delete_cases_no_match() {
        let items = list(vec![int(1), int(2), int(3)]);
        let result = builtin_delete_cases(&[items, pat_string("x")], &test_env()).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }
}
