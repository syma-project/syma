use crate::ast::Expr;
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

/// Resolve a `{m, n}` span spec into a half-open 0-based `[start, end)` range.
/// Positions are 1-indexed and negative positions count from the end. The
/// result is clamped to `[0, len]`, so out-of-range spans yield empty or
/// shortened slices instead of panicking.
pub(crate) fn resolve_span(m: i64, n: i64, len: usize) -> (usize, usize) {
    let len_i = len as i64;
    let start_i = if m > 0 {
        m - 1
    } else if m < 0 {
        len_i + m
    } else {
        0
    };
    let end_i = if n > 0 {
        n
    } else if n < 0 {
        len_i + n + 1
    } else {
        0
    };
    (
        start_i.clamp(0, len_i) as usize,
        end_i.clamp(0, len_i) as usize,
    )
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
        Value::Str(s) => Ok(Value::Integer(Integer::from(s.chars().count() as i64))),
        Value::Assoc(map) => Ok(Value::Integer(Integer::from(map.len() as i64))),
        Value::Call {
            args: call_args, ..
        } => Ok(Value::Integer(Integer::from(call_args.len() as i64))),
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
                // A rule exposes its head and two operands, like any other
                // compound expression: Part[lhs -> rhs, 0] == Rule, 1 == lhs,
                // 2 == rhs. Needed for `FindRoot[...][[1, 2]]`.
                Value::Rule { lhs, rhs, .. } => {
                    if index == 0 {
                        current = Value::Symbol("Rule".to_string());
                    } else if index == 1 {
                        current = (**lhs).clone();
                    } else if index == 2 {
                        current = (**rhs).clone();
                    } else {
                        return Err(EvalError::IndexOutOfBounds { index, length: 2 });
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
            let chars: Vec<char> = s.chars().collect();
            let idx = normalize_index(index, chars.len())?;
            Ok(Value::Str(chars[idx].to_string()))
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
        // Part[lhs -> rhs, 0|1|2] — head / lhs / rhs.
        Value::Rule { lhs, rhs, .. } => {
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
                Ok(Value::Symbol("Rule".to_string()))
            } else if index == 1 {
                Ok((**lhs).clone())
            } else if index == 2 {
                Ok((**rhs).clone())
            } else {
                Err(EvalError::IndexOutOfBounds { index, length: 2 })
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
            let (start, end) = resolve_span(m, n, items.len());
            if start >= end {
                return Ok(Value::List(vec![]));
            }
            Ok(Value::List(items[start..end].to_vec()))
        }
        (Value::List(items), Value::Integer(n)) => {
            let n_i64 = n.to_i64().unwrap_or(0);
            // Positive n takes the first n elements; negative n takes the last |n|.
            let result = if n_i64 >= 0 {
                let count = (n_i64 as usize).min(items.len());
                items[..count].to_vec()
            } else {
                let count = ((-n_i64) as usize).min(items.len());
                items[items.len() - count..].to_vec()
            };
            Ok(Value::List(result))
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
            let (start, end) = resolve_span(m, n, items.len());
            if start >= end {
                Ok(Value::List(items.to_vec()))
            } else if start == 0 {
                Ok(Value::List(items[end..].to_vec()))
            } else {
                let mut result: Vec<Value> = items[..start].to_vec();
                result.extend_from_slice(&items[end..]);
                Ok(Value::List(result))
            }
        }
        (Value::List(items), Value::Integer(n)) => {
            let n_i64 = n.to_i64().unwrap_or(0);
            // Positive n drops the first n elements; negative n drops the last |n|.
            let result = if n_i64 >= 0 {
                let count = (n_i64 as usize).min(items.len());
                items[count..].to_vec()
            } else {
                let count = ((-n_i64) as usize).min(items.len());
                items[..items.len() - count].to_vec()
            };
            Ok(Value::List(result))
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
            // Support ragged input: use the widest row so no push is out of
            // bounds. Short rows simply contribute fewer entries.
            let mut cols = 0usize;
            for row in rows {
                match row {
                    Value::List(items) => cols = cols.max(items.len()),
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: "List of Lists".to_string(),
                            got: "List of non-Lists".to_string(),
                        });
                    }
                }
            }
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
    if args.is_empty() {
        return Err(EvalError::Error(
            "Total requires at least 1 argument".to_string(),
        ));
    }
    // Total[a, b, ...] sums the arguments (WL accepts a sequence).
    if args.len() > 1 {
        return super::arithmetic::builtin_plus(args);
    }
    match &args[0] {
        Value::List(items) => super::arithmetic::builtin_plus(items),
        // Total[scalar] is the scalar (matches Total[list-of-one]).
        other => Ok(other.clone()),
    }
}

// ── Sum / Product ───────────────────────────────────────────────────────────
//
// The evaluator dispatches the `Sum`/`Product` special forms here so the
// iterator variable stays unevaluated and can be substituted symbolically.
// The functions double as the plain builtins, which makes `Apply[Sum, ...]`
// and friends work too.
//
// Finite ranges are iterated with exact arithmetic.  A symbolic upper bound
// (including `Infinity`) is handled with closed forms: Faulhaber polynomials
// for `k`, `k^2`, `k^3`, the geometric series for `r^k`, and `Factorial` for
// `Product[k, ...]`.  Anything unrecognised is returned unevaluated.

/// Iteration limit substituted for a symbolic `Infinity` bound.
const ITER_INFINITY_LIMIT: i64 = 100;

fn call_expr(head: &str, args: Vec<Expr>) -> Expr {
    Expr::Call {
        head: Box::new(Expr::Symbol(head.to_string())),
        args,
    }
}

fn int_expr(n: i64) -> Expr {
    Expr::Integer(Integer::from(n))
}

/// An integer-valued literal, tolerating the `Real` spelling (`2.0`) that `N`
/// introduces.
fn expr_int(e: &Expr) -> Option<i64> {
    match e {
        Expr::Integer(n) => n.to_i64(),
        Expr::Real(r) if r.is_integer() => Some(r.to_f64() as i64),
        _ => None,
    }
}

/// Is the literal `e` exactly one (in exact or `Real` form)?
fn expr_is_one(e: &Expr) -> bool {
    expr_int(e) == Some(1)
}

/// Integer value of a constant expression, folding literal `Plus`/`Times`
/// spellings such as `Times[-1, 1]` (from `-1`) or `Times[-1, 2]` (from `-2`).
fn const_int(e: &Expr) -> Option<i64> {
    match e {
        Expr::Integer(n) => n.to_i64(),
        Expr::Real(r) if r.is_integer() => Some(r.to_f64() as i64),
        Expr::Call { head, args } if matches!(head.as_ref(), Expr::Symbol(h) if h == "Times") => {
            let mut acc: i64 = 1;
            for a in args {
                acc = acc.checked_mul(const_int(a)?)?;
            }
            Some(acc)
        }
        Expr::Call { head, args } if matches!(head.as_ref(), Expr::Symbol(h) if h == "Plus") => {
            let mut acc: i64 = 0;
            for a in args {
                acc = acc.checked_add(const_int(a)?)?;
            }
            Some(acc)
        }
        _ => None,
    }
}

/// Recognise `expr` as `k^p` for a non-negative integer power `p`.
fn as_power_of(expr: &Expr, k: &str) -> Option<i64> {
    let p = match expr {
        Expr::Symbol(s) if s == k => 1,
        Expr::Call { head, args }
            if matches!(head.as_ref(), Expr::Symbol(h) if h == "Power")
                && args.len() == 2
                && matches!(&args[0], Expr::Symbol(s) if s == k) =>
        {
            expr_int(&args[1])?
        }
        _ => return None,
    };
    (p >= 0).then_some(p)
}

