use crate::value::{DEFAULT_PRECISION, EvalError, Value, rational_value};
use rug::Float;
use rug::Integer;
use rug::Rational;
use rug::ops::Pow;

/// Normalize a complex result: if imaginary part is zero, return as Real.
fn normalize_complex(re: f64, im: f64) -> Value {
    if im == 0.0 {
        Value::Real(Float::with_val(DEFAULT_PRECISION, re))
    } else if re == 0.0 {
        Value::Complex { re: 0.0, im }
    } else {
        Value::Complex { re, im }
    }
}

/// Convert a Value to f64 for arithmetic with Complex numbers. Returns None for non-numeric values.
fn val_to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        Value::Rational(r) => Some(r.as_ref().to_f64()),
        _ => None,
    }
}

pub fn normalize_complex_public(re: f64, im: f64) -> Value {
    normalize_complex(re, im)
}

pub fn val_to_f64_public(v: &Value) -> Option<f64> {
    val_to_f64(v)
}

pub fn builtin_plus(args: &[Value]) -> Result<Value, EvalError> {
    let mut result = Value::Integer(Integer::from(0));
    for arg in args {
        result = add_values(&result, arg)?;
    }
    Ok(result)
}

pub fn add_values_public(a: &Value, b: &Value) -> Result<Value, EvalError> {
    add_values(a, b)
}

pub fn sub_values_public(a: &Value, b: &Value) -> Result<Value, EvalError> {
    sub_values(a, b)
}

pub fn mul_values_public(a: &Value, b: &Value) -> Result<Value, EvalError> {
    mul_values(a, b)
}

fn sub_values(a: &Value, b: &Value) -> Result<Value, EvalError> {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Ok(Value::Integer(x.clone() - y)),
        (Value::Real(x), Value::Real(y)) => Ok(Value::Real(x.clone() - y)),
        (Value::Integer(x), Value::Real(y)) => {
            Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, x) - y))
        }
        (Value::Real(x), Value::Integer(y)) => {
            Ok(Value::Real(x - Float::with_val(DEFAULT_PRECISION, y)))
        }
        (Value::Rational(x), Value::Rational(y)) => {
            let diff: Rational = (x.as_ref() - y.as_ref()).into();
            let (num, den) = diff.into_numer_denom();
            Ok(rational_value(num, den))
        }
        (Value::Rational(x), Value::Integer(y)) => {
            let diff: Rational = x.as_ref() - Rational::from(y);
            let (num, den) = diff.into_numer_denom();
            Ok(rational_value(num, den))
        }
        (Value::Integer(x), Value::Rational(y)) => {
            let diff: Rational = Rational::from(x) - y.as_ref();
            let (num, den) = diff.into_numer_denom();
            Ok(rational_value(num, den))
        }
        (Value::Rational(x), Value::Real(y)) => {
            let x_f = Float::with_val(DEFAULT_PRECISION, x.numer())
                / Float::with_val(DEFAULT_PRECISION, x.denom());
            Ok(Value::Real(x_f - y))
        }
        (Value::Real(x), Value::Rational(y)) => {
            let y_f = Float::with_val(DEFAULT_PRECISION, y.numer())
                / Float::with_val(DEFAULT_PRECISION, y.denom());
            Ok(Value::Real(x - y_f))
        }
        (Value::List(xs), Value::List(ys)) => {
            if xs.len() == ys.len() {
                let result: Result<Vec<Value>, _> = xs
                    .iter()
                    .zip(ys.iter())
                    .map(|(x, y)| sub_values(x, y))
                    .collect();
                Ok(Value::List(result?))
            } else {
                Err(EvalError::Error(
                    "Lists must have same length for subtraction".to_string(),
                ))
            }
        }
        // Complex - Complex
        (Value::Complex { re: r1, im: im1 }, Value::Complex { re: r2, im: im2 }) => {
            Ok(normalize_complex(r1 - r2, im1 - im2))
        }
        // Complex - scalar
        (Value::Complex { re, im }, other) => {
            if let Some(v) = val_to_f64(other) {
                Ok(normalize_complex(*re - v, *im))
            } else {
                Ok(Value::Call {
                    head: "Plus".to_string(),
                    args: vec![
                        a.clone(),
                        Value::Call {
                            head: "Times".to_string(),
                            args: vec![Value::Integer(Integer::from(-1)), b.clone()],
                        },
                    ],
                })
            }
        }
        // scalar - Complex
        (other, Value::Complex { re, im }) => {
            if let Some(v) = val_to_f64(other) {
                Ok(normalize_complex(v - *re, -*im))
            } else {
                Ok(Value::Call {
                    head: "Plus".to_string(),
                    args: vec![
                        a.clone(),
                        Value::Call {
                            head: "Times".to_string(),
                            args: vec![Value::Integer(Integer::from(-1)), b.clone()],
                        },
                    ],
                })
            }
        }
        _ => Ok(Value::Call {
            head: "Plus".to_string(),
            args: vec![
                a.clone(),
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![Value::Integer(Integer::from(-1)), b.clone()],
                },
            ],
        }),
    }
}

