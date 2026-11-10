/// Rule application and substitution logic.
///
/// Handles ReplaceAll (/. / //.) and similar rule-based substitution.
use std::collections::HashMap;

use crate::ast::Expr;
use crate::env::Env;
use crate::pattern::{
    AttributeChecker, Bindings, MatchResult, collect_nested_guards, match_pattern,
};
use crate::value::*;

/// Evaluate RHS expression with pattern bindings in a child environment.
fn apply_rhs_pattern(rhs: &Value, bindings: &Bindings, env: &Env) -> Result<Value, EvalError> {
    if let Value::Pattern(rhs_expr) = rhs {
        let child_env = env.child();
        for (name, val) in bindings {
            child_env.set(name.clone(), val.clone());
        }
        super::eval(rhs_expr, &child_env)
    } else {
        // Fallback for non-Pattern RHS (pre-evaluated value from other code)
        Ok(rhs.clone())
    }
}

/// Extract a dispatch key from a runtime Value for use with DispatchedRules lookup.
fn extract_value_dispatch_key(value: &Value) -> (String, Vec<Option<String>>) {
    match value {
        Value::Call { head, args } => {
            let arg_keys: Vec<Option<String>> = args
                .iter()
                .map(|a| Some(a.type_name().to_string()))
                .collect();
            (head.clone(), arg_keys)
        }
        Value::List(items) => {
            let arg_keys: Vec<Option<String>> = items
                .iter()
                .map(|a| Some(a.type_name().to_string()))
                .collect();
            ("List".to_string(), arg_keys)
        }
        _ => (value.type_name().to_string(), vec![]),
    }
}