/// Recognise `expr` as `r^k` for some base `r` free of `k`, accepting both the
/// `Power[r, k]` and the `Divide[1, Power[r, k]]` spellings.
fn as_geometric(expr: &Expr, k: &str) -> Option<Expr> {
    if let Expr::Call { head, args } = expr
        && matches!(head.as_ref(), Expr::Symbol(h) if h == "Power")
        && args.len() == 2
    {
        if matches!(&args[1], Expr::Symbol(s) if s == k) {
            return Some(args[0].clone());
        }
        // r^(-k) == (1/r)^k
        if let Expr::Call { head: h, args: ea } = &args[1]
            && matches!(h.as_ref(), Expr::Symbol(s) if s == "Times")
            && ea.len() == 2
            && expr_int(&ea[0]) == Some(-1)
            && matches!(&ea[1], Expr::Symbol(s) if s == k)
        {
            return Some(call_expr("Divide", vec![int_expr(1), args[0].clone()]));
        }
    }
    if let Expr::Call { head, args } = expr
        && matches!(head.as_ref(), Expr::Symbol(h) if h == "Divide")
        && args.len() == 2
        && expr_is_one(&args[0])
        && let Expr::Call { head: h2, args: a2 } = &args[1]
        && matches!(h2.as_ref(), Expr::Symbol(h) if h == "Power")
        && a2.len() == 2
        && matches!(&a2[1], Expr::Symbol(s) if s == k)
    {
        return Some(call_expr("Divide", vec![int_expr(1), a2[0].clone()]));
    }
    None
}

/// Faulhaber polynomial `S_p(x) = sum_{j=1}^{x} j^p` as an expression in `x`.
fn faulhaber(p: i64, x: &Expr) -> Option<Expr> {
    let one = int_expr(1);
    match p {
        0 => Some(x.clone()),
        1 => Some(call_expr(
            "Divide",
            vec![
                call_expr(
                    "Times",
                    vec![x.clone(), call_expr("Plus", vec![x.clone(), one])],
                ),
                int_expr(2),
            ],
        )),
        2 => Some(call_expr(
            "Divide",
            vec![
                call_expr(
                    "Times",
                    vec![
                        x.clone(),
                        call_expr("Plus", vec![x.clone(), one.clone()]),
                        call_expr(
                            "Plus",
                            vec![call_expr("Times", vec![int_expr(2), x.clone()]), one],
                        ),
                    ],
                ),
                int_expr(6),
            ],
        )),
        3 => Some(call_expr(
            "Divide",
            vec![
                call_expr(
                    "Times",
                    vec![
                        call_expr("Power", vec![x.clone(), int_expr(2)]),
                        call_expr(
                            "Power",
                            vec![call_expr("Plus", vec![x.clone(), one]), int_expr(2)],
                        ),
                    ],
                ),
                int_expr(4),
            ],
        )),
        _ => None,
    }
}

/// Does `e` mention the iterator symbol `k` anywhere? Mirrors
/// `dsolve::contains_symbol`, handling the node kinds a summand can contain.
fn contains_sym(e: &Expr, k: &str) -> bool {
    match e {
        Expr::Symbol(s) => s == k,
        Expr::List(items) => items.iter().any(|i| contains_sym(i, k)),
        Expr::Call { head, args } => {
            contains_sym(head, k) || args.iter().any(|a| contains_sym(a, k))
        }
        Expr::Rule { lhs, rhs } | Expr::RuleDelayed { lhs, rhs } => {
            contains_sym(lhs, k) || contains_sym(rhs, k)
        }
        _ => false,
    }
}

/// Product of `factors`, dropping multiplicative ones.
fn times_of(mut factors: Vec<Expr>) -> Expr {
    factors.retain(|f| !expr_is_one(f));
    match factors.len() {
        0 => int_expr(1),
        1 => factors.pop().expect("len checked"),
        _ => call_expr("Times", factors),
    }
}

/// `num / den`, folding trivial `1`s.
fn div_of(num: Expr, den: Expr) -> Expr {
    if expr_is_one(&den) {
        num
    } else if expr_is_one(&num) {
        call_expr("Divide", vec![int_expr(1), den])
    } else {
        call_expr("Divide", vec![num, den])
    }
}

/// Split `expr` into `(constant, core)` where `constant` is free of the
/// iterator `k` and `core` collects the `k`-dependent factors. Pulling scale
/// factors out (`Sum[c f[k]]`, `Sum[f[k]/c]`) lets the recognisers below match a
/// normalised core.
fn split_coeff(expr: &Expr, k: &str) -> (Expr, Expr) {
    match expr {
        Expr::Call { head, args } if matches!(head.as_ref(), Expr::Symbol(h) if h == "Times") => {
            let mut coeffs = Vec::new();
            let mut bodies = Vec::new();
            for a in args {
                let (c, b) = split_coeff(a, k);
                if !expr_is_one(&c) {
                    coeffs.push(c);
                }
                if !expr_is_one(&b) {
                    bodies.push(b);
                }
            }
            (times_of(coeffs), times_of(bodies))
        }
        Expr::Call { head, args }
            if matches!(head.as_ref(), Expr::Symbol(h) if h == "Divide") && args.len() == 2 =>
        {
            let (cn, bn) = split_coeff(&args[0], k);
            let (cd, bd) = split_coeff(&args[1], k);
            (div_of(cn, cd), div_of(bn, bd))
        }
        _ if !contains_sym(expr, k) => (expr.clone(), int_expr(1)),
        _ => (int_expr(1), expr.clone()),
    }
}

/// If `e` is `k` or `k + a` (either order), return `a`.
fn linear_offset(e: &Expr, k: &str) -> Option<i64> {
    match e {
        Expr::Symbol(s) if s == k => Some(0),
        Expr::Call { head, args }
            if matches!(head.as_ref(), Expr::Symbol(h) if h == "Plus") && args.len() == 2 =>
        {
            if matches!(&args[0], Expr::Symbol(s) if s == k) {
                const_int(&args[1])
            } else if matches!(&args[1], Expr::Symbol(s) if s == k) {
                const_int(&args[0])
            } else {
                None
            }
        }
        _ => None,
    }
}

/// If `e` is `c k + b` for integer `c > 0` and integer `b`, return `(c, b)`.
fn linear_cb(e: &Expr, k: &str) -> Option<(i64, i64)> {
    match e {
        Expr::Symbol(s) if s == k => Some((1, 0)),
        Expr::Call { head, args }
            if matches!(head.as_ref(), Expr::Symbol(h) if h == "Plus") && args.len() == 2 =>
        {
            if let Some((c, b)) = linear_cb(&args[0], k).filter(|(c, _)| *c > 0) {
                return Some((c, b + const_int(&args[1])?));
            }
            if let Some((c, b)) = linear_cb(&args[1], k).filter(|(c, _)| *c > 0) {
                return Some((c, b + const_int(&args[0])?));
            }
            None
        }
        Expr::Call { head, args }
            if matches!(head.as_ref(), Expr::Symbol(h) if h == "Times") && args.len() == 2 =>
        {
            if let Some(c) = const_int(&args[0])
                && matches!(&args[1], Expr::Symbol(s) if s == k)
            {
                return Some((c, 0));
            }
            if let Some(c) = const_int(&args[1])
                && matches!(&args[0], Expr::Symbol(s) if s == k)
            {
                return Some((c, 0));
            }
            None
        }
        _ => None,
    }
}

/// Exact integer value of `v`, without truncating rationals (`to_integer`
/// truncates, which would misclassify `5/2` as `2`).
fn exact_int(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(n) => n.to_i64(),
        Value::Rational(r) if *r.denom() == 1 => r.numer().to_i64(),
        _ => None,
    }
}