pub fn add_values(a: &Value, b: &Value) -> Result<Value, EvalError> {
    if matches!(a, Value::Integer(n) if n.is_zero())
        || matches!(a, Value::Rational(n) if n.is_zero())
    {
        return Ok(b.clone());
    }
    if matches!(b, Value::Integer(n) if n.is_zero())
        || matches!(b, Value::Rational(n) if n.is_zero())
    {
        return Ok(a.clone());
    }
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Ok(Value::Integer(x.clone() + y)),
        (Value::Real(x), Value::Real(y)) => Ok(Value::Real(x.clone() + y)),
        (Value::Integer(x), Value::Real(y)) => {
            Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, x) + y))
        }
        (Value::Real(x), Value::Integer(y)) => {
            Ok(Value::Real(x + Float::with_val(DEFAULT_PRECISION, y)))
        }
        (Value::Rational(x), Value::Rational(y)) => {
            let sum: Rational = (x.as_ref() + y.as_ref()).into();
            let (num, den) = sum.into_numer_denom();
            Ok(rational_value(num, den))
        }
        (Value::Rational(x), Value::Integer(y)) | (Value::Integer(y), Value::Rational(x)) => {
            let sum: Rational = x.as_ref() + Rational::from(y);
            let (num, den) = sum.into_numer_denom();
            Ok(rational_value(num, den))
        }
        (Value::Rational(x), Value::Real(y)) => {
            let x_f = Float::with_val(DEFAULT_PRECISION, x.numer())
                / Float::with_val(DEFAULT_PRECISION, x.denom());
            Ok(Value::Real(x_f + y))
        }
        (Value::Real(x), Value::Rational(y)) => {
            let y_f = Float::with_val(DEFAULT_PRECISION, y.numer())
                / Float::with_val(DEFAULT_PRECISION, y.denom());
            Ok(Value::Real(x + y_f))
        }
        (Value::List(xs), Value::List(ys)) => {
            if xs.len() == ys.len() {
                let result: Result<Vec<Value>, _> = xs
                    .iter()
                    .zip(ys.iter())
                    .map(|(x, y)| add_values(x, y))
                    .collect();
                Ok(Value::List(result?))
            } else {
                Err(EvalError::Error(
                    "Lists must have same length for addition".to_string(),
                ))
            }
        }
        // Complex + Complex
        (Value::Complex { re: r1, im: im1 }, Value::Complex { re: r2, im: im2 }) => {
            Ok(normalize_complex(r1 + r2, im1 + im2))
        }
        // Complex + scalar
        (Value::Complex { re, im }, other) => {
            if let Some(v) = val_to_f64(other) {
                Ok(normalize_complex(*re + v, *im))
            } else {
                Ok(Value::Call {
                    head: "Plus".to_string(),
                    args: vec![a.clone(), b.clone()],
                })
            }
        }
        // scalar + Complex
        (other, Value::Complex { re, im }) => {
            if let Some(v) = val_to_f64(other) {
                Ok(normalize_complex(*re + v, *im))
            } else {
                Ok(Value::Call {
                    head: "Plus".to_string(),
                    args: vec![a.clone(), b.clone()],
                })
            }
        }
        // a + a → 2*a
        _ if a == b => Ok(Value::Call {
            head: "Times".to_string(),
            args: vec![Value::Integer(Integer::from(2)), a.clone()],
        }),
        // a + (-a) → 0  (where -a is Times[-1, a])
        (Value::Call { head, args: targs }, _)
            if head == "Times"
                && targs.len() == 2
                && matches!(&targs[0], Value::Integer(n) if *n == -1)
                && targs[1] == *b =>
        {
            Ok(Value::Integer(Integer::from(0)))
        }
        // (-a) + a → 0
        (_, Value::Call { head, args: targs })
            if head == "Times"
                && targs.len() == 2
                && matches!(&targs[0], Value::Integer(n) if *n == -1)
                && targs[1] == *a =>
        {
            Ok(Value::Integer(Integer::from(0)))
        }
        _ => Ok(Value::Call {
            head: "Plus".to_string(),
            args: vec![a.clone(), b.clone()],
        }),
    }
}

/// Construct a Power value, normalizing `base^1` to just `base`.
fn power_val(base: Value, exp: Value) -> Value {
    if matches!(&exp, Value::Integer(n) if *n == 1)
        || matches!(&exp, Value::Rational(n) if *n.numer() == 1 && *n.denom() == 1)
    {
        base
    } else {
        Value::Call {
            head: "Power".to_string(),
            args: vec![base, exp],
        }
    }
}

pub fn builtin_times(args: &[Value]) -> Result<Value, EvalError> {
    // Flatten nested Times and collect all factors
    let mut factors: Vec<Value> = Vec::new();
    let mut push_factor = |v: Value| {
        if let Value::Call {
            head,
            args: inner_args,
        } = &v
            && head == "Times"
        {
            for f in inner_args {
                factors.push(f.clone());
            }
            return;
        }
        factors.push(v);
    };
    for arg in args {
        push_factor(arg.clone());
    }

    // Repeatedly combine like terms: a * a → a^2, a^n * a → a^(n+1), a^n * a^m → a^(n+m)
    // Also merge Power factors with same base
    loop {
        let len = factors.len();
        let mut i = 0;
        while i < factors.len() {
            let mut j = i + 1;
            while j < factors.len() {
                let a = factors[i].clone();
                let b = factors[j].clone();
                let combined = try_combine_factors(&a, &b);
                if let Some(combined) = combined {
                    factors[i] = combined;
                    factors.swap_remove(j);
                    break; // restart scanning from i
                }
                j += 1;
            }
            if j < factors.len() {
                // break happened, restart outer loop
                break;
            }
            i += 1;
        }
        if factors.len() == len {
            break;
        }
    }

    if factors.is_empty() {
        return Ok(Value::Integer(Integer::from(1)));
    }

    // Fold mul_values over remaining factors to handle numeric/list multiplication.
    let mut result = Value::Integer(Integer::from(1));
    for factor in factors {
        result = mul_values(&result, &factor)?;
    }
    // Flatten any remaining nested Times
    flatten_times(result)
}

