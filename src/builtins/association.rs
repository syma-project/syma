use crate::ast::Expr;
use crate::env::Env;
use crate::eval::apply_function;
use crate::pattern::{AttributeChecker, MatchResult, match_pattern};
use crate::value::EvalError;
use crate::value::Value;
use std::collections::HashMap;

use super::list::compare_values;

/// Build an unevaluated `head[args...]` result.
fn unevaluated(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Call {
        head: name.to_string(),
        args: args.to_vec(),
    })
}

/// True when every element of a (non-empty) slice is an association.
fn all_assocs(items: &[Value]) -> bool {
    !items.is_empty() && items.iter().all(|x| matches!(x, Value::Assoc(_)))
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Convert a Value to a string suitable for use as an Assoc key.
fn value_to_key(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Symbol(s) => s.clone(),
        Value::Integer(n) => n.to_string(),
        Value::Real(r) => r.to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Null => "Null".to_string(),
        other => format!("{}", other),
    }
}

/// Canonical string key for a value used as an association key.
///
/// Association keys are stored in canonical string form (see `value_to_key`),
/// so any value can be used for membership/lookup — this mirrors Wolfram
/// Language, where association keys may be arbitrary expressions, as closely
/// as the string-keyed representation allows.
fn key_to_string(v: &Value) -> Result<String, EvalError> {
    Ok(value_to_key(v))
}

/// A `Missing["KeyAbsent", key]` value, used when a key is absent.
fn missing_key(key: &Value) -> Value {
    Value::Call {
        head: "Missing".to_string(),
        args: vec![Value::Str("KeyAbsent".to_string()), key.clone()],
    }
}

/// Evaluate a held `Value::Pattern` expression, falling back to a structural
/// unwrap when evaluation fails.
fn eval_pattern(expr: &Expr, env: &Env) -> Value {
    crate::eval::eval(expr, env).unwrap_or_else(|_| crate::pattern::unwrap_expr_to_value(expr))
}

/// Unwrap and evaluate a possibly pattern-held value. Rules produced by the
/// evaluator keep their sides wrapped in `Value::Pattern`, so consumers of
/// rules (e.g. `AssociateTo`) must evaluate them to get usable keys/values.
fn eval_held(v: &Value, env: &Env) -> Value {
    match v {
        Value::Pattern(expr) => eval_pattern(expr, env),
        other => other.clone(),
    }
}

/// Split a `Rule`/`RuleDelayed` — possibly held as a `Value::Pattern` — into its
/// canonical key string and evaluated value.
fn rule_entry(rule: &Value, env: &Env) -> Option<(String, Value)> {
    match rule {
        Value::Rule { lhs, rhs, .. } => {
            Some((value_to_key(&eval_held(lhs, env)), eval_held(rhs, env)))
        }
        Value::Pattern(Expr::Rule { lhs, rhs } | Expr::RuleDelayed { lhs, rhs }) => Some((
            value_to_key(&eval_pattern(lhs, env)),
            eval_pattern(rhs, env),
        )),
        _ => None,
    }
}

/// Extract keys from a Value that is either a single key or a list of keys.
fn extract_keys(v: &Value) -> Result<Vec<String>, EvalError> {
    match v {
        Value::List(items) => items.iter().map(key_to_string).collect(),
        single => Ok(vec![key_to_string(single)?]),
    }
}

// ── AssociationQ ───────────────────────────────────────────────────────────

pub fn builtin_association_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "AssociationQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(matches!(&args[0], Value::Assoc(_))))
}

// ── Normal ─────────────────────────────────────────────────────────────────

pub fn builtin_normal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Normal requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let mut rules = Vec::with_capacity(map.len());
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                rules.push(Value::Rule {
                    lhs: Box::new(Value::Str(k.clone())),
                    rhs: Box::new(map[k].clone()),
                    delayed: false,
                });
            }
            Ok(Value::List(rules))
        }
        Value::Dataset(inner) => {
            // Unwrap Dataset to inner data
            Ok(inner.as_ref().clone())
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── KeySort ────────────────────────────────────────────────────────────────

pub fn builtin_key_sort(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "KeySort requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            // Collect sorted by key, return new assoc
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut new_map = HashMap::with_capacity(map.len());
            for k in keys {
                new_map.insert(k.clone(), map[k].clone());
            }
            Ok(Value::Assoc(new_map))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── KeySortBy ──────────────────────────────────────────────────────────────

pub fn builtin_key_sort_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KeySortBy requires exactly 2 arguments: assoc and ordering function".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            // Wolfram: KeySortBy[assoc, f] sorts by applying the unary key
            // function f to each key (mirrors SortBy's key semantics).
            let f = &args[1];
            let mut keyed: Vec<(Value, &String)> = Vec::with_capacity(map.len());
            for k in map.keys() {
                let sort_key = apply_function(f, &[Value::Str(k.clone())], env)?;
                keyed.push((sort_key, k));
            }
            keyed.sort_by(|a, b| compare_values(&a.0, &b.0));
            let mut new_map = HashMap::with_capacity(map.len());
            for (_, k) in keyed {
                new_map.insert(k.clone(), map[k].clone());
            }
            Ok(Value::Assoc(new_map))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── KeyTake ────────────────────────────────────────────────────────────────

pub fn builtin_key_take(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KeyTake requires exactly 2 arguments: assoc and keys".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let keys_to_keep = extract_keys(&args[1])?;
            let mut new_map = HashMap::new();
            for k in &keys_to_keep {
                if let Some(v) = map.get(k) {
                    new_map.insert(k.clone(), v.clone());
                }
            }
            Ok(Value::Assoc(new_map))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── KeyDrop ────────────────────────────────────────────────────────────────

pub fn builtin_key_drop(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KeyDrop requires exactly 2 arguments: assoc and keys".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let keys_to_drop: Vec<String> = extract_keys(&args[1])?;
            let mut new_map = HashMap::with_capacity(map.len());
            for (k, v) in map.iter() {
                if !keys_to_drop.contains(k) {
                    new_map.insert(k.clone(), v.clone());
                }
            }
            Ok(Value::Assoc(new_map))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── KeySelect ──────────────────────────────────────────────────────────────

pub fn builtin_key_select(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KeySelect requires exactly 2 arguments: assoc and predicate".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let pred = &args[1];
            let mut new_map = HashMap::new();
            for (k, v) in map.iter() {
                let key_val = Value::Str(k.clone());
                let keep = apply_function(pred, &[key_val], env)?;
                if keep.to_bool() {
                    new_map.insert(k.clone(), v.clone());
                }
            }
            Ok(Value::Assoc(new_map))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── KeyMap ─────────────────────────────────────────────────────────────────

pub fn builtin_key_map(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KeyMap requires exactly 2 arguments: f and assoc".to_string(),
        ));
    }
    match &args[1] {
        Value::Assoc(map) => {
            let f = &args[0];
            let mut new_map = HashMap::with_capacity(map.len());
            for (k, v) in map.iter() {
                let key_val = Value::Str(k.clone());
                let new_key_val = apply_function(f, &[key_val], env)?;
                let new_key = value_to_key(&new_key_val);
                new_map.insert(new_key, v.clone());
            }
            Ok(Value::Assoc(new_map))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[1].type_name().to_string(),
        }),
    }
}

// ── KeyValueMap ────────────────────────────────────────────────────────────

pub fn builtin_key_value_map(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KeyValueMap requires exactly 2 arguments: f and assoc".to_string(),
        ));
    }
    match &args[1] {
        Value::Assoc(map) => {
            let f = &args[0];
            let mut result = Vec::with_capacity(map.len());
            for (k, v) in map.iter() {
                let key_val = Value::Str(k.clone());
                let pair = Value::List(vec![key_val, v.clone()]);
                let mapped = apply_function(f, &[pair], env)?;
                result.push(mapped);
            }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[1].type_name().to_string(),
        }),
    }
}