/// Is the numeric value `v` strictly greater than one?
fn num_gt_one(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n.to_i64().is_none_or(|x| x > 1),
        Value::Rational(r) => r.to_f64() > 1.0,
        Value::Real(f) => f.to_f64() > 1.0,
        _ => false,
    }
}

/// Is the value `v` a (real) number?
fn is_number(v: &Value) -> bool {
    matches!(v, Value::Integer(_) | Value::Rational(_) | Value::Real(_))
}

/// Is the value `v` a non-zero (real) number?
fn is_nonzero_number(v: &Value) -> bool {
    match v {
        Value::Integer(n) => !n.is_zero(),
        Value::Rational(r) => !r.is_zero(),
        Value::Real(f) => !f.is_zero(),
        _ => false,
    }
}

/// Exactly `Zeta[p]` for even `p = 2n <= 20`, as `Pi^p * (num/den)`.
/// `Zeta[2n] = |B_{2n}| 2^{2n-1} Pi^{2n} / (2n)!`.
fn zeta_even_exact(p: i64) -> Option<Expr> {
    let (num, den) = match p {
        2 => (1, 6),
        4 => (1, 90),
        6 => (1, 945),
        8 => (1, 9450),
        10 => (1, 93555),
        12 => (691, 638_512_875),
        14 => (2, 18_243_225),
        16 => (3617, 325_641_566_250),
        18 => (43867, 38_979_295_480_125),
        20 => (174611, 1_531_329_465_290_625),
        _ => return None,
    };
    let pi_pow = call_expr("Power", vec![Expr::Symbol("Pi".to_string()), int_expr(p)]);
    Some(call_expr(
        "Divide",
        vec![
            call_expr("Times", vec![int_expr(num), pi_pow]),
            int_expr(den),
        ],
    ))
}

/// Multiply a closed form by an already-factored constant and evaluate it.
fn finish_scaled(coeff: &Expr, core: &Expr, env: &Env) -> Result<Value, EvalError> {
    if expr_is_one(coeff) {
        crate::eval::eval(core, env)
    } else {
        crate::eval::eval(&call_expr("Times", vec![coeff.clone(), core.clone()]), env)
    }
}

/// Multiply an already-evaluated (possibly symbolic) value by a constant.
fn scale_value(coeff: &Expr, v: Value, env: &Env) -> Result<Value, EvalError> {
    if expr_is_one(coeff) {
        Ok(v)
    } else {
        let cv = crate::eval::eval(coeff, env)?;
        super::arithmetic::mul_values_public(&cv, &v)
    }
}

/// Closed forms for `sum_{j=lo}^{inf} body`, or `None` when unrecognised.
fn inf_closed_form(body: &Expr, k: &str, lo: i64, env: &Env) -> Result<Option<Value>, EvalError> {
    if let Some(v) = try_alt_harmonic(body, k, lo, env)? {
        return Ok(Some(v));
    }
    if let Some(v) = try_pseries(body, k, lo, env)? {
        return Ok(Some(v));
    }
    if let Some(v) = try_factorial_series(body, k, lo, env)? {
        return Ok(Some(v));
    }
    if let Some(v) = try_telescoping(body, k, lo, env)? {
        return Ok(Some(v));
    }
    Ok(None)
}

/// Recognise `body` as `(c k + b)^(-p)` for `c > 0` and a non-zero number `p`,
/// returning `(c, b, p)`. Accepts `Power[u, -p]` and `Divide[const, u^p]` /
/// `Divide[const, u]`.
fn as_neg_power(body: &Expr, k: &str, env: &Env) -> Result<Option<(i64, i64, Expr)>, EvalError> {
    if let Expr::Call { head, args } = body
        && matches!(head.as_ref(), Expr::Symbol(h) if h == "Power")
        && args.len() == 2
        && let Some((c, b)) = linear_cb(&args[0], k)
    {
        let p_expr = call_expr("Times", vec![int_expr(-1), args[1].clone()]);
        let pv = crate::eval::eval(&p_expr, env)?;
        if is_nonzero_number(&pv) {
            return Ok(Some((c, b, p_expr)));
        }
    }
    if let Expr::Call { head, args } = body
        && matches!(head.as_ref(), Expr::Symbol(h) if h == "Divide")
        && args.len() == 2
        && !contains_sym(&args[0], k)
    {
        if let Expr::Call { head: dh, args: da } = &args[1]
            && matches!(dh.as_ref(), Expr::Symbol(h) if h == "Power")
            && da.len() == 2
            && let Some((c, b)) = linear_cb(&da[0], k)
        {
            let pv = crate::eval::eval(&da[1], env)?;
            if is_nonzero_number(&pv) {
                return Ok(Some((c, b, da[1].clone())));
            }
        }
        if let Some((c, b)) = linear_cb(&args[1], k) {
            return Ok(Some((c, b, int_expr(1))));
        }
    }
    Ok(None)
}

/// `sum 1/(c k + b)^p` — harmonic divergence, `Zeta[p]`, its even closed forms
/// and, for `c > 1`, a `HurwitzZeta` reduction.
fn try_pseries(body: &Expr, k: &str, lo: i64, env: &Env) -> Result<Option<Value>, EvalError> {
    let Some((c, b, p_expr)) = as_neg_power(body, k, env)? else {
        return Ok(None);
    };
    let pv = crate::eval::eval(&p_expr, env)?;
    if !is_number(&pv) {
        return Ok(None);
    }
    let start = lo + b;
    // `1/0^p` contributes an infinity; `p <= 1` diverges.
    if start <= 0 || !num_gt_one(&pv) {
        return Ok(Some(Value::Symbol("Infinity".to_string())));
    }
    if c != 1 {
        // sum_k (c k + b)^(-p) = c^(-p) * HurwitzZeta[p, lo + b/c].
        let shift = call_expr(
            "Plus",
            vec![
                int_expr(lo),
                call_expr("Divide", vec![int_expr(b), int_expr(c)]),
            ],
        );
        let expr = call_expr(
            "Divide",
            vec![
                call_expr("HurwitzZeta", vec![p_expr.clone(), shift]),
                call_expr("Power", vec![int_expr(c), p_expr.clone()]),
            ],
        );
        return Ok(Some(crate::eval::eval(&expr, env)?));
    }
    let base = if let Some(n) = exact_int(&pv) {
        if n >= 2 && n % 2 == 0 {
            match zeta_even_exact(n) {
                Some(e) => crate::eval::eval(&e, env)?,
                None => Value::Call {
                    head: "Zeta".to_string(),
                    args: vec![pv.clone()],
                },
            }
        } else {
            Value::Call {
                head: "Zeta".to_string(),
                args: vec![pv.clone()],
            }
        }
    } else {
        Value::Call {
            head: "Zeta".to_string(),
            args: vec![pv.clone()],
        }
    };
    if start == 1 {
        return Ok(Some(base));
    }
    // start >= 2: subtract the finite head sum_{j=1}^{start-1} j^(-p).
    let summand = call_expr(
        "Power",
        vec![
            Expr::Symbol(k.to_string()),
            call_expr("Times", vec![int_expr(-1), p_expr]),
        ],
    );
    let spec = Expr::List(vec![
        Expr::Symbol(k.to_string()),
        int_expr(1),
        int_expr(start - 1),
    ]);
    let partial = crate::eval::eval(&call_expr("Sum", vec![summand, spec]), env)?;
    Ok(Some(super::arithmetic::sub_values_public(&base, &partial)?))
}