/// Flatten nested Times: Times[Times[a, b], c] → Times[a, b, c].
/// Non-Times values are returned unchanged.
fn flatten_times(v: Value) -> Result<Value, EvalError> {
    match v {
        Value::Call { head, mut args } if head == "Times" => {
            let mut flat = Vec::new();
            for arg in args.drain(..) {
                if let Value::Call {
                    head: h,
                    args: inner,
                } = &arg
                    && h == "Times"
                {
                    flat.extend(inner.clone());
                    continue;
                }
                flat.push(arg);
            }
            if flat.is_empty() {
                Ok(Value::Integer(Integer::from(1)))
            } else if flat.len() == 1 {
                Ok(flat.swap_remove(0))
            } else {
                Ok(Value::Call {
                    head: "Times".to_string(),
                    args: flat,
                })
            }
        }
        _ => Ok(v),
    }
}

/// Try to combine two factors into one (e.g., x * x → x^2, x^n * x → x^(n+1)).
/// Returns Some(combined) if combinable, None if not.
fn try_combine_factors(a: &Value, b: &Value) -> Option<Value> {
    match (a, b) {
        // a^n * a^m → a^(n+m) when same base
        (Value::Call { head: h1, args: a1 }, Value::Call { head: h2, args: a2 })
            if h1 == "Power"
                && a1.len() == 2
                && h2 == "Power"
                && a2.len() == 2
                && a1[0] == a2[0] =>
        {
            let new_exp = add_power_exponents(&a1[1], &a2[1])?;
            Some(power_val(a1[0].clone(), new_exp))
        }
        // a^n * a → a^(n+1)
        (Value::Call { head, args: pargs }, _)
            if head == "Power" && pargs.len() == 2 && pargs[0] == *b =>
        {
            let e = increment_exponent(&pargs[1])?;
            Some(power_val(pargs[0].clone(), e))
        }
        // a * a^n → a^(n+1)
        (_, Value::Call { head, args: pargs })
            if head == "Power" && pargs.len() == 2 && pargs[0] == *a =>
        {
            let e = increment_exponent(&pargs[1])?;
            Some(power_val(pargs[0].clone(), e))
        }
        // a * a → a^2 (symbolic terms only; numeric literals handled by mul_values)
        _ if a == b && !matches!(a, Value::Integer(_) | Value::Rational(_) | Value::Real(_)) => {
            Some(power_val(a.clone(), Value::Integer(Integer::from(2))))
        }
        // Can't combine
        _ => None,
    }
}

fn add_power_exponents(e1: &Value, e2: &Value) -> Option<Value> {
    match (e1, e2) {
        (Value::Integer(n), Value::Integer(m)) => Some(Value::Integer(n.clone() + m)),
        (Value::Integer(n), Value::Rational(r)) | (Value::Rational(r), Value::Integer(n)) => {
            let sum: Rational = Rational::from(n.clone()) + r.as_ref();
            let (num, den) = sum.into_numer_denom();
            Some(rational_value(num, den))
        }
        (Value::Rational(r1), Value::Rational(r2)) => {
            let sum: Rational = (r1.as_ref() + r2.as_ref()).into();
            let (num, den) = sum.into_numer_denom();
            Some(rational_value(num, den))
        }
        _ => None,
    }
}

fn increment_exponent(e: &Value) -> Option<Value> {
    match e {
        Value::Integer(n) => Some(Value::Integer(n.clone() + 1)),
        Value::Rational(r) => {
            let sum: Rational = r.as_ref() + Rational::from(1);
            let (num, den) = sum.into_numer_denom();
            Some(rational_value(num, den))
        }
        _ => None,
    }
}

