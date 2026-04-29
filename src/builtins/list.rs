use crate::env::Env;
use crate::eval::apply_function;
use crate::value::{EvalError, Value};
use rug::Integer;

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
pub(crate) fn normalize_index(index: i64, size: usize) -> Result<usize, EvalError> {
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
                Value::Call {
                    head,
                    args: call_args,
                } => {
                    if index == 0 {
                        current = Value::Symbol(head.clone());
                    } else {
                        let idx = (index - 1) as usize;
                        if idx < call_args.len() {
                            current = call_args[idx].clone();
                        } else {
                            return Err(EvalError::IndexOutOfBounds {
                                index,
                                length: call_args.len(),
                            });
                        }
                    }
                }
                Value::Symbol(name) => {
                    if index == 0 {
                        current = Value::Symbol("Symbol".to_string());
                    } else if index == 1 {
                        current = Value::Str(name.clone());
                    } else {
                        return Err(EvalError::IndexOutOfBounds { index, length: 1 });
                    }
                }
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "List or Call or String".to_string(),
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
        // Part[Call[f, args], n] — index into call arguments
        // n = 0 → head f, n > 0 → args[n-1]
        Value::Call {
            head,
            args: call_args,
        } => {
            let index = match &args[1] {
                Value::Integer(n) => n.to_i64().unwrap_or(0),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: args[1].type_name().to_string(),
                    });
                }
            };
            if index == 0 {
                Ok(Value::Symbol(head.clone()))
            } else {
                let idx = (index - 1) as usize;
                if idx < call_args.len() {
                    Ok(call_args[idx].clone())
                } else {
                    Err(EvalError::IndexOutOfBounds {
                        index,
                        length: call_args.len(),
                    })
                }
            }
        }
        Value::Symbol(name) => {
            let index = match &args[1] {
                Value::Integer(n) => n.to_i64().unwrap_or(0),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: args[1].type_name().to_string(),
                    });
                }
            };
            if index == 0 {
                Ok(Value::Symbol("Symbol".to_string()))
            } else if index == 1 {
                Ok(Value::Str(name.clone()))
            } else {
                Err(EvalError::IndexOutOfBounds { index, length: 1 })
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
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Table: requires at least 2 arguments (Table[expr, n] or Table[expr, d1, d2, ...])"
                .to_string(),
        ));
    }
    let n = match &args[1] {
        Value::Integer(n) => n,
        _ => {
            return Err(EvalError::Error(
                "Table: dimension spec must be an integer".to_string(),
            ));
        }
    };
    let Some(n_usize) = n.to_usize() else {
        return Err(EvalError::Error(
            "Table: dimension must be a non-negative integer".to_string(),
        ));
    };
    if args.len() == 2 {
        let mut result = Vec::with_capacity(n_usize);
        for _ in 0..n_usize {
            result.push(args[0].clone());
        }
        Ok(Value::List(result))
    } else {
        let remaining = &args[2..];
        let mut result = Vec::with_capacity(n_usize);
        for _ in 0..n_usize {
            let mut sub_args = Vec::with_capacity(1 + remaining.len());
            sub_args.push(args[0].clone());
            sub_args.extend_from_slice(remaining);
            result.push(builtin_table(&sub_args)?);
        }
        Ok(Value::List(result))
    }
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
        let sum: f64 = window.iter().filter_map(super::to_f64).sum();
        result.push(super::real(sum / count));
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
            if let (Some(kf), Some(lf)) = (super::to_f64(k_val), super::to_f64(&list[j + i])) {
                sum += kf * lf;
            }
        }
        result.push(super::real(sum));
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
    let target_f = super::to_f64(target)
        .ok_or_else(|| EvalError::Error("Nearest target must be numeric".to_string()))?;

    let mut scored: Vec<(f64, &Value)> = items
        .iter()
        .filter_map(|item| {
            super::to_f64(item).map(|f| {
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

/// AllApply[f, expr] (like f @@@ expr) replaces heads at level 2.
/// For each level-1 item that is a List, applies f to its elements.
/// Non-list level-1 items are passed as a single argument.
pub fn builtin_all_apply(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "AllApply requires exactly 2 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let items = get_list(&args[1])?;
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::List(sub_items) => {
                let mut row = Vec::with_capacity(sub_items.len());
                for sub in sub_items {
                    row.push(apply_function(f, &[sub.clone()], env)?);
                }
                result.push(Value::List(row));
            }
            _ => {
                result.push(apply_function(f, &[item.clone()], env)?);
            }
        }
    }
    Ok(Value::List(result))
}

/// MapAt[f, list, spec] — apply f at specified positions.
/// spec = single Integer or List of Integers.
pub fn builtin_map_at(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "MapAt requires exactly 3 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let items = get_list(&args[1])?;
    let positions: Vec<i64> = match &args[2] {
        Value::Integer(n) => vec![n.to_i64().unwrap_or(0)],
        Value::List(specs) => {
            let mut pos = Vec::new();
            for s in specs {
                pos.push(s.to_integer().ok_or_else(|| EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: s.type_name().to_string(),
                })?);
            }
            pos
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer or List of Integers".to_string(),
                got: args[2].type_name().to_string(),
            });
        }
    };
    let mut result = items.to_vec();
    for p in &positions {
        let idx = normalize_index(*p, result.len())?;
        result[idx] = apply_function(f, &[result[idx].clone()], env)?;
    }
    Ok(Value::List(result))
}

