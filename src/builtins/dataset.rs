/// Dataset value type and structured data query functions for Syma.
///
/// Provides:
/// - `Dataset[data]` — wrap data in a Dataset for pretty display and query
/// - `DatasetQ[x]` — type predicate
/// - `SortBy[list, f]` — sort list by key function
/// - `JoinAcross[list1, list2, key]` — SQL-style inner join
///
/// Query dispatch via call syntax:
///   ds[All, "field"]   -> extract column from all rows
///   ds[1]              -> first row
///   ds[1, "field"]     -> cell value
///   ds[All, {"a","b"}] -> column subset
use std::collections::HashMap;
use std::sync::Arc;

use crate::env::Env;
use crate::eval::apply_function;
use crate::value::{EvalError, Value};

/// Build an unevaluated `head[args...]` result.
fn unevaluated(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Call {
        head: name.to_string(),
        args: args.to_vec(),
    })
}

/// String form of a value when used as an association key.
fn value_to_key(v: &Value) -> String {
    match v {
        Value::Str(s) | Value::Symbol(s) => s.clone(),
        Value::Integer(n) => n.to_string(),
        Value::Real(r) => r.to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        other => other.to_string(),
    }
}

/// Normalize a 1-indexed position to a 0-indexed usize.
/// Supports negative indices (count from end).
fn normalize_index(index: i64, size: usize) -> Result<usize, EvalError> {
    if index == 0 {
        return Err(EvalError::IndexOutOfBounds {
            index,
            length: size,
        });
    }
    if index > 0 {
        let idx = (index - 1) as usize;
        if idx >= size {
            return Err(EvalError::IndexOutOfBounds {
                index,
                length: size,
            });
        }
        Ok(idx)
    } else {
        // Negative: count from end
        let abs = (-index) as usize;
        if abs > size {
            return Err(EvalError::IndexOutOfBounds {
                index,
                length: size,
            });
        }
        Ok(size - abs)
    }
}

/// Compare two values for sorting (same order as Sort in list.rs).
fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => x.cmp(y),
        (Value::Real(x), Value::Real(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Integer(x), Value::Real(y)) => {
            rug::Float::with_val(crate::value::DEFAULT_PRECISION, x)
                .partial_cmp(y)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
        (Value::Real(x), Value::Integer(y)) => x
            .partial_cmp(&rug::Float::with_val(crate::value::DEFAULT_PRECISION, y))
            .unwrap_or(std::cmp::Ordering::Equal),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        _ => std::cmp::Ordering::Equal,
    }
}

// ── Builtins ──

/// Dataset[data] creates a Dataset wrapper around data.
pub fn builtin_dataset(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Dataset requires exactly 1 argument".to_string(),
        ));
    }
    // If the argument is already a Dataset, unwrap and rewrap to avoid nesting
    let inner = match &args[0] {
        Value::Dataset(ds) => ds.as_ref().clone(),
        other => other.clone(),
    };
    Ok(Value::Dataset(Arc::new(inner)))
}

/// DatasetQ[x] returns True if x is a Dataset, False otherwise.
pub fn builtin_dataset_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "DatasetQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(matches!(&args[0], Value::Dataset(_))))
}

// ── Query engine ──

/// A `Missing["KeyAbsent", key]` value, used when a key is absent.
fn missing_key(key: &str) -> Value {
    Value::Call {
        head: "Missing".to_string(),
        args: vec![
            Value::Str("KeyAbsent".to_string()),
            Value::Str(key.to_string()),
        ],
    }
}