pub fn mul_values(a: &Value, b: &Value) -> Result<Value, EvalError> {
    if matches!(a, Value::Integer(n) if *n == 1)
        || matches!(a, Value::Rational(n) if *n.numer() == 1 && *n.denom() == 1)
    {
        return Ok(b.clone());
    }
    if matches!(b, Value::Integer(n) if *n == 1)
        || matches!(b, Value::Rational(n) if *n.numer() == 1 && *n.denom() == 1)
    {
        return Ok(a.clone());
    }
    if matches!(a, Value::Integer(n) if n.is_zero())
        || matches!(a, Value::Rational(n) if n.is_zero())
        || matches!(b, Value::Integer(n) if n.is_zero())
        || matches!(b, Value::Rational(n) if n.is_zero())
    {
        return Ok(Value::Integer(Integer::from(0)));
    }
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Ok(Value::Integer(x.clone() * y)),
        (Value::Real(x), Value::Real(y)) => Ok(Value::Real(x.clone() * y)),
        (Value::Integer(x), Value::Real(y)) => {
            Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, x) * y))
        }
        (Value::Real(x), Value::Integer(y)) => {
            Ok(Value::Real(x * Float::with_val(DEFAULT_PRECISION, y)))
        }
        (Value::Rational(x), Value::Rational(y)) => {
            let prod: Rational = (x.as_ref() * y.as_ref()).into();
            let (num, den) = prod.into_numer_denom();
            Ok(rational_value(num, den))
        }
        (Value::Rational(x), Value::Integer(y)) | (Value::Integer(y), Value::Rational(x)) => {
            let prod: Rational = x.as_ref() * Rational::from(y);
            let (num, den) = prod.into_numer_denom();
            Ok(rational_value(num, den))
        }
        (Value::Rational(x), Value::Real(y)) => {
            let x_f = Float::with_val(DEFAULT_PRECISION, x.numer())
                / Float::with_val(DEFAULT_PRECISION, x.denom());
            Ok(Value::Real(x_f * y))
        }
        (Value::Real(x), Value::Rational(y)) => {
            let y_f = Float::with_val(DEFAULT_PRECISION, y.numer())
                / Float::with_val(DEFAULT_PRECISION, y.denom());
            Ok(Value::Real(x * y_f))
        }
        // Complex * Complex
        (Value::Complex { re: r1, im: im1 }, Value::Complex { re: r2, im: im2 }) => {
            Ok(normalize_complex(r1 * r2 - im1 * im2, r1 * im2 + im1 * r2))
        }
        // Complex * scalar
        (Value::Complex { re, im }, other) => {
            if let Some(v) = val_to_f64(other) {
                Ok(normalize_complex(re * v, im * v))
            } else {
                Ok(Value::Call {
                    head: "Times".to_string(),
                    args: vec![a.clone(), b.clone()],
                })
            }
        }
        // scalar * Complex
        (other, Value::Complex { re, im }) => {
            if let Some(v) = val_to_f64(other) {
                Ok(normalize_complex(re * v, im * v))
            } else {
                Ok(Value::Call {
                    head: "Times".to_string(),
                    args: vec![a.clone(), b.clone()],
                })
            }
        }
        (Value::List(xs), Value::Integer(s)) | (Value::Integer(s), Value::List(xs)) => {
            let result: Vec<Value> = xs
                .iter()
                .map(|x| mul_values(x, &Value::Integer(s.clone())))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(result))
        }
        (Value::List(xs), Value::Real(s)) | (Value::Real(s), Value::List(xs)) => {
            let result: Vec<Value> = xs
                .iter()
                .map(|x| mul_values(x, &Value::Real(s.clone())))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(result))
        }
        // a^n * a^m → a^(n+m)
        (Value::Call { head: h1, args: a1 }, Value::Call { head: h2, args: a2 })
            if h1 == "Power"
                && a1.len() == 2
                && h2 == "Power"
                && a2.len() == 2
                && a1[0] == a2[0] =>
        {
            match (&a1[1], &a2[1]) {
                (Value::Integer(e1), Value::Integer(e2)) => {
                    Ok(power_val(a1[0].clone(), Value::Integer(e1.clone() + e2)))
                }
                (Value::Integer(e1), Value::Rational(e2)) => {
                    let sum: Rational = Rational::from(e1.clone()) + e2.as_ref();
                    let (num, den) = sum.into_numer_denom();
                    Ok(power_val(a1[0].clone(), rational_value(num, den)))
                }
                (Value::Rational(e1), Value::Integer(e2)) => {
                    let sum: Rational = e1.as_ref() + Rational::from(e2.clone());
                    let (num, den) = sum.into_numer_denom();
                    Ok(power_val(a1[0].clone(), rational_value(num, den)))
                }
                (Value::Rational(e1), Value::Rational(e2)) => {
                    let sum: Rational = (e1.as_ref() + e2.as_ref()).into();
                    let (num, den) = sum.into_numer_denom();
                    Ok(power_val(a1[0].clone(), rational_value(num, den)))
                }
                _ => Ok(Value::Call {
                    head: "Times".to_string(),
                    args: vec![a.clone(), b.clone()],
                }),
            }
        }
        // a * a → a^2
        _ if a == b => Ok(Value::Call {
            head: "Power".to_string(),
            args: vec![a.clone(), Value::Integer(Integer::from(2))],
        }),
        // a^n * a → a^(n+1)
        (Value::Call { head, args: pargs }, _)
            if head == "Power" && pargs.len() == 2 && pargs[0] == *b =>
        {
            let exp = match &pargs[1] {
                Value::Integer(n) => Value::Integer(n.clone() + 1),
                Value::Rational(n) => {
                    let sum: Rational = n.as_ref() + Rational::from(1);
                    let (num, den) = sum.into_numer_denom();
                    rational_value(num, den)
                }
                _ => {
                    return Ok(Value::Call {
                        head: "Times".to_string(),
                        args: vec![a.clone(), b.clone()],
                    });
                }
            };
            Ok(power_val(pargs[0].clone(), exp))
        }
        // a * a^n → a^(n+1)
        (_, Value::Call { head, args: pargs })
            if head == "Power" && pargs.len() == 2 && pargs[0] == *a =>
        {
            let exp = match &pargs[1] {
                Value::Integer(n) => Value::Integer(n.clone() + 1),
                Value::Rational(n) => {
                    let sum: Rational = n.as_ref() + Rational::from(1);
                    let (num, den) = sum.into_numer_denom();
                    rational_value(num, den)
                }
                _ => {
                    return Ok(Value::Call {
                        head: "Times".to_string(),
                        args: vec![a.clone(), b.clone()],
                    });
                }
            };
            Ok(power_val(pargs[0].clone(), exp))
        }
        _ => Ok(Value::Call {
            head: "Times".to_string(),
            args: vec![a.clone(), b.clone()],
        }),
    }
}