/// ApplyTo[expr, f] — returns f[expr] as a Call node.
pub fn builtin_apply_to(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ApplyTo requires exactly 2 arguments".to_string(),
        ));
    }
    let expr = args[0].clone();
    let head = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => args[1].type_name().to_string(),
    };
    Ok(Value::Call {
        head,
        args: vec![expr],
    })
}

/// Thread[f[{a,b},{c,d}]] → {f[{a,c}], f[{b,d}]}.
/// Transposes inner lists, rebuilds calls with same head.
pub fn builtin_thread(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Thread requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Call {
            head,
            args: call_args,
        } => {
            if call_args.is_empty() {
                return Ok(args[0].clone());
            }
            let mut lists: Vec<&[Value]> = Vec::with_capacity(call_args.len());
            for arg in call_args {
                match arg {
                    Value::List(items) => lists.push(items.as_slice()),
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: "all arguments must be Lists of equal length".to_string(),
                            got: arg.type_name().to_string(),
                        });
                    }
                }
            }
            let len = lists[0].len();
            for l in &lists[1..] {
                if l.len() != len {
                    return Err(EvalError::Error(
                        "Thread: all lists must have the same length".to_string(),
                    ));
                }
            }
            let mut result = Vec::with_capacity(len);
            for i in 0..len {
                let threaded_args: Vec<Value> = lists.iter().map(|l| l[i].clone()).collect();
                result.push(Value::Call {
                    head: head.clone(),
                    args: threaded_args,
                });
            }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "Call expression".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// Outer[f, l1, l2] — generalized outer product.
pub fn builtin_outer(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "Outer requires exactly 3 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let l1 = get_list(&args[1])?;
    let l2 = get_list(&args[2])?;
    let mut result = Vec::with_capacity(l1.len());
    for a in l1 {
        let mut row = Vec::with_capacity(l2.len());
        for b in l2 {
            row.push(apply_function(f, &[a.clone(), b.clone()], env)?);
        }
        result.push(Value::List(row));
    }
    Ok(Value::List(result))
}

/// Inner[f, g, l1, l2] — generalized inner product.
/// g is the combining function, f is the element-wise function.
pub fn builtin_inner(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let (f, g, l1, l2) = match args.len() {
        3 => {
            // Inner[f, l1, l2] — f is both element-wise and combining (default Times then Plus)
            let l1 = get_list(&args[1])?;
            let l2 = get_list(&args[2])?;
            (&args[0], &Value::Symbol("Plus".to_string()), l1, l2)
        }
        4 => {
            let f = &args[0];
            let g = &args[1];
            let l1 = get_list(&args[2])?;
            let l2 = get_list(&args[3])?;
            (f, g, l1, l2)
        }
        _ => {
            return Err(EvalError::Error(
                "Inner requires 3 or 4 arguments".to_string(),
            ));
        }
    };
    if l1.len() != l2.len() {
        return Err(EvalError::Error(
            "Inner: lists must have the same length".to_string(),
        ));
    }
    let mut products = Vec::with_capacity(l1.len());
    for (a, b) in l1.iter().zip(l2.iter()) {
        products.push(apply_function(f, &[a.clone(), b.clone()], env)?);
    }
    apply_function(g, &products, env)
}

/// MapIndexed[f, list] → {f[{1}, a], f[{2}, b], ...}
pub fn builtin_map_indexed(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MapIndexed requires exactly 2 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let items = get_list(&args[1])?;
    let mut result = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let idx = Value::List(vec![Value::Integer(Integer::from((i + 1) as i64))]);
        result.push(apply_function(f, &[idx, item.clone()], env)?);
    }
    Ok(Value::List(result))
}