/// Execute a call-syntax query on Dataset inner data.
///
/// ds[All, "field"]     -> extract column from all rows
/// ds[i]                -> row selection (1-indexed)
/// ds[{i, j}]           -> row selection by a list of positions
/// ds[i, "field"]       -> cell value
/// ds[All, {"a","b"}]   -> column subset (returns list of assocs with only those keys)
pub fn dataset_query(data: &Value, indices: &[Value], env: &Env) -> Result<Value, EvalError> {
    let mut current: Value = data.clone();

    for idx in indices {
        match idx {
            Value::Symbol(s) if s == "All" => {
                // All is transparent
            }
            Value::Integer(n) => {
                current = select_row(&current, n.to_i64().unwrap_or(0))?;
            }
            Value::Str(key) => {
                current = extract_column(&current, key)?;
            }
            Value::Call { head, args } if head == "Span" => {
                // `a;;b` — row range (see `Span`).
                current = select_span(&current, args)?;
            }
            Value::List(sel)
                if !sel.is_empty() && sel.iter().all(|x| matches!(x, Value::Integer(_))) =>
            {
                // Row selection by a list of positions (e.g. ds[{1, 3}]).
                let mut rows = Vec::with_capacity(sel.len());
                for n in sel {
                    rows.push(select_row(&current, n.to_integer().unwrap_or(0))?);
                }
                current = Value::List(rows);
            }
            Value::List(keys) => {
                // Column subset: keep only specified keys from each assoc
                let str_keys: Vec<&str> = keys
                    .iter()
                    .map(|k| match k {
                        Value::Str(s) => Ok(s.as_str()),
                        _ => Err(EvalError::TypeError {
                            expected: "String".to_string(),
                            got: k.type_name().to_string(),
                        }),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                current = select_columns(&current, &str_keys)?;
            }
            // A function-valued specification is a query operator: apply it to
            // the data accumulated so far (e.g. ds[All, "a", Total]).
            Value::Builtin(..) | Value::PureFunction { .. } => {
                current = apply_function(idx, std::slice::from_ref(&current), env)?;
            }
            _ => {
                return Err(EvalError::TypeError {
                    expected: "Integer, String, List, All, or a function".to_string(),
                    got: idx.type_name().to_string(),
                });
            }
        }
    }

    Ok(current)
}

/// Select rows from a list by a `Span[start, end, step]` specification.
///
/// `All` (or a missing bound) stands for the first/last position and negative
/// positions count from the end, mirroring `Part`'s `;;` row ranges.
fn select_span(data: &Value, spec: &[Value]) -> Result<Value, EvalError> {
    let items = match data {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: data.type_name().to_string(),
            });
        }
    };
    if spec.is_empty() || spec.len() > 3 {
        return Err(EvalError::Error(
            "Span: expected 1 to 3 arguments".to_string(),
        ));
    }
    let n = items.len() as i64;
    let bound = |v: &Value, default: i64| -> Result<i64, EvalError> {
        match v {
            Value::Symbol(s) if s == "All" => Ok(default),
            other => other.to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer or All".to_string(),
                got: other.type_name().to_string(),
            }),
        }
    };
    let start = bound(&spec[0], 1)?;
    let end = if spec.len() >= 2 {
        bound(&spec[1], n)?
    } else {
        start
    };
    let step = if spec.len() >= 3 {
        spec[2].to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer".to_string(),
            got: spec[2].type_name().to_string(),
        })?
    } else {
        1
    };
    if step == 0 {
        return Err(EvalError::Error("Span: step cannot be zero".to_string()));
    }
    // Resolve a possibly-negative 1-based bound to a 0-based index.
    let resolve = |i: i64| -> Option<i64> {
        let idx = if i > 0 { i - 1 } else { n + i };
        if idx < 0 || idx >= n { None } else { Some(idx) }
    };
    let (start, end) = match (resolve(start), resolve(end)) {
        (Some(a), Some(b)) => (a, b),
        _ => return Ok(Value::List(vec![])),
    };
    let mut out = Vec::new();
    let mut cur = start;
    if step > 0 {
        while cur <= end {
            out.push(items[cur as usize].clone());
            cur += step;
        }
    } else {
        while cur >= end {
            out.push(items[cur as usize].clone());
            cur += step;
        }
    }
    Ok(Value::List(out))
}

/// Select one (possibly negative) 1-indexed position from a list.
fn select_row(data: &Value, index: i64) -> Result<Value, EvalError> {
    match data {
        Value::List(items) => {
            let idx = normalize_index(index, items.len())?;
            Ok(items[idx].clone())
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: data.type_name().to_string(),
        }),
    }
}

/// Extract a column (by key string) from a list of assocs or a single assoc.
/// Absent keys produce `Missing["KeyAbsent", key]`, as in Wolfram Language.
fn extract_column(data: &Value, key: &str) -> Result<Value, EvalError> {
    match data {
        Value::List(items) => {
            let values: Result<Vec<Value>, EvalError> = items
                .iter()
                .map(|item| match item {
                    Value::Assoc(map) => {
                        Ok(map.get(key).cloned().unwrap_or_else(|| missing_key(key)))
                    }
                    _ => Err(EvalError::TypeError {
                        expected: "Assoc".to_string(),
                        got: item.type_name().to_string(),
                    }),
                })
                .collect();
            Ok(Value::List(values?))
        }
        Value::Assoc(map) => Ok(map.get(key).cloned().unwrap_or_else(|| missing_key(key))),
        _ => Err(EvalError::TypeError {
            expected: "List or Assoc".to_string(),
            got: data.type_name().to_string(),
        }),
    }
}