pub fn builtin_power(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Power requires exactly 2 arguments".to_string(),
        ));
    }
    if matches!(&args[1], Value::Integer(n) if n.is_zero())
        || matches!(&args[1], Value::Rational(n) if n.is_zero())
    {
        return Ok(Value::Integer(Integer::from(1)));
    }
    if matches!(&args[1], Value::Integer(n) if *n == 1)
        || matches!(&args[1], Value::Rational(n) if *n.numer() == 1 && *n.denom() == 1)
    {
        return Ok(args[0].clone());
    }
    if matches!(&args[0], Value::Integer(n) if n.is_zero())
        || matches!(&args[0], Value::Rational(n) if n.is_zero())
    {
        return Ok(Value::Integer(Integer::from(0)));
    }
    // 1^anything = 1
    if matches!(&args[0], Value::Integer(n) if *n == 1)
        || matches!(&args[0], Value::Rational(n) if *n.numer() == 1 && *n.denom() == 1)
    {
        return Ok(Value::Integer(Integer::from(1)));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(base), Value::Integer(exp)) => {
            if let Some(e) = exp.to_u32() {
                Ok(Value::Integer(base.clone().pow(e)))
            } else {
                let e =
                    exp.clone().abs().to_u32().ok_or_else(|| {
                        EvalError::Error("Power: exponent out of range".to_string())
                    })?;
                let abs_pow = base.clone().pow(e);
                Ok(rational_value(Integer::from(1), abs_pow))
            }
        }
        (Value::Rational(base), Value::Integer(exp)) => {
            if let Some(e) = exp.to_u32() {
                let result: Rational = rug::ops::Pow::pow(base.as_ref(), e).into();
                let (num, den) = result.into_numer_denom();
                Ok(rational_value(num, den))
            } else {
                let e =
                    exp.clone().abs().to_u32().ok_or_else(|| {
                        EvalError::Error("Power: exponent out of range".to_string())
                    })?;
                let pow_result: Rational = rug::ops::Pow::pow(base.as_ref(), e).into();
                let (num, den) = pow_result.into_numer_denom();
                Ok(rational_value(den, num))
            }
        }
        (Value::Real(base), Value::Real(exp)) => Ok(Value::Real(base.clone().pow(exp))),
        (Value::Integer(base), Value::Real(exp)) => {
            let b = Float::with_val(DEFAULT_PRECISION, base);
            Ok(Value::Real(b.pow(exp)))
        }
        (Value::Real(base), Value::Integer(exp)) => {
            let e = Float::with_val(DEFAULT_PRECISION, exp);
            Ok(Value::Real(base.clone().pow(e)))
        }
        // Complex ^ Integer >= 0
        (Value::Complex { re, im }, Value::Integer(exp)) => {
            if !exp.is_negative() {
                let n = exp.to_usize().unwrap_or(0);
                let r = (re * re + im * im).sqrt();
                if r < 1e-300 {
                    return if n == 0 {
                        Ok(Value::Integer(Integer::from(1)))
                    } else {
                        Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, 0.0)))
                    };
                }
                let theta = im.atan2(*re);
                let r_pow = r.powi(n as i32);
                let theta_n = theta * (n as f64);
                Ok(normalize_complex(
                    r_pow * theta_n.cos(),
                    r_pow * theta_n.sin(),
                ))
            } else {
                let abs_exp = Value::Integer(-exp.clone());
                let power = builtin_power(&[args[0].clone(), abs_exp])?;
                builtin_divide(&[Value::Integer(Integer::from(1)), power])
            }
        }
        // Complex ^ Real
        (Value::Complex { re, im }, Value::Real(exp)) => {
            let r = (re * re + im * im).sqrt();
            if r < 1e-300 {
                return Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, 0.0)));
            }
            let theta = im.atan2(*re);
            let exp_val = exp.to_f64();
            let r_pow = r.powf(exp_val);
            let angle = exp_val * theta;
            Ok(normalize_complex(r_pow * angle.cos(), r_pow * angle.sin()))
        }
        // Complex ^ Complex
        (Value::Complex { re, im }, Value::Complex { re: wr, im: wi }) => {
            let r = (re * re + im * im).sqrt();
            if r < 1e-300 {
                return Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, 0.0)));
            }
            let theta = im.atan2(*re);
            let ln_r = r.ln();
            let real_part = wr * ln_r - wi * theta;
            let imag_part = wr * theta + wi * ln_r;
            let exp_real = real_part.exp();
            Ok(normalize_complex(
                exp_real * imag_part.cos(),
                exp_real * imag_part.sin(),
            ))
        }
        // (a^n)^m → a^(n*m) when exponents are numeric
        (Value::Call { head, args: inner }, Value::Integer(outer_exp))
            if head == "Power" && inner.len() == 2 =>
        {
            match &inner[1] {
                Value::Integer(inner_exp) => Ok(power_val(
                    inner[0].clone(),
                    Value::Integer(inner_exp.clone() * outer_exp),
                )),
                Value::Rational(inner_exp) => {
                    let prod: Rational = inner_exp.as_ref() * Rational::from(outer_exp.clone());
                    let (num, den) = prod.into_numer_denom();
                    Ok(power_val(inner[0].clone(), rational_value(num, den)))
                }
                _ => Ok(Value::Call {
                    head: "Power".to_string(),
                    args: args.to_vec(),
                }),
            }
        }
        // (a^n)^m → a^(n*m) with rational outer exponent
        (Value::Call { head, args: inner }, Value::Rational(outer_exp))
            if head == "Power" && inner.len() == 2 =>
        {
            match &inner[1] {
                Value::Integer(inner_exp) => {
                    let prod: Rational = Rational::from(inner_exp.clone()) * outer_exp.as_ref();
                    let (num, den) = prod.into_numer_denom();
                    Ok(power_val(inner[0].clone(), rational_value(num, den)))
                }
                Value::Rational(inner_exp) => {
                    let prod: Rational = (inner_exp.as_ref() * outer_exp.as_ref()).into();
                    let (num, den) = prod.into_numer_denom();
                    Ok(power_val(inner[0].clone(), rational_value(num, den)))
                }
                _ => Ok(Value::Call {
                    head: "Power".to_string(),
                    args: args.to_vec(),
                }),
            }
        }
        _ => Ok(power_val(args[0].clone(), args[1].clone())),
    }
}