// ── KeyMemberQ ─────────────────────────────────────────────────────────────

pub fn builtin_key_member_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KeyMemberQ requires exactly 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let key = key_to_string(&args[1])?;
            Ok(Value::Bool(map.contains_key(&key)))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── KeyFreeQ ───────────────────────────────────────────────────────────────

pub fn builtin_key_free_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KeyFreeQ requires exactly 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let key = key_to_string(&args[1])?;
            Ok(Value::Bool(!map.contains_key(&key)))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── AssociateTo ────────────────────────────────────────────────────────────

pub fn builtin_associate_to(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "AssociateTo requires exactly 2 arguments: assoc and rules".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let mut new_map = map.clone();
            match &args[1] {
                Value::List(items) => {
                    for item in items {
                        match rule_entry(item, env) {
                            Some((key, value)) => {
                                new_map.insert(key, value);
                            }
                            None => {
                                return Err(EvalError::Error(
                                    "AssociateTo: each element must be a Rule".to_string(),
                                ));
                            }
                        }
                    }
                }
                single => match rule_entry(single, env) {
                    Some((key, value)) => {
                        new_map.insert(key, value);
                    }
                    None => {
                        return Err(EvalError::TypeError {
                            expected: "Rule or List of Rules".to_string(),
                            got: single.type_name().to_string(),
                        });
                    }
                },
            }
            Ok(Value::Assoc(new_map))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── KeyDropFrom ────────────────────────────────────────────────────────────

pub fn builtin_key_drop_from(args: &[Value]) -> Result<Value, EvalError> {
    // Same logic as KeyDrop for now (pure function returning new assoc)
    builtin_key_drop(args)
}

// ── Counts ─────────────────────────────────────────────────────────────────

pub fn builtin_counts(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Counts requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let mut counts: HashMap<String, i64> = HashMap::new();
            for item in items {
                let key = value_to_key(item);
                *counts.entry(key).or_insert(0) += 1;
            }
            let mut map = HashMap::with_capacity(counts.len());
            for (k, c) in counts {
                map.insert(k, Value::Integer(c.into()));
            }
            Ok(Value::Assoc(map))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── CountsBy ───────────────────────────────────────────────────────────────

pub fn builtin_counts_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "CountsBy requires exactly 2 arguments: list and f".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let f = &args[1];
            let mut counts: HashMap<String, i64> = HashMap::new();
            for item in items {
                let key_val = apply_function(f, &[item.clone()], env)?;
                let key = value_to_key(&key_val);
                *counts.entry(key).or_insert(0) += 1;
            }
            let mut map = HashMap::with_capacity(counts.len());
            for (k, c) in counts {
                map.insert(k, Value::Integer(c.into()));
            }
            Ok(Value::Assoc(map))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── GroupBy ────────────────────────────────────────────────────────────────

/// GroupBy[list, f] groups the elements of list by the values of f.
/// `GroupBy[list, f, red]` reduces each group by applying `red` to the list of
/// grouped elements.
pub fn builtin_group_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "GroupBy requires 2 or 3 arguments: list, f and optional reducer".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let f = &args[1];
            let mut groups: HashMap<String, Vec<Value>> = HashMap::new();
            for item in items {
                let key_val = apply_function(f, &[item.clone()], env)?;
                let key = value_to_key(&key_val);
                groups.entry(key).or_default().push(item.clone());
            }
            let mut map = HashMap::with_capacity(groups.len());
            for (k, vals) in groups {
                let group = Value::List(vals);
                let value = match args.get(2) {
                    Some(red) => apply_function(red, std::slice::from_ref(&group), env)?,
                    None => group,
                };
                map.insert(k, value);
            }
            Ok(Value::Assoc(map))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Merge ──────────────────────────────────────────────────────────────────

/// Merge[{assoc1, assoc2, ...}] merges associations; `Merge[list, f]` combines
/// collisions by applying `f` to the colliding values as separate arguments
/// (`f[v1, v2, ...]`), matching Wolfram Language. Without a combiner the last
/// value for a duplicated key wins.
pub fn builtin_merge(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Merge requires 1 or 2 arguments: list of assocs and optional combiner".to_string(),
        ));
    }
    match &args[0] {
        Value::List(assocs) => {
            let combiner = args.get(1);
            let mut merged: HashMap<String, Vec<Value>> = HashMap::new();
            for assoc in assocs {
                match assoc {
                    Value::Assoc(map) => {
                        for (k, v) in map.iter() {
                            merged.entry(k.clone()).or_default().push(v.clone());
                        }
                    }
                    _ => {
                        return Err(EvalError::Error(
                            "Merge: each element of the list must be an Assoc".to_string(),
                        ));
                    }
                }
            }
            let mut result = HashMap::with_capacity(merged.len());
            for (k, vals) in merged {
                let combined = if vals.len() == 1 {
                    vals.into_iter().next().unwrap()
                } else {
                    match combiner {
                        Some(f) => apply_function(f, &vals, env)?,
                        None => vals.into_iter().last().unwrap(),
                    }
                };
                result.insert(k, combined);
            }
            Ok(Value::Assoc(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── KeyUnion ───────────────────────────────────────────────────────────────

pub fn builtin_key_union(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "KeyUnion requires exactly 1 argument: a list of assocs".to_string(),
        ));
    }
    match &args[0] {
        Value::List(assocs) => {
            let mut all_keys: Vec<String> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for assoc in assocs {
                match assoc {
                    Value::Assoc(map) => {
                        for k in map.keys() {
                            if seen.insert(k.clone()) {
                                all_keys.push(k.clone());
                            }
                        }
                    }
                    _ => {
                        return Err(EvalError::Error(
                            "KeyUnion: each element must be an Assoc".to_string(),
                        ));
                    }
                }
            }
            Ok(Value::List(all_keys.into_iter().map(Value::Str).collect()))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── KeyIntersection ────────────────────────────────────────────────────────

pub fn builtin_key_intersection(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "KeyIntersection requires exactly 1 argument: a list of assocs".to_string(),
        ));
    }
    match &args[0] {
        Value::List(assocs) => {
            if assocs.is_empty() {
                return Ok(Value::List(vec![]));
            }
            // Start with keys from the first assoc
            let first = match &assocs[0] {
                Value::Assoc(map) => {
                    let mut keys: Vec<String> = map.keys().cloned().collect();
                    keys.sort();
                    keys
                }
                _ => {
                    return Err(EvalError::Error(
                        "KeyIntersection: each element must be an Assoc".to_string(),
                    ));
                }
            };
            // Intersect with each subsequent assoc
            let mut result: Vec<String> = first;
            for assoc in &assocs[1..] {
                match assoc {
                    Value::Assoc(map) => {
                        result.retain(|k| map.contains_key(k));
                    }
                    _ => {
                        return Err(EvalError::Error(
                            "KeyIntersection: each element must be an Assoc".to_string(),
                        ));
                    }
                }
            }
            result.sort();
            Ok(Value::List(result.into_iter().map(Value::Str).collect()))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── KeyComplement ──────────────────────────────────────────────────────────

pub fn builtin_key_complement(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KeyComplement requires exactly 2 arguments: assoc1, assoc2".to_string(),
        ));
    }
    let first = match &args[0] {
        Value::Assoc(map) => map,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Assoc".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let second = match &args[1] {
        Value::Assoc(map) => map,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Assoc".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let mut keys: Vec<String> = first
        .keys()
        .filter(|k| !second.contains_key(*k))
        .cloned()
        .collect();
    keys.sort();
    Ok(Value::List(keys.into_iter().map(Value::Str).collect()))
}

// ── Keys ────────────────────────────────────────────────────────────────

/// Keys[assoc] gives a list of the keys in an association. `Keys` also maps
/// over a list of associations.
pub fn builtin_keys(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Keys requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Ok(Value::List(
                keys.into_iter().map(|k| Value::Str(k.clone())).collect(),
            ))
        }
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(builtin_keys(std::slice::from_ref(item))?);
            }
            Ok(Value::List(out))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Values ──────────────────────────────────────────────────────────────

/// Values[assoc] gives a list of the values in an association. `Values` also
/// maps over a list of associations.
pub fn builtin_values(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Values requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Ok(Value::List(
                keys.into_iter().map(|k| map[k].clone()).collect(),
            ))
        }
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(builtin_values(std::slice::from_ref(item))?);
            }
            Ok(Value::List(out))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Lookup ──────────────────────────────────────────────────────────────

