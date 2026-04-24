use crate::env::Env;
use crate::eval::apply_function;
use crate::value::{EvalError, Value};
use rug::Float;
use rug::Integer;

// Helper: convert a Value to f64.
fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        Value::Rational(r) => Some(r.to_f64()),
        _ => None,
    }
}

// Helper: create a Real value from f64.
fn real(v: f64) -> Value {
    Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, v))
}

// ── Helper functions ──

/// Compare two values for ordering (used by Sort, Ordering).
pub(crate) fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
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

/// Normalize a 1-indexed position to a 0-indexed usize.
/// Supports negative indices (count from end).
/// `size` is the number of valid slots.
fn normalize_index(index: i64, size: usize) -> Result<usize, EvalError> {
    if index == 0 {
        return Err(EvalError::IndexOutOfBounds {
            index,
            length: size,
        });
    }
    let idx = if index > 0 {
        (index - 1) as usize
    } else {
        let abs = (-index) as usize;
        if abs > size {
            return Err(EvalError::IndexOutOfBounds {
                index,
                length: size,
            });
        }
        size - abs
    };
    if idx >= size {
        return Err(EvalError::IndexOutOfBounds {
            index,
            length: size,
        });
    }
    Ok(idx)
}

/// Extract a list slice from a Value, returning a TypeError for non-lists.
fn get_list(val: &Value) -> Result<&[Value], EvalError> {
    match val {
        Value::List(items) => Ok(items.as_slice()),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: val.type_name().to_string(),
        }),
    }
}

pub fn builtin_length(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Length requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => Ok(Value::Integer(Integer::from(items.len() as i64))),
        Value::Str(s) => Ok(Value::Integer(Integer::from(s.len() as i64))),
        _ => Ok(Value::Integer(Integer::from(1))),
    }
}

pub fn builtin_first(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "First requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            if items.is_empty() {
                Err(EvalError::Error("First called on empty list".to_string()))
            } else {
                Ok(items[0].clone())
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_last(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Last requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            if items.is_empty() {
                Err(EvalError::Error("Last called on empty list".to_string()))
            } else {
                Ok(items[items.len() - 1].clone())
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_rest(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Rest requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            if items.is_empty() {
                Err(EvalError::Error("Rest called on empty list".to_string()))
            } else {
                Ok(Value::List(items[1..].to_vec()))
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_most(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Most requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            if items.is_empty() {
                Err(EvalError::Error("Most called on empty list".to_string()))
            } else {
                Ok(Value::List(items[..items.len() - 1].to_vec()))
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_append(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Append requires exactly 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let mut new_items = items.clone();
            new_items.push(args[1].clone());
            Ok(Value::List(new_items))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_prepend(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Prepend requires exactly 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let mut new_items = vec![args[1].clone()];
            new_items.extend(items.clone());
            Ok(Value::List(new_items))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_join(args: &[Value]) -> Result<Value, EvalError> {
    let mut result = Vec::new();
    for arg in args {
        match arg {
            Value::List(items) => result.extend(items.clone()),
            _ => {
                return Err(EvalError::TypeError {
                    expected: "List".to_string(),
                    got: arg.type_name().to_string(),
                });
            }
        }
    }
    Ok(Value::List(result))
}

pub fn builtin_flatten(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Flatten requires 1 or 2 arguments".to_string(),
        ));
    }
    let max_depth = if args.len() == 2 {
        match &args[1] {
            Value::Integer(n) => {
                if let Some(u) = n.to_usize() {
                    u
                } else {
                    return Err(EvalError::Error(
                        "Flatten: depth must be a non-negative integer".to_string(),
                    ));
                }
            }
            _ => {
                return Err(EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: args[1].type_name().to_string(),
                });
            }
        }
    } else {
        usize::MAX
    };

    fn flatten_depth(val: &Value, depth: usize) -> Vec<Value> {
        match val {
            Value::List(items) if depth > 0 => {
                let mut result = Vec::new();
                for item in items {
                    match item {
                        Value::List(sub_items) => {
                            if depth > 1 {
                                result.extend(flatten_depth(item, depth - 1));
                            } else {
                                result.extend(sub_items.iter().cloned());
                            }
                        }
                        _ => result.push(item.clone()),
                    }
                }
                result
            }
            _ => vec![val.clone()],
        }
    }
    Ok(Value::List(flatten_depth(&args[0], max_depth)))
}

pub fn builtin_sort(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Sort requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let mut sorted = items.clone();
            sorted.sort_by(compare_values);
            Ok(Value::List(sorted))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_reverse(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Reverse requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let mut reversed = items.clone();
            reversed.reverse();
            Ok(Value::List(reversed))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_part(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Part requires at least 2 arguments".to_string(),
        ));
    }
    // Multi-index: Part[list, i, j, ...] — recursively descend
    if args.len() >= 3 {
        let mut current: Value = args[0].clone();
        for idx_val in &args[1..] {
            let index = match idx_val {
                Value::Integer(n) => n.to_i64().unwrap_or(0),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: idx_val.type_name().to_string(),
                    });
                }
            };
            match &current {
                Value::List(items) => {
                    let idx = normalize_index(index, items.len())?;
                    current = items[idx].clone();
                }
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "List".to_string(),
                        got: current.type_name().to_string(),
                    });
                }
            }
        }
        return Ok(current);
    }
    match &args[0] {
        Value::List(items) => {
            let index = match &args[1] {
                Value::Integer(n) => n.to_i64().unwrap_or(0),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: args[1].type_name().to_string(),
                    });
                }
            };
            let idx = normalize_index(index, items.len())?;
            Ok(items[idx].clone())
        }
        Value::Str(s) => {
            let index = match &args[1] {
                Value::Integer(n) => n.to_i64().unwrap_or(0),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: args[1].type_name().to_string(),
                    });
                }
            };
            let idx = if index > 0 {
                (index - 1) as usize
            } else {
                return Err(EvalError::IndexOutOfBounds {
                    index,
                    length: s.len(),
                });
            };
            if idx < s.len() {
                Ok(Value::Str(s.chars().nth(idx).unwrap().to_string()))
            } else {
                Err(EvalError::IndexOutOfBounds {
                    index,
                    length: s.len(),
                })
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "List or String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_range(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => {
            let n = args[0].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            Ok(Value::List(
                (1..=n).map(|i| Value::Integer(Integer::from(i))).collect(),
            ))
        }
        2 => {
            let start = args[0].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            let end = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[1].type_name().to_string(),
            })?;
            Ok(Value::List(
                (start..=end)
                    .map(|i| Value::Integer(Integer::from(i)))
                    .collect(),
            ))
        }
        3 => {
            let start = args[0].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            let end = args[2].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[2].type_name().to_string(),
            })?;
            let step = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[1].type_name().to_string(),
            })?;
            if step == 0 {
                return Err(EvalError::Error("Range step cannot be zero".to_string()));
            }
            let mut result = Vec::new();
            if step > 0 {
                let mut i = start;
                while i <= end {
                    result.push(Value::Integer(Integer::from(i)));
                    i += step;
                }
            } else {
                let mut i = start;
                while i >= end {
                    result.push(Value::Integer(Integer::from(i)));
                    i += step;
                }
            }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::Error("Range requires 1-3 arguments".to_string())),
    }
}