/// Select a subset of columns from a list of assocs or a single assoc. Absent
/// keys are filled with `Missing["KeyAbsent", key]`.
fn select_columns(data: &Value, keys: &[&str]) -> Result<Value, EvalError> {
    match data {
        Value::List(items) => {
            let filtered: Result<Vec<Value>, EvalError> = items
                .iter()
                .map(|item| match item {
                    Value::Assoc(map) => {
                        let mut new_map = std::collections::HashMap::new();
                        for k in keys {
                            let v = map.get(*k).cloned().unwrap_or_else(|| missing_key(k));
                            new_map.insert(k.to_string(), v);
                        }
                        Ok(Value::Assoc(new_map))
                    }
                    _ => Err(EvalError::TypeError {
                        expected: "Assoc".to_string(),
                        got: item.type_name().to_string(),
                    }),
                })
                .collect();
            Ok(Value::List(filtered?))
        }
        Value::Assoc(map) => {
            let mut new_map = std::collections::HashMap::new();
            for k in keys {
                let v = map.get(*k).cloned().unwrap_or_else(|| missing_key(k));
                new_map.insert(k.to_string(), v);
            }
            Ok(Value::Assoc(new_map))
        }
        _ => Err(EvalError::TypeError {
            expected: "List or Assoc".to_string(),
            got: data.type_name().to_string(),
        }),
    }
}

// ── SortBy ──

/// SortBy[list, f] sorts list elements by the key produced by applying f to each element.
pub fn builtin_sort_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "SortBy requires exactly 2 arguments: list and f".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let f = &args[1];
            let mut pairs: Vec<(Value, Value)> = Vec::with_capacity(items.len());
            for item in items {
                let key = apply_function(f, &[item.clone()], env)?;
                pairs.push((item.clone(), key));
            }
            pairs.sort_by(|a, b| compare_values(&a.1, &b.1));
            Ok(Value::List(pairs.into_iter().map(|(v, _)| v).collect()))
        }
        Value::Dataset(inner) => {
            // Unwrap, sort, re-wrap in Dataset
            let sorted = builtin_sort_by(&[inner.as_ref().clone(), args[1].clone()], env)?;
            Ok(Value::Dataset(Arc::new(sorted)))
        }
        _ => Err(EvalError::TypeError {
            expected: "List or Dataset".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── JoinAcross ──

/// JoinAcross[list1, list2, key] performs an inner join of two lists of associations
/// on the specified key. Returns a list of merged associations.
pub fn builtin_join_across(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "JoinAcross requires exactly 3 arguments: list1, list2, key".to_string(),
        ));
    }

    let key = match &args[2] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[2].type_name().to_string(),
            });
        }
    };

    let list1 = match &args[0] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };

    let list2 = match &args[1] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };

    // Verify both lists contain associations
    if list1.is_empty() || list2.is_empty() {
        return Ok(Value::List(vec![]));
    }

    // Build a hash map from key value to assocs in list2 for efficient lookup
    let mut key_to_assocs: std::collections::HashMap<
        String,
        Vec<&std::collections::HashMap<String, Value>>,
    > = std::collections::HashMap::new();

    for item in list2 {
        match item {
            Value::Assoc(map) => {
                if let Some(key_val) = map.get(&key) {
                    let key_str = key_val.to_string();
                    key_to_assocs.entry(key_str).or_default().push(map);
                }
            }
            _ => {
                return Err(EvalError::TypeError {
                    expected: "Assoc".to_string(),
                    got: item.type_name().to_string(),
                });
            }
        }
    }

    // For each assoc in list1, find matching assocs in list2 and merge
    let mut result = Vec::new();

    for item in list1 {
        match item {
            Value::Assoc(map) => {
                let key_val = map.get(&key).ok_or_else(|| {
                    EvalError::Error(format!("Key '{}' not found in association", key))
                })?;
                let key_str = key_val.to_string();

                if let Some(matches) = key_to_assocs.get(&key_str) {
                    for map2 in matches {
                        let mut merged = map.clone();
                        for (k2, v2) in *map2 {
                            merged.insert(k2.clone(), v2.clone());
                        }
                        result.push(Value::Assoc(merged));
                    }
                }
            }
            _ => {
                return Err(EvalError::TypeError {
                    expected: "Assoc".to_string(),
                    got: item.type_name().to_string(),
                });
            }
        }
    }

    Ok(Value::List(result))
}

// ── Query ──

/// `Query[target, spec...]` runs a Dataset/association query using the same
/// specification language as `ds[spec...]` (e.g. `Query[ds, All, "col"]`).
///
/// A `Dataset` target yields a `Dataset` result; an association or list target
/// yields its bare result. Returns unevaluated when the target is inapplicable.
pub fn builtin_query(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return unevaluated("Query", args);
    }
    let (inner, wrap_in_dataset) = match &args[0] {
        Value::Dataset(data) => (data.as_ref().clone(), true),
        Value::Assoc(_) | Value::List(_) => (args[0].clone(), false),
        _ => return unevaluated("Query", args),
    };
    let result = if args.len() == 1 {
        inner
    } else {
        dataset_query(&inner, &args[1..], env)?
    };
    if wrap_in_dataset {
        Ok(Value::Dataset(Arc::new(result)))
    } else {
        Ok(result)
    }
}