/// Lookup[assoc, key] returns the value associated with key, or
/// `Missing["KeyAbsent", key]` when the key is absent.
///
/// Also supports:
///   Lookup[assoc, key, default]        -> `default` when the key is absent
///   Lookup[assoc, {k1, k2, ...}]       -> list of lookups
///   Lookup[{assoc1, assoc2, ...}, key] -> list of lookups
pub fn builtin_lookup(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Lookup requires 2 or 3 arguments".to_string(),
        ));
    }
    lookup_rec(&args[0], &args[1], args.get(2))
}

fn lookup_rec(target: &Value, key: &Value, default: Option<&Value>) -> Result<Value, EvalError> {
    match target {
        Value::Assoc(map) => match key {
            Value::List(keys) => {
                let mut out = Vec::with_capacity(keys.len());
                for k in keys {
                    out.push(lookup_rec(target, k, default)?);
                }
                Ok(Value::List(out))
            }
            _ => {
                let k = value_to_key(key);
                Ok(match map.get(&k) {
                    Some(v) => v.clone(),
                    None => match default {
                        Some(d) => d.clone(),
                        None => missing_key(key),
                    },
                })
            }
        },
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(lookup_rec(item, key, default)?);
            }
            Ok(Value::List(out))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc or List".to_string(),
            got: target.type_name().to_string(),
        }),
    }
}

// ── AssociationThread ──────────────────────────────────────────────────────

/// AssociationThread[keys, values] — create an association pairing keys with
/// values. Also accepts the single-argument rule form
/// `AssociationThread[keys -> values]`.
pub fn builtin_association_thread(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "AssociationThread requires 1 or 2 arguments".to_string(),
        ));
    }
    let (keys, values) = if args.len() == 1 {
        match &args[0] {
            Value::Rule { lhs, rhs, .. } => (eval_held(lhs, env), eval_held(rhs, env)),
            Value::Pattern(Expr::Rule { lhs, rhs } | Expr::RuleDelayed { lhs, rhs }) => {
                (eval_pattern(lhs, env), eval_pattern(rhs, env))
            }
            _ => {
                return Err(EvalError::TypeError {
                    expected: "Rule".to_string(),
                    got: args[0].type_name().to_string(),
                });
            }
        }
    } else {
        (args[0].clone(), args[1].clone())
    };
    let keys = match &keys {
        Value::List(l) => l,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: keys.type_name().to_string(),
            });
        }
    };
    let values = match values {
        Value::List(l) => l,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: values.type_name().to_string(),
            });
        }
    };
    if keys.len() != values.len() {
        return Err(EvalError::Error(format!(
            "AssociationThread: key list length ({}) != value list length ({})",
            keys.len(),
            values.len()
        )));
    }
    let mut map = HashMap::new();
    for (k, v) in keys.iter().zip(values.iter()) {
        let key = value_to_key(k);
        map.insert(key, v.clone());
    }
    Ok(Value::Assoc(map))
}

// ── Association ────────────────────────────────────────────────────────────

/// `Association[rules...]` / `Association[{rules...}]` — construct an
/// association from a rule or list of rules. An already-built association is
/// returned unchanged. When an element is not a rule (or the argument shape is
/// otherwise inapplicable) the call is returned unevaluated, mirroring the
/// Wolfram Language convention.
pub fn builtin_association(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let elements: &[Value] = match args {
        [] => return Ok(Value::Assoc(HashMap::new())),
        [Value::Assoc(_)] => return Ok(args[0].clone()),
        [Value::List(items)] => items.as_slice(),
        _ => args,
    };
    let mut map = HashMap::with_capacity(elements.len());
    for element in elements {
        match rule_entry(element, env) {
            Some((key, value)) => {
                map.insert(key, value);
            }
            None => return unevaluated("Association", args),
        }
    }
    Ok(Value::Assoc(map))
}

// ── ValueCount ─────────────────────────────────────────────────────────────

