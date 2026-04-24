use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;

// ── Trigonometric ──

pub fn builtin_sin(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Sin requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(_) => Ok(Value::Call {
            head: "Sin".to_string(),
            args: args.to_vec(),
        }),
        Value::Real(r) => Ok(Value::Real(r.clone().sin())),
        _ => Ok(Value::Call {
            head: "Sin".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_cos(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Cos requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(1))),
        Value::Integer(_) => Ok(Value::Call {
            head: "Cos".to_string(),
            args: args.to_vec(),
        }),
        Value::Real(r) => Ok(Value::Real(r.clone().cos())),
        _ => Ok(Value::Call {
            head: "Cos".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_tan(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Tan requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(_) => Ok(Value::Call {
            head: "Tan".to_string(),
            args: args.to_vec(),
        }),
        Value::Real(r) => Ok(Value::Real(r.clone().tan())),
        _ => Ok(Value::Call {
            head: "Tan".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_arcsin(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ArcSin requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.asin()))
        }
        Value::Real(r) => Ok(Value::Real(r.clone().asin())),
        _ => Ok(Value::Call {
            head: "ArcSin".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_arccos(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ArcCos requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.acos()))
        }
        Value::Real(r) => Ok(Value::Real(r.clone().acos())),
        _ => Ok(Value::Call {
            head: "ArcCos".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_arctan(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ArcTan requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.atan()))
        }
        Value::Real(r) => Ok(Value::Real(r.clone().atan())),
        _ => Ok(Value::Call {
            head: "ArcTan".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── Logarithmic / Exponential ──

pub fn builtin_log(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        // Log[x] — natural logarithm
        1 => log_natural(&args[0]),
        // Log[b, x] — base-b logarithm = Ln[x] / Ln[b]
        2 => {
            let b = &args[0];
            let x = &args[1];
            // Log[b, 1] = 0
            if let Value::Integer(n) = x {
                if *n == 1 {
                    return Ok(Value::Integer(Integer::from(0)));
                }
            }
            // Log[b, b] = 1
            if b == x {
                return Ok(Value::Integer(Integer::from(1)));
            }
            // Log[b, b^n] = n  (symbolic: x is Power[b, exp])
            if let Value::Call { head, args: pargs } = x {
                if head == "Power" && pargs.len() == 2 && &pargs[0] == b {
                    return Ok(pargs[1].clone());
                }
            }
            // Log[b, x] where b and x are positive integers: try exact integer result
            if let (Value::Integer(bi), Value::Integer(xi)) = (b, x) {
                if !bi.is_negative() && !bi.is_zero() && !xi.is_negative() && !xi.is_zero() {
                    if let Some(n) = exact_integer_log(xi, bi) {
                        return Ok(Value::Integer(Integer::from(n)));
                    }
                }
            }
            // Numerical evaluation when at least one arg is a float
            match (b, x) {
                (Value::Real(_) | Value::Integer(_), Value::Real(_))
                | (Value::Real(_), Value::Integer(_)) => {
                    let ln_x = log_natural(x)?;
                    let ln_b = log_natural(b)?;
                    match (ln_x, ln_b) {
                        (Value::Real(lx), Value::Real(lb)) => {
                            if lb.is_zero() {
                                return Err(EvalError::Error(
                                    "Log base cannot be 1 or 0".to_string(),
                                ));
                            }
                            let prec = lx.prec().max(lb.prec());
                            Ok(Value::Real(
                                Float::with_val(prec, &lx) / Float::with_val(prec, &lb),
                            ))
                        }
                        _ => Ok(Value::Call {
                            head: "Log".to_string(),
                            args: args.to_vec(),
                        }),
                    }
                }
                _ => Ok(Value::Call {
                    head: "Log".to_string(),
                    args: args.to_vec(),
                }),
            }
        }
        _ => Err(EvalError::Error(
            "Log requires 1 or 2 arguments".to_string(),
        )),
    }
}

/// If x == b^n for a positive integer n, return Some(n). Otherwise None.
fn exact_integer_log(x: &Integer, b: &Integer) -> Option<u32> {
    if *b <= 1 {
        return None;
    }
    let mut remaining = x.clone();
    let mut n: u32 = 0;
    loop {
        if remaining == 1 {
            return Some(n);
        }
        let (q, r) = remaining.clone().div_rem(b.clone());
        if r != 0 {
            return None;
        }
        remaining = q;
        n += 1;
    }
}

fn log_natural(v: &Value) -> Result<Value, EvalError> {
    match v {
        // Exact special values
        Value::Integer(n) if *n == 1 => Ok(Value::Integer(Integer::from(0))),
        // Integer arguments stay symbolic (like Mathematica)
        Value::Integer(n) => {
            if n.is_negative() {
                Err(EvalError::Error("Log of non-positive number".to_string()))
            } else {
                Ok(Value::Call {
                    head: "Log".to_string(),
                    args: vec![v.clone()],
                })
            }
        }
        // Float arguments evaluate numerically
        Value::Real(r) => {
            if r.is_zero() || r.is_sign_negative() {
                Err(EvalError::Error("Log of non-positive number".to_string()))
            } else {
                let prec = r.prec();
                Ok(Value::Real(Float::with_val(prec, r).ln()))
            }
        }
        _ => Ok(Value::Call {
            head: "Log".to_string(),
            args: vec![v.clone()],
        }),
    }
}

pub fn builtin_log2(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Log2 requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_zero() && !n.is_negative() => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.log2()))
        }
        Value::Real(r) if !r.is_zero() && !r.is_sign_negative() => {
            Ok(Value::Real(r.clone().log2()))
        }
        _ => Err(EvalError::Error("Log2 of non-positive number".to_string())),
    }
}

pub fn builtin_log10(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Log10 requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_zero() && !n.is_negative() => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.log10()))
        }
        Value::Real(r) if !r.is_zero() && !r.is_sign_negative() => {
            Ok(Value::Real(r.clone().log10()))
        }
        _ => Err(EvalError::Error("Log10 of non-positive number".to_string())),
    }
}