/// `sum (-1)^(k+c)/(k+a)` = `Log[2]` (up to a phase and a finite head).
fn try_alt_harmonic(body: &Expr, k: &str, lo: i64, env: &Env) -> Result<Option<Value>, EvalError> {
    let Expr::Call { head, args } = body else {
        return Ok(None);
    };
    if !matches!(head.as_ref(), Expr::Symbol(h) if h == "Divide") || args.len() != 2 {
        return Ok(None);
    }
    let Expr::Call { head: nh, args: na } = &args[0] else {
        return Ok(None);
    };
    if !matches!(nh.as_ref(), Expr::Symbol(h) if h == "Power") || na.len() != 2 {
        return Ok(None);
    }
    if crate::eval::eval(&na[0], env)? != Value::Integer(Integer::from(-1)) {
        return Ok(None);
    }
    let Some(c) = linear_offset(&na[1], k) else {
        return Ok(None);
    };
    let Some(a) = linear_offset(&args[1], k) else {
        return Ok(None);
    };
    let j0 = lo + a;
    if j0 <= 0 {
        return Ok(Some(Value::Symbol("Infinity".to_string())));
    }
    // sum_{j=j0}^inf (-1)^j/j = -Log[2] - sum_{j=1}^{j0-1} (-1)^j/j
    let mut inner = call_expr(
        "Times",
        vec![int_expr(-1), call_expr("Log", vec![int_expr(2)])],
    );
    if j0 > 1 {
        let summand = call_expr(
            "Divide",
            vec![
                call_expr("Power", vec![int_expr(-1), Expr::Symbol(k.to_string())]),
                Expr::Symbol(k.to_string()),
            ],
        );
        let spec = Expr::List(vec![
            Expr::Symbol(k.to_string()),
            int_expr(1),
            int_expr(j0 - 1),
        ]);
        let head_sum = call_expr("Sum", vec![summand, spec]);
        inner = call_expr(
            "Plus",
            vec![inner, call_expr("Times", vec![int_expr(-1), head_sum])],
        );
    }
    let phase = call_expr("Power", vec![int_expr(-1), int_expr(c - a)]);
    Ok(Some(crate::eval::eval(
        &call_expr("Times", vec![phase, inner]),
        env,
    )?))
}

/// `sum c^k/k!` = `E^c` (up to a finite head and shift phase).
fn try_factorial_series(
    body: &Expr,
    k: &str,
    lo: i64,
    env: &Env,
) -> Result<Option<Value>, EvalError> {
    let Expr::Call { head, args } = body else {
        return Ok(None);
    };
    if !matches!(head.as_ref(), Expr::Symbol(h) if h == "Divide") || args.len() != 2 {
        return Ok(None);
    }
    let g: &Expr = match &args[1] {
        Expr::Call { head: dh, args: da }
            if matches!(dh.as_ref(), Expr::Symbol(s) if s == "Factorial") && da.len() == 1 =>
        {
            &da[0]
        }
        Expr::Call { head: dh, args: da }
            if matches!(dh.as_ref(), Expr::Symbol(s) if s == "Power")
                && da.len() == 2
                && expr_int(&da[1]) == Some(-1) =>
        {
            match &da[0] {
                Expr::Call { head: fh, args: fa }
                    if matches!(fh.as_ref(), Expr::Symbol(s) if s == "Factorial")
                        && fa.len() == 1 =>
                {
                    &fa[0]
                }
                _ => return Ok(None),
            }
        }
        _ => return Ok(None),
    };
    let Some(a) = linear_offset(g, k) else {
        return Ok(None);
    };
    let c = match &args[0] {
        e if expr_is_one(e) => int_expr(1),
        Expr::Call { head: nh, args: na }
            if matches!(nh.as_ref(), Expr::Symbol(s) if s == "Power")
                && na.len() == 2
                && linear_offset(&na[1], k) == Some(0)
                && !contains_sym(&na[0], k) =>
        {
            na[0].clone()
        }
        _ => return Ok(None),
    };
    let m = lo + a;
    let e_pow = call_expr("Power", vec![Expr::Symbol("E".to_string()), c.clone()]);
    let mut full = if m <= 0 {
        e_pow
    } else {
        let summand = call_expr(
            "Divide",
            vec![
                call_expr("Power", vec![c.clone(), Expr::Symbol(k.to_string())]),
                call_expr("Factorial", vec![Expr::Symbol(k.to_string())]),
            ],
        );
        let spec = Expr::List(vec![
            Expr::Symbol(k.to_string()),
            int_expr(0),
            int_expr(m - 1),
        ]);
        let partial = call_expr("Sum", vec![summand, spec]);
        call_expr(
            "Plus",
            vec![e_pow, call_expr("Times", vec![int_expr(-1), partial])],
        )
    };
    if a != 0 {
        full = call_expr(
            "Times",
            vec![call_expr("Power", vec![c, int_expr(-a)]), full],
        );
    }
    Ok(Some(crate::eval::eval(&full, env)?))
}

/// `sum 1/((k+p)(k+q))` (integers `p < q`) via a telescoping harmonic pair.
fn try_telescoping(body: &Expr, k: &str, lo: i64, env: &Env) -> Result<Option<Value>, EvalError> {
    let Expr::Call { head, args } = body else {
        return Ok(None);
    };
    if !matches!(head.as_ref(), Expr::Symbol(h) if h == "Divide")
        || args.len() != 2
        || contains_sym(&args[0], k)
    {
        return Ok(None);
    }
    let Expr::Call { head: dh, args: da } = &args[1] else {
        return Ok(None);
    };
    if !matches!(dh.as_ref(), Expr::Symbol(s) if s == "Times") || da.len() != 2 {
        return Ok(None);
    }
    let (Some(x), Some(y)) = (linear_offset(&da[0], k), linear_offset(&da[1], k)) else {
        return Ok(None);
    };
    let (p, q) = if x < y { (x, y) } else { (y, x) };
    let d = q - p;
    if d <= 0 {
        return Ok(None);
    }
    let m = lo + p;
    if m <= 0 {
        return Ok(Some(Value::Symbol("Infinity".to_string())));
    }
    // (1/d) * (H_{m+d-1} - H_{m-1})
    let h1 = call_expr("HarmonicNumber", vec![int_expr(m + d - 1)]);
    let h2 = call_expr("HarmonicNumber", vec![int_expr(m - 1)]);
    let e = call_expr(
        "Divide",
        vec![
            call_expr(
                "Times",
                vec![
                    args[0].clone(),
                    call_expr("Plus", vec![h1, call_expr("Times", vec![int_expr(-1), h2])]),
                ],
            ),
            int_expr(d),
        ],
    );
    Ok(Some(crate::eval::eval(&e, env)?))
}

fn unevaluated_term(is_product: bool, expr: &Expr, k: &str, lo: &Expr, hi: &Expr) -> Value {
    let head = if is_product { "Product" } else { "Sum" };
    Value::Call {
        head: head.to_string(),
        args: vec![
            Value::Pattern(expr.clone()),
            Value::List(vec![
                Value::Symbol(k.to_string()),
                Value::Pattern(lo.clone()),
                Value::Pattern(hi.clone()),
            ]),
        ],
    }
}

fn eval_int_bound(expr: &Expr, env: &Env, label: &str) -> Result<i64, EvalError> {
    let v = crate::eval::eval(expr, env)?;
    v.to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: format!("{label} bound {}", v.type_name()),
    })
}

/// Like `eval_int_bound` but accepts a symbolic `Infinity`.
fn eval_iter_bound(expr: &Expr, env: &Env) -> Result<i64, EvalError> {
    let v = crate::eval::eval(expr, env)?;
    if matches!(&v, Value::Symbol(s) if s == "Infinity") {
        Ok(ITER_INFINITY_LIMIT)
    } else {
        v.to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer or Infinity".to_string(),
            got: format!("bound {}", v.type_name()),
        })
    }
}