pub fn builtin_table(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 2 {
        if let Value::Integer(n) = &args[1] {
            if let Some(n_usize) = n.to_usize() {
                let mut result = Vec::with_capacity(n_usize);
                for _ in 0..n_usize {
                    result.push(args[0].clone());
                }
                return Ok(Value::List(result));
            }
        }
    }
    Err(EvalError::Error(
        "Table: unsupported form (use Table[expr, n] for n copies, \
         or the special form for iterator specs)"
            .to_string(),
    ))
}

pub fn builtin_map(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Map requires exactly 2 arguments".to_string(),
        ));
    }
    let f = &args[0];
    match &args[1] {
        Value::List(items) => {
            let mut result = Vec::new();
            for item in items {
                result.push(apply_function(f, &[item.clone()], env)?);
            }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[1].type_name().to_string(),
        }),
    }
}

pub fn builtin_fold(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let (f, init, items) = match args.len() {
        2 => match &args[1] {
            Value::List(list) if !list.is_empty() => (&args[0], list[0].clone(), &list[1..]),
            Value::List(_) => {
                return Err(EvalError::Error(
                    "Fold on empty list requires initial value".to_string(),
                ));
            }
            _ => {
                return Err(EvalError::TypeError {
                    expected: "List".to_string(),
                    got: args[1].type_name().to_string(),
                });
            }
        },
        3 => match &args[2] {
            Value::List(list) => (&args[0], args[1].clone(), list.as_slice()),
            _ => {
                return Err(EvalError::TypeError {
                    expected: "List".to_string(),
                    got: args[2].type_name().to_string(),
                });
            }
        },
        _ => {
            return Err(EvalError::Error(
                "Fold requires 2 or 3 arguments".to_string(),
            ));
        }
    };
    let mut acc = init;
    for item in items {
        acc = apply_function(f, &[acc, item.clone()], env)?;
    }
    Ok(acc)
}

pub fn builtin_select(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Select requires exactly 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let test = &args[1];
            let mut result = Vec::new();
            for item in items {
                let keep = apply_function(test, &[item.clone()], env)?;
                if keep.to_bool() {
                    result.push(item.clone());
                }
            }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_scan(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Scan requires exactly 2 arguments".to_string(),
        ));
    }
    match &args[1] {
        Value::List(items) => {
            for item in items {
                apply_function(&args[0], &[item.clone()], env)?;
            }
            Ok(Value::Null)
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[1].type_name().to_string(),
        }),
    }
}

pub fn builtin_nest(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "Nest requires exactly 3 arguments".to_string(),
        ));
    }
    let n = args[2].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[2].type_name().to_string(),
    })?;
    if n < 0 {
        return Err(EvalError::Error(
            "Nest count must be non-negative".to_string(),
        ));
    }
    let mut val = args[1].clone();
    for _ in 0..n {
        val = apply_function(&args[0], &[val], env)?;
    }
    Ok(val)
}

pub fn builtin_take(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Take requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::List(items), Value::List(range)) if range.len() == 2 => {
            let m = range[0].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: range[0].type_name().to_string(),
            })?;
            let n = range[1].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: range[1].type_name().to_string(),
            })?;
            let start = if m >= 1 { (m - 1) as usize } else { 0 };
            let end = (n as usize).min(items.len());
            if start >= end {
                return Ok(Value::List(vec![]));
            }
            Ok(Value::List(items[start..end].to_vec()))
        }
        (Value::List(items), Value::Integer(n)) => {
            let n_i64 = n.to_i64().unwrap_or(0);
            let count = if n_i64 >= 0 {
                n_i64 as usize
            } else {
                items.len() - (-n_i64) as usize
            };
            Ok(Value::List(items[..count.min(items.len())].to_vec()))
        }
        _ => Err(EvalError::TypeError {
            expected: "List and Integer".to_string(),
            got: format!("{} and {}", args[0].type_name(), args[1].type_name()),
        }),
    }
}