pub fn builtin_exp(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Exp requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(1))),
        Value::Integer(_) => Ok(Value::Call {
            head: "Exp".to_string(),
            args: args.to_vec(),
        }),
        Value::Real(r) => Ok(Value::Real(r.clone().exp())),
        _ => Ok(Value::Call {
            head: "Exp".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── Root / Rounding ──

pub fn builtin_sqrt(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Sqrt requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n.is_negative() {
                Err(EvalError::Error("Sqrt of negative number".to_string()))
            } else {
                let f = Float::with_val(DEFAULT_PRECISION, n);
                let r = f.sqrt();
                // Check if result is an exact integer
                if r.is_integer() {
                    let i = r.to_f64() as i64;
                    return Ok(Value::Integer(Integer::from(i)));
                }
                Ok(Value::Real(r))
            }
        }
        Value::Real(r) => {
            if r.is_sign_negative() {
                Err(EvalError::Error("Sqrt of negative number".to_string()))
            } else {
                Ok(Value::Real(r.clone().sqrt()))
            }
        }
        _ => Ok(Value::Call {
            head: "Sqrt".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_floor(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Floor requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(n.clone())),
        Value::Real(r) => {
            let floored = r.clone().floor();
            let int_val = floored.to_integer().unwrap_or(Integer::from(0));
            Ok(Value::Integer(int_val))
        }
        _ => Err(EvalError::TypeError {
            expected: "Number".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_ceiling(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Ceiling requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(n.clone())),
        Value::Real(r) => {
            let ceiled = r.clone().ceil();
            let int_val = ceiled.to_integer().unwrap_or(Integer::from(0));
            Ok(Value::Integer(int_val))
        }
        _ => Err(EvalError::TypeError {
            expected: "Number".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_round(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Round requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(n.clone())),
        Value::Real(r) => {
            let rounded = r.clone().round();
            let int_val = rounded.to_integer().unwrap_or(Integer::from(0));
            Ok(Value::Integer(int_val))
        }
        _ => Err(EvalError::TypeError {
            expected: "Number".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Min / Max ──

pub fn builtin_max(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Max requires at least 1 argument".to_string(),
        ));
    }
    let mut max = &args[0];
    for arg in &args[1..] {
        match (max, arg) {
            (Value::Integer(a), Value::Integer(b)) => {
                if b > a {
                    max = arg;
                }
            }
            (Value::Real(a), Value::Real(b)) => {
                if b > a {
                    max = arg;
                }
            }
            _ => {}
        }
    }
    Ok(max.clone())
}

pub fn builtin_min(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Min requires at least 1 argument".to_string(),
        ));
    }
    let mut min = &args[0];
    for arg in &args[1..] {
        match (min, arg) {
            (Value::Integer(a), Value::Integer(b)) => {
                if b < a {
                    min = arg;
                }
            }
            (Value::Real(a), Value::Real(b)) => {
                if b < a {
                    min = arg;
                }
            }
            _ => {}
        }
    }
    Ok(min.clone())
}

// ── Modular arithmetic / Number theory ──

pub fn builtin_mod(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Mod requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) if !b.is_zero() => {
            let result = a.clone() % b;
            let result = if result < 0 {
                result + b.clone().abs()
            } else {
                result
            };
            Ok(Value::Integer(result))
        }
        (Value::Real(a), Value::Real(b)) if !b.is_zero() => {
            let div = a.clone() / b;
            let floored = div.floor();
            let result = a - b * floored;
            Ok(Value::Real(result))
        }
        (Value::Integer(a), Value::Real(b)) if !b.is_zero() => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a);
            let div = a_f.clone() / b;
            let floored = div.floor();
            let result = a_f - b * floored;
            Ok(Value::Real(result))
        }
        (Value::Real(a), Value::Integer(b)) if !b.is_zero() => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b);
            let div = a.clone() / &b_f;
            let floored = div.floor();
            let result = a - &b_f * floored;
            Ok(Value::Real(result))
        }
        _ => Err(EvalError::Error(
            "Mod: division by zero or invalid types".to_string(),
        )),
    }
}

