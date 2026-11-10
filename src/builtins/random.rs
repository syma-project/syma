use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;
use std::cell::RefCell;

thread_local! {
    /// Thread-local RNG shared by all random builtins (RandomReal, RandomChoice,
    /// RandomInteger, RandomVariate, …). Seeded deterministically so that
    /// `SeedRandom[n]` makes subsequent draws reproducible.
    static RNG: RefCell<fastrand::Rng> =
        const { RefCell::new(fastrand::Rng::with_seed(0x9E37_79B9_7F4A_7C15)) };
}

/// Reseed the shared RNG with an explicit seed (`SeedRandom[n]`).
pub fn seed_rng(seed: u64) {
    RNG.with(|cell| *cell.borrow_mut() = fastrand::Rng::with_seed(seed));
}

/// Reseed the shared RNG from system entropy (`SeedRandom[]`).
pub fn reseed_entropy() {
    RNG.with(|cell| *cell.borrow_mut() = fastrand::Rng::new());
}

/// Run `f` with mutable access to the shared RNG.
pub fn with_rng<T>(f: impl FnOnce(&mut fastrand::Rng) -> T) -> T {
    RNG.with(|cell| f(&mut cell.borrow_mut()))
}

/// Draw the next raw 64-bit value from the shared RNG.
pub fn next_u64() -> u64 {
    with_rng(|rng| rng.u64(..))
}

fn next_random() -> u64 {
    next_u64()
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
            // RandomReal[max] — a real in [0, max].
            other if other.to_real().is_some() => {
                let max = other.to_real().unwrap();
                let r = (next_random() as f64) / (u64::MAX as f64);
                Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, r * max)))
            }
            _ => Err(EvalError::TypeError {
                expected: "{min, max} or max".to_string(),
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
                        args: Box::new(args.to_vec()),
                    });
                }
            };
            let dims = parse_dims(&args[1])?;
            Ok(random_real_array(&dims, min, max))
        }
        _ => Err(EvalError::NoMatch {
            head: "RandomReal".to_string(),
            args: Box::new(args.to_vec()),
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

/// Draw a uniform `f64` in `[min, max)`.
fn uniform_real(min: f64, max: f64) -> f64 {
    let r = (next_random() as f64) / (u64::MAX as f64);
    min + r * (max - min)
}

/// Parse a complex range spec into `(remin, remax, imin, imax)`.
fn parse_complex_spec(v: &Value) -> Option<(f64, f64, f64, f64)> {
    match v {
        Value::List(items) if items.len() == 2 => {
            // {{rmin, rmax}, {imin, imax}} or {min, max}
            if let (Value::List(re), Value::List(im)) = (&items[0], &items[1]) {
                if re.len() == 2 && im.len() == 2 {
                    return Some((
                        re[0].to_real()?,
                        re[1].to_real()?,
                        im[0].to_real()?,
                        im[1].to_real()?,
                    ));
                }
                None
            } else {
                let min = items[0].to_real()?;
                let max = items[1].to_real()?;
                Some((min, max, min, max))
            }
        }
        _ => {
            let max = v.to_real()?;
            Some((0.0, max, 0.0, max))
        }
    }
}

fn random_complex_value(rem: f64, rex: f64, imm: f64, imx: f64) -> Value {
    Value::Complex {
        re: uniform_real(rem, rex),
        im: uniform_real(imm, imx),
    }
}

fn random_complex_array(dims: &[usize], rem: f64, rex: f64, imm: f64, imx: f64) -> Value {
    if dims.is_empty() {
        random_complex_value(rem, rex, imm, imx)
    } else {
        let mut items = Vec::with_capacity(dims[0]);
        for _ in 0..dims[0] {
            items.push(random_complex_array(&dims[1..], rem, rex, imm, imx));
        }
        Value::List(items)
    }
}

/// RandomComplex[spec?, dims?] — a pseudorandom complex number.
pub fn builtin_random_complex(args: &[Value]) -> Result<Value, EvalError> {
    let (spec, dims) = match args.len() {
        0 => (None, None),
        1 => (Some(&args[0]), None),
        2 => (Some(&args[0]), Some(parse_dims(&args[1])?)),
        _ => {
            return Err(EvalError::NoMatch {
                head: "RandomComplex".to_string(),
                args: Box::new(args.to_vec()),
            });
        }
    };
    let (rem, rex, imm, imx) = match spec {
        Some(s) => parse_complex_spec(s).ok_or_else(|| EvalError::TypeError {
            expected: "range spec".to_string(),
            got: s.type_name().to_string(),
        })?,
        None => (0.0, 1.0, 0.0, 1.0),
    };
    match dims {
        Some(d) => Ok(random_complex_array(&d, rem, rex, imm, imx)),
        None => Ok(random_complex_value(rem, rex, imm, imx)),
    }
}

/// RandomArray[dist, dims?] — array of pseudorandom variates. Delegates to
/// `RandomVariate`, expanding a dimension list into a nested array.
pub fn builtin_random_array(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "RandomArray requires 1 or 2 arguments".to_string(),
        ));
    }
    if args.len() == 2
        && let Value::List(_) = &args[1]
    {
        let dims = parse_dims(&args[1])?;
        return Ok(random_variate_array(&args[0], &dims));
    }
    super::statistics::builtin_random_variate(args)
}

fn random_variate_array(dist: &Value, dims: &[usize]) -> Value {
    if dims.is_empty() {
        return super::statistics::builtin_random_variate(std::slice::from_ref(dist)).unwrap_or(
            Value::Call {
                head: "RandomArray".to_string(),
                args: vec![dist.clone()],
            },
        );
    }
    Value::List(
        (0..dims[0])
            .map(|_| random_variate_array(dist, &dims[1..]))
            .collect(),
    )
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

    #[test]
    fn test_random_real_max_argument() {
        let v = builtin_random_real(&[int(3)]).unwrap();
        let f = super::super::to_f64(&v).unwrap();
        assert!((0.0..=3.0).contains(&f));
    }

    #[test]
    fn test_random_complex_bounds() {
        let v = builtin_random_complex(&[int(2)]).unwrap();
        let Value::Complex { re, im } = v else {
            panic!("expected complex");
        };
        assert!((0.0..=2.0).contains(&re) && (0.0..=2.0).contains(&im));
    }

    #[test]
    fn test_random_complex_array() {
        let v = builtin_random_complex(&[int(2), list(vec![int(2), int(3)])]).unwrap();
        let Value::List(rows) = v else {
            panic!("expected list");
        };
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn test_random_array_dims() {
        let mut dist = std::collections::HashMap::new();
        dist.insert("Distribution".to_string(), Value::Str("Normal".to_string()));
        dist.insert("Mean".to_string(), real(0.0));
        dist.insert("SD".to_string(), real(1.0));
        let v = builtin_random_array(&[Value::Assoc(dist), list(vec![int(2), int(3)])]).unwrap();
        let Value::List(rows) = v else {
            panic!("expected list");
        };
        assert_eq!(rows.len(), 2);
        let Value::List(cols) = &rows[0] else {
            panic!("expected nested list");
        };
        assert_eq!(cols.len(), 3);
    }
}