pub fn builtin_drop(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Drop requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::List(items), Value::List(range)) if range.len() == 2 => {
            let m = range[0].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: range[0].type_name().to_string(),
            })?;
            let n = range[1].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: range[1].type_name().to_string(),
            })?;
            let start = if m >= 1 { (m - 1) as usize } else { 0 };
            let end = (n as usize).min(items.len());
            if start == 0 {
                Ok(Value::List(items[end..].to_vec()))
            } else {
                let mut result: Vec<Value> = items[..start].to_vec();
                result.extend_from_slice(&items[end..]);
                Ok(Value::List(result))
            }
        }
        (Value::List(items), Value::Integer(n)) => {
            let n_i64 = n.to_i64().unwrap_or(0);
            let count = if n_i64 >= 0 {
                n_i64 as usize
            } else {
                items.len() - (-n_i64) as usize
            };
            Ok(Value::List(items[count.min(items.len())..].to_vec()))
        }
        _ => Err(EvalError::TypeError {
            expected: "List and Integer".to_string(),
            got: format!("{} and {}", args[0].type_name(), args[1].type_name()),
        }),
    }
}

pub fn builtin_riffle(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Riffle requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::List(items), sep) => {
            let mut result = Vec::new();
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    result.push(sep.clone());
                }
                result.push(item.clone());
            }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_transpose(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Transpose requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(rows) => {
            if rows.is_empty() {
                return Ok(Value::List(vec![]));
            }
            let cols = match &rows[0] {
                Value::List(items) => items.len(),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "List of Lists".to_string(),
                        got: "List of non-Lists".to_string(),
                    });
                }
            };
            let mut result = vec![Vec::new(); cols];
            for row in rows {
                match row {
                    Value::List(items) => {
                        for (j, item) in items.iter().enumerate() {
                            result[j].push(item.clone());
                        }
                    }
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: "List of Lists".to_string(),
                            got: "List of non-Lists".to_string(),
                        });
                    }
                }
            }
            Ok(Value::List(result.into_iter().map(Value::List).collect()))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_total(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Total requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => super::arithmetic::builtin_plus(items),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_sum(_args: &[Value]) -> Result<Value, EvalError> {
    // Sum[expr, {i, min, max}] — handled by evaluator
    Err(EvalError::Error(
        "Sum should be handled by evaluator".to_string(),
    ))
}

pub fn builtin_product(_args: &[Value]) -> Result<Value, EvalError> {
    // Product[expr, {i, min, max}] — handled by evaluator
    Err(EvalError::Error(
        "Product should be handled by evaluator".to_string(),
    ))
}

// ── Extended list operations ──

pub fn builtin_member_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MemberQ requires exactly 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => Ok(Value::Bool(
            items.iter().any(|item| item.struct_eq(&args[1])),
        )),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_count(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Count requires exactly 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => Ok(Value::Integer(Integer::from(
            items.iter().filter(|item| item.struct_eq(&args[1])).count() as i64,
        ))),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_position(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Position requires exactly 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let positions: Vec<Value> = items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.struct_eq(&args[1]))
                .map(|(i, _)| Value::Integer(Integer::from(i as i64 + 1)))
                .collect();
            Ok(Value::List(positions))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_union(args: &[Value]) -> Result<Value, EvalError> {
    let mut seen = Vec::new();
    for arg in args {
        match arg {
            Value::List(items) => {
                for item in items {
                    if !seen.iter().any(|s: &Value| s.struct_eq(item)) {
                        seen.push(item.clone());
                    }
                }
            }
            _ => {
                return Err(EvalError::TypeError {
                    expected: "List".to_string(),
                    got: arg.type_name().to_string(),
                });
            }
        }
    }
    Ok(Value::List(seen))
}

pub fn builtin_intersection(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Intersection requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::List(a), Value::List(b)) => Ok(Value::List(
            a.iter()
                .filter(|item| b.iter().any(|bitem| bitem.struct_eq(item)))
                .cloned()
                .collect(),
        )),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: format!("{} and {}", args[0].type_name(), args[1].type_name()),
        }),
    }
}

pub fn builtin_complement(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Complement requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::List(a), Value::List(b)) => Ok(Value::List(
            a.iter()
                .filter(|item| !b.iter().any(|bitem| bitem.struct_eq(item)))
                .cloned()
                .collect(),
        )),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: format!("{} and {}", args[0].type_name(), args[1].type_name()),
        }),
    }
}

pub fn builtin_tally(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Tally requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let mut counts: Vec<(Value, i64)> = Vec::new();
            for item in items {
                if let Some(entry) = counts.iter_mut().find(|(k, _)| k.struct_eq(item)) {
                    entry.1 += 1;
                } else {
                    counts.push((item.clone(), 1));
                }
            }
            Ok(Value::List(
                counts
                    .into_iter()
                    .map(|(val, count)| {
                        Value::List(vec![val, Value::Integer(Integer::from(count))])
                    })
                    .collect(),
            ))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_pad_left(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "PadLeft requires 2 or 3 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[1].type_name().to_string(),
            })? as usize;
            let pad_val = if args.len() == 3 {
                args[2].clone()
            } else {
                Value::Null
            };
            if n <= items.len() {
                Ok(Value::List(items[items.len() - n..].to_vec()))
            } else {
                let mut result: Vec<Value> = vec![pad_val; n - items.len()];
                result.extend(items.iter().cloned());
                Ok(Value::List(result))
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_pad_right(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "PadRight requires 2 or 3 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[1].type_name().to_string(),
            })? as usize;
            let pad_val = if args.len() == 3 {
                args[2].clone()
            } else {
                Value::Null
            };
            let mut result = items.clone();
            if n > items.len() {
                result.resize(n, pad_val);
            } else {
                result.truncate(n);
            }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── New list functions ──

/// Partition[list, n] — split list into sublists of length n.
/// Partition[list, n, d] — use offset d between successive sublists.
pub fn builtin_partition(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Partition requires 2 or 3 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if n <= 0 {
        return Err(EvalError::Error(
            "Partition size must be positive".to_string(),
        ));
    }
    let d = if args.len() == 3 {
        let step = args[2].to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[2].type_name().to_string(),
        })?;
        if step <= 0 {
            return Err(EvalError::Error(
                "Partition offset must be positive".to_string(),
            ));
        }
        step as usize
    } else {
        n as usize
    };
    let n = n as usize;
    let mut result = Vec::new();
    let mut i = 0;
    while i + n <= items.len() {
        result.push(Value::List(items[i..i + n].to_vec()));
        i += d;
    }
    Ok(Value::List(result))
}