/// `ValueCount[assoc]` — the number of distinct values in an association.
/// `ValueCount[assoc, pattern]` — the number of values matching `pattern`.
/// Returns unevaluated when the first argument is not an association.
pub fn builtin_value_count(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return unevaluated("ValueCount", args);
    }
    let map = match &args[0] {
        Value::Assoc(map) => map,
        _ => return unevaluated("ValueCount", args),
    };
    let mut count: i64 = 0;
    match args.get(1) {
        None => {
            let mut seen: Vec<&Value> = Vec::new();
            for value in map.values() {
                if !seen.iter().any(|s| s.struct_eq(value)) {
                    seen.push(value);
                    count += 1;
                }
            }
        }
        Some(held_pattern) => {
            let checker = AttributeChecker::new(env.attributes.clone());
            let pattern = match held_pattern {
                Value::Pattern(expr) => expr.clone(),
                other => crate::eval::table::value_to_expr(other),
            };
            for value in map.values() {
                if matches!(
                    match_pattern(&pattern, value, Some(&checker)),
                    MatchResult::Match(_)
                ) {
                    count += 1;
                }
            }
        }
    }
    Ok(Value::Integer(count.into()))
}

// ── KeyValueSelect ─────────────────────────────────────────────────────────

/// `KeyValueSelect[assoc, crit]` keeps the entries whose `key -> value` rule
/// satisfies `crit`. Also maps over a list of associations. Returns
/// unevaluated when inapplicable.
pub fn builtin_key_value_select(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("KeyValueSelect", args);
    }
    if let Value::List(items) = &args[0]
        && all_assocs(items)
    {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(builtin_key_value_select(
                &[item.clone(), args[1].clone()],
                env,
            )?);
        }
        return Ok(Value::List(out));
    }
    let map = match &args[0] {
        Value::Assoc(map) => map,
        _ => return unevaluated("KeyValueSelect", args),
    };
    let pred = &args[1];
    let mut new_map = HashMap::new();
    for (k, v) in map.iter() {
        let rule = Value::Rule {
            lhs: Box::new(Value::Str(k.clone())),
            rhs: Box::new(v.clone()),
            delayed: false,
        };
        if apply_function(pred, std::slice::from_ref(&rule), env)?.to_bool() {
            new_map.insert(k.clone(), v.clone());
        }
    }
    Ok(Value::Assoc(new_map))
}

// ── AssociationMap ─────────────────────────────────────────────────────────

/// `AssociationMap[f, list]` maps each element to `<|f[item] -> item|>`;
/// `AssociationMap[f, k1, ..., kn]` maps `<|f[k1, ..., kn] -> {k1, ..., kn}|>`.
/// Returns unevaluated when the shape is inapplicable (e.g. the second argument
/// is neither a list nor a key sequence).
pub fn builtin_association_map(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return unevaluated("AssociationMap", args);
    }
    let f = &args[0];
    if args.len() == 2 {
        let items = match &args[1] {
            Value::List(items) => items,
            _ => return unevaluated("AssociationMap", args),
        };
        let mut map = HashMap::with_capacity(items.len());
        for item in items {
            let key = apply_function(f, std::slice::from_ref(item), env)?;
            map.insert(value_to_key(&key), item.clone());
        }
        Ok(Value::Assoc(map))
    } else {
        let keys = &args[1..];
        let key = apply_function(f, keys, env)?;
        let mut map = HashMap::with_capacity(1);
        map.insert(value_to_key(&key), Value::List(keys.to_vec()));
        Ok(Value::Assoc(map))
    }
}

// ── KeyTake / KeyDrop / KeySelect extras ───────────────────────────────────

/// `KeyTake[assoc, keys]` extended to map over a list of associations and to
/// return unevaluated when the argument is not an association (or list thereof).
pub fn builtin_key_take_ext(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("KeyTake", args);
    }
    if let Value::List(items) = &args[0]
        && all_assocs(items)
    {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(builtin_key_take(&[item.clone(), args[1].clone()])?);
        }
        return Ok(Value::List(out));
    }
    match builtin_key_take(args) {
        Ok(v) => Ok(v),
        Err(EvalError::TypeError { .. }) => unevaluated("KeyTake", args),
        Err(e) => Err(e),
    }
}

/// `KeyDrop[assoc, keys]` extended to map over a list of associations and to
/// return unevaluated when the argument is not an association (or list thereof).
pub fn builtin_key_drop_ext(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("KeyDrop", args);
    }
    if let Value::List(items) = &args[0]
        && all_assocs(items)
    {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(builtin_key_drop(&[item.clone(), args[1].clone()])?);
        }
        return Ok(Value::List(out));
    }
    match builtin_key_drop(args) {
        Ok(v) => Ok(v),
        Err(EvalError::TypeError { .. }) => unevaluated("KeyDrop", args),
        Err(e) => Err(e),
    }
}

/// `KeySelect[assoc, pred]` extended to map over a list of associations and to
/// return unevaluated when the argument is not an association (or list thereof).
pub fn builtin_key_select_ext(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated("KeySelect", args);
    }
    if let Value::List(items) = &args[0]
        && all_assocs(items)
    {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(builtin_key_select(&[item.clone(), args[1].clone()], env)?);
        }
        return Ok(Value::List(out));
    }
    match builtin_key_select(args, env) {
        Ok(v) => Ok(v),
        Err(EvalError::TypeError { .. }) => unevaluated("KeySelect", args),
        Err(e) => Err(e),
    }
}

// ── MissingQ ───────────────────────────────────────────────────────────────

/// `MissingQ[x]` is `True` when `x` is a `Missing[...]` expression.
pub fn builtin_missing_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return unevaluated("MissingQ", args);
    }
    Ok(Value::Bool(
        matches!(&args[0], Value::Call { head, .. } if head == "Missing"),
    ))
}

// ── Registration ───────────────────────────────────────────────────────────