// ── Dataset accessors (Dimensions / Part / All) ────────────────────────────

/// `Dimensions[...]` that unwraps a `Dataset` (and handles associations) before
/// delegating to the linear-algebra implementation.
pub fn builtin_dataset_dimensions(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 1 {
        match &args[0] {
            Value::Dataset(inner) => {
                return builtin_dataset_dimensions(std::slice::from_ref(inner));
            }
            Value::Assoc(map) => return Ok(Value::List(vec![Value::Integer(map.len().into())])),
            _ => {}
        }
    }
    crate::builtins::linalg::builtin_dimensions(args)
}

/// `Part[...]` that understands `Dataset` and association targets.
///
/// For a `Dataset`, the remaining specifications are interpreted exactly like a
/// query (`All`, integers, key strings and key lists all work); for an
/// association, a key yields the value (or `Missing["KeyAbsent", key]`) and a
/// list of keys yields a sub-association. Everything else delegates to
/// `list::builtin_part`.
pub fn builtin_dataset_part(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() >= 2 {
        if let Value::Dataset(inner) = &args[0] {
            return dataset_query(inner, &args[1..], env);
        }
        if let Value::Assoc(map) = &args[0] {
            return assoc_part(map, &args[1..]);
        }
    }
    crate::builtins::list::builtin_part(args)
}

/// Index into an association by a key or list of keys, recursing for
/// multi-part specifications.
fn assoc_part(map: &HashMap<String, Value>, specs: &[Value]) -> Result<Value, EvalError> {
    if specs.is_empty() {
        return Ok(Value::Assoc(map.clone()));
    }
    let spec = &specs[0];
    let selected = match spec {
        Value::List(keys) => {
            let mut new_map = HashMap::with_capacity(keys.len());
            for key in keys {
                let k = value_to_key(key);
                let v = map.get(&k).cloned().unwrap_or_else(|| missing_key(&k));
                new_map.insert(k, v);
            }
            Value::Assoc(new_map)
        }
        other => {
            let k = value_to_key(other);
            map.get(&k).cloned().unwrap_or_else(|| missing_key(&k))
        }
    };
    if specs.len() == 1 {
        return Ok(selected);
    }
    match selected {
        Value::Assoc(inner) => assoc_part(&inner, &specs[1..]),
        Value::List(_) => {
            let mut sub_args = vec![selected];
            sub_args.extend(specs[1..].iter().cloned());
            crate::builtins::list::builtin_part(&sub_args)
        }
        other => Ok(other),
    }
}

// ── Registration ───────────────────────────────────────────────────────────