/// Closed form for a symbolic single-iterator sum/product, or `None` when the
/// bounds are numeric and the caller should iterate.
fn try_symbolic(
    expr: &Expr,
    items: &[Expr],
    is_product: bool,
    env: &Env,
) -> Result<Option<Value>, EvalError> {
    let k = match items.first() {
        Some(Expr::Symbol(s)) => s.clone(),
        _ => return Ok(None),
    };
    let (lo_expr, hi_expr) = match items.len() {
        2 => {
            if matches!(items[1], Expr::List(_)) {
                return Ok(None);
            }
            (int_expr(1), items[1].clone())
        }
        3 => (items[1].clone(), items[2].clone()),
        _ => return Ok(None),
    };

    let hi_val = crate::eval::eval(&hi_expr, env)?;
    if hi_val.to_integer().is_some() {
        return Ok(None);
    }
    let inf = matches!(&hi_val, Value::Symbol(s) if s == "Infinity");

    let lo = match crate::eval::eval(&lo_expr, env)?.to_integer() {
        Some(l) => l,
        None => {
            return Ok(Some(unevaluated_term(
                is_product, expr, &k, &lo_expr, &hi_expr,
            )));
        }
    };

    // Product[k, ...] == hi! / (lo - 1)! (only for a finite upper bound).
    if is_product && !inf && matches!(expr, Expr::Symbol(s) if s == &k) {
        let fact = call_expr("Factorial", vec![hi_expr.clone()]);
        let e = if lo == 1 {
            fact
        } else {
            call_expr(
                "Divide",
                vec![fact, call_expr("Factorial", vec![int_expr(lo - 1)])],
            )
        };
        return Ok(Some(crate::eval::eval(&e, env)?));
    }

    if !is_product {
        // Factor any constant scale part out of the summand so `c f[k]`,
        // `f[k]/c` and shift variants reduce to a recognised core.
        let (coeff, body) = split_coeff(expr, &k);
        let core: &Expr = &body;

        // Faulhaber: sum k^p = S_p(hi) - S_p(lo - 1).
        if let Some(p) = as_power_of(core, &k)
            && let Some(s_hi) = faulhaber(p, &hi_expr)
        {
            let e = if lo == 1 {
                s_hi
            } else if let Some(s_lo) = faulhaber(p, &int_expr(lo - 1)) {
                call_expr(
                    "Plus",
                    vec![s_hi, call_expr("Times", vec![int_expr(-1), s_lo])],
                )
            } else {
                s_hi
            };
            return Ok(Some(finish_scaled(&coeff, &e, env)?));
        }
        // Geometric: sum r^k (finite, or infinite when convergent).
        if let Some(r) = as_geometric(core, &k) {
            if inf {
                let num = call_expr("Power", vec![r.clone(), int_expr(lo)]);
                let den = call_expr(
                    "Plus",
                    vec![int_expr(1), call_expr("Times", vec![int_expr(-1), r])],
                );
                return Ok(Some(finish_scaled(
                    &coeff,
                    &call_expr("Divide", vec![num, den]),
                    env,
                )?));
            }
            let num = call_expr(
                "Plus",
                vec![
                    call_expr(
                        "Power",
                        vec![
                            r.clone(),
                            call_expr("Plus", vec![hi_expr.clone(), int_expr(1)]),
                        ],
                    ),
                    call_expr(
                        "Times",
                        vec![
                            int_expr(-1),
                            call_expr("Power", vec![r.clone(), int_expr(lo)]),
                        ],
                    ),
                ],
            );
            let den = call_expr("Plus", vec![r, int_expr(-1)]);
            return Ok(Some(finish_scaled(
                &coeff,
                &call_expr("Divide", vec![num, den]),
                env,
            )?));
        }
        // Infinite special series: p-series, alternating harmonic, factorial
        // and telescoping (with the constant factor reattached).
        if inf && let Some(v) = inf_closed_form(core, &k, lo, env)? {
            return Ok(Some(scale_value(&coeff, v, env)?));
        }
    }

    Ok(Some(unevaluated_term(
        is_product, expr, &k, &lo_expr, &hi_expr,
    )))
}

/// Symbolic fast path for the evaluator: returns `Some` only when the
/// single-iterator bounds are non-numeric and a closed form applies, so that
/// numeric/multi-iterator calls can keep using the `eval::table` machinery.
pub fn try_sum_symbolic(args: &[Expr], env: &Env) -> Result<Option<Value>, EvalError> {
    try_symbolic_spec(args, false, env)
}

/// Symbolic fast path for `Product` (see [`try_sum_symbolic`]).
pub fn try_product_symbolic(args: &[Expr], env: &Env) -> Result<Option<Value>, EvalError> {
    try_symbolic_spec(args, true, env)
}

fn try_symbolic_spec(
    args: &[Expr],
    is_product: bool,
    env: &Env,
) -> Result<Option<Value>, EvalError> {
    if args.len() != 2 {
        return Ok(None);
    }
    let items = match &args[1] {
        Expr::List(v) => v,
        _ => return Ok(None),
    };
    if items.is_empty() {
        return Ok(None);
    }
    try_symbolic(&args[0], items, is_product, env)
}

/// Expand an iterator spec into the concrete values to iterate over.
fn iterator_values(items: &[Expr], env: &Env) -> Result<(String, Vec<Value>), EvalError> {
    let var = match items.first() {
        Some(Expr::Symbol(s)) => s.clone(),
        _ => {
            return Err(EvalError::Error(
                "iterator variable must be a symbol".to_string(),
            ));
        }
    };
    let ints = |lo: i64, hi: i64, step: i64| -> Vec<Value> {
        let mut values = Vec::new();
        if step == 0 {
            return values;
        }
        let mut i = lo;
        if step > 0 {
            while i <= hi {
                values.push(Value::Integer(Integer::from(i)));
                i = i.saturating_add(step);
            }
        } else {
            while i >= hi {
                values.push(Value::Integer(Integer::from(i)));
                i = i.saturating_add(step);
            }
        }
        values
    };
    match items.len() {
        2 => {
            if matches!(items[1], Expr::List(_)) {
                match crate::eval::eval(&items[1], env)? {
                    Value::List(v) => Ok((var, v)),
                    other => Err(EvalError::TypeError {
                        expected: "List".to_string(),
                        got: other.type_name().to_string(),
                    }),
                }
            } else {
                let n = eval_iter_bound(&items[1], env)?;
                Ok((var, ints(1, n, 1)))
            }
        }
        3 => {
            let lo = eval_int_bound(&items[1], env, "min")?;
            let hi = eval_iter_bound(&items[2], env)?;
            Ok((var, ints(lo, hi, 1)))
        }
        4 => {
            let lo = eval_int_bound(&items[1], env, "min")?;
            let hi = eval_iter_bound(&items[2], env)?;
            let step = eval_int_bound(&items[3], env, "step")?;
            if step == 0 {
                return Err(EvalError::Error("iterator step cannot be zero".to_string()));
            }
            Ok((var, ints(lo, hi, step)))
        }
        _ => Err(EvalError::Error(
            "iterator spec must have 2-4 elements".to_string(),
        )),
    }
}

/// `Sum[expr, {i, min, max}, ...]` — exact summation.
pub fn eval_sum(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Sum requires at least 1 argument".to_string(),
        ));
    }
    eval_sum_recursive(&args[0], &args[1..], env)
}

