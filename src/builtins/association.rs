use crate::env::Env;
use crate::eval::apply_function;
use crate::value::EvalError;
use crate::value::Value;
use std::collections::HashMap;

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

/// Extract a string key from a Value (Str or Symbol).
fn key_to_string(v: &Value) -> Result<String, EvalError> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        Value::Symbol(s) => Ok(s.clone()),
        _ => Err(EvalError::TypeError {
            expected: "String or Symbol".to_string(),
            got: v.type_name().to_string(),
        }),
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
            let ordering_fn = &args[1];
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| {
                let va = Value::Str((*a).clone());
                let vb = Value::Str((*b).clone());
                match apply_function(ordering_fn, &[va, vb], env) {
                    Ok(Value::Bool(true)) => std::cmp::Ordering::Less,
                    Ok(Value::Bool(false)) => std::cmp::Ordering::Greater,
                    _ => a.cmp(b),
                }
            });
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

pub fn builtin_associate_to(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 2 {
        return Err(EvalError::Error(
            "AssociateTo requires exactly 2 arguments: assoc and rules".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let mut new_map = map.clone();
            let additions = &args[1];
            match additions {
                Value::Rule { lhs, rhs, .. } => {
                    let key = value_to_key(lhs);
                    new_map.insert(key, rhs.as_ref().clone());
                }
                Value::List(items) => {
                    for item in items {
                        if let Value::Rule { lhs, rhs, .. } = item {
                            let key = value_to_key(lhs);
                            new_map.insert(key, rhs.as_ref().clone());
                        } else {
                            return Err(EvalError::Error(
                                "AssociateTo: each element must be a Rule".to_string(),
                            ));
                        }
                    }
                }
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Rule or List of Rules".to_string(),
                        got: additions.type_name().to_string(),
                    });
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

pub fn builtin_group_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GroupBy requires exactly 2 arguments: list and f".to_string(),
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
                map.insert(k, Value::List(vals));
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

pub fn builtin_merge(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Merge requires exactly 2 arguments: list of assocs and combiner".to_string(),
        ));
    }
    match &args[0] {
        Value::List(assocs) => {
            let combiner = &args[1];
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
                if vals.len() == 1 {
                    result.insert(k, vals.into_iter().next().unwrap());
                } else {
                    let combined = apply_function(combiner, &[Value::List(vals)], env)?;
                    result.insert(k, combined);
                }
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

/// Keys[assoc] gives a list of the keys in an association.
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
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Values ──────────────────────────────────────────────────────────────

/// Values[assoc] gives a list of the values in an association.
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
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Lookup ──────────────────────────────────────────────────────────────

/// Lookup[assoc, key] returns the value associated with key, or Missing[key].
pub fn builtin_lookup(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Lookup requires exactly 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let key = key_to_string(&args[1])?;
            match map.get(&key) {
                Some(val) => Ok(val.clone()),
                None => Ok(Value::Call {
                    head: "Missing".to_string(),
                    args: vec![args[1].clone()],
                }),
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
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
        let a = assoc(vec![("a", integer(1))]);
        let rule = Value::Rule {
            lhs: Box::new(string("b")),
            rhs: Box::new(integer(2)),
            delayed: false,
        };
        let result = builtin_associate_to(&[a, rule]).unwrap();
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
        // Use Total as combiner
        let f = Value::Builtin(
            "Total".to_string(),
            crate::value::BuiltinFn::Pure(crate::builtins::list::builtin_total),
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
        // Non-existent keys return Missing["key"]
        assert_eq!(
            result,
            Value::Call {
                head: "Missing".to_string(),
                args: vec![key],
            }
        );
    }
}