/// Register the Dataset/query builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin_env(env, "Query", builtin_query);
    super::register_builtin(env, "Dimensions", builtin_dataset_dimensions);
    super::register_builtin_env(env, "Part", builtin_dataset_part);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use crate::env::Env;
    use crate::eval::eval_program;
    use crate::lexer;
    use crate::parser;
    use rug::Integer;
    use std::collections::HashMap;

    fn eval_str(input: &str) -> Value {
        let env = Env::new();
        builtins::register_builtins(&env);
        let tokens = lexer::tokenize(input).unwrap();
        let ast = parser::parse(tokens).unwrap();
        eval_program(&ast, &env).unwrap()
    }

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn assoc(pairs: Vec<(&str, Value)>) -> Value {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v);
        }
        Value::Assoc(map)
    }

    // ── Dataset creation and predicates ──

    #[test]
    fn test_dataset_creation() {
        let data = eval_str("Dataset[{<|\"a\"->1, \"b\"->2|>}]");
        assert!(matches!(data, Value::Dataset(_)));
    }

    #[test]
    fn test_dataset_q_true() {
        let result = eval_str("DatasetQ[Dataset[{<|\"a\"->1|>}]]");
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_dataset_q_false() {
        let result = eval_str("DatasetQ[{1, 2, 3}]");
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_dataset_creation_nested() {
        // Dataset of Dataset should unwrap
        let result = eval_str("Dataset[Dataset[{<|\"a\"->1|>}]]");
        assert!(matches!(result, Value::Dataset(_)));
    }

    // ── Row selection ──

    #[test]
    fn test_query_row() {
        // ds[1] should return first assoc
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->10, \"b\"->20|>, <|\"a\"->30, \"b\"->40|>}];
            ds[1]
        )",
        );
        assert_eq!(result, assoc(vec![("a", int(10)), ("b", int(20))]));
    }

    #[test]
    fn test_query_row_negative() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->1|>, <|\"a\"->2|>, <|\"a\"->3|>}];
            ds[-1]
        )",
        );
        assert_eq!(result, assoc(vec![("a", int(3))]));
    }

    // ── Column extraction ──

    #[test]
    fn test_query_column() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->10, \"b\"->20|>, <|\"a\"->30, \"b\"->40|>}];
            ds[All, \"a\"]
        )",
        );
        assert_eq!(result, Value::List(vec![int(10), int(30)]));
    }

    #[test]
    fn test_query_cell() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->10, \"b\"->20|>, <|\"a\"->30, \"b\"->40|>}];
            ds[2, \"a\"]
        )",
        );
        assert_eq!(result, int(30));
    }

    #[test]
    fn test_query_column_subset() {
        // ds[All, {"a"}] should return list of assocs with only key "a"
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->10, \"b\"->20|>, <|\"a\"->30, \"b\"->40|>}];
            ds[All, {\"a\"}]
        )",
        );
        assert_eq!(
            result,
            Value::List(vec![
                assoc(vec![("a", int(10))]),
                assoc(vec![("a", int(30))]),
            ])
        );
    }

    #[test]
    fn test_query_all_identity() {
        // ds[All] should return the inner data as-is
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->1|>, <|\"a\"->2|>}];
            ds[All]
        )",
        );
        // All is transparent, so we get the inner list
        assert_eq!(
            result,
            Value::List(vec![assoc(vec![("a", int(1))]), assoc(vec![("a", int(2))]),])
        );
    }

    // ── Missing keys ──

    #[test]
    fn test_query_missing_key_returns_missing() {
        // Absent keys produce Missing["KeyAbsent", key] in a query.
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->1|>}];
            ds[All, \"x\"]
        )",
        );
        assert_eq!(
            result,
            Value::List(vec![Value::Call {
                head: "Missing".to_string(),
                args: vec![
                    Value::Str("KeyAbsent".to_string()),
                    Value::Str("x".to_string())
                ],
            }])
        );
    }

    #[test]
    fn test_query_cell_missing_key_returns_missing() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->1|>}];
            ds[1, \"x\"]
        )",
        );
        assert_eq!(
            result,
            Value::Call {
                head: "Missing".to_string(),
                args: vec![
                    Value::Str("KeyAbsent".to_string()),
                    Value::Str("x".to_string())
                ],
            }
        );
    }

    #[test]
    fn test_query_column_subset_missing_key() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->1|>}];
            ds[All, {\"a\", \"z\"}]
        )",
        );
        let mut map = std::collections::HashMap::new();
        map.insert("a".to_string(), int(1));
        map.insert(
            "z".to_string(),
            Value::Call {
                head: "Missing".to_string(),
                args: vec![
                    Value::Str("KeyAbsent".to_string()),
                    Value::Str("z".to_string()),
                ],
            },
        );
        assert_eq!(result, Value::List(vec![Value::Assoc(map)]));
    }

    // ── Row list selection ──

    #[test]
    fn test_query_row_list() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->1|>, <|\"a\"->2|>, <|\"a\"->3|>}];
            ds[{3, 1}]
        )",
        );
        assert_eq!(
            result,
            Value::List(vec![assoc(vec![("a", int(3))]), assoc(vec![("a", int(1))])])
        );
    }

    #[test]
    fn test_query_row_list_then_column() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->1, \"b\"->10|>, <|\"a\"->2, \"b\"->20|>, <|\"a\"->3, \"b\"->30|>}];
            ds[{1, 3}, \"b\"]
        )",
        );
        assert_eq!(result, Value::List(vec![int(10), int(30)]));
    }

    // ── Normal ──

    #[test]
    fn test_normal_dataset() {
        let result = eval_str("Normal[Dataset[{1, 2, 3}]]");
        assert_eq!(result, Value::List(vec![int(1), int(2), int(3)]));
    }

    // ── SortBy ──

    #[test]
    fn test_sort_by_integers() {
        // Use First as identity on single-element lists
        let result = eval_str("id[x_] := x; SortBy[{3, 1, 2}, id]");
        assert_eq!(result, Value::List(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_sort_by_assoc() {
        // Sort list of assocs by the "a" key
        let result = eval_str(
            "key[x_] := Lookup[x, \"a\"]; SortBy[{<|\"a\"->3|>, <|\"a\"->1|>, <|\"a\"->2|>}, key]",
        );
        assert_eq!(
            result,
            Value::List(vec![
                assoc(vec![("a", int(1))]),
                assoc(vec![("a", int(2))]),
                assoc(vec![("a", int(3))]),
            ])
        );
    }

    #[test]
    fn test_sort_by_empty() {
        let result = eval_str("id[x_] := x; SortBy[{}, id]");
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_sort_by_single() {
        let result = eval_str("id[x_] := x; SortBy[{42}, id]");
        assert_eq!(result, Value::List(vec![int(42)]));
    }

    // ── JoinAcross ──

    #[test]
    fn test_join_across_basic() {
        let result = eval_str(
            "JoinAcross[{<|\"id\"->1, \"x\"->10|>, <|\"id\"->2, \"x\"->20|>}, \
                        {<|\"id\"->1, \"y\"->100|>}, \"id\"]",
        );
        assert_eq!(
            result,
            Value::List(vec![assoc(vec![
                ("id", int(1)),
                ("x", int(10)),
                ("y", int(100)),
            ])])
        );
    }

    #[test]
    fn test_join_across_no_match() {
        let result = eval_str("JoinAcross[{<|\"id\"->1|>}, {<|\"id\"->99|>}, \"id\"]");
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_join_across_empty_first() {
        let result = eval_str("JoinAcross[{}, {<|\"id\"->1|>}, \"id\"]");
        assert_eq!(result, Value::List(vec![]));
    }

    // ── Display ──

    #[test]
    fn test_dataset_display_empty() {
        let result = eval_str("Dataset[{}]");
        assert!(matches!(result, Value::Dataset(_)));
    }

    #[test]
    fn test_dataset_display_single_assoc() {
        let result = eval_str("Dataset[<|\"a\"->1|>]");
        assert!(matches!(result, Value::Dataset(_)));
    }

    // ── Direct tests ──

    #[test]
    fn test_normalize_index_positive() {
        assert_eq!(normalize_index(1, 5).unwrap(), 0);
        assert_eq!(normalize_index(5, 5).unwrap(), 4);
    }

    #[test]
    fn test_normalize_index_negative() {
        assert_eq!(normalize_index(-1, 5).unwrap(), 4);
        assert_eq!(normalize_index(-5, 5).unwrap(), 0);
    }

    #[test]
    fn test_normalize_index_zero() {
        assert!(normalize_index(0, 5).is_err());
    }

    #[test]
    fn test_normalize_index_out_of_bounds() {
        assert!(normalize_index(6, 5).is_err());
        assert!(normalize_index(-6, 5).is_err());
    }

    #[test]
    fn test_sort_by_propagates_error() {
        // A key function that errors should surface the error rather than
        // silently sorting by Null.
        let env = Env::new();
        builtins::register_builtins(&env);
        let f = Value::Builtin(
            "boom".to_string(),
            crate::value::BuiltinFn::Pure(|_| Err(EvalError::Error("boom".to_string()))),
        );
        let candles = Value::List(vec![int(1), int(2)]);
        assert!(builtin_sort_by(&[candles, f], &env).is_err());
    }

    // ── Query ──

    #[test]
    fn test_query_dataset_column_returns_dataset() {
        let result = eval_str(
            "Query[Dataset[{<|\"a\"->10, \"b\"->20|>, <|\"a\"->30, \"b\"->40|>}], All, \"a\"]",
        );
        assert_eq!(
            result,
            Value::Dataset(std::sync::Arc::new(Value::List(vec![int(10), int(30)])))
        );
    }

    #[test]
    fn test_query_dataset_row_selection() {
        let result = eval_str("Query[Dataset[{<|\"a\"->10|>, <|\"a\"->30|>}], 2]");
        assert_eq!(
            result,
            Value::Dataset(std::sync::Arc::new(assoc(vec![("a", int(30))])))
        );
    }

    #[test]
    fn test_query_assoc_basic() {
        assert_eq!(eval_str("Query[<|\"a\"->1, \"b\"->2|>, \"a\"]"), int(1));
        assert_eq!(
            eval_str("Query[<|\"a\"->1, \"b\"->2|>, All]"),
            assoc(vec![("a", int(1)), ("b", int(2))])
        );
        assert_eq!(
            eval_str("Query[<|\"a\"->1|>, {\"a\"}]"),
            assoc(vec![("a", int(1))])
        );
    }

    #[test]
    fn test_query_missing_key_produces_missing() {
        let result = eval_str("Query[<|\"a\"->1|>, \"z\"]");
        assert_eq!(
            result,
            Value::Call {
                head: "Missing".to_string(),
                args: vec![
                    Value::Str("KeyAbsent".to_string()),
                    Value::Str("z".to_string())
                ],
            }
        );
    }

    #[test]
    fn test_query_unevaluated_on_inapplicable_target() {
        assert_eq!(
            eval_str("Query[5, All]"),
            Value::Call {
                head: "Query".to_string(),
                args: vec![int(5), Value::Symbol("All".to_string())],
            }
        );
    }

    // ── Dataset accessors: Dimensions / Part / All ──

    #[test]
    fn test_dimensions_dataset_and_assoc() {
        assert_eq!(
            eval_str("Dimensions[Dataset[{{1, 2, 3}, {4, 5, 6}}]]"),
            Value::List(vec![int(2), int(3)])
        );
        assert_eq!(
            eval_str("Dimensions[<|\"a\"->1, \"b\"->2|>]"),
            Value::List(vec![int(2)])
        );
        // Non-Dataset values still delegate to the linear-algebra version.
        assert_eq!(eval_str("Dimensions[{1, 2, 3}]"), Value::List(vec![int(3)]));
    }

    #[test]
    fn test_part_dataset_row_and_all() {
        let result = eval_str("Part[Dataset[{<|\"a\"->10|>, <|\"a\"->30|>}], 1]");
        assert_eq!(result, assoc(vec![("a", int(10))]));
        let all = eval_str("Part[Dataset[{<|\"a\"->10|>, <|\"a\"->30|>}], All]");
        assert_eq!(
            all,
            Value::List(vec![
                assoc(vec![("a", int(10))]),
                assoc(vec![("a", int(30))])
            ])
        );
        // ds[[All, "a"]]-style nested access is routed through the query engine.
        let col = eval_str("Part[Dataset[{<|\"a\"->10|>, <|\"a\"->30|>}], All, \"a\"]");
        assert_eq!(col, Value::List(vec![int(10), int(30)]));
    }

    #[test]
    fn test_part_assoc_key_and_list() {
        assert_eq!(eval_str("Part[<|\"a\"->1, \"b\"->2|>, \"b\"]"), int(2));
        assert_eq!(
            eval_str("Part[<|\"a\"->1, \"b\"->2|>, {\"a\"}]"),
            assoc(vec![("a", int(1))])
        );
        assert_eq!(
            eval_str("Part[<|\"a\"->1|>, \"z\"]"),
            Value::Call {
                head: "Missing".to_string(),
                args: vec![
                    Value::Str("KeyAbsent".to_string()),
                    Value::Str("z".to_string())
                ],
            }
        );
    }

    #[test]
    fn test_part_still_delegates_for_plain_lists() {
        assert_eq!(eval_str("Part[{10, 20, 30}, 2]"), int(20));
        assert_eq!(eval_str("Part[{10, 20, 30}, -1]"), int(30));
    }

    // ── Dataset construction shapes ──

    #[test]
    fn test_dataset_from_assoc() {
        let ds = eval_str("Dataset[<|\"a\"->1, \"b\"->2|>]");
        assert!(matches!(ds, Value::Dataset(_)));
        assert_eq!(eval_str("Dataset[<|\"a\"->1, \"b\"->2|>][\"a\"]"), int(1));
    }

    #[test]
    fn test_dataset_from_list_of_assocs_unevaluated_head() {
        // A Dataset of rows round-trips through Normal.
        assert_eq!(
            eval_str("Normal[Dataset[{<|\"a\"->1|>, <|\"a\"->2|>}]]"),
            Value::List(vec![assoc(vec![("a", int(1))]), assoc(vec![("a", int(2))])])
        );
    }

    // ── Query: row ranges (`;;`) and operators ──

    #[test]
    fn test_span_parses_to_span_head() {
        assert_eq!(eval_str("Head[1;;3]"), Value::Symbol("Span".to_string()));
        assert_eq!(
            eval_str("1;;3"),
            Value::Call {
                head: "Span".to_string(),
                args: vec![int(1), int(3)],
            }
        );
    }

    #[test]
    fn test_query_row_span() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->1|>, <|\"a\"->2|>, <|\"a\"->3|>, <|\"a\"->4|>}];
            ds[2;;3]
        )",
        );
        assert_eq!(
            result,
            Value::List(vec![assoc(vec![("a", int(2))]), assoc(vec![("a", int(3))])])
        );
    }

    #[test]
    fn test_query_row_span_open_ended() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->1|>, <|\"a\"->2|>, <|\"a\"->3|>}];
            ds[2;;]
        )",
        );
        assert_eq!(
            result,
            Value::List(vec![assoc(vec![("a", int(2))]), assoc(vec![("a", int(3))])])
        );
    }

    #[test]
    fn test_query_span_then_column() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"b\"->10|>, <|\"b\"->20|>, <|\"b\"->30|>}];
            ds[1;;2, \"b\"]
        )",
        );
        assert_eq!(result, Value::List(vec![int(10), int(20)]));
    }

    #[test]
    fn test_query_operator_total() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->3|>, <|\"a\"->1|>, <|\"a\"->2|>}];
            ds[All, \"a\", Total]
        )",
        );
        assert_eq!(result, int(6));
    }

    #[test]
    fn test_query_operator_reverse() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->1|>, <|\"a\"->2|>}];
            ds[Reverse]
        )",
        );
        assert_eq!(
            result,
            Value::List(vec![assoc(vec![("a", int(2))]), assoc(vec![("a", int(1))])])
        );
    }

    // ── Normal / Values / Keys ──

    #[test]
    fn test_keys_values_assoc() {
        assert_eq!(
            eval_str("Keys[<|\"a\"->1, \"b\"->2|>]"),
            Value::List(vec![
                Value::Str("a".to_string()),
                Value::Str("b".to_string())
            ])
        );
        assert_eq!(
            eval_str("Values[<|\"a\"->1, \"b\"->2|>]"),
            Value::List(vec![int(1), int(2)])
        );
    }

    #[test]
    fn test_keys_values_unwrap_dataset() {
        assert_eq!(
            eval_str("Keys[Dataset[<|\"a\"->1, \"b\"->2|>]]"),
            Value::List(vec![
                Value::Str("a".to_string()),
                Value::Str("b".to_string())
            ])
        );
        assert_eq!(
            eval_str("Values[Dataset[<|\"a\"->1, \"b\"->2|>]]"),
            Value::List(vec![int(1), int(2)])
        );
    }

    #[test]
    fn test_keys_values_dataset_column() {
        // Values over the rows produced by a query.
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->1, \"b\"->2|>, <|\"a\"->3, \"b\"->4|>}];
            Values[ds[All]]
        )",
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::List(vec![int(1), int(2)]),
                Value::List(vec![int(3), int(4)]),
            ])
        );
    }

    // ── Aggregation over query results ──

    #[test]
    fn test_aggregate_total_mean() {
        let d = "ds = Dataset[{<|\"a\"->3|>, <|\"a\"->1|>, <|\"a\"->2|>}];";
        assert_eq!(eval_str(&format!("{d} Total[ds[All, \"a\"]]")), int(6));
        assert_eq!(eval_str(&format!("{d} Mean[ds[All, \"a\"]]")), int(2));
        assert_eq!(eval_str(&format!("{d} Count[ds[All, \"a\"], 2]")), int(1));
    }

    #[test]
    fn test_aggregate_sort_select() {
        let d = "ds = Dataset[{<|\"a\"->3|>, <|\"a\"->1|>, <|\"a\"->2|>}];";
        assert_eq!(
            eval_str(&format!("{d} Select[ds[All], Lookup[#, \"a\"] > 1 &]")),
            Value::List(vec![assoc(vec![("a", int(3))]), assoc(vec![("a", int(2))])])
        );
        assert_eq!(
            eval_str(&format!("{d} SortBy[ds[All], Lookup[#, \"a\"] &]")),
            Value::List(vec![
                assoc(vec![("a", int(1))]),
                assoc(vec![("a", int(2))]),
                assoc(vec![("a", int(3))]),
            ])
        );
    }

    #[test]
    fn test_aggregate_group_by() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"g\"->\"x\"|>, <|\"g\"->\"y\"|>, <|\"g\"->\"x\"|>}];
            GroupBy[ds[All], Lookup[#, \"g\"] &]
        )",
        );
        let Value::Assoc(map) = result else {
            panic!("GroupBy should yield an association");
        };
        assert_eq!(map.len(), 2);
        match (&map["x"], &map["y"]) {
            (Value::List(xs), Value::List(ys)) => {
                assert_eq!(xs.len(), 2);
                assert_eq!(ys.len(), 1);
            }
            other => panic!("GroupBy groups should be lists, got {other:?}"),
        }
    }

    // ── SortBy directly on a Dataset ──

    #[test]
    fn test_sort_by_dataset_rewraps() {
        let result = eval_str(
            "(
            ds = Dataset[{<|\"a\"->3|>, <|\"a\"->1|>}];
            SortBy[ds, Lookup[#, \"a\"] &]
        )",
        );
        assert_eq!(
            result,
            Value::Dataset(std::sync::Arc::new(Value::List(vec![
                assoc(vec![("a", int(1))]),
                assoc(vec![("a", int(3))]),
            ])))
        );
    }

    // ── Unevaluated when inapplicable ──

    #[test]
    fn test_query_unevaluated_assoc_span_inapplicable() {
        // A Span on a query that has no list to range over is still a list op;
        // Query with a non-data target stays unevaluated.
        assert_eq!(
            eval_str("Query[5, All, \"a\"]"),
            Value::Call {
                head: "Query".to_string(),
                args: vec![
                    int(5),
                    Value::Symbol("All".to_string()),
                    Value::Str("a".to_string()),
                ],
            }
        );
    }
}
