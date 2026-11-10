//! association/dataset extras: KeyValuePattern, AssociationMap, KeyUnion, TakeLargest/Smallest, MaxBy/MinBy, OrderingBy, CountsBy.
//!
//! Gap-fill module; see the individual builtin docs for semantics.

use rug::Integer;
use std::cmp::Ordering;
use std::collections::HashMap;

use crate::ast::Expr;
use crate::env::Env;
use crate::eval::apply_function;
use crate::value::{EvalError, Value};

use super::list::compare_values;

// ── Helpers ────────────────────────────────────────────────────────────────

/// Build an unevaluated `head[args...]` result.
fn unevaluated(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Call {
        head: name.to_string(),
        args: args.to_vec(),
    })
}

/// True for `Missing[...]` values.
fn is_missing(v: &Value) -> bool {
    matches!(v, Value::Call { head, .. } if head == "Missing")
}

/// Parse a count argument, accepting the `UpTo[n]` wrapper.
fn parse_count(v: &Value) -> Option<i64> {
    match v {
        Value::Call { head, args } if head == "UpTo" && args.len() == 1 => args[0].to_integer(),
        other => other.to_integer(),
    }
}

/// 0-based indices ordering `keys` ascending or descending (stable).
fn sort_indices_by_keys(keys: &[Value], descending: bool) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..keys.len()).collect();
    idx.sort_by(|&a, &b| {
        let ord = compare_values(&keys[a], &keys[b]);
        if descending { ord.reverse() } else { ord }
    });
    idx
}

/// Select `n` elements (from the front, or the back when `n` is negative)
/// following a pre-computed ordering of `items`.
fn slice_by_count(items: &[Value], sorted: &[usize], n: i64) -> Vec<Value> {
    let count = (n.unsigned_abs() as usize).min(sorted.len());
    let slice = if n < 0 {
        &sorted[sorted.len() - count..]
    } else {
        &sorted[..count]
    };
    slice.iter().map(|&i| items[i].clone()).collect()
}

/// Shared implementation of `TakeLargest`/`TakeSmallest`.
fn take_extreme(name: &str, args: &[Value], descending: bool) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return unevaluated(name, args);
    }
    let items = match &args[0] {
        Value::List(items) => items,
        _ => return unevaluated(name, args),
    };
    let single = args.len() == 1;
    let n = match args.get(1) {
        None => 1,
        Some(v) => match parse_count(v) {
            Some(n) => n,
            None => return unevaluated(name, args),
        },
    };
    let sorted = sort_indices_by_keys(items, descending);
    take_result(name, items, &sorted, n, single, args)
}

/// Turn a sorted index list into the final result (single value or list).
fn take_result(
    name: &str,
    items: &[Value],
    sorted: &[usize],
    n: i64,
    single: bool,
    args: &[Value],
) -> Result<Value, EvalError> {
    let chosen = slice_by_count(items, sorted, n);
    if single {
        match chosen.into_iter().next() {
            Some(v) => Ok(v),
            None => unevaluated(name, args),
        }
    } else {
        Ok(Value::List(chosen))
    }
}

/// Shared implementation of `TakeLargestBy`/`TakeSmallestBy`.
fn take_extreme_by(
    name: &str,
    args: &[Value],
    descending: bool,
    env: &Env,
) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return unevaluated(name, args);
    }
    let items = match &args[0] {
        Value::List(items) => items,
        _ => return unevaluated(name, args),
    };
    let f = &args[1];
    let mut keys = Vec::with_capacity(items.len());
    for item in items {
        keys.push(apply_function(f, std::slice::from_ref(item), env)?);
    }
    let single = args.len() == 2;
    let n = match args.get(2) {
        None => 1,
        Some(v) => match parse_count(v) {
            Some(n) => n,
            None => return unevaluated(name, args),
        },
    };
    let sorted = sort_indices_by_keys(&keys, descending);
    take_result(name, items, &sorted, n, single, args)
}

