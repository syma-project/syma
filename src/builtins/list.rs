use crate::value::{EvalError, Value};
use rug::Integer;

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
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Flatten requires exactly 1 argument".to_string(),
        ));
    }
    fn flatten(val: &Value) -> Vec<Value> {
        match val {
            Value::List(items) => {
                let mut result = Vec::new();
                for item in items {
                    result.extend(flatten(item));
                }
                result
            }
            _ => vec![val.clone()],
        }
    }
    Ok(Value::List(flatten(&args[0])))
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
            sorted.sort_by(|a, b| match (a, b) {
                (Value::Integer(x), Value::Integer(y)) => x.cmp(y),
                (Value::Real(x), Value::Real(y)) => {
                    x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                }
                (Value::Str(x), Value::Str(y)) => x.cmp(y),
                _ => std::cmp::Ordering::Equal,
            });
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
            let idx = if index > 0 {
                (index - 1) as usize
            } else if index < 0 {
                (items.len() as i64 + index) as usize
            } else {
                return Err(EvalError::IndexOutOfBounds {
                    index,
                    length: items.len(),
                });
            };
            if idx < items.len() {
                Ok(items[idx].clone())
            } else {
                Err(EvalError::IndexOutOfBounds {
                    index,
                    length: items.len(),
                })
            }
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

pub fn builtin_table(_args: &[Value]) -> Result<Value, EvalError> {
    // TODO: implement Table with iterator spec
    Err(EvalError::Error("Table not yet implemented".to_string()))
}

pub fn builtin_map(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Map requires exactly 2 arguments".to_string(),
        ));
    }
    // Map is handled by the evaluator for proper function application
    Err(EvalError::Error(
        "Map should be handled by evaluator".to_string(),
    ))
}

pub fn builtin_fold(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "Fold requires exactly 3 arguments".to_string(),
        ));
    }
    // Fold is handled by the evaluator for proper function application
    Err(EvalError::Error(
        "Fold should be handled by evaluator".to_string(),
    ))
}

pub fn builtin_select(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Select requires exactly 2 arguments".to_string(),
        ));
    }
    // Select is handled by the evaluator for proper function application
    Err(EvalError::Error(
        "Select should be handled by evaluator".to_string(),
    ))
}

pub fn builtin_scan(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "Scan should be handled by evaluator".to_string(),
    ))
}

pub fn builtin_nest(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "Nest should be handled by evaluator".to_string(),
    ))
}

pub fn builtin_take(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Take requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
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
}
