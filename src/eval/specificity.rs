use std::collections::HashMap;

use crate::ast::Expr;
use crate::env::Env;
use crate::eval::AttributeChecker;
use crate::pattern::{MatchResult, collect_nested_guards, match_pattern};
use crate::value::{EvalError, Value};

use super::eval;

/// Re-export for use in sibling modules.
pub(crate) use crate::pattern::Bindings;

/// Rank specificity of a function definition's parameter list.
pub(super) fn specificity(params: &[Expr]) -> usize {
    params.iter().map(specificity_expr).sum()
}

/// Specificity score for a single pattern expression.
pub(super) fn specificity_expr(p: &Expr) -> usize {
    match p {
        Expr::Integer(_) | Expr::Real(_) | Expr::Str(_) | Expr::Bool(_) => 3,
        Expr::Blank {
            type_constraint: Some(_),
        }
        | Expr::NamedBlank {
            type_constraint: Some(_),
            ..
        } => 2,
        Expr::NamedBlank { .. } => 1,
        Expr::Blank { .. } | Expr::BlankSequence { .. } | Expr::BlankNullSequence { .. } => 0,
        Expr::PatternGuard { pattern, .. } => specificity_expr(pattern) + 1,
        Expr::Call { head, args } => {
            let head_score = match head.as_ref() {
                Expr::Symbol(_) => 2,
                Expr::Integer(_) | Expr::Real(_) | Expr::Str(_) | Expr::Bool(_) => 3,
                _ => 0,
            };
            let arg_scores: usize = args.iter().map(specificity_expr).sum();
            head_score + arg_scores
        }
        Expr::List(items) => items.iter().map(specificity_expr).sum(),
        _ => 0,
    }
}

/// Try to match parameters against arguments, evaluating any pattern guards.
///
/// Returns Ok(Some(bindings)) on success, Ok(None) if no match, Err on guard eval error.
///
/// Handles sequence patterns (__ / ___) in function parameters using backtracking:
///   f[x__] := Total[x]             — sequence of 1+ elements
///   f[x___] := {x}                 — sequence of 0+ elements
///   f[a_, b__, c_] := {a, b, c}    — mixed fixed and sequence parameters
pub(crate) fn try_match_params(
    params: &[Expr],
    args: &[Value],
    env: &Env,
) -> Result<Option<Bindings>, EvalError> {
    let _attr_checker = AttributeChecker::new(env.attributes.clone());
    let has_sequences = params.iter().any(|p| {
        let (inner, _guard) = extract_guard_expr(p);
        has_sequence_pattern(inner)
    });

    let mut guard_exprs: Vec<Expr> = Vec::new();

    if has_sequences {
        let inner_params: Vec<Expr> = params
            .iter()
            .map(|p| {
                let (inner, guard) = extract_guard_expr(p);
                if let Some(g) = guard {
                    guard_exprs.push(g.clone());
                }
                collect_nested_guards(inner, &mut guard_exprs);
                inner.clone()
            })
            .collect();

        let list_pattern = Expr::List(inner_params);
        let list_value = Value::List(args.to_vec());

        match match_pattern(&list_pattern, &list_value, Some(&_attr_checker)) {
            MatchResult::Match(mut bindings) => {
                for guard in &guard_exprs {
                    let guard_env = env.child();
                    for (name, val) in &bindings {
                        guard_env.set(name.clone(), val.clone());
                    }
                    for (i, arg) in args.iter().enumerate() {
                        guard_env.set(format!("#{}", i + 1), arg.clone());
                    }
                    if !args.is_empty() {
                        guard_env.set("#".to_string(), args[0].clone());
                    }
                    if !eval(guard, &guard_env)?.to_bool() {
                        return Ok(None);
                    }
                }
                apply_defaults(params, &mut bindings, env)?;
                Ok(Some(bindings))
            }
            MatchResult::NoMatch => Ok(None),
        }
    } else {
        let has_optional = params.iter().any(|p| {
            let (inner, _) = extract_guard_expr(p);
            matches!(
                inner,
                Expr::OptionalBlank { .. } | Expr::OptionalNamedBlank { .. }
            )
        });

        if has_optional {
            if args.len() > params.len() {
                return Ok(None);
            }
        } else if params.len() != args.len() {
            return Ok(None);
        }

        let mut bindings = HashMap::new();

        for (i, param) in params.iter().enumerate() {
            let arg = args.get(i).unwrap_or(&Value::Null);
            let (inner_pat, guard) = extract_guard_expr(param);
            match match_pattern(inner_pat, arg, Some(&_attr_checker)) {
                MatchResult::Match(b) => {
                    bindings.extend(b);
                    if let Some(g) = guard {
                        guard_exprs.push(g.clone());
                    }
                    collect_nested_guards(inner_pat, &mut guard_exprs);
                }
                MatchResult::NoMatch => return Ok(None),
            }
        }

        for guard in &guard_exprs {
            let guard_env = env.child();
            for (name, val) in &bindings {
                guard_env.set(name.clone(), val.clone());
            }
            for (i, arg) in args.iter().enumerate() {
                guard_env.set(format!("#{}", i + 1), arg.clone());
            }
            if !args.is_empty() {
                guard_env.set("#".to_string(), args[0].clone());
            }
            if !eval(guard, &guard_env)?.to_bool() {
                return Ok(None);
            }
        }

        apply_defaults(params, &mut bindings, env)?;
        Ok(Some(bindings))
    }
}

/// Apply default values for optional named patterns (_:default, x_:default).
pub(super) fn apply_defaults(
    params: &[Expr],
    bindings: &mut Bindings,
    env: &Env,
) -> Result<(), EvalError> {
    for param in params {
        let (inner, _guard) = extract_guard_expr(param);
        if let Expr::OptionalNamedBlank {
            name,
            default_value: Some(default),
            ..
        } = inner
            && let Some(Value::Null) = bindings.get(name)
        {
            let val = eval(default, env)?;
            bindings.insert(name.clone(), val);
        }
    }
    Ok(())
}

/// Flatten Sequence values into the surrounding list or call arguments.
pub(super) fn flatten_sequences(items: Vec<Value>, skip_sequence: bool) -> Vec<Value> {
    if skip_sequence {
        return items;
    }
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::Sequence(seq) => result.extend(seq),
            other => result.push(other),
        }
    }
    result
}

/// Check if an expression contains a sequence pattern (BlankSequence or BlankNullSequence).
fn has_sequence_pattern(expr: &Expr) -> bool {
    match expr {
        Expr::BlankSequence { .. } | Expr::BlankNullSequence { .. } => true,
        Expr::List(items) => items.iter().any(has_sequence_pattern),
        Expr::Call { args, .. } => args.iter().any(has_sequence_pattern),
        _ => false,
    }
}

/// Extract the guard condition from a PatternGuard expression.
/// Returns (inner_pattern, optional_guard_expr).
fn extract_guard_expr(expr: &Expr) -> (&Expr, Option<&Expr>) {
    if let Expr::PatternGuard { pattern, condition } = expr {
        (pattern, Some(condition))
    } else {
        (expr, None)
    }
}