/// Split[list] — split into runs of identical adjacent elements.
pub fn builtin_split(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Split requires exactly 1 argument".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    if items.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let mut result: Vec<Value> = Vec::new();
    let mut run: Vec<Value> = vec![items[0].clone()];
    for item in &items[1..] {
        if item.struct_eq(run.last().unwrap()) {
            run.push(item.clone());
        } else {
            result.push(Value::List(run));
            run = vec![item.clone()];
        }
    }
    result.push(Value::List(run));
    Ok(Value::List(result))
}

/// Gather[list] — group identical elements into sublists.
pub fn builtin_gather(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Gather requires exactly 1 argument".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let mut groups: Vec<Vec<Value>> = Vec::new();
    for item in items {
        if let Some(group) = groups.iter_mut().find(|g| g[0].struct_eq(item)) {
            group.push(item.clone());
        } else {
            groups.push(vec![item.clone()]);
        }
    }
    Ok(Value::List(groups.into_iter().map(Value::List).collect()))
}

/// DeleteDuplicates[list] — remove duplicates, keeping the first occurrence.
pub fn builtin_delete_duplicates(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "DeleteDuplicates requires exactly 1 argument".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let mut seen: Vec<Value> = Vec::new();
    for item in items {
        if !seen.iter().any(|s| s.struct_eq(item)) {
            seen.push(item.clone());
        }
    }
    Ok(Value::List(seen))
}

/// Insert[list, elem, n] — insert element at position n (1-indexed).
/// Negative n counts from the end (n = -1 inserts before the last element).
pub fn builtin_insert(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "Insert requires exactly 3 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let n = args[2].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[2].type_name().to_string(),
    })?;
    // Insert uses different negative indexing: -1 means "before the last element"
    // i.e., position = items.len() (1-indexed)
    let idx = if n > 0 {
        let idx = (n - 1) as usize;
        if idx > items.len() {
            return Err(EvalError::IndexOutOfBounds {
                index: n,
                length: items.len(),
            });
        }
        idx
    } else if n < 0 {
        let abs = (-n) as usize;
        if abs > items.len() {
            return Err(EvalError::IndexOutOfBounds {
                index: n,
                length: items.len(),
            });
        }
        items.len() - abs
    } else {
        return Err(EvalError::IndexOutOfBounds {
            index: 0,
            length: items.len(),
        });
    };
    let elem = args[1].clone();
    let mut result = items.to_vec();
    result.insert(idx, elem);
    Ok(Value::List(result))
}

/// Delete[list, n] — delete element at position n (1-indexed).
pub fn builtin_delete(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Delete requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    let idx = normalize_index(n, items.len())?;
    let mut result = items.to_vec();
    result.remove(idx);
    Ok(Value::List(result))
}

/// ReplacePart[list, n, new] — replace element at position n with new.
pub fn builtin_replace_part(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "ReplacePart requires exactly 3 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    let idx = normalize_index(n, items.len())?;
    let mut result = items.to_vec();
    result[idx] = args[2].clone();
    Ok(Value::List(result))
}

/// RotateLeft[list, n] — rotate elements n positions to the left.
pub fn builtin_rotate_left(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "RotateLeft requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    if items.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    let len = items.len();
    let shift = ((n % len as i64 + len as i64) % len as i64) as usize;
    let mut result = Vec::with_capacity(len);
    result.extend_from_slice(&items[shift..]);
    result.extend_from_slice(&items[..shift]);
    Ok(Value::List(result))
}

/// RotateRight[list, n] — rotate elements n positions to the right.
pub fn builtin_rotate_right(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "RotateRight requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    if items.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    // RotateRight[n] == RotateLeft[-n]
    let len = items.len();
    let shift = ((-n) % len as i64 + len as i64) % len as i64;
    let shift = shift as usize;
    let mut result = Vec::with_capacity(len);
    result.extend_from_slice(&items[shift..]);
    result.extend_from_slice(&items[..shift]);
    Ok(Value::List(result))
}

/// Ordering[list] — return positions that would sort the list.
/// Ordering[list, n] — return first n positions.
/// Ordering[list, -n] — return last n positions.
pub fn builtin_ordering(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Ordering requires 1 or 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let mut indices: Vec<usize> = (0..items.len()).collect();
    indices.sort_by(|&i, &j| compare_values(&items[i], &items[j]));
    let n = if args.len() == 2 {
        args[1].to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[1].type_name().to_string(),
        })?
    } else {
        items.len() as i64
    };
    let positions: Vec<Value> = if n >= 0 {
        let count = (n as usize).min(indices.len());
        indices[..count]
            .iter()
            .map(|&i| Value::Integer(Integer::from((i + 1) as i64)))
            .collect()
    } else {
        let count = ((-n) as usize).min(indices.len());
        indices[indices.len() - count..]
            .iter()
            .map(|&i| Value::Integer(Integer::from((i + 1) as i64)))
            .collect()
    };
    Ok(Value::List(positions))
}

/// ConstantArray[val, n] — create a list of n copies of val.
pub fn builtin_constant_array(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ConstantArray requires exactly 2 arguments".to_string(),
        ));
    }
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if n < 0 {
        return Err(EvalError::Error(
            "ConstantArray count must be non-negative".to_string(),
        ));
    }
    Ok(Value::List(vec![args[0].clone(); n as usize]))
}