fn eval_sum_recursive(expr: &Expr, specs: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if specs.is_empty() {
        return crate::eval::eval(expr, env);
    }
    let items = match &specs[0] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Sum iterator spec must be a list".to_string(),
            ));
        }
    };
    if items.is_empty() {
        return eval_sum_recursive(expr, &specs[1..], env);
    }
    if specs.len() == 1
        && let Some(v) = try_symbolic(expr, items, false, env)?
    {
        return Ok(v);
    }
    let (var, values) = iterator_values(items, env)?;
    let child = env.child();
    let mut acc = Value::Integer(Integer::from(0));
    for val in values {
        child.set(var.clone(), val);
        let term = eval_sum_recursive(expr, &specs[1..], &child)?;
        acc = super::arithmetic::add_values_public(&acc, &term)?;
    }
    Ok(acc)
}

/// `Product[expr, {i, min, max}, ...]` — exact product.
pub fn eval_product(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Product requires at least 1 argument".to_string(),
        ));
    }
    eval_product_recursive(&args[0], &args[1..], env)
}

fn eval_product_recursive(expr: &Expr, specs: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if specs.is_empty() {
        return crate::eval::eval(expr, env);
    }
    let items = match &specs[0] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Product iterator spec must be a list".to_string(),
            ));
        }
    };
    if items.is_empty() {
        return eval_product_recursive(expr, &specs[1..], env);
    }
    if specs.len() == 1
        && let Some(v) = try_symbolic(expr, items, true, env)?
    {
        return Ok(v);
    }
    let (var, values) = iterator_values(items, env)?;
    let child = env.child();
    let mut acc = Value::Integer(Integer::from(1));
    for val in values {
        child.set(var.clone(), val);
        let term = eval_product_recursive(expr, &specs[1..], &child)?;
        acc = super::arithmetic::mul_values_public(&acc, &term)?;
    }
    Ok(acc)
}

/// `Sum[...]` / `Product[...]` when reached as plain (already-evaluated)
/// builtins, e.g. via `Apply[Sum, ...]`.
pub fn builtin_sum(args: &[Value]) -> Result<Value, EvalError> {
    let exprs: Vec<Expr> = args.iter().map(crate::eval::table::value_to_expr).collect();
    let env = Env::new();
    super::register_builtins(&env);
    eval_sum(&exprs, &env)
}

pub fn builtin_product(args: &[Value]) -> Result<Value, EvalError> {
    let exprs: Vec<Expr> = args.iter().map(crate::eval::table::value_to_expr).collect();
    let env = Env::new();
    super::register_builtins(&env);
    eval_product(&exprs, &env)
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
    seen.sort_by(compare_values);
    Ok(Value::List(seen))
}

pub fn builtin_intersection(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Intersection requires at least 2 arguments".to_string(),
        ));
    }
    let lists: Vec<&[Value]> = args
        .iter()
        .map(|a| match a {
            Value::List(items) => Ok(items.as_slice()),
            _ => Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: a.type_name().to_string(),
            }),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result: Vec<Value> = lists[0]
        .iter()
        .filter(|item| {
            lists[1..]
                .iter()
                .all(|list| list.iter().any(|x| x.struct_eq(item)))
        })
        .cloned()
        .collect();
    let mut deduped = Vec::new();
    for v in result {
        if !deduped.iter().any(|d: &Value| d.struct_eq(&v)) {
            deduped.push(v);
        }
    }
    deduped.sort_by(compare_values);
    Ok(Value::List(deduped))
}

pub fn builtin_complement(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Complement requires at least 2 arguments".to_string(),
        ));
    }
    let lists: Vec<&[Value]> = args
        .iter()
        .map(|a| match a {
            Value::List(items) => Ok(items.as_slice()),
            _ => Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: a.type_name().to_string(),
            }),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result: Vec<Value> = lists[0]
        .iter()
        .filter(|item| {
            !lists[1..]
                .iter()
                .any(|list| list.iter().any(|x| x.struct_eq(item)))
        })
        .cloned()
        .collect();
    let mut deduped = Vec::new();
    for v in result {
        if !deduped.iter().any(|d: &Value| d.struct_eq(&v)) {
            deduped.push(v);
        }
    }
    deduped.sort_by(compare_values);
    Ok(Value::List(deduped))
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
            })?;
            // `n` is the target length; a negative `n` pads on the right.
            let width = n.unsigned_abs() as usize;
            let pad_val = if args.len() == 3 {
                args[2].clone()
            } else {
                Value::Integer(Integer::from(0))
            };
            if n >= 0 {
                if width <= items.len() {
                    Ok(Value::List(items[items.len() - width..].to_vec()))
                } else {
                    let mut result: Vec<Value> = vec![pad_val; width - items.len()];
                    result.extend(items.iter().cloned());
                    Ok(Value::List(result))
                }
            } else {
                let mut result = items.clone();
                if width > items.len() {
                    result.resize(width, pad_val);
                } else {
                    result.truncate(width);
                }
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
            })?;
            // `n` is the target length; a negative `n` pads on the left.
            let width = n.unsigned_abs() as usize;
            let pad_val = if args.len() == 3 {
                args[2].clone()
            } else {
                Value::Null
            };
            if n >= 0 {
                let mut result = items.clone();
                if width > items.len() {
                    result.resize(width, pad_val);
                } else {
                    result.truncate(width);
                }
                Ok(Value::List(result))
            } else if width <= items.len() {
                Ok(Value::List(items[items.len() - width..].to_vec()))
            } else {
                let mut result: Vec<Value> = vec![pad_val; width - items.len()];
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
            // Non-list arguments are broadcast (Wolfram threads a single list
            // over scalar arguments). Threading length comes from the first
            // list argument.
            let len = call_args
                .iter()
                .find_map(|a| match a {
                    Value::List(items) => Some(items.len()),
                    _ => None,
                })
                .unwrap_or(0);
            for arg in call_args {
                if let Value::List(items) = arg
                    && items.len() != len
                {
                    return Err(EvalError::Error(
                        "Thread: all lists must have the same length".to_string(),
                    ));
                }
            }
            let mut result = Vec::with_capacity(len);
            for i in 0..len {
                let threaded_args: Vec<Value> = call_args
                    .iter()
                    .map(|a| match a {
                        Value::List(items) => items[i].clone(),
                        other => other.clone(),
                    })
                    .collect();
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

// ── TakeWhile ──────────────────────────────────────────────────────────────

/// TakeWhile[list, crit] — takes elements from the front of list while crit returns True.
pub fn builtin_take_while(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "TakeWhile requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let mut result = Vec::new();
    for item in items {
        let keep = apply_function(&args[1], &[item.clone()], env)?;
        match keep {
            Value::Bool(true) => result.push(item.clone()),
            _ => break,
        }
    }
    Ok(Value::List(result))
}

// ── DropWhile ──────────────────────────────────────────────────────────────

/// DropWhile[list, crit] — drops elements from the front of list while crit returns True.
pub fn builtin_drop_while(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "DropWhile requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let mut drop_count = 0;
    for item in items {
        let keep = apply_function(&args[1], &[item.clone()], env)?;
        match keep {
            Value::Bool(true) => drop_count += 1,
            _ => break,
        }
    }
    Ok(Value::List(items[drop_count..].to_vec()))
}

// ── MapIf ──────────────────────────────────────────────────────────────────

/// MapIf[f, list, crit] — applies f only to elements matching crit.
pub fn builtin_map_if(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "MapIf requires exactly 3 arguments".to_string(),
        ));
    }
    let items = get_list(&args[1])?;
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let test = apply_function(&args[2], &[item.clone()], env)?;
        match test {
            Value::Bool(true) => {
                let mapped = apply_function(&args[0], &[item.clone()], env)?;
                result.push(mapped);
            }
            _ => result.push(item.clone()),
        }
    }
    Ok(Value::List(result))
}

