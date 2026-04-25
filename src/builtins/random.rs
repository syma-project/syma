use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;
use std::cell::RefCell;

thread_local! {
    static RNG_STATE: RefCell<u64> = const { RefCell::new(1) };
}

fn next_random() -> u64 {
    RNG_STATE.with(|state| {
        let mut s = state.borrow_mut();
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    })
}

pub fn builtin_random_integer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "RandomInteger requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if *n > 0 => {
            let n_i64 = n.to_i64().unwrap_or(1);
            let rand_val = (next_random() as i64).rem_euclid(n_i64);
            Ok(Value::Integer(Integer::from(rand_val)))
        }
        Value::List(items) if items.len() == 2 => {
            let min = items[0].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: items[0].type_name().to_string(),
            })?;
            let max = items[1].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: items[1].type_name().to_string(),
            })?;
            if min > max {
                return Err(EvalError::Error(
                    "RandomInteger: min must be <= max".to_string(),
                ));
            }
            let rand_val = min + (next_random() as i64).rem_euclid(max - min + 1);
            Ok(Value::Integer(Integer::from(rand_val)))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer or {min, max}".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// Parse a dimension spec: integer n → 1D [n], list of ints → ND.
fn parse_dims(val: &Value) -> Result<Vec<usize>, EvalError> {
    match val {
        Value::Integer(n) => {
            let n_i64 = n.to_i64().ok_or_else(|| EvalError::TypeError {
                expected: "non-negative integer dimension".to_string(),
                got: "negative or too large".to_string(),
            })?;
            if n_i64 < 0 {
                return Err(EvalError::Error(
                    "RandomReal: dimension must be non-negative".to_string(),
                ));
            }
            Ok(vec![n_i64 as usize])
        }
        Value::List(items) => {
            let mut dims = Vec::with_capacity(items.len());
            for item in items {
                let n = item.to_integer().ok_or_else(|| EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: item.type_name().to_string(),
                })?;
                if n < 0 {
                    return Err(EvalError::Error(
                        "RandomReal: dimension must be non-negative".to_string(),
                    ));
                }
                dims.push(n as usize);
            }
            Ok(dims)
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer or List of integers".to_string(),
            got: val.type_name().to_string(),
        }),
    }
}

/// Recursively build a nested list of random reals in [min, max].
fn random_real_array(dims: &[usize], min: f64, max: f64) -> Value {
    if dims.is_empty() {
        value_random_real(min, max)
    } else {
        let count = dims[0];
        let rest = &dims[1..];
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            items.push(random_real_array(rest, min, max));
        }
        Value::List(items)
    }
}

/// Generate a single random real in [min, max].
fn value_random_real(min: f64, max: f64) -> Value {
    let r = (next_random() as f64) / (u64::MAX as f64);
    Value::Real(Float::with_val(DEFAULT_PRECISION, min + r * (max - min)))
}

pub fn builtin_random_real(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        0 => {
            let r = (next_random() as f64) / (u64::MAX as f64);
            Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, r)))
        }
        1 => match &args[0] {
            Value::List(items) if items.len() == 2 => {
                let min = items[0].to_real().ok_or_else(|| EvalError::TypeError {
                    expected: "Number".to_string(),
                    got: items[0].type_name().to_string(),
                })?;
                let max = items[1].to_real().ok_or_else(|| EvalError::TypeError {
                    expected: "Number".to_string(),
                    got: items[1].type_name().to_string(),
                })?;
                let r = (next_random() as f64) / (u64::MAX as f64);
                let result = min + r * (max - min);
                Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, result)))
            }
            _ => Err(EvalError::TypeError {
                expected: "{min, max}".to_string(),
                got: args[0].type_name().to_string(),
            }),
        },
        2 => {
            // RandomReal[{min, max}, dims]
            let (min, max) = match &args[0] {
                Value::List(items) if items.len() == 2 => {
                    let min = items[0].to_real().ok_or_else(|| EvalError::TypeError {
                        expected: "Number".to_string(),
                        got: items[0].type_name().to_string(),
                    })?;
                    let max = items[1].to_real().ok_or_else(|| EvalError::TypeError {
                        expected: "Number".to_string(),
                        got: items[1].type_name().to_string(),
                    })?;
                    (min, max)
                }
                _ => {
                    return Err(EvalError::NoMatch {
                        head: "RandomReal".to_string(),
                        args: args.to_vec(),
                    });
                }
            };
            let dims = parse_dims(&args[1])?;
            Ok(random_real_array(&dims, min, max))
        }
        _ => Err(EvalError::NoMatch {
            head: "RandomReal".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_random_choice(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "RandomChoice requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) if !items.is_empty() => {
            Ok(items[(next_random() as usize) % items.len()].clone())
        }
        Value::List(_) => Err(EvalError::Error("RandomChoice on empty list".to_string())),
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

    fn real(f: f64) -> Value {
        Value::Real(Float::with_val(DEFAULT_PRECISION, f))
    }

    fn list(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    #[test]
    fn test_random_real_0_args() {
        let result = builtin_random_real(&[]).unwrap();
        assert!(matches!(result, Value::Real(_)));
    }

    #[test]
    fn test_random_real_1_arg_range() {
        let result = builtin_random_real(&[list(vec![real(0.0), real(1.0)])]).unwrap();
        assert!(matches!(result, Value::Real(_)));
    }

    #[test]
    fn test_random_real_2_arg_integer_dim() {
        let result = builtin_random_real(&[list(vec![real(0.0), real(1.0)]), int(5)]).unwrap();
        assert!(matches!(&result, Value::List(items) if items.len() == 5));
    }

    #[test]
    fn test_random_real_2_arg_list_dim() {
        let result =
            builtin_random_real(&[list(vec![real(0.0), real(1.0)]), list(vec![int(3), int(4)])])
                .unwrap();
        assert!(matches!(&result, Value::List(rows) if rows.len() == 3));
        if let Value::List(rows) = &result {
            assert!(
                rows.iter()
                    .all(|row| matches!(row, Value::List(cols) if cols.len() == 4))
            );
        }
    }

    #[test]
    fn test_random_real_too_many_args_returns_no_match() {
        let result = builtin_random_real(&[list(vec![real(0.0), real(1.0)]), int(5), int(3)]);
        assert!(matches!(result, Err(EvalError::NoMatch { .. })));
    }

    #[test]
    fn test_random_real_wrong_dims_type() {
        let result = builtin_random_real(&[list(vec![real(0.0), real(1.0)]), real(5.0)]);
        assert!(matches!(result, Err(EvalError::TypeError { .. })));
    }
}