/// Shared implementation of `MaxBy`/`MinBy`.
fn extreme_by(name: &str, args: &[Value], want_max: bool, env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return unevaluated(name, args);
    }
    let items = match &args[0] {
        Value::List(items) => items,
        _ => return unevaluated(name, args),
    };
    let f = &args[1];
    let mut best: Option<(usize, Value)> = None;
    for (i, item) in items.iter().enumerate() {
        let key = apply_function(f, std::slice::from_ref(item), env)?;
        let replace = match &best {
            None => true,
            Some((_, best_key)) => {
                let ord = compare_values(&key, best_key);
                if want_max {
                    ord == std::cmp::Ordering::Greater
                } else {
                    ord == std::cmp::Ordering::Less
                }
            }
        };
        if replace {
            best = Some((i, key));
        }
    }
    match best {
        Some((i, _)) => Ok(items[i].clone()),
        None => unevaluated(name, args),
    }
}

// ── KeyValuePattern ────────────────────────────────────────────────────────

/// `KeyValuePattern[{k1 -> p1, ...}]` — a pattern matching an association that
/// contains at least the given key/value patterns. The actual matching is
/// performed by the pattern engine (`crate::pattern`); this builtin only wraps
/// the held argument back into a `Value::Pattern`.
pub fn builtin_key_value_pattern(args: &[Value]) -> Result<Value, EvalError> {
    let inner = match args.first() {
        Some(Value::Pattern(e)) => e.clone(),
        Some(other) => crate::eval::table::value_to_expr(other),
        None => return unevaluated("KeyValuePattern", args),
    };
    Ok(Value::Pattern(Expr::Call {
        head: Box::new(Expr::Symbol("KeyValuePattern".to_string())),
        args: vec![inner],
    }))
}

// ── KeyUnion ───────────────────────────────────────────────────────────────

/// `KeyUnion[{assoc1, assoc2, ...}]` (and `KeyUnion[{...}, default]`) returns a
/// list of associations that all share the union of the input keys, with
/// missing values filled in with `default` or `Missing["KeyAbsent", key]`.
pub fn builtin_key_union(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return unevaluated("KeyUnion", args);
    }
    let assocs = match &args[0] {
        Value::List(items) => items,
        _ => return unevaluated("KeyUnion", args),
    };
    let default = args.get(1);

    let mut all_keys: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for a in assocs {
        match a {
            Value::Assoc(map) => {
                for k in map.keys() {
                    if seen.insert(k.clone()) {
                        all_keys.push(k.clone());
                    }
                }
            }
            _ => return unevaluated("KeyUnion", args),
        }
    }

    let mut result = Vec::with_capacity(assocs.len());
    for a in assocs {
        let map = match a {
            Value::Assoc(map) => map,
            _ => return unevaluated("KeyUnion", args),
        };
        let mut new_map = HashMap::with_capacity(all_keys.len());
        for k in &all_keys {
            let v = match map.get(k) {
                Some(v) => v.clone(),
                None => match default {
                    Some(d) => d.clone(),
                    None => Value::Call {
                        head: "Missing".to_string(),
                        args: vec![Value::Str("KeyAbsent".to_string()), Value::Str(k.clone())],
                    },
                },
            };
            new_map.insert(k.clone(), v);
        }
        result.push(Value::Assoc(new_map));
    }
    Ok(Value::List(result))
}

// ── TakeLargest / TakeSmallest (and *By) ───────────────────────────────────

/// `TakeLargest[list]` / `TakeLargest[list, n]`.
pub fn builtin_take_largest(args: &[Value]) -> Result<Value, EvalError> {
    take_extreme("TakeLargest", args, true)
}

/// `TakeSmallest[list]` / `TakeSmallest[list, n]`.
pub fn builtin_take_smallest(args: &[Value]) -> Result<Value, EvalError> {
    take_extreme("TakeSmallest", args, false)
}

/// `TakeLargestBy[list, f]` / `TakeLargestBy[list, f, n]`.
pub fn builtin_take_largest_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    take_extreme_by("TakeLargestBy", args, true, env)
}

/// `TakeSmallestBy[list, f]` / `TakeSmallestBy[list, f, n]`.
pub fn builtin_take_smallest_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    take_extreme_by("TakeSmallestBy", args, false, env)
}

// ── MaxBy / MinBy ──────────────────────────────────────────────────────────

/// `MaxBy[list, f]` — the element maximizing `f`.
pub fn builtin_max_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    extreme_by("MaxBy", args, true, env)
}

/// `MinBy[list, f]` — the element minimizing `f`.
pub fn builtin_min_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    extreme_by("MinBy", args, false, env)
}

// ── MaximalBy / MinimalBy ──────────────────────────────────────────────────