/// MapThread[f, {{a,b,c}, {x,y,z}}] → {f[a,x], f[b,y], f[c,z]}
pub fn builtin_map_thread(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MapThread requires exactly 2 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let lists: Vec<&[Value]> = match &args[1] {
        Value::List(specs) => {
            let mut ls = Vec::new();
            for s in specs {
                ls.push(get_list(s)?);
            }
            ls
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "List of Lists".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    if lists.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let len = lists[0].len();
    for l in &lists[1..] {
        if l.len() != len {
            return Err(EvalError::Error(
                "MapThread: all lists must have the same length".to_string(),
            ));
        }
    }
    let mut result = Vec::with_capacity(len);
    for i in 0..len {
        let threaded: Vec<Value> = lists.iter().map(|l| l[i].clone()).collect();
        result.push(apply_function(f, &threaded, env)?);
    }
    Ok(Value::List(result))
}

/// NestWhile[f, x, test] — keep applying f while test[result] is True.
pub fn builtin_nest_while(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "NestWhile requires exactly 3 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let test = &args[2];
    let mut val = args[1].clone();
    for _ in 0..10_000 {
        val = apply_function(f, &[val], env)?;
        let t = apply_function(test, &[val.clone()], env)?;
        if !t.to_bool() {
            return Ok(val);
        }
    }
    Err(EvalError::Error(
        "NestWhile: iteration limit (10000) reached".to_string(),
    ))
}

/// NestWhileList[f, x, test] — same as NestWhile but returns all intermediates.
pub fn builtin_nest_while_list(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "NestWhileList requires exactly 3 arguments".to_string(),
        ));
    }
    let f = &args[0];
    let test = &args[2];
    let mut val = args[1].clone();
    let mut result = vec![val.clone()];
    for _ in 0..10_000 {
        val = apply_function(f, &[val], env)?;
        result.push(val.clone());
        let t = apply_function(test, &[val.clone()], env)?;
        if !t.to_bool() {
            return Ok(Value::List(result));
        }
    }
    Err(EvalError::Error(
        "NestWhileList: iteration limit (10000) reached".to_string(),
    ))
}

/// FixedPointList[f, x, maxIter?] — same as FixedPoint but returns all intermediates.
pub fn builtin_fixed_point_list(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "FixedPointList requires 2 or 3 arguments".to_string(),
        ));
    }
    let max_iter = if args.len() == 3 {
        args[2].to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[2].type_name().to_string(),
        })? as usize
    } else {
        1000
    };
    let f = &args[0];
    let mut val = args[1].clone();
    let mut result = vec![val.clone()];
    for _ in 0..max_iter {
        let new_val = apply_function(f, &[val.clone()], env)?;
        // Floating-point convergence check
        if let (Value::Real(a), Value::Real(b)) = (&new_val, &val) {
            let diff = rug::Float::with_val(crate::value::DEFAULT_PRECISION, a - b).abs();
            if diff < 1e-12 {
                result.push(new_val.clone());
                return Ok(Value::List(result));
            }
        }
        if new_val.struct_eq(&val) {
            result.push(new_val.clone());
            return Ok(Value::List(result));
        }
        val = new_val;
        result.push(val.clone());
    }
    Ok(Value::List(result))
}