/// Register the association builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin_env(env, "Association", builtin_association);
    env.set_attributes("Association", vec!["Protected".to_string()]);

    // Re-register with list-mapping / unevaluated-fallback semantics.
    super::register_builtin(env, "KeyTake", builtin_key_take_ext);
    super::register_builtin(env, "KeyDrop", builtin_key_drop_ext);
    super::register_builtin_env(env, "KeySelect", builtin_key_select_ext);
    super::register_builtin_env(env, "KeyValueSelect", builtin_key_value_select);

    // A pattern second argument must stay held for structural matching.
    super::register_builtin_env(env, "ValueCount", builtin_value_count);
    env.set_attributes(
        "ValueCount",
        vec!["HoldRest".to_string(), "Protected".to_string()],
    );

    // Overrides the datamore definition with an identical-but-defensive version.
    super::register_builtin_env(env, "AssociationMap", builtin_association_map);

    super::register_builtin(env, "MissingQ", builtin_missing_q);
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn assoc(pairs: Vec<(&str, Value)>) -> Value {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v);
        }
        Value::Assoc(map)
    }

    fn list(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    fn string(s: &str) -> Value {
        Value::Str(s.to_string())
    }

    fn integer(n: i64) -> Value {
        Value::Integer(n.into())
    }

    fn make_env() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    fn eval_str(input: &str) -> Value {
        let env = make_env();
        let tokens = crate::lexer::tokenize(input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        crate::eval::eval_program(&ast, &env).unwrap()
    }

    #[test]
    fn test_association_q() {
        let a = assoc(vec![("x", integer(1))]);
        assert_eq!(builtin_association_q(&[a]).unwrap(), Value::Bool(true));
        assert_eq!(
            builtin_association_q(&[Value::List(vec![])]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_association_q(&[integer(42)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_normal() {
        let a = assoc(vec![("a", integer(1)), ("b", integer(2))]);
        let result = builtin_normal(&[a]).unwrap();
        match result {
            Value::List(rules) => {
                assert_eq!(rules.len(), 2);
                // Keys are sorted, so "a" then "b"
                assert!(matches!(&rules[0], Value::Rule { lhs, rhs, delayed: false }
                    if **lhs == Value::Str("a".to_string()) && **rhs == integer(1)));
                assert!(matches!(&rules[1], Value::Rule { lhs, rhs, delayed: false }
                    if **lhs == Value::Str("b".to_string()) && **rhs == integer(2)));
            }
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn test_key_sort() {
        let a = assoc(vec![("b", integer(2)), ("a", integer(1))]);
        let sorted = builtin_key_sort(&[a]).unwrap();
        match sorted {
            Value::Assoc(map) => {
                let _keys: Vec<&String> = map.keys().collect();
                // HashMap iteration order is not guaranteed, but the
                // entries should all be present
                assert_eq!(map.len(), 2);
                assert_eq!(map.get("a"), Some(&integer(1)));
                assert_eq!(map.get("b"), Some(&integer(2)));
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_key_sort_by_uses_unary_key_function() {
        let env = make_env();
        let a = assoc(vec![
            ("aaa", integer(1)),
            ("ab", integer(2)),
            ("abcd", integer(3)),
        ]);
        // StringLength is a unary key function; calling it with two arguments
        // would fail, so this exercises the unary path.
        let f = Value::Builtin(
            "StringLength".to_string(),
            crate::value::BuiltinFn::Pure(crate::builtins::string::builtin_string_length),
        );
        let result = builtin_key_sort_by(&[a, f], &env).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.len(), 3);
                assert_eq!(map.get("aaa"), Some(&integer(1)));
                assert_eq!(map.get("ab"), Some(&integer(2)));
                assert_eq!(map.get("abcd"), Some(&integer(3)));
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_key_sort_by_propagates_error() {
        // Previously a failing key function was silently swallowed by the
        // comparator fallback; it must now surface as an error.
        let env = make_env();
        let a = assoc(vec![("a", integer(1))]);
        let f = Value::Builtin(
            "boom".to_string(),
            crate::value::BuiltinFn::Pure(|_| Err(EvalError::Error("boom".to_string()))),
        );
        assert!(builtin_key_sort_by(&[a, f], &env).is_err());
    }

    #[test]
    fn test_key_take() {
        let a = assoc(vec![
            ("a", integer(1)),
            ("b", integer(2)),
            ("c", integer(3)),
        ]);
        let result = builtin_key_take(&[a, list(vec![string("a"), string("c")])]).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.len(), 2);
                assert_eq!(map.get("a"), Some(&integer(1)));
                assert_eq!(map.get("c"), Some(&integer(3)));
                assert_eq!(map.get("b"), None);
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_key_drop() {
        let a = assoc(vec![
            ("a", integer(1)),
            ("b", integer(2)),
            ("c", integer(3)),
        ]);
        let result = builtin_key_drop(&[a, list(vec![string("a"), string("c")])]).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.len(), 1);
                assert_eq!(map.get("b"), Some(&integer(2)));
                assert_eq!(map.get("a"), None);
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_key_select() {
        let env = make_env();
        // Key length > 1: select keys longer than 1 char
        let a = assoc(vec![
            ("x", integer(1)),
            ("yy", integer(2)),
            ("zzz", integer(3)),
        ]);
        let pred = Value::Builtin(
            "_pred".to_string(),
            crate::value::BuiltinFn::Pure(|args| match &args[0] {
                Value::Str(s) => Ok(Value::Bool(s.len() > 1)),
                _ => Ok(Value::Bool(false)),
            }),
        );
        let result = builtin_key_select(&[a, pred], &env).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.len(), 2);
                assert!(map.contains_key("yy"));
                assert!(map.contains_key("zzz"));
                assert!(!map.contains_key("x"));
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_key_map() {
        let env = make_env();
        let a = assoc(vec![("a", integer(1)), ("b", integer(2))]);
        // Map ToUpperCase over keys
        let f = Value::Builtin(
            "ToUpperCase".to_string(),
            crate::value::BuiltinFn::Pure(crate::builtins::string::builtin_to_upper_case),
        );
        let result = builtin_key_map(&[f, a], &env).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.len(), 2);
                assert_eq!(map.get("A"), Some(&integer(1)));
                assert_eq!(map.get("B"), Some(&integer(2)));
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_key_value_map() {
        let env = make_env();
        let a = assoc(vec![("x", integer(10)), ("y", integer(20))]);
        // Use a function that extracts the second element (the value) from {key, val}
        let extract_val = Value::Builtin(
            "_Extract".to_string(),
            crate::value::BuiltinFn::Pure(|args| match &args[0] {
                Value::List(items) if items.len() >= 2 => Ok(items[1].clone()),
                _ => Err(EvalError::Error("expected pair".to_string())),
            }),
        );
        let result = builtin_key_value_map(&[extract_val, a], &env).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert!(items.contains(&integer(10)));
                assert!(items.contains(&integer(20)));
            }
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn test_key_member_q() {
        let a = assoc(vec![("a", integer(1))]);
        assert_eq!(
            builtin_key_member_q(&[a.clone(), string("a")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_key_member_q(&[a, string("b")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_key_free_q() {
        let a = assoc(vec![("a", integer(1))]);
        assert_eq!(
            builtin_key_free_q(&[a.clone(), string("a")]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_key_free_q(&[a, string("b")]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_associate_to() {
        let env = make_env();
        let a = assoc(vec![("a", integer(1))]);
        let rule = Value::Rule {
            lhs: Box::new(string("b")),
            rhs: Box::new(integer(2)),
            delayed: false,
        };
        let result = builtin_associate_to(&[a, rule], &env).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.len(), 2);
                assert_eq!(map.get("a"), Some(&integer(1)));
                assert_eq!(map.get("b"), Some(&integer(2)));
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_associate_to_via_eval() {
        // A rule argument arrives with `Value::Pattern`-wrapped sides; the key
        // must not be the literal `Pattern[...]` and the value must be
        // evaluated.
        let result = eval_str("AssociateTo[<|\"a\"->1|>, \"b\"->(1+1)]");
        assert_eq!(result, assoc(vec![("a", integer(1)), ("b", integer(2))]));
        let result = eval_str("AssociateTo[<|\"a\"->1|>, {\"b\"->2, \"c\"->3}]");
        assert_eq!(
            result,
            assoc(vec![
                ("a", integer(1)),
                ("b", integer(2)),
                ("c", integer(3)),
            ])
        );
    }

    #[test]
    fn test_association_thread_rule_form_via_eval() {
        let result = eval_str("AssociationThread[{\"a\", \"b\"} -> {1, 2}]");
        assert_eq!(result, assoc(vec![("a", integer(1)), ("b", integer(2))]));
    }

    #[test]
    fn test_counts() {
        let items = list(vec![
            string("a"),
            string("b"),
            string("a"),
            string("c"),
            string("b"),
            string("a"),
        ]);
        let result = builtin_counts(&[items]).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.get("a"), Some(&integer(3)));
                assert_eq!(map.get("b"), Some(&integer(2)));
                assert_eq!(map.get("c"), Some(&integer(1)));
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_counts_by() {
        let env = make_env();
        // Count strings by their first character
        let items = list(vec![string("ab"), string("ac"), string("ba")]);
        let f = Value::Builtin(
            "StringTake".to_string(),
            crate::value::BuiltinFn::Pure(|args| match &args[0] {
                Value::Str(s) => Ok(Value::Str(s.chars().next().unwrap_or(' ').to_string())),
                _ => Err(EvalError::Error("expected string".to_string())),
            }),
        );
        let result = builtin_counts_by(&[items, f], &env).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.get("a"), Some(&integer(2)));
                assert_eq!(map.get("b"), Some(&integer(1)));
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_group_by() {
        let env = make_env();
        let items = list(vec![
            integer(1),
            integer(2),
            integer(3),
            integer(4),
            integer(5),
        ]);
        // Group by parity
        let f = Value::Builtin(
            "Mod".to_string(),
            crate::value::BuiltinFn::Pure(|args| {
                let n = match &args[0] {
                    Value::Integer(n) => n.clone(),
                    _ => return Err(EvalError::Error("expected integer".to_string())),
                };
                Ok(Value::Str(if n.to_i64().unwrap_or(0) % 2 == 0 {
                    "even".to_string()
                } else {
                    "odd".to_string()
                }))
            }),
        );
        let result = builtin_group_by(&[items, f], &env).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.len(), 2);
                assert!(map.contains_key("even"));
                assert!(map.contains_key("odd"));
                if let Some(Value::List(evens)) = map.get("even") {
                    assert_eq!(evens.len(), 2);
                }
                if let Some(Value::List(odds)) = map.get("odd") {
                    assert_eq!(odds.len(), 3);
                }
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_merge() {
        let env = make_env();
        let a1 = assoc(vec![("a", integer(1)), ("b", integer(2))]);
        let a2 = assoc(vec![("b", integer(3)), ("c", integer(4))]);
        let assocs = list(vec![a1, a2]);
        // Plus receives the colliding values as separate arguments.
        let f = Value::Builtin(
            "Plus".to_string(),
            crate::value::BuiltinFn::Pure(crate::builtins::arithmetic::builtin_plus),
        );
        let result = builtin_merge(&[assocs, f], &env).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.len(), 3);
                assert_eq!(map.get("a"), Some(&integer(1)));
                assert_eq!(map.get("c"), Some(&integer(4)));
                // "b" is merged: 2 + 3 = 5 with Total
                assert_eq!(map.get("b"), Some(&integer(5)));
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_key_union() {
        let a1 = assoc(vec![("a", integer(1)), ("b", integer(2))]);
        let a2 = assoc(vec![("b", integer(3)), ("c", integer(4))]);
        let assocs = list(vec![a1, a2]);
        let result = builtin_key_union(&[assocs]).unwrap();
        match result {
            Value::List(keys) => {
                let key_strs: Vec<String> = keys
                    .iter()
                    .map(|k| match k {
                        Value::Str(s) => s.clone(),
                        _ => panic!("expected string key"),
                    })
                    .collect();
                assert_eq!(key_strs.len(), 3);
                assert!(key_strs.contains(&"a".to_string()));
                assert!(key_strs.contains(&"b".to_string()));
                assert!(key_strs.contains(&"c".to_string()));
            }
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn test_key_intersection() {
        let a1 = assoc(vec![("a", integer(1)), ("b", integer(2))]);
        let a2 = assoc(vec![("b", integer(3)), ("c", integer(4))]);
        let assocs = list(vec![a1, a2]);
        let result = builtin_key_intersection(&[assocs]).unwrap();
        match result {
            Value::List(keys) => {
                let key_strs: Vec<String> = keys
                    .iter()
                    .map(|k| match k {
                        Value::Str(s) => s.clone(),
                        _ => panic!("expected string key"),
                    })
                    .collect();
                assert_eq!(key_strs, vec!["b"]);
            }
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn test_key_complement() {
        let a1 = assoc(vec![
            ("a", integer(1)),
            ("b", integer(2)),
            ("c", integer(3)),
        ]);
        let a2 = assoc(vec![("b", integer(4))]);
        let result = builtin_key_complement(&[a1, a2]).unwrap();
        match result {
            Value::List(keys) => {
                let key_strs: Vec<String> = keys
                    .iter()
                    .map(|k| match k {
                        Value::Str(s) => s.clone(),
                        _ => panic!("expected string key"),
                    })
                    .collect();
                assert_eq!(key_strs.len(), 2);
                assert!(key_strs.contains(&"a".to_string()));
                assert!(key_strs.contains(&"c".to_string()));
            }
            _ => panic!("Expected List"),
        }
    }

    // ── Keys, Values, Lookup tests ──

    #[test]
    fn test_keys_basic() {
        let a = assoc(vec![("x", integer(10)), ("y", integer(20))]);
        let result = builtin_keys(&[a]).unwrap();
        match result {
            Value::List(items) => {
                let key_strs: Vec<String> = items
                    .iter()
                    .map(|v| match v {
                        Value::Str(s) => s.clone(),
                        _ => panic!("Expected Str"),
                    })
                    .collect();
                assert!(key_strs.contains(&"x".to_string()));
                assert!(key_strs.contains(&"y".to_string()));
            }
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn test_keys_empty() {
        let a = assoc(vec![]);
        let result = builtin_keys(&[a]).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_values_basic() {
        let a = assoc(vec![("a", integer(1)), ("b", integer(2))]);
        let result = builtin_values(&[a]).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert!(items.contains(&integer(1)));
                assert!(items.contains(&integer(2)));
            }
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn test_lookup_found() {
        let a = assoc(vec![("key", integer(42))]);
        let key = Value::Str("key".to_string());
        let result = builtin_lookup(&[a, key]).unwrap();
        assert_eq!(result, integer(42));
    }

    #[test]
    fn test_lookup_not_found() {
        let a = assoc(vec![("key", integer(42))]);
        let key = Value::Str("nope".to_string());
        let result = builtin_lookup(&[a, key.clone()]).unwrap();
        // Absent keys return Missing["KeyAbsent", key].
        assert_eq!(result, missing_key(&key));
    }

    #[test]
    fn test_lookup_default() {
        let a = assoc(vec![("key", integer(42))]);
        let result = builtin_lookup(&[a.clone(), string("nope"), integer(0)]).unwrap();
        assert_eq!(result, integer(0));
        let result = builtin_lookup(&[a, string("key"), integer(0)]).unwrap();
        assert_eq!(result, integer(42));
    }

    #[test]
    fn test_lookup_key_list() {
        let a = assoc(vec![("a", integer(1)), ("b", integer(2))]);
        let result =
            builtin_lookup(&[a, list(vec![string("a"), string("b"), string("z")])]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![integer(1), integer(2), missing_key(&string("z")),])
        );
    }

    #[test]
    fn test_lookup_list_of_assocs() {
        let a1 = assoc(vec![("a", integer(1))]);
        let a2 = assoc(vec![("a", integer(2))]);
        let result = builtin_lookup(&[list(vec![a1, a2]), string("a")]).unwrap();
        assert_eq!(result, Value::List(vec![integer(1), integer(2)]));
    }

    #[test]
    fn test_lookup_numeric_key() {
        // Keys are stored canonically, so numeric lookups must match.
        let a = assoc(vec![("1", integer(10))]);
        assert_eq!(builtin_lookup(&[a, integer(1)]).unwrap(), integer(10));
    }

    #[test]
    fn test_key_exists_q_numeric_key() {
        let a = assoc(vec![("1", integer(10))]);
        assert_eq!(
            builtin_key_member_q(&[a, integer(1)]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_association_thread_rule_form() {
        let env = make_env();
        let result = builtin_association_thread(
            &[Value::Rule {
                lhs: Box::new(list(vec![string("a"), string("b")])),
                rhs: Box::new(list(vec![integer(1), integer(2)])),
                delayed: false,
            }],
            &env,
        )
        .unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.get("a"), Some(&integer(1)));
                assert_eq!(map.get("b"), Some(&integer(2)));
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_keys_values_round_trip() {
        // Keys/Values are both ordered the same way, so threading them back
        // together must reproduce the association exactly.
        let env = make_env();
        let a = assoc(vec![
            ("b", integer(2)),
            ("a", integer(1)),
            ("c", integer(3)),
        ]);
        let keys = builtin_keys(&[a.clone()]).unwrap();
        let values = builtin_values(&[a.clone()]).unwrap();
        let rebuilt = builtin_association_thread(&[keys, values], &env).unwrap();
        assert_eq!(rebuilt, a);
    }

    #[test]
    fn test_keys_over_list_of_assocs() {
        let a1 = assoc(vec![("a", integer(1))]);
        let a2 = assoc(vec![("b", integer(2))]);
        let result = builtin_keys(&[list(vec![a1, a2])]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![list(vec![string("a")]), list(vec![string("b")]),])
        );
    }

    #[test]
    fn test_merge_combiner_separate_args() {
        // WL applies the combiner to the colliding values as separate
        // arguments, i.e. f[v1, v2] (not f[{v1, v2}]).
        let env = make_env();
        let f = Value::Builtin(
            "argcount".to_string(),
            crate::value::BuiltinFn::Pure(|args| Ok(integer(args.len() as i64))),
        );
        let a1 = assoc(vec![("x", integer(1))]);
        let a2 = assoc(vec![("x", integer(2)), ("y", integer(3))]);
        let result = builtin_merge(&[list(vec![a1, a2]), f], &env).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.get("x"), Some(&integer(2)));
                assert_eq!(map.get("y"), Some(&integer(3)));
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_merge_without_combiner_last_wins() {
        let env = make_env();
        let a1 = assoc(vec![("x", integer(1))]);
        let a2 = assoc(vec![("x", integer(2))]);
        let result = builtin_merge(&[list(vec![a1, a2])], &env).unwrap();
        match result {
            Value::Assoc(map) => assert_eq!(map.get("x"), Some(&integer(2))),
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_group_by_reducer() {
        let env = make_env();
        let items = list(vec![integer(1), integer(2), integer(3), integer(4)]);
        let f = Value::Builtin(
            "evenq".to_string(),
            crate::value::BuiltinFn::Pure(|args| match &args[0] {
                Value::Integer(n) => Ok(Value::Str(if n.to_i64().unwrap_or(0) % 2 == 0 {
                    "even".to_string()
                } else {
                    "odd".to_string()
                })),
                _ => Err(EvalError::Error("expected integer".to_string())),
            }),
        );
        let red = Value::Builtin(
            "Total".to_string(),
            crate::value::BuiltinFn::Pure(crate::builtins::list::builtin_total),
        );
        let result = builtin_group_by(&[items, f, red], &env).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.get("even"), Some(&integer(6)));
                assert_eq!(map.get("odd"), Some(&integer(4)));
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_tally_first_occurrence_order() {
        // Tally lives in list.rs; verify the WL common case (first-occurrence
        // order, structural equality) without editing that module.
        let result = crate::builtins::list::builtin_tally(&[list(vec![
            string("b"),
            string("a"),
            string("b"),
            string("c"),
        ])])
        .unwrap();
        assert_eq!(
            result,
            list(vec![
                list(vec![string("b"), integer(2)]),
                list(vec![string("a"), integer(1)]),
                list(vec![string("c"), integer(1)]),
            ])
        );
    }

    // ── Association constructor ──

    #[test]
    fn test_association_variadic_and_list_form() {
        let expected = assoc(vec![("a", integer(1)), ("b", integer(2))]);
        assert_eq!(eval_str("Association[\"a\"->1, \"b\"->2]"), expected);
        assert_eq!(eval_str("Association[{\"a\"->1, \"b\"->2}]"), expected);
    }

    #[test]
    fn test_association_empty_and_identity() {
        assert_eq!(eval_str("Association[]"), assoc(vec![]));
        let a = assoc(vec![("a", integer(1))]);
        assert_eq!(
            builtin_association(&[a.clone()], &make_env()).unwrap(),
            a.clone()
        );
        assert_eq!(eval_str("Association[<|\"a\"->1|>]"), a);
    }

    #[test]
    fn test_association_unevaluated_on_non_rule() {
        assert_eq!(
            eval_str("Association[1, 2]"),
            Value::Call {
                head: "Association".to_string(),
                args: vec![integer(1), integer(2)],
            }
        );
        let env = make_env();
        assert_eq!(
            builtin_association(&[integer(7)], &env).unwrap(),
            Value::Call {
                head: "Association".to_string(),
                args: vec![integer(7)],
            }
        );
    }

    // ── ValueCount ──

    #[test]
    fn test_value_count_distinct() {
        assert_eq!(
            eval_str("ValueCount[<|\"a\"->1, \"b\"->1, \"c\"->2|>]"),
            integer(2)
        );
        assert_eq!(eval_str("ValueCount[<||>]"), integer(0));
    }

    #[test]
    fn test_value_count_with_pattern() {
        assert_eq!(
            eval_str("ValueCount[<|\"a\"->1, \"b\"->2, \"c\"->2|>, _Integer]"),
            integer(3)
        );
        assert_eq!(
            eval_str("ValueCount[<|\"a\"->1, \"b\"->2|>, 2]"),
            integer(1)
        );
        assert_eq!(
            eval_str("ValueCount[<|\"a\"->\"x\", \"b\"->2|>, _String]"),
            integer(1)
        );
    }

    #[test]
    fn test_value_count_unevaluated() {
        assert_eq!(
            eval_str("ValueCount[5]"),
            Value::Call {
                head: "ValueCount".to_string(),
                args: vec![integer(5)],
            }
        );
    }

    // ── KeyValueSelect ──

    #[test]
    fn test_key_value_select() {
        let env = make_env();
        let a = assoc(vec![
            ("a", integer(1)),
            ("b", integer(2)),
            ("c", integer(3)),
        ]);
        // Keep entries whose value (the rule rhs) is greater than 1.
        let pred = Value::Builtin(
            "_gt1".to_string(),
            crate::value::BuiltinFn::Pure(|args| match &args[0] {
                Value::Rule { rhs, .. } => Ok(Value::Bool(
                    matches!(&**rhs, Value::Integer(n) if n.to_i64().unwrap_or(0) > 1),
                )),
                _ => Ok(Value::Bool(false)),
            }),
        );
        let result = builtin_key_value_select(&[a, pred], &env).unwrap();
        match result {
            Value::Assoc(map) => {
                assert_eq!(map.len(), 2);
                assert_eq!(map.get("b"), Some(&integer(2)));
                assert_eq!(map.get("c"), Some(&integer(3)));
                assert!(!map.contains_key("a"));
            }
            _ => panic!("Expected Assoc"),
        }
    }

    #[test]
    fn test_key_value_select_unevaluated() {
        let env = make_env();
        assert_eq!(
            builtin_key_value_select(&[integer(1), Value::Bool(true)], &env).unwrap(),
            Value::Call {
                head: "KeyValueSelect".to_string(),
                args: vec![integer(1), Value::Bool(true)],
            }
        );
    }

    // ── AssociationMap edge cases ──

    #[test]
    fn test_association_map_via_eval() {
        let result = eval_str("AssociationMap[StringLength, {\"a\", \"bb\"}]");
        assert_eq!(result, assoc(vec![("1", string("a")), ("2", string("bb"))]));
    }

    #[test]
    fn test_association_map_edge_cases() {
        let env = make_env();
        let f = Value::Builtin(
            "id".to_string(),
            crate::value::BuiltinFn::Pure(|args| Ok(args[0].clone())),
        );
        // Empty list -> empty association.
        assert_eq!(
            builtin_association_map(&[f.clone(), list(vec![])], &env).unwrap(),
            assoc(vec![])
        );
        // Key-sequence form.
        let result = builtin_association_map(&[f.clone(), string("k")], &env).unwrap();
        // Note: two-arg form requires a list, so a bare key is inapplicable.
        assert_eq!(
            result,
            Value::Call {
                head: "AssociationMap".to_string(),
                args: vec![f.clone(), string("k")],
            }
        );
        let seq = builtin_association_map(&[f.clone(), string("k1"), string("k2")], &env).unwrap();
        match seq {
            Value::Assoc(map) => {
                assert_eq!(map.get("k1"), Some(&list(vec![string("k1"), string("k2")])));
            }
            _ => panic!("Expected Assoc"),
        }
        // Too few arguments -> unevaluated.
        assert_eq!(
            builtin_association_map(&[f], &env).unwrap(),
            Value::Call {
                head: "AssociationMap".to_string(),
                args: vec![Value::Builtin(
                    "id".to_string(),
                    crate::value::BuiltinFn::Pure(|args| Ok(args[0].clone())),
                )],
            }
        );
    }

    // ── KeyTake / KeyDrop / KeySelect extras ──

    #[test]
    fn test_key_take_ext_maps_over_list() {
        let a1 = assoc(vec![("a", integer(1)), ("b", integer(2))]);
        let a2 = assoc(vec![("a", integer(3)), ("b", integer(4))]);
        let result = builtin_key_take_ext(&[list(vec![a1, a2]), string("a")]).unwrap();
        assert_eq!(
            result,
            list(vec![
                assoc(vec![("a", integer(1))]),
                assoc(vec![("a", integer(3))]),
            ])
        );
    }

    #[test]
    fn test_key_take_ext_unevaluated() {
        assert_eq!(
            builtin_key_take_ext(&[integer(5), string("a")]).unwrap(),
            Value::Call {
                head: "KeyTake".to_string(),
                args: vec![integer(5), string("a")],
            }
        );
    }

    #[test]
    fn test_key_drop_ext_maps_over_list() {
        let a1 = assoc(vec![("a", integer(1)), ("b", integer(2))]);
        let a2 = assoc(vec![("a", integer(3)), ("b", integer(4))]);
        let result = builtin_key_drop_ext(&[list(vec![a1, a2]), string("b")]).unwrap();
        assert_eq!(
            result,
            list(vec![
                assoc(vec![("a", integer(1))]),
                assoc(vec![("a", integer(3))]),
            ])
        );
        assert_eq!(
            builtin_key_drop_ext(&[integer(5), string("a")]).unwrap(),
            Value::Call {
                head: "KeyDrop".to_string(),
                args: vec![integer(5), string("a")],
            }
        );
    }

    #[test]
    fn test_key_select_ext_maps_over_list_and_unevaluated() {
        let env = make_env();
        let pred = Value::Builtin(
            "_long".to_string(),
            crate::value::BuiltinFn::Pure(|args| match &args[0] {
                Value::Str(s) => Ok(Value::Bool(s.len() > 1)),
                _ => Ok(Value::Bool(false)),
            }),
        );
        let a1 = assoc(vec![("a", integer(1)), ("bb", integer(2))]);
        let result = builtin_key_select_ext(&[list(vec![a1]), pred.clone()], &env).unwrap();
        assert_eq!(result, list(vec![assoc(vec![("bb", integer(2))])]));

        assert_eq!(
            builtin_key_select_ext(&[integer(5), pred], &env).unwrap(),
            Value::Call {
                head: "KeySelect".to_string(),
                args: vec![
                    integer(5),
                    Value::Builtin(
                        "_long".to_string(),
                        crate::value::BuiltinFn::Pure(|args| match &args[0] {
                            Value::Str(s) => Ok(Value::Bool(s.len() > 1)),
                            _ => Ok(Value::Bool(false)),
                        }),
                    ),
                ],
            }
        );
    }

    // ── MissingQ ──

    #[test]
    fn test_missing_q() {
        assert_eq!(
            eval_str("MissingQ[Missing[\"KeyAbsent\", \"x\"]]"),
            Value::Bool(true)
        );
        assert_eq!(eval_str("MissingQ[1]"), Value::Bool(false));
        assert_eq!(
            builtin_missing_q(&[missing_key(&string("x"))]).unwrap(),
            Value::Bool(true)
        );
    }
}