/// Shared implementation of `MaximalBy`/`MinimalBy`.
///
/// `MaximalBy[list, f]` returns every element whose `f`-value is extremal, in
/// order of appearance in `list`. `MaximalBy[list, f, n]` returns the `n`
/// elements with the largest `f`-values, in decreasing order (ties broken by
/// input order); `UpTo[n]` and negative `n` are accepted via `parse_count`.
/// Returns unevaluated when `list` is not a list or `n` is not a count.
fn maximal_by(name: &str, args: &[Value], want_max: bool, env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return unevaluated(name, args);
    }
    let items = match &args[0] {
        Value::List(items) => items,
        _ => return unevaluated(name, args),
    };
    let f = &args[1];
    let mut keys = Vec::with_capacity(items.len());
    for item in items {
        keys.push(apply_function(f, std::slice::from_ref(item), env)?);
    }

    if args.len() == 2 {
        // Keep every element attaining the extremal key, in input order.
        let extremum = keys.iter().cloned().reduce(|a, b| {
            let take_b = match compare_values(&a, &b) {
                Ordering::Less => want_max,
                Ordering::Greater => !want_max,
                Ordering::Equal => false,
            };
            if take_b { b } else { a }
        });
        let chosen = match extremum {
            None => Vec::new(),
            Some(extremum) => items
                .iter()
                .zip(keys.iter())
                .filter(|(_, k)| compare_values(k, &extremum) == Ordering::Equal)
                .map(|(item, _)| item.clone())
                .collect(),
        };
        return Ok(Value::List(chosen));
    }

    let n = match parse_count(&args[2]) {
        Some(n) => n,
        None => return unevaluated(name, args),
    };
    let sorted = sort_indices_by_keys(&keys, want_max);
    Ok(Value::List(slice_by_count(items, &sorted, n)))
}

/// `MaximalBy[list, f]` / `MaximalBy[list, f, n]` — elements maximizing `f`.
pub fn builtin_maximal_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    maximal_by("MaximalBy", args, true, env)
}

/// `MinimalBy[list, f]` / `MinimalBy[list, f, n]` — elements minimizing `f`.
pub fn builtin_minimal_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    maximal_by("MinimalBy", args, false, env)
}

// ── OrderingBy ─────────────────────────────────────────────────────────────

/// `OrderingBy[list, f]` / `OrderingBy[list, f, n]` — positions sorting `list`
/// by the key produced by `f`.
pub fn builtin_ordering_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return unevaluated("OrderingBy", args);
    }
    let items = match &args[0] {
        Value::List(items) => items,
        _ => return unevaluated("OrderingBy", args),
    };
    let f = &args[1];
    let mut keys = Vec::with_capacity(items.len());
    for item in items {
        keys.push(apply_function(f, std::slice::from_ref(item), env)?);
    }
    let sorted = sort_indices_by_keys(&keys, false);
    if args.len() == 2 {
        return Ok(Value::List(positions(&sorted)));
    }
    let n = match parse_count(&args[2]) {
        Some(n) => n,
        None => return unevaluated("OrderingBy", args),
    };
    let count = (n.unsigned_abs() as usize).min(sorted.len());
    let slice = if n < 0 {
        &sorted[sorted.len() - count..]
    } else {
        &sorted[..count]
    };
    Ok(Value::List(positions(slice)))
}

/// Map 0-based indices to 1-based `Integer` positions.
fn positions(sorted: &[usize]) -> Vec<Value> {
    sorted
        .iter()
        .map(|&i| Value::Integer(Integer::from((i + 1) as i64)))
        .collect()
}

// ── DeleteMissing ──────────────────────────────────────────────────────────

/// `DeleteMissing[list]` / `DeleteMissing[assoc]` — remove `Missing[...]` entries.
pub fn builtin_delete_missing(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return unevaluated("DeleteMissing", args);
    }
    match &args[0] {
        Value::List(items) => Ok(Value::List(
            items.iter().filter(|x| !is_missing(x)).cloned().collect(),
        )),
        Value::Assoc(map) => Ok(Value::Assoc(
            map.iter()
                .filter(|(_, x)| !is_missing(x))
                .map(|(k, x)| (k.clone(), x.clone()))
                .collect(),
        )),
        _ => unevaluated("DeleteMissing", args),
    }
}