pub fn gcd(mut a: Integer, mut b: Integer) -> Integer {
    while !b.is_zero() {
        let t = b;
        b = a % t.clone();
        a = t;
    }
    a.abs()
}

pub fn builtin_gcd(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GCD requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(gcd(a.clone(), b.clone()))),
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: format!("{} and {}", args[0].type_name(), args[1].type_name()),
        }),
    }
}

pub fn builtin_lcm(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LCM requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => {
            if a.is_zero() || b.is_zero() {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let product = a.clone() * b;
            let abs_product = product.abs();
            let gcd_val = gcd(a.clone(), b.clone());
            Ok(Value::Integer(abs_product / gcd_val))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: format!("{} and {}", args[0].type_name(), args[1].type_name()),
        }),
    }
}

pub fn factorial(n: i64) -> Integer {
    if n <= 1 {
        Integer::from(1)
    } else {
        Integer::from(n) * factorial(n - 1)
    }
}

pub fn builtin_factorial(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Factorial requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if *n >= 0 => {
            let n_i64 = n.to_i64().unwrap_or(0);
            Ok(Value::Integer(factorial(n_i64)))
        }
        _ => Err(EvalError::Error(
            "Factorial requires a non-negative integer".to_string(),
        )),
    }
}

// ── FixedPoint stub (evaluator handles this) ──

pub fn builtin_fixed_point_stub(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "FixedPoint should be handled by evaluator".to_string(),
    ))
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

    #[test]
    fn test_sin() {
        let result = builtin_sin(&[real(0.0)]).unwrap();
        if let Value::Real(r) = result {
            assert!(r.to_f64().abs() < f64::EPSILON);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_cos() {
        let result = builtin_cos(&[real(0.0)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 1.0).abs() < f64::EPSILON);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_log() {
        let result = builtin_log(&[real(1.0)]).unwrap();
        if let Value::Real(r) = result {
            assert!(r.to_f64().abs() < f64::EPSILON);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_log_negative() {
        assert!(builtin_log(&[int(-1)]).is_err());
    }

    #[test]
    fn test_exp() {
        let result = builtin_exp(&[real(0.0)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 1.0).abs() < f64::EPSILON);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_sqrt() {
        assert_eq!(builtin_sqrt(&[int(4)]).unwrap(), int(2));
        assert_eq!(builtin_sqrt(&[int(9)]).unwrap(), int(3));
    }

    #[test]
    fn test_sqrt_real() {
        let result = builtin_sqrt(&[real(2.0)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - std::f64::consts::SQRT_2).abs() < f64::EPSILON);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_sqrt_negative() {
        assert!(builtin_sqrt(&[int(-1)]).is_err());
    }

    #[test]
    fn test_floor() {
        assert_eq!(builtin_floor(&[real(3.7)]).unwrap(), int(3));
        assert_eq!(builtin_floor(&[real(-2.3)]).unwrap(), int(-3));
    }

    #[test]
    fn test_ceiling() {
        assert_eq!(builtin_ceiling(&[real(3.2)]).unwrap(), int(4));
        assert_eq!(builtin_ceiling(&[real(-2.7)]).unwrap(), int(-2));
    }

    #[test]
    fn test_round() {
        assert_eq!(builtin_round(&[real(3.5)]).unwrap(), int(4));
        assert_eq!(builtin_round(&[real(3.4)]).unwrap(), int(3));
    }

    #[test]
    fn test_max() {
        assert_eq!(builtin_max(&[int(1), int(3), int(2)]).unwrap(), int(3));
    }

    #[test]
    fn test_min() {
        assert_eq!(builtin_min(&[int(3), int(1), int(2)]).unwrap(), int(1));
    }
}