/// ArrayPad[list, n] — pad list with n zeros on each side.
/// ArrayPad[list, {before, after}] — pad with different amounts.
/// ArrayPad[list, n, val] — pad with val instead of 0.
pub fn builtin_array_pad(args: &[Value]) -> Result<Value, EvalError> {
    super::require_min_args("ArrayPad", args, 2)?;
    let list = match &args[0] {
        Value::List(l) => l.clone(),
        other => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };
    let (before, after) = match &args[1] {
        Value::Integer(n) => {
            let v = n.to_i32().unwrap_or(0);
            (v, v)
        }
        Value::List(ab) if ab.len() == 2 => {
            let b = super::require_f64(&ab[0], "ArrayPad", 2)? as i32;
            let a = super::require_f64(&ab[1], "ArrayPad", 2)? as i32;
            (b, a)
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "integer or {before, after}".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let fill = if args.len() >= 3 {
        args[2].clone()
    } else {
        Value::Integer(Integer::from(0))
    };
    let mut result = Vec::new();
    for _ in 0..before.max(0) {
        result.push(fill.clone());
    }
    result.extend(list);
    for _ in 0..after.max(0) {
        result.push(fill.clone());
    }
    Ok(Value::List(result))
}

/// ArrayReshape[list, {d1, d2, ...}] — reshape a flat list into the given dimensions.
/// Total elements must match the product of dimensions.
pub fn builtin_array_reshape(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("ArrayReshape", args, 2)?;
    let flat = match &args[0] {
        Value::List(l) => l.clone(),
        other => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };
    let dims = match &args[1] {
        Value::List(d) => {
            let mut result = Vec::new();
            for v in d {
                let n = v.to_integer().ok_or_else(|| EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: v.type_name().to_string(),
                })? as usize;
                if n == 0 {
                    return Err(EvalError::Error(
                        "ArrayReshape: dimension must be positive".to_string(),
                    ));
                }
                result.push(n);
            }
            result
        }
        other => {
            return Err(EvalError::TypeError {
                expected: "List of dimensions".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };

    let total: usize = dims.iter().product();
    if total != flat.len() {
        return Err(EvalError::Error(format!(
            "ArrayReshape: total elements ({}) does not match product of dimensions ({})",
            flat.len(),
            total
        )));
    }

    // Build nested list structure from flat list
    fn build_nested(flat: &[Value], dims: &[usize], offset: usize) -> Value {
        if dims.len() == 1 {
            Value::List(flat[offset..offset + dims[0]].to_vec())
        } else {
            let stride: usize = dims[1..].iter().product();
            let mut result = Vec::with_capacity(dims[0]);
            for i in 0..dims[0] {
                result.push(build_nested(flat, &dims[1..], offset + i * stride));
            }
            Value::List(result)
        }
    }

    Ok(build_nested(&flat, &dims, 0))
}

/// StringCases[string, pattern] — find all substrings matching a literal pattern.
/// For now, supports literal string matching and `"*"` as a wildcard (matches any substring).
pub fn builtin_string_cases(args: &[Value]) -> Result<Value, EvalError> {
    super::require_min_args("StringCases", args, 2)?;
    let haystack = match &args[0] {
        Value::Str(s) => s.clone(),
        other => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };
    let pattern = match &args[1] {
        Value::Str(s) => s.clone(),
        other => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };

    let mut matches = Vec::new();

    if pattern == "*" {
        // Wildcard: match the entire string
        if !haystack.is_empty() {
            matches.push(Value::Str(haystack));
        }
    } else if pattern.contains('*') {
        // Simple wildcard pattern: split by '*' and match prefix/suffix
        let parts: Vec<&str> = pattern.splitn(2, '*').collect();
        let prefix = parts[0];
        let suffix = parts.get(1).unwrap_or(&"");
        // Find all occurrences of the pattern
        for i in 0..haystack.len() {
            if haystack[i..].starts_with(prefix) {
                let after_prefix = i + prefix.len();
                // Find suffix starting from after_prefix
                if let Some(pos) = haystack[after_prefix..].find(suffix) {
                    let end = after_prefix + pos + suffix.len();
                    matches.push(Value::Str(haystack[i..end].to_string()));
                }
            }
        }
    } else {
        // Literal match: find all occurrences
        let mut start = 0;
        while let Some(pos) = haystack[start..].find(&pattern) {
            matches.push(Value::Str(pattern.clone()));
            start += pos + pattern.len();
        }
    }

    Ok(Value::List(matches))
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

    // ── Env-aware function tests ──

    fn test_env() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    // ── Map tests ──

    #[test]
    fn test_map_empty() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_map(&[sqrt, list(vec![])], &env).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_map_single() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_map(&[sqrt, list(vec![int(9)])], &env).unwrap();
        assert_eq!(result, list(vec![int(3)]));
    }

    #[test]
    fn test_map_multi() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_map(&[sqrt, list(vec![int(1), int(4), int(9)])], &env).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_map_non_list() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        assert!(builtin_map(&[sqrt, int(5)], &env).is_err());
    }

    // ── Apply tests ──

    #[test]
    fn test_apply_list() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        let result = builtin_apply(&[plus, list(vec![int(1), int(2), int(3)])], &env).unwrap();
        assert_eq!(result, int(6));
    }

    #[test]
    fn test_apply_single() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_apply(&[sqrt, int(9)], &env).unwrap();
        assert_eq!(result, int(3));
    }

    #[test]
    fn test_apply_empty_list() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        let result = builtin_apply(&[plus, list(vec![])], &env).unwrap();
        assert_eq!(result, int(0));
    }

    #[test]
    fn test_apply_wrong_count() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        assert!(builtin_apply(&[plus], &env).is_err());
    }

    // ── Fold tests ──

    #[test]
    fn test_fold_with_init() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        let result =
            builtin_fold(&[plus, int(0), list(vec![int(1), int(2), int(3)])], &env).unwrap();
        assert_eq!(result, int(6));
    }

    #[test]
    fn test_fold_no_init() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        let result = builtin_fold(&[plus, list(vec![int(1), int(2), int(3)])], &env).unwrap();
        assert_eq!(result, int(6));
    }

    #[test]
    fn test_fold_single() {
        let env = test_env();
        let times = Value::Symbol("Times".to_string());
        let result = builtin_fold(&[times, int(1), list(vec![int(5)])], &env).unwrap();
        assert_eq!(result, int(5));
    }

    #[test]
    fn test_fold_empty_no_init() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        assert!(builtin_fold(&[plus, list(vec![])], &env).is_err());
    }

    #[test]
    fn test_fold_wrong_count() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        assert!(builtin_fold(&[plus], &env).is_err());
    }

    // ── Scan tests ──

    #[test]
    fn test_scan_multi() {
        let env = test_env();
        let length = Value::Symbol("Length".to_string());
        let result = builtin_scan(&[length, list(vec![int(1), int(2)])], &env).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_scan_empty() {
        let env = test_env();
        let length = Value::Symbol("Length".to_string());
        let result = builtin_scan(&[length, list(vec![])], &env).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_scan_non_list() {
        let env = test_env();
        let length = Value::Symbol("Length".to_string());
        assert!(builtin_scan(&[length, int(5)], &env).is_err());
    }

    // ── Nest tests ──

    #[test]
    fn test_nest_sqr() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_nest(&[sqrt, int(81), int(2)], &env).unwrap();
        assert_eq!(result, int(3));
    }

    #[test]
    fn test_nest_zero() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_nest(&[sqrt, int(5), int(0)], &env).unwrap();
        assert_eq!(result, int(5));
    }

    #[test]
    fn test_nest_negative() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        assert!(builtin_nest(&[sqrt, int(5), int(-1)], &env).is_err());
    }

    #[test]
    fn test_nest_wrong_count() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        assert!(builtin_nest(&[sqrt, int(5)], &env).is_err());
    }

    // ── FoldList tests ──

    #[test]
    fn test_foldlist_basic() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        let result =
            builtin_fold_list(&[plus, int(0), list(vec![int(1), int(2), int(3)])], &env).unwrap();
        assert_eq!(result, list(vec![int(0), int(1), int(3), int(6)]));
    }

    #[test]
    fn test_foldlist_single() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        let result = builtin_fold_list(&[plus, int(0), list(vec![int(5)])], &env).unwrap();
        assert_eq!(result, list(vec![int(0), int(5)]));
    }

    #[test]
    fn test_foldlist_empty() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        let result = builtin_fold_list(&[plus, int(0), list(vec![])], &env).unwrap();
        assert_eq!(result, list(vec![int(0)]));
    }

    #[test]
    fn test_foldlist_wrong() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        assert!(builtin_fold_list(&[plus], &env).is_err());
    }

    // ── NestList tests ──

    #[test]
    fn test_nestlist_basic() {
        let env = test_env();
        // NestList[Sqrt, 81, 2] → {81, 9, 3}
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_nest_list(&[sqrt, int(81), int(2)], &env).unwrap();
        assert_eq!(result, list(vec![int(81), int(9), int(3)]));
    }

    #[test]
    fn test_nestlist_zero() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_nest_list(&[sqrt, int(5), int(0)], &env).unwrap();
        assert_eq!(result, list(vec![int(5)]));
    }

    #[test]
    fn test_nestlist_one() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_nest_list(&[sqrt, int(81), int(1)], &env).unwrap();
        assert_eq!(result, list(vec![int(81), int(9)]));
    }

    #[test]
    fn test_nestlist_negative() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        assert!(builtin_nest_list(&[sqrt, int(5), int(-1)], &env).is_err());
    }

    // ── MapApply tests ──

    #[test]
    fn test_mapapply_nested() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        let result = builtin_map_apply(
            &[
                plus,
                list(vec![list(vec![int(1), int(2)]), list(vec![int(3), int(4)])]),
            ],
            &env,
        )
        .unwrap();
        assert_eq!(result, list(vec![int(3), int(7)]));
    }

    #[test]
    fn test_mapapply_empty() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        let result = builtin_map_apply(&[plus, list(vec![])], &env).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_mapapply_flat() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_map_apply(&[sqrt, list(vec![int(9)])], &env).unwrap();
        assert_eq!(result, list(vec![int(3)]));
    }

    #[test]
    fn test_mapapply_mixed() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        let result = builtin_map_apply(
            &[
                plus,
                list(vec![
                    list(vec![int(1), int(2)]),
                    int(3),
                    list(vec![int(4), int(5)]),
                ]),
            ],
            &env,
        )
        .unwrap();
        assert_eq!(result, list(vec![int(3), int(3), int(9)]));
    }

    // ── New function tests ──

    // AllApply

    #[test]
    fn test_allapply_basic() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_all_apply(
            &[
                sqrt,
                list(vec![
                    list(vec![int(1), int(4)]),
                    list(vec![int(9), int(16)]),
                ]),
            ],
            &env,
        )
        .unwrap();
        assert_eq!(
            result,
            list(vec![list(vec![int(1), int(2)]), list(vec![int(3), int(4)])])
        );
    }

    #[test]
    fn test_allapply_empty() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_all_apply(&[sqrt, list(vec![])], &env).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_allapply_wrong_count() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        assert!(builtin_all_apply(&[sqrt], &env).is_err());
    }

    // MapAt

    #[test]
    fn test_mapat_single() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result =
            builtin_map_at(&[sqrt, list(vec![int(1), int(16), int(3)]), int(2)], &env).unwrap();
        assert_eq!(result, list(vec![int(1), int(4), int(3)]));
    }

    #[test]
    fn test_mapat_multi_positions() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_map_at(
            &[
                sqrt,
                list(vec![int(1), int(16), int(25)]),
                list(vec![int(1), int(3)]),
            ],
            &env,
        )
        .unwrap();
        assert_eq!(result, list(vec![int(1), int(16), int(5)]));
    }

    #[test]
    fn test_mapat_negative() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result =
            builtin_map_at(&[sqrt, list(vec![int(1), int(16), int(4)]), int(-1)], &env).unwrap();
        assert_eq!(result, list(vec![int(1), int(16), int(2)]));
    }

    #[test]
    fn test_mapat_wrong_count() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        assert!(builtin_map_at(&[sqrt, list(vec![int(1)])], &env).is_err());
    }

    // ApplyTo

    #[test]
    fn test_applyto_symbol() {
        let f = Value::Symbol("f".to_string());
        let result = builtin_apply_to(&[int(5), f]).unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "f".to_string(),
                args: vec![int(5)]
            }
        );
    }

    #[test]
    fn test_applyto_wrong_count() {
        assert!(builtin_apply_to(&[int(5)]).is_err());
    }

    // Thread

    #[test]
    fn test_thread_basic() {
        let call = Value::Call {
            head: "f".to_string(),
            args: vec![list(vec![int(1), int(2)]), list(vec![int(3), int(4)])],
        };
        let result = builtin_thread(&[call]).unwrap();
        assert_eq!(
            result,
            list(vec![
                Value::Call {
                    head: "f".to_string(),
                    args: vec![int(1), int(3)]
                },
                Value::Call {
                    head: "f".to_string(),
                    args: vec![int(2), int(4)]
                },
            ])
        );
    }

    #[test]
    fn test_thread_empty_call() {
        let call = Value::Call {
            head: "f".to_string(),
            args: vec![],
        };
        let result = builtin_thread(&[call.clone()]).unwrap();
        assert_eq!(result, call);
    }

    #[test]
    fn test_thread_wrong_count() {
        assert!(builtin_thread(&[]).is_err());
    }

    // Outer

    #[test]
    fn test_outer_basic() {
        let env = test_env();
        let times = Value::Symbol("Times".to_string());
        let result = builtin_outer(
            &[
                times,
                list(vec![int(1), int(2)]),
                list(vec![int(3), int(4)]),
            ],
            &env,
        )
        .unwrap();
        assert_eq!(
            result,
            list(vec![list(vec![int(3), int(4)]), list(vec![int(6), int(8)])])
        );
    }

    #[test]
    fn test_outer_empty() {
        let env = test_env();
        let times = Value::Symbol("Times".to_string());
        let result =
            builtin_outer(&[times, list(vec![]), list(vec![int(1), int(2)])], &env).unwrap();
        assert_eq!(result, list(vec![]));
    }

    // Inner

    #[test]
    fn test_inner_basic() {
        let env = test_env();
        let times = Value::Symbol("Times".to_string());
        let plus = Value::Symbol("Plus".to_string());
        let result = builtin_inner(
            &[
                times,
                plus,
                list(vec![int(1), int(2), int(3)]),
                list(vec![int(4), int(5), int(6)]),
            ],
            &env,
        )
        .unwrap();
        assert_eq!(result, int(32));
    }

    #[test]
    fn test_inner_length_mismatch() {
        let env = test_env();
        let times = Value::Symbol("Times".to_string());
        let plus = Value::Symbol("Plus".to_string());
        assert!(
            builtin_inner(
                &[times, plus, list(vec![int(1)]), list(vec![int(1), int(2)])],
                &env,
            )
            .is_err()
        );
    }

    // MapIndexed

    #[test]
    fn test_mapindexed_basic() {
        let env = test_env();
        // MapIndexed calls f[{index}, item]. Plus[{1}, 1] = {2}, Plus[{2}, 2] = {4}, etc.
        let plus = Value::Symbol("Plus".to_string());
        let result =
            builtin_map_indexed(&[plus, list(vec![int(1), int(2), int(3)])], &env).unwrap();
        // Each Plus[{i}, v] = {i+v} wrapped in a list
        assert_eq!(
            result,
            list(vec![
                list(vec![int(2)]),
                list(vec![int(4)]),
                list(vec![int(6)])
            ])
        );
    }

    #[test]
    fn test_mapindexed_empty() {
        let env = test_env();
        let length = Value::Symbol("Length".to_string());
        let result = builtin_map_indexed(&[length, list(vec![])], &env).unwrap();
        assert_eq!(result, list(vec![]));
    }

    // MapThread

    #[test]
    fn test_mapthread_basic() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        let result = builtin_map_thread(
            &[
                plus,
                list(vec![list(vec![int(1), int(2)]), list(vec![int(3), int(4)])]),
            ],
            &env,
        )
        .unwrap();
        assert_eq!(result, list(vec![int(4), int(6)]));
    }

    #[test]
    fn test_mapthread_empty() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        let result = builtin_map_thread(&[plus, list(vec![])], &env).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_mapthread_length_mismatch() {
        let env = test_env();
        let plus = Value::Symbol("Plus".to_string());
        assert!(
            builtin_map_thread(
                &[
                    plus,
                    list(vec![list(vec![int(1)]), list(vec![int(1), int(2)])])
                ],
                &env,
            )
            .is_err()
        );
    }

    // NestWhile

    #[test]
    fn test_nestwhile_basic() {
        // NestWhile[Sqrt, 81, IntegerQ] → Sqrt[81]=9 (int), Sqrt[9]=3 (int), Sqrt[3]=Call (not int, stop)
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let integerq = Value::Symbol("IntegerQ".to_string());
        let result = builtin_nest_while(&[sqrt, int(81), integerq], &env).unwrap();
        // Sqrt[3] is a Call node, not an Integer
        assert!(
            matches!(&result, Value::Call { head, args } if *head == "Sqrt" && args[0] == int(3))
        );
    }

    #[test]
    fn test_nestwhile_wrong_count() {
        let env = test_env();
        assert!(builtin_nest_while(&[int(1), int(2)], &env).is_err());
    }

    // NestWhileList

    #[test]
    fn test_nestwhilelist_basic() {
        // NestWhileList[Sqrt, 81, IntegerQ] → {81, 9, 3, Sqrt[3]}
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let integerq = Value::Symbol("IntegerQ".to_string());
        let result = builtin_nest_while_list(&[sqrt, int(81), integerq], &env).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 4); // 81, 9, 3, Sqrt[3]
                assert_eq!(items[0], int(81));
                assert_eq!(items[1], int(9));
                assert_eq!(items[2], int(3));
                assert!(
                    matches!(&items[3], Value::Call { head, args } if *head == "Sqrt" && args[0] == int(3))
                );
            }
            _ => panic!("Expected List, got {:?}", result),
        }
    }

    #[test]
    fn test_nestwhilelist_wrong_count() {
        let env = test_env();
        assert!(builtin_nest_while_list(&[int(1), int(2)], &env).is_err());
    }

    // FixedPointList

    #[test]
    fn test_fixedpointlist_basic() {
        let env = test_env();
        let sqrt = Value::Symbol("Sqrt".to_string());
        let result = builtin_fixed_point_list(&[sqrt, int(81), int(5)], &env).unwrap();
        assert!(matches!(&result, Value::List(items) if items.len() > 1));
    }

    #[test]
    fn test_fixedpointlist_immediate_fixed() {
        let env = test_env();
        // Abs[5] = 5, already fixed point → list should be short
        let abs = Value::Symbol("Abs".to_string());
        let result = builtin_fixed_point_list(&[abs, int(5)], &env).unwrap();
        let list = match result {
            Value::List(items) => items,
            _ => panic!(),
        };
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_fixedpointlist_wrong_count() {
        let env = test_env();
        assert!(builtin_fixed_point_list(&[], &env).is_err());
    }

    // ── ArrayPad tests ──

    #[test]
    fn test_array_pad_basic() {
        let result = builtin_array_pad(&[list(vec![int(1), int(2)]), int(1)]).unwrap();
        assert_eq!(result, list(vec![int(0), int(1), int(2), int(0)]));
    }

    #[test]
    fn test_array_pad_asymmetric() {
        let result =
            builtin_array_pad(&[list(vec![int(1), int(2)]), list(vec![int(2), int(1)])]).unwrap();
        assert_eq!(result, list(vec![int(0), int(0), int(1), int(2), int(0)]));
    }

    #[test]
    fn test_array_pad_custom_value() {
        let result =
            builtin_array_pad(&[list(vec![int(1)]), int(2), Value::Str("x".to_string())]).unwrap();
        assert_eq!(
            result,
            list(vec![
                Value::Str("x".to_string()),
                Value::Str("x".to_string()),
                int(1),
                Value::Str("x".to_string()),
                Value::Str("x".to_string()),
            ])
        );
    }

    #[test]
    fn test_array_pad_non_list() {
        assert!(builtin_array_pad(&[int(5), int(1)]).is_err());
    }

    #[test]
    fn test_array_pad_few_args() {
        assert!(builtin_array_pad(&[list(vec![int(1)])]).is_err());
    }

    // ── ArrayReshape tests ──

    #[test]
    fn test_array_reshape_2x3() {
        let flat = list(vec![int(1), int(2), int(3), int(4), int(5), int(6)]);
        let dims = list(vec![int(2), int(3)]);
        let result = builtin_array_reshape(&[flat, dims]).unwrap();
        assert_eq!(
            result,
            list(vec![
                list(vec![int(1), int(2), int(3)]),
                list(vec![int(4), int(5), int(6)]),
            ])
        );
    }

    #[test]
    fn test_array_reshape_3x2() {
        let flat = list(vec![int(1), int(2), int(3), int(4), int(5), int(6)]);
        let dims = list(vec![int(3), int(2)]);
        let result = builtin_array_reshape(&[flat, dims]).unwrap();
        assert_eq!(
            result,
            list(vec![
                list(vec![int(1), int(2)]),
                list(vec![int(3), int(4)]),
                list(vec![int(5), int(6)]),
            ])
        );
    }

    #[test]
    fn test_array_reshape_size_mismatch() {
        let flat = list(vec![int(1), int(2), int(3)]);
        let dims = list(vec![int(2), int(2)]);
        assert!(builtin_array_reshape(&[flat, dims]).is_err());
    }

    #[test]
    fn test_array_reshape_1d() {
        let flat = list(vec![int(1), int(2), int(3)]);
        let dims = list(vec![int(3)]);
        let result = builtin_array_reshape(&[flat, dims]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    // ── StringCases tests ──

    #[test]
    fn test_string_cases_literal() {
        let result = builtin_string_cases(&[
            Value::Str("abcabc".to_string()),
            Value::Str("ab".to_string()),
        ])
        .unwrap();
        assert_eq!(
            result,
            list(vec![
                Value::Str("ab".to_string()),
                Value::Str("ab".to_string()),
            ])
        );
    }

    #[test]
    fn test_string_cases_no_match() {
        let result = builtin_string_cases(&[
            Value::Str("hello".to_string()),
            Value::Str("xyz".to_string()),
        ])
        .unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_string_cases_wildcard() {
        let result = builtin_string_cases(&[
            Value::Str("hello world".to_string()),
            Value::Str("*".to_string()),
        ])
        .unwrap();
        assert_eq!(result, list(vec![Value::Str("hello world".to_string())]));
    }

    #[test]
    fn test_string_cases_non_string() {
        assert!(builtin_string_cases(&[int(5), Value::Str("a".to_string())]).is_err());
    }
}