/// Diagonal[matrix] — extract the diagonal elements from a matrix.
pub fn builtin_diagonal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Diagonal requires exactly 1 argument".to_string(),
        ));
    }
    let rows = get_list(&args[0])?;
    let mut result = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        match row {
            Value::List(items) => {
                if i < items.len() {
                    result.push(items[i].clone());
                }
            }
            _ => {
                return Err(EvalError::TypeError {
                    expected: "List".to_string(),
                    got: row.type_name().to_string(),
                });
            }
        }
    }
    Ok(Value::List(result))
}

/// Accumulate[list] — running total (cumulative sum) of list elements.
pub fn builtin_accumulate(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Accumulate requires exactly 1 argument".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "Accumulate called on empty list".to_string(),
        ));
    }
    let mut result = Vec::with_capacity(items.len());
    let mut acc = items[0].clone();
    result.push(acc.clone());
    for item in &items[1..] {
        acc = crate::builtins::arithmetic::add_values_public(&acc, item)?;
        result.push(acc.clone());
    }
    Ok(Value::List(result))
}

/// Differences[list] — adjacent differences of list elements.
pub fn builtin_differences(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Differences requires exactly 1 argument".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    if items.len() < 2 {
        return Ok(Value::List(vec![]));
    }
    let mut result = Vec::with_capacity(items.len() - 1);
    for pair in items.windows(2) {
        result.push(crate::builtins::arithmetic::sub_values_public(
            &pair[1], &pair[0],
        )?);
    }
    Ok(Value::List(result))
}

/// Clip[val] — clamp val to [0, 1].
/// Clip[val, {min, max}] — clamp val to [min, max].
pub fn builtin_clip(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Clip requires 1 or 2 arguments".to_string(),
        ));
    }
    let (min, max) = if args.len() == 2 {
        match &args[1] {
            Value::List(bounds) if bounds.len() == 2 => (bounds[0].clone(), bounds[1].clone()),
            _ => {
                return Err(EvalError::TypeError {
                    expected: "List of length 2".to_string(),
                    got: args[1].type_name().to_string(),
                });
            }
        }
    } else {
        (
            Value::Integer(Integer::from(0)),
            Value::Integer(Integer::from(1)),
        )
    };
    // Use builtin_less for comparison: if val < min, return min
    let less_min = crate::builtins::comparison::builtin_less(&[args[0].clone(), min.clone()])?;
    if less_min.to_bool() {
        return Ok(min);
    }
    // if max < val, return max
    let less_max = crate::builtins::comparison::builtin_less(&[max.clone(), args[0].clone()])?;
    if less_max.to_bool() {
        return Ok(max);
    }
    Ok(args[0].clone())
}

// ── Env-aware list functions ──

/// Array[f, n] — generate {f[1], f[2], ..., f[n]}.
/// Array[f, {n}] — same.
/// Array[f, {n, m}] — generate {f[n], f[n+1], ..., f[m]}.
pub fn builtin_array(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Array requires exactly 2 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let (start, end) = match &args[1] {
        Value::Integer(n) => {
            let n_i64 = n.to_i64().unwrap_or(0);
            if n_i64 <= 0 {
                return Ok(Value::List(vec![]));
            }
            (1_i64, n_i64)
        }
        Value::List(spec) if spec.len() == 1 => {
            let n = spec[0].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: spec[0].type_name().to_string(),
            })?;
            if n <= 0 {
                return Ok(Value::List(vec![]));
            }
            (1_i64, n)
        }
        Value::List(spec) if spec.len() == 2 => {
            let a = spec[0].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: spec[0].type_name().to_string(),
            })?;
            let b = spec[1].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: spec[1].type_name().to_string(),
            })?;
            if a > b {
                return Ok(Value::List(vec![]));
            }
            (a, b)
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer or List".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let mut result = Vec::with_capacity((end - start + 1) as usize);
    for i in start..=end {
        let val = apply_function(f, &[Value::Integer(Integer::from(i))], env)?;
        result.push(val);
    }
    Ok(Value::List(result))
}

/// SplitBy[list, f] — split into runs where f gives identical values.
pub fn builtin_split_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "SplitBy requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let f = &args[1];
    if items.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let mut result: Vec<Value> = Vec::new();
    let mut key = apply_function(f, &[items[0].clone()], env)?;
    let mut run: Vec<Value> = vec![items[0].clone()];
    for item in &items[1..] {
        let new_key = apply_function(f, &[item.clone()], env)?;
        if new_key.struct_eq(&key) {
            run.push(item.clone());
        } else {
            result.push(Value::List(run));
            key = new_key;
            run = vec![item.clone()];
        }
    }
    result.push(Value::List(run));
    Ok(Value::List(result))
}

/// GatherBy[list, f] — group elements by the values of f applied to each.
pub fn builtin_gather_by(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GatherBy requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let f = &args[1];
    let mut groups: Vec<(Value, Vec<Value>)> = Vec::new();
    for item in items {
        let key = apply_function(f, &[item.clone()], env)?;
        if let Some((_, group)) = groups.iter_mut().find(|(k, _)| k.struct_eq(&key)) {
            group.push(item.clone());
        } else {
            groups.push((key, vec![item.clone()]));
        }
    }
    Ok(Value::List(
        groups.into_iter().map(|(_, g)| Value::List(g)).collect(),
    ))
}

/// FoldList[f, init, list] — give all intermediate results of folding f from left.
pub fn builtin_fold_list(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "FoldList requires exactly 3 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let list = get_list(&args[2])?;
    let mut acc = args[1].clone();
    let mut result = vec![acc.clone()];
    for item in list {
        acc = apply_function(f, &[acc, item.clone()], env)?;
        result.push(acc.clone());
    }
    Ok(Value::List(result))
}