// ── Registration ───────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "KeyValuePattern", builtin_key_value_pattern);
    env.set_attributes(
        "KeyValuePattern",
        vec!["HoldAll".to_string(), "Locked".to_string()],
    );

    // `AssociationMap` is owned by `association::register`, which runs later;
    // `KeyUnion` is owned here.
    super::register_builtin(env, "KeyUnion", builtin_key_union);
    super::register_builtin(env, "TakeLargest", builtin_take_largest);
    super::register_builtin(env, "TakeSmallest", builtin_take_smallest);
    super::register_builtin_env(env, "TakeLargestBy", builtin_take_largest_by);
    super::register_builtin_env(env, "TakeSmallestBy", builtin_take_smallest_by);
    super::register_builtin_env(env, "MaxBy", builtin_max_by);
    super::register_builtin_env(env, "MinBy", builtin_min_by);
    super::register_builtin_env(env, "MaximalBy", builtin_maximal_by);
    super::register_builtin_env(env, "MinimalBy", builtin_minimal_by);
    super::register_builtin_env(env, "OrderingBy", builtin_ordering_by);
    super::register_builtin(env, "DeleteMissing", builtin_delete_missing);
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use crate::lexer;
    use crate::parser;

    fn eval_str(input: &str) -> Value {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn({
                let input = input.to_string();
                move || {
                    let env = Env::new();
                    builtins::register_builtins(&env);
                    let tokens = lexer::tokenize(&input).unwrap();
                    let ast = parser::parse(tokens).unwrap();
                    crate::eval::eval_program(&ast, &env).unwrap()
                }
            })
            .unwrap()
            .join()
            .unwrap()
    }

    fn list(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    fn integer(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn string(s: &str) -> Value {
        Value::Str(s.to_string())
    }

    fn assoc(pairs: Vec<(&str, Value)>) -> Value {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v);
        }
        Value::Assoc(map)
    }

    // ── TakeLargest / TakeSmallest ──

    #[test]
    fn test_take_largest() {
        assert_eq!(
            eval_str("TakeLargest[{3, 1, 4, 1, 5, 9, 2, 6}]"),
            integer(9)
        );
        assert_eq!(
            eval_str("TakeLargest[{3, 1, 4, 1, 5, 9, 2, 6}, 3]"),
            list(vec![integer(9), integer(6), integer(5)])
        );
    }

    #[test]
    fn test_take_smallest() {
        assert_eq!(eval_str("TakeSmallest[{3, 1, 4, 1, 5}]"), integer(1));
        assert_eq!(
            eval_str("TakeSmallest[{3, 1, 4, 1, 5}, 2]"),
            list(vec![integer(1), integer(1)])
        );
    }

    #[test]
    fn test_take_largest_negative_count() {
        // Negative count takes from the other end.
        assert_eq!(
            eval_str("TakeLargest[{1, 2, 3, 4, 5}, -2]"),
            list(vec![integer(2), integer(1)])
        );
        assert_eq!(
            eval_str("TakeSmallest[{1, 2, 3, 4, 5}, -2]"),
            list(vec![integer(4), integer(5)])
        );
    }

    #[test]
    fn test_take_extreme_unevaluated() {
        assert_eq!(
            eval_str("TakeLargest[x]"),
            Value::Call {
                head: "TakeLargest".to_string(),
                args: vec![Value::Symbol("x".to_string())],
            }
        );
        assert_eq!(
            eval_str("TakeLargest[{1, 2}, y]"),
            Value::Call {
                head: "TakeLargest".to_string(),
                args: vec![
                    list(vec![integer(1), integer(2)]),
                    Value::Symbol("y".to_string())
                ],
            }
        );
    }

    #[test]
    fn test_take_extreme_by() {
        let data = "{{1, \"a\"}, {3, \"c\"}, {2, \"b\"}}";
        assert_eq!(
            eval_str(&format!("TakeLargestBy[{data}, First]")),
            list(vec![integer(3), string("c")])
        );
        assert_eq!(
            eval_str(&format!("TakeSmallestBy[{data}, First, 2]")),
            list(vec![
                list(vec![integer(1), string("a")]),
                list(vec![integer(2), string("b")]),
            ])
        );
    }

    // ── MaxBy / MinBy ──

    #[test]
    fn test_max_by_min_by() {
        let data = "{{1, \"a\"}, {3, \"c\"}, {2, \"b\"}}";
        assert_eq!(
            eval_str(&format!("MaxBy[{data}, First]")),
            list(vec![integer(3), string("c")])
        );
        assert_eq!(
            eval_str(&format!("MinBy[{data}, First]")),
            list(vec![integer(1), string("a")])
        );
    }

    #[test]
    fn test_max_by_empty_unevaluated() {
        let out = eval_str("MaxBy[{}, First]");
        match out {
            Value::Call { head, args } => {
                assert_eq!(head, "MaxBy");
                assert_eq!(args[0], list(vec![]));
            }
            other => panic!("expected unevaluated MaxBy, got {other:?}"),
        }
    }

    // ── MaximalBy / MinimalBy ──

    #[test]
    fn test_maximal_by_all_maxima_in_order() {
        // Every element attaining the maximal key, in input order.
        assert_eq!(
            eval_str("MaximalBy[{{a, 1}, {b, 1}, {b, 3}, {d, 2}, {a, 3}}, Last]"),
            list(vec![
                list(vec![Value::Symbol("b".to_string()), integer(3)]),
                list(vec![Value::Symbol("a".to_string()), integer(3)]),
            ])
        );
    }

    #[test]
    fn test_maximal_by_n_is_sorted() {
        assert_eq!(
            eval_str("MaximalBy[{1, 2, 3, 4, 5}, Identity, 3]"),
            list(vec![integer(5), integer(4), integer(3)])
        );
        // Ties keep their input order.
        assert_eq!(
            eval_str("MaximalBy[Range[-5, 5], Abs, 4]"),
            list(vec![integer(-5), integer(5), integer(-4), integer(4),])
        );
    }

    #[test]
    fn test_maximal_by_upto() {
        assert_eq!(
            eval_str("MaximalBy[{1, 2, 3}, Identity, UpTo[5]]"),
            list(vec![integer(3), integer(2), integer(1)])
        );
    }

    #[test]
    fn test_minimal_by() {
        assert_eq!(
            eval_str("MinimalBy[{{a, 3}, {b, 1}, {c, 1}}, Last]"),
            list(vec![
                list(vec![Value::Symbol("b".to_string()), integer(1)]),
                list(vec![Value::Symbol("c".to_string()), integer(1)]),
            ])
        );
    }

    #[test]
    fn test_maximal_by_empty() {
        assert_eq!(eval_str("MaximalBy[{}, Identity]"), list(vec![]));
        assert_eq!(eval_str("MaximalBy[{}, Identity, 3]"), list(vec![]));
    }

    #[test]
    fn test_maximal_by_unevaluated() {
        assert_eq!(
            eval_str("MaximalBy[x, g]"),
            Value::Call {
                head: "MaximalBy".to_string(),
                args: vec![
                    Value::Symbol("x".to_string()),
                    Value::Symbol("g".to_string()),
                ],
            }
        );
        assert_eq!(
            eval_str("MaximalBy[g]"),
            Value::Call {
                head: "MaximalBy".to_string(),
                args: vec![Value::Symbol("g".to_string())],
            }
        );
    }

    // ── OrderingBy ──

    #[test]
    fn test_ordering_by() {
        assert_eq!(
            eval_str("OrderingBy[{3, 1, 2}, Identity]"),
            list(vec![integer(2), integer(3), integer(1)])
        );
        assert_eq!(
            eval_str("OrderingBy[{3, 1, 2}, Identity, 2]"),
            list(vec![integer(2), integer(3)])
        );
    }

    // ── AssociationMap ──

    #[test]
    fn test_association_map_list() {
        assert_eq!(
            eval_str("AssociationMap[StringLength, {\"a\", \"bb\"}]"),
            assoc(vec![("1", string("a")), ("2", string("bb"))])
        );
    }

    #[test]
    fn test_association_map_keys() {
        assert_eq!(
            eval_str("AssociationMap[f, 1, 2]"),
            assoc(vec![("f[1, 2]", list(vec![integer(1), integer(2)]))])
        );
    }

    // ── KeyUnion ──

    #[test]
    fn test_key_union() {
        let out = eval_str("KeyUnion[{<|\"a\" -> 1|>, <|\"b\" -> 2|>}]");
        match out {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                for item in &items {
                    match item {
                        Value::Assoc(map) => assert_eq!(map.len(), 2),
                        _ => panic!("expected Assoc"),
                    }
                }
                let first = match &items[0] {
                    Value::Assoc(map) => map,
                    _ => unreachable!(),
                };
                assert_eq!(first.get("a"), Some(&integer(1)));
                assert!(is_missing(first.get("b").unwrap()));
            }
            _ => panic!("expected List of Assoc"),
        }
    }

    #[test]
    fn test_key_union_default() {
        let out = eval_str("KeyUnion[{<|\"a\" -> 1|>, <|\"b\" -> 2|>}, 0]");
        match out {
            Value::List(items) => {
                let first = match &items[0] {
                    Value::Assoc(map) => map,
                    _ => panic!("expected Assoc"),
                };
                assert_eq!(first.get("b"), Some(&integer(0)));
            }
            _ => panic!("expected List"),
        }
    }

    // ── DeleteMissing ──

    #[test]
    fn test_delete_missing_list() {
        assert_eq!(
            eval_str("DeleteMissing[{1, Missing[\"x\"], 3, Missing[]}]"),
            list(vec![integer(1), integer(3)])
        );
    }

    #[test]
    fn test_delete_missing_assoc() {
        assert_eq!(
            eval_str("DeleteMissing[<|\"a\" -> 1, \"b\" -> Missing[\"x\"]|>]"),
            assoc(vec![("a", integer(1))])
        );
    }

    #[test]
    fn test_delete_missing_unevaluated() {
        assert_eq!(
            eval_str("DeleteMissing[x]"),
            Value::Call {
                head: "DeleteMissing".to_string(),
                args: vec![Value::Symbol("x".to_string())],
            }
        );
    }

    // ── KeyValuePattern ──

    #[test]
    fn test_key_value_pattern_match_q() {
        assert_eq!(
            eval_str("MatchQ[<|\"a\" -> 1, \"b\" -> 2|>, KeyValuePattern[{\"a\" -> _Integer}]]"),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str("MatchQ[<|\"a\" -> 1|>, KeyValuePattern[{\"a\" -> _String}]]"),
            Value::Bool(false)
        );
        assert_eq!(
            eval_str("MatchQ[<|\"a\" -> 1|>, KeyValuePattern[{\"z\" -> _}]]"),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_key_value_pattern_cases() {
        assert_eq!(
            eval_str(
                "Cases[{<|\"a\" -> 1, \"b\" -> 2|>, <|\"b\" -> 3|>}, KeyValuePattern[{\"a\" -> _}]]"
            ),
            list(vec![assoc(vec![("a", integer(1)), ("b", integer(2))])])
        );
    }

    #[test]
    fn test_key_value_pattern_binds() {
        assert_eq!(
            eval_str("f[KeyValuePattern[{\"a\" -> x_}]] := x; f[<|\"a\" -> 7|>]"),
            integer(7)
        );
    }

    #[test]
    fn test_key_value_pattern_cases_rule() {
        // NOTE: `Cases[list, patt :> rhs]` depends on the pattern engine
        // handling `Value::Rule` patterns; verify it for KeyValuePattern.
        assert_eq!(
            eval_str("Cases[{<|\"a\" -> 7|>}, KeyValuePattern[{\"a\" -> x_}] :> x]"),
            list(vec![integer(7)])
        );
    }

    #[test]
    fn test_key_value_pattern_empty_matches_assoc() {
        assert_eq!(
            eval_str("MatchQ[<|\"a\" -> 1|>, KeyValuePattern[{}]]"),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str("MatchQ[5, KeyValuePattern[{}]]"),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_key_value_pattern_symbol_and_pattern_keys() {
        // Symbol key.
        assert_eq!(
            eval_str("MatchQ[<|\"a\" -> 1|>, KeyValuePattern[{a -> _}]]"),
            Value::Bool(true)
        );
        // Pattern key binds against the association key.
        assert_eq!(
            eval_str("MatchQ[<|\"a\" -> 1|>, KeyValuePattern[{k_ -> _}]]"),
            Value::Bool(true)
        );
        // Numeric key (assoc keys are stored as strings).
        assert_eq!(
            eval_str("MatchQ[<|\"1\" -> \"x\"|>, KeyValuePattern[{1 -> _}]]"),
            Value::Bool(true)
        );
    }
}