// ── LongestCommonSubsequencePositions ──────────────────────────────────────

/// LongestCommonSubsequencePositions[list1, list2] — positions of LCS in both lists.
pub fn builtin_longest_common_subsequence_positions(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LongestCommonSubsequencePositions requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::List(l1), Value::List(l2)) => {
            let (pos1, pos2) = lcs_positions(l1, l2);
            Ok(Value::List(vec![
                Value::List(
                    pos1.into_iter()
                        .map(|i| Value::Integer(Integer::from(i as i64 + 1)))
                        .collect(),
                ),
                Value::List(
                    pos2.into_iter()
                        .map(|i| Value::Integer(Integer::from(i as i64 + 1)))
                        .collect(),
                ),
            ]))
        }
        _ => Err(EvalError::TypeError {
            expected: "two Lists".to_string(),
            got: format!("{} and {}", args[0].type_name(), args[1].type_name()),
        }),
    }
}

/// Compute LCS positions using DP backtracking.
fn lcs_positions(a: &[Value], b: &[Value]) -> (Vec<usize>, Vec<usize>) {
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }
    let mut pos1 = Vec::new();
    let mut pos2 = Vec::new();
    let (mut i, mut j) = (m, n);
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            pos1.push(i - 1);
            pos2.push(j - 1);
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pos1.reverse();
    pos2.reverse();
    (pos1, pos2)
}

// ── FindTransientRepeat ────────────────────────────────────────────────────