/// NestList[f, expr, n] — give all intermediate results of applying f repeatedly n times.
pub fn builtin_nest_list(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "NestList requires exactly 3 arguments".to_string(),
        ));
    }
    let n = args[2].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[2].type_name().to_string(),
    })?;
    if n < 0 {
        return Err(EvalError::Error(
            "NestList count must be non-negative".to_string(),
        ));
    }
    let mut val = args[1].clone();
    let mut result = vec![val.clone()];
    for _ in 0..n {
        val = apply_function(&args[0], &[val], env)?;
        result.push(val.clone());
    }
    Ok(Value::List(result))
}

// ── MapApply ──────────────────────────────────────────────────────────

/// MapApply[f, expr] (like f @@@ expr) replaces heads at level 1.
///
/// For lists: MapApply[f, {{a,b}, {c,d}}] → {f[a,b], f[c,d]}
/// For non-list items: MapApply[f, {a, b}] → {f[a], f[b]}
pub fn builtin_map_apply(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MapApply requires exactly 2 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let items = get_list(&args[1])?;
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::List(sub_args) => {
                result.push(apply_function(f, sub_args, env)?);
            }
            other => {
                result.push(apply_function(f, &[other.clone()], env)?);
            }
        }
    }
    Ok(Value::List(result))
}

// ── MovingAverage ─────────────────────────────────────────────────────

/// MovingAverage[list, n] computes the moving average with window size n.
pub fn builtin_moving_average(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MovingAverage requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if n <= 0 {
        return Err(EvalError::Error(
            "MovingAverage window size must be positive".to_string(),
        ));
    }
    let n = n as usize;
    if items.len() < n {
        return Ok(Value::List(vec![]));
    }
    let mut result = Vec::with_capacity(items.len() - n + 1);
    for window in items.windows(n) {
        let count = window.len() as f64;
        let sum: f64 = window.iter().filter_map(to_f64).sum();
        result.push(real(sum / count));
    }
    Ok(Value::List(result))
}

// ── BlockMap ──────────────────────────────────────────────────────────

/// BlockMap[f, list, n] partitions list into non-overlapping blocks of
/// size n, applies f to each block, and returns the list of results.
pub fn builtin_block_map(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 3 || args.len() > 4 {
        return Err(EvalError::Error(
            "BlockMap requires 3 or 4 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let items = get_list(&args[1])?;
    let n = args[2].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[2].type_name().to_string(),
    })?;
    if n <= 0 {
        return Err(EvalError::Error(
            "BlockMap block size must be positive".to_string(),
        ));
    }
    let n = n as usize;
    let mut result = Vec::new();
    for chunk in items.chunks(n) {
        if chunk.len() == n {
            let block = Value::List(chunk.to_vec());
            result.push(apply_function(f, &[block], env)?);
        }
    }
    Ok(Value::List(result))
}

// ── ListConvolve ──────────────────────────────────────────────────────

/// ListConvolve[kernel, list] computes the convolution of kernel with list.
///
/// output[j] = sum_i kernel[i] * list[j + i]  for j = 0..len(list)-len(kernel)
///
/// Numeric elements only; non-numeric elements are treated as zero.
pub fn builtin_list_convolve(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 5 {
        return Err(EvalError::Error(
            "ListConvolve requires 2 to 5 arguments".to_string(),
        ));
    }
    let kernel = get_list(&args[0])?;
    let list = get_list(&args[1])?;
    if kernel.is_empty() || list.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let k = kernel.len();
    let n = list.len();
    if n < k {
        return Ok(Value::List(vec![]));
    }
    let mut result = Vec::with_capacity(n - k + 1);
    for j in 0..=(n - k) {
        let mut sum = 0.0f64;
        for (i, k_val) in kernel.iter().enumerate() {
            if let (Some(kf), Some(lf)) = (to_f64(k_val), to_f64(&list[j + i])) {
                sum += kf * lf;
            }
        }
        result.push(real(sum));
    }
    Ok(Value::List(result))
}

// ── Nearest ───────────────────────────────────────────────────────────

/// Nearest[list, x] finds the element(s) in list closest to x.
/// Nearest[list, x, n] finds the n closest elements.
///
/// Uses absolute numeric distance; non-numeric elements are ignored.
pub fn builtin_nearest(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Nearest requires 2 or 3 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let target = &args[1];
    let n = if args.len() == 3 {
        let nn = args[2].to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[2].type_name().to_string(),
        })?;
        if nn <= 0 {
            return Err(EvalError::Error("Nearest n must be positive".to_string()));
        }
        nn as usize
    } else {
        1
    };

    if items.is_empty() {
        return Ok(Value::List(vec![]));
    }

    // Compute distances as f64
    let target_f = to_f64(target)
        .ok_or_else(|| EvalError::Error("Nearest target must be numeric".to_string()))?;

    let mut scored: Vec<(f64, &Value)> = items
        .iter()
        .filter_map(|item| {
            to_f64(item).map(|f| {
                let d = (f - target_f).abs();
                (d, item)
            })
        })
        .collect();

    if scored.is_empty() {
        return Ok(Value::List(vec![]));
    }

    scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let result: Vec<Value> = scored.into_iter().take(n).map(|(_, v)| v.clone()).collect();
    if n == 1 {
        Ok(result.into_iter().next().unwrap_or(Value::Null))
    } else {
        Ok(Value::List(result))
    }
}

