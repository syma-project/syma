use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;
use std::cell::RefCell;

thread_local! {
    static RNG_STATE: RefCell<u64> = RefCell::new(1);
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
        _ => Err(EvalError::Error(
            "RandomReal requires 0 or 1 arguments".to_string(),
        )),
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
