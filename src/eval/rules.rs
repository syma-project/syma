/// Rule application and substitution logic.
///
/// Handles ReplaceAll (/. / //.) and similar rule-based substitution.
use std::collections::HashMap;

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

/// Apply rules to a value, optionally evaluating pattern guards.
pub fn apply_rules_value(value: &Value, rules: &Value, env: &Env) -> Result<Value, EvalError> {
    let _attr_checker = AttributeChecker::new(env.attributes.clone());
    match rules {
        Value::RuleSet {
            rules: rule_pairs, ..
        } => {
            'next_rule: for (lhs, rhs) in rule_pairs {
                if let Value::Pattern(lhs_expr) = lhs
                    && let MatchResult::Match(bindings) =
                        match_pattern(lhs_expr, value, Some(&_attr_checker))
                {
                    // Evaluate guards if present
                    let (inner_pat, guard) = super::extract_guard_expr(lhs_expr);
                    // Bind numbered slots for guard evaluation based on value structure
                    let slot_values: Vec<Value> = match value {
                        Value::Call { args, .. } => args.clone(),
                        Value::List(items) => items.clone(),
                        _ => vec![],
                    };
                    if let Some(guard_expr) = guard {
                        let guard_env = env.child();
                        for (name, val) in &bindings {
                            guard_env.set(name.clone(), val.clone());
                        }
                        guard_env.set("#".to_string(), value.clone());
                        for (i, sv) in slot_values.iter().enumerate() {
                            guard_env.set(format!("#{}", i + 1), sv.clone());
                        }
                        if !super::eval(guard_expr, &guard_env)?.to_bool() {
                            continue 'next_rule;
                        }
                    }
                    // Evaluate nested guards
                    let mut guard_exprs = Vec::new();
                    collect_nested_guards(inner_pat, &mut guard_exprs);
                    for guard_expr in &guard_exprs {
                        let guard_env = env.child();
                        for (name, val) in &bindings {
                            guard_env.set(name.clone(), val.clone());
                        }
                        guard_env.set("#".to_string(), value.clone());
                        for (i, sv) in slot_values.iter().enumerate() {
                            guard_env.set(format!("#{}", i + 1), sv.clone());
                        }
                        if !super::eval(guard_expr, &guard_env)?.to_bool() {
                            continue 'next_rule;
                        }
                    }
                    return apply_rhs_pattern(rhs, &bindings, env);
                }
            }
            Ok(value.clone())
        }
        Value::DispatchedRules { index, rules } => {
            let (head_name, arg_keys) = extract_value_dispatch_key(value);
            let candidate_indices = index
                .get(&head_name)
                .and_then(|sig_map| sig_map.get(&arg_keys));
            if let Some(indices) = candidate_indices {
                'next_rule: for &idx in indices {
                    let (lhs, rhs) = &rules[idx];
                    if let Value::Pattern(lhs_expr) = lhs
                        && let MatchResult::Match(bindings) =
                            match_pattern(&lhs_expr, value, Some(&_attr_checker))
                    {
                        let (inner_pat, guard) = super::extract_guard_expr(&lhs_expr);
                        let slot_values: Vec<Value> = match value {
                            Value::Call { args, .. } => args.clone(),
                            Value::List(items) => items.clone(),
                            _ => vec![],
                        };
                        if let Some(guard_expr) = guard {
                            let guard_env = env.child();
                            for (name, val) in &bindings {
                                guard_env.set(name.clone(), val.clone());
                            }
                            guard_env.set("#".to_string(), value.clone());
                            for (i, sv) in slot_values.iter().enumerate() {
                                guard_env.set(format!("#{}", i + 1), sv.clone());
                            }
                            if !super::eval(guard_expr, &guard_env)?.to_bool() {
                                continue 'next_rule;
                            }
                        }
                        let mut guard_exprs = Vec::new();
                        collect_nested_guards(inner_pat, &mut guard_exprs);
                        for guard_expr in &guard_exprs {
                            let guard_env = env.child();
                            for (name, val) in &bindings {
                                guard_env.set(name.clone(), val.clone());
                            }
                            guard_env.set("#".to_string(), value.clone());
                            for (i, sv) in slot_values.iter().enumerate() {
                                guard_env.set(format!("#{}", i + 1), sv.clone());
                            }
                            if !super::eval(guard_expr, &guard_env)?.to_bool() {
                                continue 'next_rule;
                            }
                        }
                        return apply_rhs_pattern(rhs, &bindings, env);
                    }
                }
            } else {
                'next_rule: for (lhs, rhs) in rules {
                    if let Value::Pattern(lhs_expr) = lhs
                        && let MatchResult::Match(bindings) =
                            match_pattern(&lhs_expr, value, Some(&_attr_checker))
                    {
                        let (inner_pat, guard) = super::extract_guard_expr(&lhs_expr);
                        let slot_values: Vec<Value> = match value {
                            Value::Call { args, .. } => args.clone(),
                            Value::List(items) => items.clone(),
                            _ => vec![],
                        };
                        if let Some(guard_expr) = guard {
                            let guard_env = env.child();
                            for (name, val) in &bindings {
                                guard_env.set(name.clone(), val.clone());
                            }
                            guard_env.set("#".to_string(), value.clone());
                            for (i, sv) in slot_values.iter().enumerate() {
                                guard_env.set(format!("#{}", i + 1), sv.clone());
                            }
                            if !super::eval(guard_expr, &guard_env)?.to_bool() {
                                continue 'next_rule;
                            }
                        }
                        let mut guard_exprs = Vec::new();
                        collect_nested_guards(inner_pat, &mut guard_exprs);
                        for guard_expr in &guard_exprs {
                            let guard_env = env.child();
                            for (name, val) in &bindings {
                                guard_env.set(name.clone(), val.clone());
                            }
                            guard_env.set("#".to_string(), value.clone());
                            for (i, sv) in slot_values.iter().enumerate() {
                                guard_env.set(format!("#{}", i + 1), sv.clone());
                            }
                            if !super::eval(guard_expr, &guard_env)?.to_bool() {
                                continue 'next_rule;
                            }
                        }
                        return apply_rhs_pattern(rhs, &bindings, env);
                    }
                }
            }
            Ok(value.clone())
        }
        Value::List(rule_list) => {
            for rule in rule_list {
                let result = apply_rules_value(value, rule, env)?;
                if !result.struct_eq(value) {
                    return Ok(result);
                }
            }
            Ok(value.clone())
        }
        Value::Rule {
            lhs,
            rhs,
            delayed: _,
        } => {
            // Extract LHS as Pattern, or fall back to structural match
            let lhs_expr = if let Value::Pattern(ref lhs_expr) = **lhs {
                lhs_expr.clone()
            } else {
                // Non-Pattern LHS: structural match (no bindings)
                if lhs.struct_eq(value) {
                    return apply_rhs_pattern(rhs, &HashMap::new(), env);
                }
                return Ok(value.clone());
            };

            if let MatchResult::Match(bindings) =
                match_pattern(&lhs_expr, value, Some(&_attr_checker))
            {
                // Evaluate guards if present
                let (inner_pat, guard) = super::extract_guard_expr(&lhs_expr);
                // Bind numbered slots for guard evaluation based on value structure
                let slot_values: Vec<Value> = match value {
                    Value::Call { args, .. } => args.clone(),
                    Value::List(items) => items.clone(),
                    _ => vec![],
                };
                if let Some(guard_expr) = guard {
                    let guard_env = env.child();
                    for (name, val) in &bindings {
                        guard_env.set(name.clone(), val.clone());
                    }
                    guard_env.set("#".to_string(), value.clone());
                    for (i, sv) in slot_values.iter().enumerate() {
                        guard_env.set(format!("#{}", i + 1), sv.clone());
                    }
                    if !super::eval(guard_expr, &guard_env)?.to_bool() {
                        return Ok(value.clone());
                    }
                }
                let mut guard_exprs = Vec::new();
                collect_nested_guards(inner_pat, &mut guard_exprs);
                for guard_expr in &guard_exprs {
                    let guard_env = env.child();
                    for (name, val) in &bindings {
                        guard_env.set(name.clone(), val.clone());
                    }
                    guard_env.set("#".to_string(), value.clone());
                    for (i, sv) in slot_values.iter().enumerate() {
                        guard_env.set(format!("#{}", i + 1), sv.clone());
                    }
                    if !super::eval(guard_expr, &guard_env)?.to_bool() {
                        return Ok(value.clone());
                    }
                }

                apply_rhs_pattern(rhs, &bindings, env)
            } else {
                Ok(value.clone())
            }
        }
        _ => Ok(value.clone()),
    }
}

/// Builtin: ReplaceAll[expr, rules] — apply rules once.
pub fn builtin_replace_all(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ReplaceAll requires exactly 2 arguments".to_string(),
        ));
    }
    apply_rules_value(&args[0], &args[1], env)
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
        let next = apply_rules_value(&val, &args[1], env)?;
        if next.struct_eq(&val) {
            return Ok(val);
        }
        val = next;
    }
    Ok(val)
}