/// Apply[func, expr] — calls func with the elements of expr as arguments.
/// If expr is a List, the list elements become the arguments.
/// Otherwise, func is called with expr as a single argument.
pub fn builtin_apply(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Apply requires exactly 2 arguments".to_string(),
        ));
    }
    let func = &args[0];
    let expr = &args[1];
    match expr {
        Value::List(items) => apply_function(func, items, env),
        _ => apply_function(func, &[expr.clone()], env),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn list(vals: Vec<Value>) -> Value {
        Value::List(vals)
    }

    #[test]
    fn test_length() {
        assert_eq!(
            builtin_length(&[list(vec![int(1), int(2), int(3)])]).unwrap(),
            int(3)
        );
        assert_eq!(builtin_length(&[list(vec![])]).unwrap(), int(0));
    }

    #[test]
    fn test_first() {
        assert_eq!(
            builtin_first(&[list(vec![int(1), int(2)])]).unwrap(),
            int(1)
        );
    }

    #[test]
    fn test_first_empty() {
        assert!(builtin_first(&[list(vec![])]).is_err());
    }

    #[test]
    fn test_last() {
        assert_eq!(
            builtin_last(&[list(vec![int(1), int(2), int(3)])]).unwrap(),
            int(3)
        );
    }

    #[test]
    fn test_rest() {
        let result = builtin_rest(&[list(vec![int(1), int(2), int(3)])]).unwrap();
        assert_eq!(result, list(vec![int(2), int(3)]));
    }

    #[test]
    fn test_most() {
        let result = builtin_most(&[list(vec![int(1), int(2), int(3)])]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2)]));
    }

    #[test]
    fn test_append() {
        let result = builtin_append(&[list(vec![int(1), int(2)]), int(3)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_prepend() {
        let result = builtin_prepend(&[list(vec![int(2), int(3)]), int(1)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_join() {
        let result =
            builtin_join(&[list(vec![int(1), int(2)]), list(vec![int(3), int(4)])]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3), int(4)]));
    }

    #[test]
    fn test_flatten() {
        let nested = list(vec![
            list(vec![int(1), int(2)]),
            list(vec![int(3), list(vec![int(4)])]),
        ]);
        let result = builtin_flatten(&[nested]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3), int(4)]));
    }

    #[test]
    fn test_sort() {
        let result = builtin_sort(&[list(vec![int(3), int(1), int(2)])]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_reverse() {
        let result = builtin_reverse(&[list(vec![int(1), int(2), int(3)])]).unwrap();
        assert_eq!(result, list(vec![int(3), int(2), int(1)]));
    }

    #[test]
    fn test_part_positive_index() {
        assert_eq!(
            builtin_part(&[list(vec![int(10), int(20), int(30)]), int(1)]).unwrap(),
            int(10)
        );
        assert_eq!(
            builtin_part(&[list(vec![int(10), int(20), int(30)]), int(3)]).unwrap(),
            int(30)
        );
    }

    #[test]
    fn test_part_negative_index() {
        assert_eq!(
            builtin_part(&[list(vec![int(10), int(20), int(30)]), int(-1)]).unwrap(),
            int(30)
        );
    }

    #[test]
    fn test_part_out_of_bounds() {
        assert!(builtin_part(&[list(vec![int(1)]), int(5)]).is_err());
    }

    #[test]
    fn test_range_one_arg() {
        let result = builtin_range(&[int(3)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_range_two_args() {
        let result = builtin_range(&[int(2), int(5)]).unwrap();
        assert_eq!(result, list(vec![int(2), int(3), int(4), int(5)]));
    }

    #[test]
    fn test_range_three_args() {
        let result = builtin_range(&[int(0), int(2), int(10)]).unwrap();
        assert_eq!(
            result,
            list(vec![int(0), int(2), int(4), int(6), int(8), int(10)])
        );
    }

    #[test]
    fn test_take() {
        let result = builtin_take(&[list(vec![int(1), int(2), int(3), int(4)]), int(2)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2)]));
    }

    #[test]
    fn test_drop() {
        let result = builtin_drop(&[list(vec![int(1), int(2), int(3), int(4)]), int(2)]).unwrap();
        assert_eq!(result, list(vec![int(3), int(4)]));
    }

    #[test]
    fn test_total() {
        let result = builtin_total(&[list(vec![int(1), int(2), int(3)])]).unwrap();
        assert_eq!(result, int(6));
    }

    // ── New function tests ──

    #[test]
    fn test_partition_basic() {
        let result =
            builtin_partition(&[list(vec![int(1), int(2), int(3), int(4)]), int(2)]).unwrap();
        assert_eq!(
            result,
            list(vec![list(vec![int(1), int(2)]), list(vec![int(3), int(4)])])
        );
    }

    #[test]
    fn test_partition_with_offset() {
        let result = builtin_partition(&[
            list(vec![int(1), int(2), int(3), int(4), int(5)]),
            int(3),
            int(1),
        ])
        .unwrap();
        assert_eq!(
            result,
            list(vec![
                list(vec![int(1), int(2), int(3)]),
                list(vec![int(2), int(3), int(4)]),
                list(vec![int(3), int(4), int(5)]),
            ])
        );
    }

    #[test]
    fn test_partition_empty() {
        let result = builtin_partition(&[list(vec![]), int(2)]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_partition_errors() {
        assert!(builtin_partition(&[list(vec![int(1)]), int(0)]).is_err());
        assert!(builtin_partition(&[list(vec![int(1)]), int(-1)]).is_err());
    }

    #[test]
    fn test_split_basic() {
        let result = builtin_split(&[list(vec![int(1), int(1), int(2), int(2), int(3)])]).unwrap();
        assert_eq!(
            result,
            list(vec![
                list(vec![int(1), int(1)]),
                list(vec![int(2), int(2)]),
                list(vec![int(3)]),
            ])
        );
    }

    #[test]
    fn test_split_empty() {
        let result = builtin_split(&[list(vec![])]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_split_single() {
        let result = builtin_split(&[list(vec![int(42)])]).unwrap();
        assert_eq!(result, list(vec![list(vec![int(42)])]));
    }

    #[test]
    fn test_gather_basic() {
        let result = builtin_gather(&[list(vec![int(1), int(2), int(1), int(3), int(2)])]).unwrap();
        assert_eq!(
            result,
            list(vec![
                list(vec![int(1), int(1)]),
                list(vec![int(2), int(2)]),
                list(vec![int(3)]),
            ])
        );
    }

    #[test]
    fn test_gather_empty() {
        let result = builtin_gather(&[list(vec![])]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_delete_duplicates_basic() {
        let result =
            builtin_delete_duplicates(&[list(vec![int(1), int(2), int(1), int(3), int(2)])])
                .unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_delete_duplicates_empty() {
        let result = builtin_delete_duplicates(&[list(vec![])]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_insert_basic() {
        let result =
            builtin_insert(&[list(vec![int(1), int(2), int(3)]), int(99), int(2)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(99), int(2), int(3)]));
    }

    #[test]
    fn test_insert_at_end() {
        let result = builtin_insert(&[list(vec![int(1), int(2)]), int(99), int(3)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(99)]));
    }

    #[test]
    fn test_insert_negative() {
        let result =
            builtin_insert(&[list(vec![int(1), int(2), int(3)]), int(99), int(-1)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(99), int(3)]));
    }

    #[test]
    fn test_delete_basic() {
        let result = builtin_delete(&[list(vec![int(1), int(2), int(3)]), int(2)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(3)]));
    }

    #[test]
    fn test_delete_negative() {
        let result = builtin_delete(&[list(vec![int(1), int(2), int(3)]), int(-1)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2)]));
    }

    #[test]
    fn test_replace_part_basic() {
        let result =
            builtin_replace_part(&[list(vec![int(1), int(2), int(3)]), int(2), int(99)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(99), int(3)]));
    }

    #[test]
    fn test_rotate_left_basic() {
        let result =
            builtin_rotate_left(&[list(vec![int(1), int(2), int(3), int(4)]), int(2)]).unwrap();
        assert_eq!(result, list(vec![int(3), int(4), int(1), int(2)]));
    }

    #[test]
    fn test_rotate_left_zero() {
        let result = builtin_rotate_left(&[list(vec![int(1), int(2), int(3)]), int(0)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_rotate_left_wrap() {
        let result = builtin_rotate_left(&[list(vec![int(1), int(2), int(3)]), int(5)]).unwrap();
        assert_eq!(result, list(vec![int(3), int(1), int(2)]));
    }

    #[test]
    fn test_rotate_right_basic() {
        let result =
            builtin_rotate_right(&[list(vec![int(1), int(2), int(3), int(4)]), int(2)]).unwrap();
        assert_eq!(result, list(vec![int(3), int(4), int(1), int(2)]));
    }

    #[test]
    fn test_ordering_basic() {
        let result = builtin_ordering(&[list(vec![int(3), int(1), int(2)])]).unwrap();
        assert_eq!(result, list(vec![int(2), int(3), int(1)]));
    }

    #[test]
    fn test_ordering_with_n() {
        let result = builtin_ordering(&[list(vec![int(3), int(1), int(2)]), int(2)]).unwrap();
        assert_eq!(result, list(vec![int(2), int(3)]));
    }

    #[test]
    fn test_ordering_negative_n() {
        let result = builtin_ordering(&[list(vec![int(3), int(1), int(2)]), int(-1)]).unwrap();
        assert_eq!(result, list(vec![int(1)]));
    }

    #[test]
    fn test_constant_array() {
        let result = builtin_constant_array(&[int(7), int(5)]).unwrap();
        assert_eq!(result, list(vec![int(7), int(7), int(7), int(7), int(7)]));
    }

    #[test]
    fn test_constant_array_zero() {
        let result = builtin_constant_array(&[int(7), int(0)]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_diagonal() {
        let mat = list(vec![list(vec![int(1), int(2)]), list(vec![int(3), int(4)])]);
        let result = builtin_diagonal(&[mat]).unwrap();
        assert_eq!(result, list(vec![int(1), int(4)]));
    }

    #[test]
    fn test_diagonal_empty() {
        let result = builtin_diagonal(&[list(vec![])]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_accumulate() {
        let result = builtin_accumulate(&[list(vec![int(1), int(2), int(3), int(4)])]).unwrap();
        assert_eq!(result, list(vec![int(1), int(3), int(6), int(10)]));
    }

    #[test]
    fn test_accumulate_empty() {
        assert!(builtin_accumulate(&[list(vec![])]).is_err());
    }

    #[test]
    fn test_differences() {
        let result = builtin_differences(&[list(vec![int(1), int(4), int(9), int(16)])]).unwrap();
        assert_eq!(result, list(vec![int(3), int(5), int(7)]));
    }

    #[test]
    fn test_differences_short() {
        let result = builtin_differences(&[list(vec![int(1)])]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_differences_empty() {
        let result = builtin_differences(&[list(vec![])]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_clip_default() {
        let result = builtin_clip(&[int(5)]).unwrap();
        assert_eq!(result, int(1));
    }

    #[test]
    fn test_clip_low() {
        let result = builtin_clip(&[int(-1)]).unwrap();
        assert_eq!(result, int(0));
    }

    #[test]
    fn test_clip_mid() {
        let result = builtin_clip(&[int(3), list(vec![int(0), int(10)])]).unwrap();
        assert_eq!(result, int(3));
    }

    #[test]
    fn test_take_range() {
        let result = builtin_take(&[
            list(vec![int(1), int(2), int(3), int(4), int(5)]),
            list(vec![int(2), int(4)]),
        ])
        .unwrap();
        assert_eq!(result, list(vec![int(2), int(3), int(4)]));
    }

    #[test]
    fn test_drop_range() {
        let result = builtin_drop(&[
            list(vec![int(1), int(2), int(3), int(4), int(5)]),
            list(vec![int(2), int(4)]),
        ])
        .unwrap();
        assert_eq!(result, list(vec![int(1), int(5)]));
    }

    #[test]
    fn test_part_multi_index() {
        let mat = list(vec![list(vec![int(1), int(2)]), list(vec![int(3), int(4)])]);
        let result = builtin_part(&[mat, int(2), int(1)]).unwrap();
        assert_eq!(result, int(3));
    }
}