pub fn builtin_divide(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Divide requires exactly 2 arguments".to_string(),
        ));
    }
    if matches!(&args[1], Value::Integer(n) if *n == 1)
        || matches!(&args[1], Value::Rational(n) if *n.numer() == 1 && *n.denom() == 1)
    {
        return Ok(args[0].clone());
    }
    // Check for zero denominator first (before zero numerator check)
    let denom_zero = matches!(&args[1], Value::Integer(b) if b.is_zero())
        || matches!(&args[1], Value::Real(b) if b.is_zero())
        || matches!(&args[1], Value::Rational(b) if b.is_zero());
    if denom_zero {
        let numer_zero = matches!(&args[0], Value::Integer(a) if a.is_zero())
            || matches!(&args[0], Value::Real(a) if a.is_zero())
            || matches!(&args[0], Value::Rational(a) if a.is_zero());
        if numer_zero {
            // 0/0: emit Power::infy then Infinity::indet
            crate::messages::emit("Power::infy", &[format!("{}/{}", args[0], args[1])]);
            crate::messages::emit("Infinity::indet", &[format!("{} ComplexInfinity", args[0])]);
            return Ok(Value::Symbol("Indeterminate".to_string()));
        }
        // nonzero / 0
        crate::messages::emit("Power::infy", &[format!("{}/{}", args[0], args[1])]);
        return Ok(Value::Symbol("ComplexInfinity".to_string()));
    }
    if matches!(&args[0], Value::Integer(n) if n.is_zero())
        || matches!(&args[0], Value::Rational(n) if n.is_zero())
    {
        return Ok(Value::Integer(Integer::from(0)));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => {
            if a.is_divisible(b) {
                Ok(Value::Integer(a.clone() / b))
            } else {
                Ok(rational_value(a.clone(), b.clone()))
            }
        }
        (Value::Real(a), Value::Real(b)) => Ok(Value::Real(a.clone() / b)),
        (Value::Integer(a), Value::Real(b)) => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a);
            Ok(Value::Real(a_f / b))
        }
        (Value::Real(a), Value::Integer(b)) => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b);
            Ok(Value::Real(a / b_f))
        }
        (Value::Rational(a), Value::Rational(b)) => {
            let quot: Rational = (a.as_ref() / b.as_ref()).into();
            let (num, den) = quot.into_numer_denom();
            Ok(rational_value(num, den))
        }
        (Value::Rational(a), Value::Integer(b)) => {
            let quot: Rational = a.as_ref() / Rational::from(b);
            let (num, den) = quot.into_numer_denom();
            Ok(rational_value(num, den))
        }
        (Value::Integer(a), Value::Rational(b)) => {
            let quot: Rational = Rational::from(a) / b.as_ref();
            let (num, den) = quot.into_numer_denom();
            Ok(rational_value(num, den))
        }
        (Value::Rational(a), Value::Real(b)) => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a.numer())
                / Float::with_val(DEFAULT_PRECISION, a.denom());
            Ok(Value::Real(a_f / b))
        }
        (Value::Real(a), Value::Rational(b)) => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b.numer())
                / Float::with_val(DEFAULT_PRECISION, b.denom());
            Ok(Value::Real(a / b_f))
        }
        // Complex / Complex
        (Value::Complex { re: r1, im: im1 }, Value::Complex { re: r2, im: im2 }) => {
            let denom = r2 * r2 + im2 * im2;
            if denom == 0.0 {
                crate::messages::emit("Power::infy", &[format!("{}/{}", args[0], args[1])]);
                crate::messages::emit("Infinity::indet", &[format!("{} ComplexInfinity", args[0])]);
                Ok(Value::Symbol("Indeterminate".to_string()))
            } else {
                Ok(normalize_complex(
                    (r1 * r2 + im1 * im2) / denom,
                    (im1 * r2 - r1 * im2) / denom,
                ))
            }
        }
        // Complex / scalar
        (Value::Complex { re, im }, other) => {
            if let Some(v) = val_to_f64(other) {
                if v == 0.0 {
                    crate::messages::emit("Power::infy", &[format!("{}/{}", args[0], args[1])]);
                    crate::messages::emit(
                        "Infinity::indet",
                        &[format!("{} ComplexInfinity", args[0])],
                    );
                    Ok(Value::Symbol("ComplexInfinity".to_string()))
                } else {
                    Ok(normalize_complex(re / v, im / v))
                }
            } else {
                Ok(Value::Call {
                    head: "Divide".to_string(),
                    args: args.to_vec(),
                })
            }
        }
        // scalar / Complex
        (other, Value::Complex { re, im }) => {
            if let Some(v) = val_to_f64(other) {
                let denom = re * re + im * im;
                if denom == 0.0 {
                    crate::messages::emit("Power::infy", &[format!("{}/{}", args[0], args[1])]);
                    crate::messages::emit(
                        "Infinity::indet",
                        &[format!("{} ComplexInfinity", args[0])],
                    );
                    Ok(Value::Symbol("ComplexInfinity".to_string()))
                } else {
                    Ok(normalize_complex(v * re / denom, -v * im / denom))
                }
            } else {
                Ok(Value::Call {
                    head: "Divide".to_string(),
                    args: args.to_vec(),
                })
            }
        }
        // a / a → 1 (for same non-zero values) — checked after zero checks
        _ if args[0] == args[1] => Ok(Value::Integer(Integer::from(1))),
        _ => Ok(Value::Call {
            head: "Divide".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_minus(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 1 {
        match &args[0] {
            Value::Integer(n) => Ok(Value::Integer(-n.clone())),
            Value::Real(r) => Ok(Value::Real(-r.clone())),
            Value::Rational(r) => {
                let neg: Rational = (-r.as_ref()).into();
                let (num, den) = neg.into_numer_denom();
                Ok(rational_value(num, den))
            }
            Value::Complex { re, im } => Ok(normalize_complex(-re, -im)),
            _ => Ok(Value::Call {
                head: "Times".to_string(),
                args: vec![Value::Integer(Integer::from(-1)), args[0].clone()],
            }),
        }
    } else if args.len() == 2 {
        let neg = builtin_minus(&[args[1].clone()])?;
        add_values(&args[0], &neg)
    } else {
        Err(EvalError::Error(
            "Minus requires 1 or 2 arguments".to_string(),
        ))
    }
}

/// Walk symbolic numeric tree, resolve known constants to f64.
/// Returns None if any leaf isn't a number or known constant.
fn symbolic_to_f64(val: &Value) -> Option<f64> {
    match val {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        Value::Rational(r) => Some(r.as_ref().to_f64()),
        Value::Symbol(s) => match s.as_str() {
            "Pi" => Some(std::f64::consts::PI),
            "E" => Some(std::f64::consts::E),
            "Degree" => Some(std::f64::consts::PI / 180.0),
            _ => None,
        },
        Value::Call { head, args } => {
            let nums: Vec<f64> = args.iter().filter_map(symbolic_to_f64).collect();
            if nums.len() != args.len() {
                return None;
            }
            match head.as_str() {
                "Plus" => Some(nums.iter().sum()),
                "Times" => Some(nums.iter().product()),
                "Power" if nums.len() == 2 => Some(nums[0].powf(nums[1])),
                "Minus" if nums.len() == 1 => Some(-nums[0]),
                _ => None,
            }
        }
        _ => None,
    }
}

pub fn builtin_abs(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Abs requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(n.clone().abs())),
        Value::Real(r) => Ok(Value::Real(r.clone().abs())),
        Value::Rational(r) => {
            let abs = r.as_ref().clone().abs();
            let (num, den) = abs.into_numer_denom();
            Ok(rational_value(num, den))
        }
        Value::Complex { re, im } => {
            let abs = (re * re + im * im).sqrt();
            Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, abs)))
        }
        other => {
            // Try numeric resolution for symbolic forms like Abs[3 - Pi]
            if let Some(f) = symbolic_to_f64(other) {
                if f < 0.0 {
                    Ok(Value::Call {
                        head: "Times".to_string(),
                        args: vec![Value::Integer(Integer::from(-1)), args[0].clone()],
                    })
                } else {
                    Ok(args[0].clone())
                }
            } else {
                Ok(Value::Call {
                    head: "Abs".to_string(),
                    args: args.to_vec(),
                })
            }
        }
    }
}