/// Evaluate guards attached to a matched rule LHS. Returns Ok(true) if all
/// guards pass, Ok(false) if any guard fails.
fn rule_guards_pass(
    lhs_expr: &Expr,
    value: &Value,
    bindings: &Bindings,
    env: &Env,
) -> Result<bool, EvalError> {
    let (inner_pat, guard) = super::extract_guard_expr(lhs_expr);
    let mut guard_exprs = Vec::new();
    if let Some(g) = guard {
        guard_exprs.push(g.clone());
    }
    collect_nested_guards(inner_pat, &mut guard_exprs);
    if guard_exprs.is_empty() {
        return Ok(true);
    }
    // Bind numbered slots for guard evaluation based on value structure
    let slot_values: Vec<Value> = match value {
        Value::Call { args, .. } => args.clone(),
        Value::List(items) => items.clone(),
        _ => vec![],
    };
    for guard_expr in &guard_exprs {
        let guard_env = env.child();
        for (name, val) in bindings {
            guard_env.set(name.clone(), val.clone());
        }
        guard_env.set("#".to_string(), value.clone());
        for (i, sv) in slot_values.iter().enumerate() {
            guard_env.set(format!("#{}", i + 1), sv.clone());
        }
        if !super::eval(guard_expr, &guard_env)?.to_bool() {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Try a single (lhs, rhs) rule against `value` at the top level.
/// Returns Some(replacement) on a match (with passing guards), None otherwise.
fn try_single_rule(
    lhs: &Value,
    rhs: &Value,
    value: &Value,
    env: &Env,
) -> Result<Option<Value>, EvalError> {
    let attr_checker = AttributeChecker::new(env.attributes.clone());
    if let Value::Pattern(lhs_expr) = lhs {
        if let MatchResult::Match(bindings) = match_pattern(lhs_expr, value, Some(&attr_checker)) {
            if !rule_guards_pass(lhs_expr, value, &bindings, env)? {
                return Ok(None);
            }
            return Ok(Some(apply_rhs_pattern(rhs, &bindings, env)?));
        }
    } else if lhs.struct_eq(value) {
        // Non-Pattern LHS: structural match (no bindings, no guards)
        return Ok(Some(apply_rhs_pattern(rhs, &HashMap::new(), env)?));
    }
    Ok(None)
}

/// Try a sequence of rules in order, returning the first match.
fn try_rule_pairs(
    rule_pairs: &[(Value, Value)],
    value: &Value,
    env: &Env,
) -> Result<Option<Value>, EvalError> {
    for (lhs, rhs) in rule_pairs {
        if let Some(result) = try_single_rule(lhs, rhs, value, env)? {
            return Ok(Some(result));
        }
    }
    Ok(None)
}

/// Try a list of `Value::Rule`/`Value::RuleDelayed` values in order.
fn try_rule_list(
    rule_list: &[Value],
    value: &Value,
    env: &Env,
) -> Result<Option<Value>, EvalError> {
    for rule in rule_list {
        let matched = match rule {
            Value::Rule { lhs, rhs, .. } => try_single_rule(lhs, rhs, value, env)?,
            _ => None,
        };
        if matched.is_some() {
            return Ok(matched);
        }
    }
    Ok(None)
}

/// Apply rules once at the top level of `value`. Returns Some(result) if a
/// rule matched (including when the replacement happens to equal the input),
/// or None if no rule matched.
fn apply_rules_once(value: &Value, rules: &Value, env: &Env) -> Result<Option<Value>, EvalError> {
    match rules {
        Value::RuleSet {
            rules: rule_pairs, ..
        } => try_rule_pairs(rule_pairs, value, env),
        Value::DispatchedRules { index, rules } => {
            let (head_name, arg_keys) = extract_value_dispatch_key(value);
            let candidate_indices = index
                .get(&head_name)
                .and_then(|sig_map| sig_map.get(&arg_keys));
            if let Some(indices) = candidate_indices {
                for &idx in indices {
                    let (lhs, rhs) = &rules[idx];
                    if let Some(result) = try_single_rule(lhs, rhs, value, env)? {
                        return Ok(Some(result));
                    }
                }
                Ok(None)
            } else {
                try_rule_pairs(rules, value, env)
            }
        }
        Value::List(rule_list) => try_rule_list(rule_list, value, env),
        Value::Rule { lhs, rhs, .. } => try_single_rule(lhs, rhs, value, env),
        _ => Ok(None),
    }
}

/// Apply rules to a value at the top level only (used by `Replace`).
/// Returns the value unchanged when no rule matches.
pub fn apply_rules_value(value: &Value, rules: &Value, env: &Env) -> Result<Value, EvalError> {
    Ok(apply_rules_once(value, rules, env)?.unwrap_or_else(|| value.clone()))
}

/// Apply rules to `value` and all of its subexpressions (used by `ReplaceAll`).
///
/// Like the Wolfram Language, the whole expression is tried first; if a rule
/// matches, its subparts are not inspected further during this pass.
pub fn apply_rules_deep(value: &Value, rules: &Value, env: &Env) -> Result<Value, EvalError> {
    if let Some(result) = apply_rules_once(value, rules, env)? {
        return Ok(result);
    }
    match value {
        Value::List(items) => {
            let mapped: Result<Vec<Value>, _> = items
                .iter()
                .map(|i| apply_rules_deep(i, rules, env))
                .collect();
            Ok(Value::List(mapped?))
        }
        Value::Call { head, args } => {
            let new_head = apply_rules_deep(&Value::Symbol(head.clone()), rules, env)?;
            let head_name = match new_head {
                Value::Symbol(s) => s,
                other => other.to_string(),
            };
            let new_args: Result<Vec<Value>, _> = args
                .iter()
                .map(|a| apply_rules_deep(a, rules, env))
                .collect();
            Ok(Value::Call {
                head: head_name,
                args: new_args?,
            })
        }
        Value::Assoc(map) => {
            let mut new_map = HashMap::with_capacity(map.len());
            for (k, v) in map {
                new_map.insert(k.clone(), apply_rules_deep(v, rules, env)?);
            }
            Ok(Value::Assoc(new_map))
        }
        // Descend into held expressions, preserving the hold wrapper.
        Value::Hold(inner) => Ok(Value::Hold(Box::new(apply_rules_deep(inner, rules, env)?))),
        Value::HoldComplete(inner) => Ok(Value::HoldComplete(Box::new(apply_rules_deep(
            inner, rules, env,
        )?))),
        // A Pattern wraps a held Expr; unwrap, transform, and re-wrap so that
        // ReleaseHold still evaluates the result.
        Value::Pattern(expr) => {
            let inner = crate::pattern::unwrap_expr_to_value(expr);
            let processed = apply_rules_deep(&inner, rules, env)?;
            Ok(Value::Pattern(super::table::value_to_expr(&processed)))
        }
        Value::Rule { lhs, rhs, delayed } => {
            let new_lhs = apply_rules_deep(lhs, rules, env)?;
            let new_rhs = apply_rules_deep(rhs, rules, env)?;
            Ok(Value::Rule {
                lhs: Box::new(new_lhs),
                rhs: Box::new(new_rhs),
                delayed: *delayed,
            })
        }
        other => Ok(other.clone()),
    }
}

/// Builtin: ReplaceAll[expr, rules] — apply rules to expr and its subparts once.
pub fn builtin_replace_all(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ReplaceAll requires exactly 2 arguments".to_string(),
        ));
    }
    apply_rules_deep(&args[0], &args[1], env)
}

/// Builtin: ReplaceRepeated[expr, rules] — apply rules repeatedly until no change.
pub fn builtin_replace_repeated(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ReplaceRepeated requires exactly 2 arguments".to_string(),
        ));
    }
    let mut val = args[0].clone();
    let max_iterations = 1000;
    for _ in 0..max_iterations {
        let next = apply_rules_deep(&val, &args[1], env)?;
        if next.struct_eq(&val) {
            return Ok(val);
        }
        val = next;
    }
    Ok(val)
}
