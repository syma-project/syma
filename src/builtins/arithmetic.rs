use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;
use rug::ops::Pow;

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
        _ => {
            // Return symbolic: Plus[a, Times[-1, b]]
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
}

pub fn add_values(a: &Value, b: &Value) -> Result<Value, EvalError> {
    // Identity: 0 + x = x
    if matches!(a, Value::Integer(n) if n.is_zero()) {
        return Ok(b.clone());
    }
    if matches!(b, Value::Integer(n) if n.is_zero()) {
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
        _ => {
            // Return symbolic: Plus[a, b]
            Ok(Value::Call {
                head: "Plus".to_string(),
                args: vec![a.clone(), b.clone()],
            })
        }
    }
}

pub fn builtin_times(args: &[Value]) -> Result<Value, EvalError> {
    let mut result = Value::Integer(Integer::from(1));
    for arg in args {
        result = mul_values(&result, arg)?;
    }
    Ok(result)
}

pub fn mul_values(a: &Value, b: &Value) -> Result<Value, EvalError> {
    // Identity: 1 * x = x
    if matches!(a, Value::Integer(n) if *n == 1) {
        return Ok(b.clone());
    }
    if matches!(b, Value::Integer(n) if *n == 1) {
        return Ok(a.clone());
    }
    // Annihilator: 0 * x = 0
    if matches!(a, Value::Integer(n) if n.is_zero())
        || matches!(b, Value::Integer(n) if n.is_zero())
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
    // Identity: x^0 = 1, x^1 = x
    if matches!(&args[1], Value::Integer(n) if n.is_zero()) {
        return Ok(Value::Integer(Integer::from(1)));
    }
    if matches!(&args[1], Value::Integer(n) if *n == 1) {
        return Ok(args[0].clone());
    }
    // Annihilator: 0^x = 0
    if matches!(&args[0], Value::Integer(n) if n.is_zero()) {
        return Ok(Value::Integer(Integer::from(0)));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(base), Value::Integer(exp)) => {
            if let Some(e) = exp.to_u32() {
                Ok(Value::Integer(base.clone().pow(e)))
            } else {
                // Negative exponent: convert to float
                let b = Float::with_val(DEFAULT_PRECISION, base);
                let e = Float::with_val(DEFAULT_PRECISION, exp);
                Ok(Value::Real(b.pow(e)))
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
        _ => Ok(Value::Call {
            head: "Power".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_divide(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Divide requires exactly 2 arguments".to_string(),
        ));
    }
    // Identity: x/1 = x
    if matches!(&args[1], Value::Integer(n) if *n == 1) {
        return Ok(args[0].clone());
    }
    // Annihilator: 0/x = 0
    if matches!(&args[0], Value::Integer(n) if n.is_zero()) {
        return Ok(Value::Integer(Integer::from(0)));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(_), Value::Integer(b)) if b.is_zero() => Err(EvalError::DivisionByZero),
        (Value::Real(_), Value::Real(b)) if b.is_zero() => Err(EvalError::DivisionByZero),
        (Value::Integer(a), Value::Integer(b)) => {
            if a.is_divisible(b) {
                Ok(Value::Integer(a.clone() / b))
            } else {
                let a_f = Float::with_val(DEFAULT_PRECISION, a);
                let b_f = Float::with_val(DEFAULT_PRECISION, b);
                Ok(Value::Real(a_f / b_f))
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
        _ => Ok(Value::Call {
            head: "Divide".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_minus(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 1 {
        // Negation
        match &args[0] {
            Value::Integer(n) => Ok(Value::Integer(-n.clone())),
            Value::Real(r) => Ok(Value::Real(-r.clone())),
            _ => Ok(Value::Call {
                head: "Times".to_string(),
                args: vec![Value::Integer(Integer::from(-1)), args[0].clone()],
            }),
        }
    } else if args.len() == 2 {
        // Subtraction
        let neg = builtin_minus(&[args[1].clone()])?;
        add_values(&args[0], &neg)
    } else {
        Err(EvalError::Error(
            "Minus requires 1 or 2 arguments".to_string(),
        ))
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
        _ => Ok(Value::Call {
            head: "Abs".to_string(),
            args: args.to_vec(),
        }),
    }
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
        assert_eq!(result, real(0.5));
    }

    #[test]
    fn test_divide() {
        let result = builtin_divide(&[int(6), int(2)]).unwrap();
        assert_eq!(result, int(3));
    }

    #[test]
    fn test_divide_non_exact() {
        let result = builtin_divide(&[int(5), int(2)]).unwrap();
        assert_eq!(result, real(2.5));
    }

    #[test]
    fn test_divide_by_zero() {
        let result = builtin_divide(&[int(1), int(0)]);
        assert!(matches!(result, Err(EvalError::DivisionByZero)));
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
}