// ── Complex number functions ──

pub fn builtin_re(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Re requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Complex { re, .. } => Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, *re))),
        Value::Integer(n) => Ok(Value::Integer(n.clone())),
        Value::Real(r) => Ok(Value::Real(r.clone())),
        Value::Rational(_) => Ok(args[0].clone()),
        _ => Ok(Value::Call {
            head: "Re".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_im(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Im requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Complex { im, .. } => Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, *im))),
        Value::Integer(_) => Ok(Value::Integer(Integer::from(0))),
        Value::Real(r) if r.is_zero() => Ok(Value::Real(r.clone())),
        _ => Ok(Value::Call {
            head: "Im".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_reim(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ReIm requires exactly 1 argument".to_string(),
        ));
    }
    let re_val = builtin_re(args)?;
    let im_val = builtin_im(args)?;
    Ok(Value::List(vec![re_val, im_val]))
}

pub fn builtin_conjugate(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Conjugate requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Complex { re, im } => Ok(normalize_complex(*re, -im)),
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) => Ok(args[0].clone()),
        _ => Ok(Value::Call {
            head: "Conjugate".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_arg(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Arg requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Complex { re, im } => {
            let angle = im.atan2(*re);
            Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, angle)))
        }
        Value::Integer(n) => {
            if !n.is_negative() {
                Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, 0.0)))
            } else {
                Ok(Value::Real(Float::with_val(
                    DEFAULT_PRECISION,
                    std::f64::consts::PI,
                )))
            }
        }
        Value::Real(r) => {
            if !r.is_sign_negative() {
                Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, 0.0)))
            } else {
                let pi = Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi);
                Ok(Value::Real(pi))
            }
        }
        _ => Ok(Value::Call {
            head: "Arg".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_sign(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Sign requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Complex { re, im } => {
            let abs = (re * re + im * im).sqrt();
            if abs < 1e-300 {
                Ok(Value::Integer(Integer::from(0)))
            } else {
                Ok(normalize_complex(re / abs, im / abs))
            }
        }
        Value::Integer(n) => {
            if n.is_zero() {
                Ok(Value::Integer(Integer::from(0)))
            } else if n.is_positive() {
                Ok(Value::Integer(Integer::from(1)))
            } else {
                Ok(Value::Integer(Integer::from(-1)))
            }
        }
        Value::Rational(r) => {
            if r.numer().is_zero() {
                Ok(Value::Integer(Integer::from(0)))
            } else if r.is_positive() {
                Ok(Value::Integer(Integer::from(1)))
            } else {
                Ok(Value::Integer(Integer::from(-1)))
            }
        }
        Value::Real(r) => {
            if r.is_zero() {
                Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, 0.0)))
            } else if r.is_sign_positive() {
                Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0)))
            } else {
                Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, -1.0)))
            }
        }
        _ => Ok(Value::Call {
            head: "Sign".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_abs_arg(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "AbsArg requires exactly 1 argument".to_string(),
        ));
    }
    let abs_val = builtin_abs(args)?;
    let arg_val = builtin_arg(args)?;
    Ok(Value::List(vec![abs_val, arg_val]))
}