/// FindTransientRepeat[list, n] — find transient prefix and repeating part.
pub fn builtin_find_transient_repeat(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FindTransientRepeat requires exactly 2 arguments".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let max_period = match &args[1] {
        Value::Integer(n) => n
            .to_usize()
            .ok_or_else(|| EvalError::Error("max period too large".to_string()))?,
        _ => {
            return Err(EvalError::TypeError {
                expected: "positive Integer".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    if items.is_empty() || max_period == 0 {
        return Ok(Value::List(vec![Value::List(vec![]), Value::List(vec![])]));
    }
    // Try periods from 1 to max_period
    for period in 1..=max_period.min(items.len()) {
        // Find the shortest transient such that the rest is repeating with this period
        let min_transient = items.len().saturating_sub(period * 2);
        for transient_len in (0..=min_transient).rev() {
            let remaining = &items[transient_len..];
            if remaining.is_empty() {
                continue;
            }
            if remaining.len() % period != 0 {
                continue;
            }
            let pattern = &remaining[0..period];
            let mut valid = true;
            for chunk in remaining.chunks(period) {
                if chunk != pattern {
                    valid = false;
                    break;
                }
            }
            if valid {
                let transient: Vec<Value> = items[..transient_len].to_vec();
                let repeat: Vec<Value> = pattern.to_vec();
                return Ok(Value::List(vec![
                    Value::List(transient),
                    Value::List(repeat),
                ]));
            }
        }
    }
    // No repeating pattern found
    Ok(Value::List(vec![
        Value::List(items.to_vec()),
        Value::List(vec![]),
    ]))
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

    // ── Regression: negative Take/Drop must use last |n| elements ──

    #[test]
    fn test_take_negative_returns_last_n() {
        let result = builtin_take(&[list(vec![int(1), int(2), int(3), int(4)]), int(-2)]).unwrap();
        assert_eq!(result, list(vec![int(3), int(4)]));
    }

    #[test]
    fn test_take_negative_overlong_no_panic() {
        // |n| larger than the list must yield the whole list, not underflow.
        let result = builtin_take(&[list(vec![int(1), int(2), int(3)]), int(-5)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_drop_negative_removes_last_n() {
        let result = builtin_drop(&[list(vec![int(1), int(2), int(3), int(4)]), int(-2)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2)]));
    }

    #[test]
    fn test_drop_negative_overlong_no_panic() {
        let result = builtin_drop(&[list(vec![int(1), int(2), int(3)]), int(-5)]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_take_range_negative() {
        let result = builtin_take(&[
            list(vec![int(1), int(2), int(3), int(4), int(5)]),
            list(vec![int(-2), int(-1)]),
        ])
        .unwrap();
        assert_eq!(result, list(vec![int(4), int(5)]));
    }

    #[test]
    fn test_drop_range_negative() {
        let result = builtin_drop(&[
            list(vec![int(1), int(2), int(3), int(4), int(5)]),
            list(vec![int(-2), int(-1)]),
        ])
        .unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_take_range_out_of_range_clamped() {
        let result =
            builtin_take(&[list(vec![int(1), int(2)]), list(vec![int(1), int(9)])]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2)]));
    }

    // ── Regression: Transpose of ragged input must not panic ──

    #[test]
    fn test_transpose_ragged() {
        let ragged = list(vec![
            list(vec![int(1), int(2)]),
            list(vec![int(3), int(4), int(5)]),
        ]);
        let result = builtin_transpose(&[ragged]).unwrap();
        assert_eq!(
            result,
            list(vec![
                list(vec![int(1), int(3)]),
                list(vec![int(2), int(4)]),
                list(vec![int(5)]),
            ])
        );
    }

    // ── Regression: negative PadLeft/PadRight must not blow up ──

    #[test]
    fn test_pad_left_negative_pads_right() {
        // Negative n pads on the right (Wolfram: "negative length
        // specifications pad on the right"); |n| is the target length.
        let result = builtin_pad_left(&[list(vec![int(1), int(2), int(3)]), int(-5)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3), int(0), int(0)]));
        let result = builtin_pad_left(&[list(vec![int(1), int(2), int(3)]), int(-1)]).unwrap();
        assert_eq!(result, list(vec![int(1)]));
    }

    #[test]
    fn test_pad_right_negative_pads_left() {
        let result = builtin_pad_right(&[list(vec![int(1), int(2), int(3)]), int(-5)]).unwrap();
        assert_eq!(
            result,
            list(vec![Value::Null, Value::Null, int(1), int(2), int(3)])
        );
        let result = builtin_pad_right(&[list(vec![int(1), int(2), int(3)]), int(-1)]).unwrap();
        assert_eq!(result, list(vec![int(3)]));
    }

    // ── Regression: Part on strings uses char indexing / no panic ──

    #[test]
    fn test_part_string_unicode() {
        // "hé" has 2 chars but 3 bytes; Part[...,2] must give "é".
        let result = builtin_part(&[Value::Str("hé".to_string()), int(2)]).unwrap();
        assert_eq!(result, Value::Str("é".to_string()));
        let result = builtin_part(&[Value::Str("hé".to_string()), int(-1)]).unwrap();
        assert_eq!(result, Value::Str("é".to_string()));
    }

    #[test]
    fn test_part_string_out_of_bounds() {
        assert!(builtin_part(&[Value::Str("hi".to_string()), int(10)]).is_err());
        assert!(builtin_part(&[Value::Str("hi".to_string()), int(0)]).is_err());
    }

    #[test]
    fn test_length_string_counts_chars() {
        assert_eq!(
            builtin_length(&[Value::Str("hé".to_string())]).unwrap(),
            int(2)
        );
    }

    // ── Env-aware function tests ──

    fn test_env() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    fn eval_str(input: &str) -> Value {
        let env = test_env();
        let tokens = crate::lexer::tokenize(input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap_or_else(|e| panic!("parse {input}: {e:?}"));
        crate::eval::eval_program(&ast, &env).unwrap_or_else(|e| panic!("eval {input}: {e:?}"))
    }

    /// Assert `input` evaluates to `expected`. Falls back to requiring the
    /// simplified difference to vanish when the two are only equal up to
    /// automatic simplification.
    fn sum_eq(input: &str, expected: &str) {
        let lhs = eval_str(input);
        let rhs = eval_str(expected);
        if lhs == rhs {
            return;
        }
        let diff = eval_str(&format!("Simplify[({input}) - ({expected})]"));
        let s = format!("{diff}");
        let stripped: String = s.chars().filter(|c| !" {},".contains(*c)).collect();
        assert!(
            !stripped.is_empty() && stripped.chars().all(|c| c == '0'),
            "`{input}` should equal `{expected}`, got residual `{s}`"
        );
    }

    // ── Infinite Sum closed forms ──

    #[test]
    fn test_sum_inf_pseries_even() {
        sum_eq("Sum[1/k^2, {k, 1, Infinity}]", "Pi^2/6");
        sum_eq("Sum[1/k^4, {k, 1, Infinity}]", "Pi^4/90");
        sum_eq("Sum[1/k^6, {k, 1, Infinity}]", "Pi^6/945");
    }

    #[test]
    fn test_sum_inf_pseries_odd_is_zeta() {
        assert_eq!(
            eval_str("Sum[1/k^3, {k, 1, Infinity}]"),
            Value::Call {
                head: "Zeta".to_string(),
                args: vec![int(3)],
            }
        );
        // Finite head `k` from 2: Zeta[3] - 1 (kept symbolic).
        assert_eq!(
            eval_str("Sum[1/k^3, {k, 2, Infinity}]"),
            Value::Call {
                head: "Plus".to_string(),
                args: vec![
                    Value::Call {
                        head: "Zeta".to_string(),
                        args: vec![int(3)],
                    },
                    int(-1),
                ],
            }
        );
    }

    #[test]
    fn test_sum_inf_pseries_rational() {
        assert_eq!(
            eval_str("Sum[1/k^(5/2), {k, 1, Infinity}]"),
            Value::Call {
                head: "Zeta".to_string(),
                args: vec![Value::Rational(Box::new(rug::Rational::from((5, 2))))],
            }
        );
    }

    #[test]
    fn test_sum_inf_harmonic_diverges() {
        assert_eq!(
            eval_str("Sum[1/k, {k, 1, Infinity}]"),
            Value::Symbol("Infinity".to_string())
        );
    }

    #[test]
    fn test_sum_inf_alternating_harmonic() {
        sum_eq("Sum[(-1)^(k+1)/k, {k, 1, Infinity}]", "Log[2]");
    }

    #[test]
    fn test_sum_inf_factorial() {
        sum_eq("Sum[1/k!, {k, 0, Infinity}]", "E");
        sum_eq("Sum[x^k/k!, {k, 0, Infinity}]", "E^x");
        // From k = 1: E - 1.
        sum_eq("Sum[1/k!, {k, 1, Infinity}]", "E - 1");
    }

    #[test]
    fn test_sum_inf_telescoping() {
        sum_eq("Sum[1/(k (k + 1)), {k, 1, Infinity}]", "1");
        sum_eq("Sum[1/(k (k + 2)), {k, 1, Infinity}]", "3/4");
        sum_eq("Sum[1/(k (k - 1)), {k, 2, Infinity}]", "1");
        sum_eq("Sum[1/((k - 2) (k + 1)), {k, 3, Infinity}]", "11/18");
        // A factor is zero at `k = 1`, so the sum diverges.
        assert_eq!(
            eval_str("Sum[1/(k (k - 1)), {k, 1, Infinity}]"),
            Value::Symbol("Infinity".to_string())
        );
    }

    #[test]
    fn test_sum_inf_geometric() {
        sum_eq("Sum[r^k, {k, 0, Infinity}]", "1/(1 - r)");
        sum_eq("Sum[a r^k, {k, 0, Infinity}]", "a/(1 - r)");
        sum_eq("Sum[1/2^k, {k, 1, Infinity}]", "1");
        sum_eq("Sum[r^k, {k, 2, Infinity}]", "r^2/(1 - r)");
    }

    #[test]
    fn test_sum_inf_scale_and_shift() {
        sum_eq("Sum[2/k^2, {k, 1, Infinity}]", "Pi^2/3");
        sum_eq("Sum[c/k^2, {k, 1, Infinity}]", "c Pi^2/6");
        sum_eq("Sum[1/k^2/2, {k, 1, Infinity}]", "Pi^2/12");
        // Shifted index: sum_{k>=1} 1/(k+1)^2 == Zeta[2] - 1.
        sum_eq("Sum[1/(k + 1)^2, {k, 1, Infinity}]", "Pi^2/6 - 1");
        sum_eq("Sum[1/(k + 1), {k, 0, Infinity}]", "Infinity");
        sum_eq("Sum[1/(k - 1)^2, {k, 2, Infinity}]", "Pi^2/6");
        assert_eq!(
            eval_str("Sum[1/(k - 1)^2, {k, 0, Infinity}]"),
            Value::Symbol("Infinity".to_string())
        );
    }

    /// Numeric comparison with tolerance (for results that reduce to
    /// `HurwitzZeta`/reals rather than exact closed forms).
    fn sum_n_eq(input: &str, expected: &str) {
        let d = eval_str(&format!("N[({input}) - ({expected})]"));
        let r = match d {
            Value::Real(f) => f.to_f64(),
            Value::Integer(n) => n.to_f64(),
            other => panic!("`{input}` vs `{expected}`: non-numeric residual {other}"),
        };
        assert!(r.abs() < 1e-8, "`{input}` != `{expected}` (residual {r})");
    }

    #[test]
    fn test_sum_inf_scaled_argument() {
        sum_n_eq("Sum[1/(2 k + 1)^2, {k, 1, Infinity}]", "Pi^2/8 - 1");
        sum_n_eq("Sum[1/(2 k)^2, {k, 1, Infinity}]", "Pi^2/24");
        sum_n_eq("Sum[1/(2 k + 1)^3, {k, 1, Infinity}]", "7 Zeta[3]/8 - 1");
    }

    #[test]
    fn test_sum_inf_positive_power_diverges() {
        // `sum k^s` with `s > 0` diverges; `s = 3/2` is not Faulhaber-handled.
        assert_eq!(
            eval_str("Sum[k^(3/2), {k, 1, Infinity}]"),
            Value::Symbol("Infinity".to_string())
        );
        assert_eq!(
            eval_str("Sum[Power[k, 1/2], {k, 1, Infinity}]"),
            Value::Symbol("Infinity".to_string())
        );
    }

    #[test]
    fn test_sum_inf_unrecognised_stays_symbolic() {
        assert_eq!(
            eval_str("Sum[Sin[k]/k^2, {k, 1, Infinity}]"),
            eval_str("Sum[Sin[k]/k^2, {k, 1, Infinity}]")
        );
        // Guard against silently wrong values: must not have collapsed.
        let v = eval_str("Sum[Sin[k]/k^2, {k, 1, Infinity}]");
        assert!(matches!(v, Value::Call { ref head, .. } if head == "Sum"));
    }

    #[test]
    fn test_sum_finite_unchanged() {
        assert_eq!(eval_str("Sum[k, {k, 1, 10}]"), int(55));
        assert_eq!(eval_str("Sum[k^2, {k, 1, 4}]"), int(30));
        sum_eq("Sum[k^3, {k, 1, 5}]", "225");
        // Finite symbolic upper bound still uses Faulhaber.
        sum_eq("Sum[k^2, {k, 1, n}]", "n (n + 1) (2 n + 1)/6");
        // Finite geometric with a scale factor.
        sum_eq("Sum[2 * 3^k, {k, 0, 3}]", "80");
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