pub fn builtin_complex(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Complex requires exactly 2 arguments".to_string(),
        ));
    }
    let re_val = match &args[0] {
        Value::Integer(n) => n.to_f64(),
        Value::Real(r) => r.to_f64(),
        Value::Rational(r) => r.as_ref().to_f64(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Number".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let im_val = match &args[1] {
        Value::Integer(n) => n.to_f64(),
        Value::Real(r) => r.to_f64(),
        Value::Rational(r) => r.as_ref().to_f64(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Number".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    Ok(normalize_complex(re_val, im_val))
}

pub fn builtin_complex_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ComplexQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(matches!(&args[0], Value::Complex { .. })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn real(r: f64) -> Value {
        Value::Real(Float::with_val(DEFAULT_PRECISION, r))
    }
    fn rat(n: i64, d: i64) -> Value {
        rational_value(Integer::from(n), Integer::from(d))
    }
    fn list(vals: Vec<Value>) -> Value {
        Value::List(vals)
    }

    #[test]
    fn test_plus_integers() {
        let result = builtin_plus(&[int(1), int(2)]).unwrap();
        assert_eq!(result, int(3));
    }

    #[test]
    fn test_plus_reals() {
        let result = builtin_plus(&[real(1.5), real(2.5)]).unwrap();
        assert_eq!(result, real(4.0));
    }

    #[test]
    fn test_plus_mixed() {
        let result = builtin_plus(&[int(1), real(2.5)]).unwrap();
        assert_eq!(result, real(3.5));
    }

    #[test]
    fn test_plus_multiple_args() {
        let result = builtin_plus(&[int(1), int(2), int(3)]).unwrap();
        assert_eq!(result, int(6));
    }

    #[test]
    fn test_plus_lists() {
        let result = add_values(&list(vec![int(1), int(2)]), &list(vec![int(3), int(4)])).unwrap();
        assert_eq!(result, list(vec![int(4), int(6)]));
    }

    #[test]
    fn test_times_integers() {
        let result = builtin_times(&[int(3), int(4)]).unwrap();
        assert_eq!(result, int(12));
    }

    #[test]
    fn test_times_scalar_list() {
        let result = builtin_times(&[int(2), list(vec![int(1), int(2), int(3)])]).unwrap();
        assert_eq!(result, list(vec![int(2), int(4), int(6)]));
    }

    #[test]
    fn test_power() {
        let result = builtin_power(&[int(2), int(3)]).unwrap();
        assert_eq!(result, int(8));
    }

    #[test]
    fn test_power_negative_exp() {
        let result = builtin_power(&[int(2), int(-1)]).unwrap();
        assert_eq!(result, rat(1, 2));
    }

    #[test]
    fn test_divide() {
        let result = builtin_divide(&[int(6), int(2)]).unwrap();
        assert_eq!(result, int(3));
    }

    #[test]
    fn test_divide_non_exact() {
        let result = builtin_divide(&[int(5), int(2)]).unwrap();
        assert_eq!(result, rat(5, 2));
    }

    #[test]
    fn test_divide_by_zero() {
        // Non-zero / 0 => ComplexInfinity (Wolfram Language behavior)
        let result = builtin_divide(&[int(1), int(0)]).unwrap();
        assert_eq!(result, Value::Symbol("ComplexInfinity".to_string()));
    }

    #[test]
    fn test_divide_zero_by_zero() {
        // 0 / 0 => Indeterminate
        let result = builtin_divide(&[int(0), int(0)]).unwrap();
        assert_eq!(result, Value::Symbol("Indeterminate".to_string()));
    }

    #[test]
    fn test_minus_negation() {
        let result = builtin_minus(&[int(5)]).unwrap();
        assert_eq!(result, int(-5));
    }

    #[test]
    fn test_minus_subtraction() {
        let result = builtin_minus(&[int(10), int(3)]).unwrap();
        assert_eq!(result, int(7));
    }

    #[test]
    fn test_abs_integer() {
        assert_eq!(builtin_abs(&[int(-5)]).unwrap(), int(5));
        assert_eq!(builtin_abs(&[int(5)]).unwrap(), int(5));
    }

    #[test]
    fn test_abs_real() {
        assert_eq!(builtin_abs(&[real(-3.14)]).unwrap(), real(3.14));
    }

    #[test]
    fn test_abs_symbolic_positive() {
        // Abs[3 + Pi] → 3 + Pi (positive numeric resolution)
        let arg = Value::Call {
            head: "Plus".to_string(),
            args: vec![
                int(3),
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![int(1), Value::Symbol("Pi".to_string())],
                },
            ],
        };
        let result = builtin_abs(&[arg.clone()]).unwrap();
        assert_eq!(result, arg);
    }

    #[test]
    fn test_abs_symbolic_negative() {
        // Abs[3 - Pi] → Times[-1, 3 - Pi] (negative numeric resolution)
        let arg = Value::Call {
            head: "Plus".to_string(),
            args: vec![
                int(3),
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![int(-1), Value::Symbol("Pi".to_string())],
                },
            ],
        };
        let result = builtin_abs(&[arg.clone()]).unwrap();
        let expected = Value::Call {
            head: "Times".to_string(),
            args: vec![int(-1), arg],
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_abs_symbolic_unknown() {
        // Abs[x] → Abs[x] (can't resolve symbolically)
        let arg = Value::Symbol("x".to_string());
        let result = builtin_abs(&[arg.clone()]).unwrap();
        let expected = Value::Call {
            head: "Abs".to_string(),
            args: vec![arg],
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_abs_pi_minus_3() {
        // Abs[Pi - 3] → Pi - 3 (positive numeric resolution)
        let arg = Value::Call {
            head: "Plus".to_string(),
            args: vec![
                Value::Symbol("Pi".to_string()),
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![int(-1), int(3)],
                },
            ],
        };
        let result = builtin_abs(&[arg.clone()]).unwrap();
        assert_eq!(result, arg);
    }
}
