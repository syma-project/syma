use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Rational;

/// Numeric equality that promotes across the numeric types.
///
/// Returns `None` when either operand is not a number, so callers can fall
/// back to structural comparison.
fn numeric_eq(a: &Value, b: &Value) -> Option<bool> {
    use Value::*;
    let rat_to_float = |r: &rug::Rational| -> Float {
        Float::with_val(DEFAULT_PRECISION, r.numer())
            / Float::with_val(DEFAULT_PRECISION, r.denom())
    };
    match (a, b) {
        (Integer(x), Integer(y)) => Some(x == y),
        (Rational(x), Rational(y)) => Some(x == y),
        (Real(x), Real(y)) => Some(x == y),
        (Integer(x), Rational(y)) | (Rational(y), Integer(x)) => {
            Some(y.numer() == &rug::Integer::from(x * y.denom()))
        }
        (Integer(x), Real(y)) | (Real(y), Integer(x)) => {
            Some(Float::with_val(DEFAULT_PRECISION, x) == *y)
        }
        (Rational(x), Real(y)) | (Real(y), Rational(x)) => Some(rat_to_float(x) == *y),
        (Complex { re: r1, im: i1 }, Complex { re: r2, im: i2 }) => Some(r1 == r2 && i1 == i2),
        (Complex { re, im }, other) | (other, Complex { re, im }) if *im == 0.0 => {
            other.to_real().map(|o| *re == o)
        }
        _ => None,
    }
}

/// Equality used by `Equal`/`Unequal`: numbers compare by value (so `1 == 1.0`)
/// while everything else compares structurally.
fn values_equal(a: &Value, b: &Value) -> bool {
    // `Formatted` is only a display hint; compare the underlying values.
    if let Value::Formatted { value, .. } = a {
        return values_equal(value, b);
    }
    if let Value::Formatted { value, .. } = b {
        return values_equal(a, value);
    }
    if let Some(eq) = numeric_eq(a, b) {
        return eq;
    }
    match (a, b) {
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| values_equal(p, q))
        }
        (Value::Call { head: h1, args: a1 }, Value::Call { head: h2, args: a2 }) => {
            h1 == h2
                && a1.len() == a2.len()
                && a1.iter().zip(a2.iter()).all(|(p, q)| values_equal(p, q))
        }
        _ => a.struct_eq(b),
    }
}

pub fn builtin_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Equal requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(values_equal(&args[0], &args[1])))
}

pub fn builtin_unequal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Unequal requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(!values_equal(&args[0], &args[1])))
}

pub fn builtin_less(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Less requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a < b)),
        (Value::Real(a), Value::Real(b)) => Ok(Value::Bool(a < b)),
        (Value::Integer(a), Value::Real(b)) => {
            Ok(Value::Bool(Float::with_val(DEFAULT_PRECISION, a) < *b))
        }
        (Value::Real(a), Value::Integer(b)) => {
            Ok(Value::Bool(*a < Float::with_val(DEFAULT_PRECISION, b)))
        }
        (Value::Rational(a), Value::Rational(b)) => Ok(Value::Bool(a.as_ref() < b.as_ref())),
        (Value::Rational(a), Value::Integer(b)) => Ok(Value::Bool(a.as_ref() < &Rational::from(b))),
        (Value::Integer(a), Value::Rational(b)) => Ok(Value::Bool(&Rational::from(a) < b.as_ref())),
        (Value::Rational(a), Value::Real(b)) => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a.numer())
                / Float::with_val(DEFAULT_PRECISION, a.denom());
            Ok(Value::Bool(a_f < *b))
        }
        (Value::Real(a), Value::Rational(b)) => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b.numer())
                / Float::with_val(DEFAULT_PRECISION, b.denom());
            Ok(Value::Bool(*a < b_f))
        }
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a < b)),
        _ => Ok(Value::Call {
            head: "Less".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_greater(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Greater requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a > b)),
        (Value::Real(a), Value::Real(b)) => Ok(Value::Bool(a > b)),
        (Value::Integer(a), Value::Real(b)) => {
            Ok(Value::Bool(Float::with_val(DEFAULT_PRECISION, a) > *b))
        }
        (Value::Real(a), Value::Integer(b)) => {
            Ok(Value::Bool(*a > Float::with_val(DEFAULT_PRECISION, b)))
        }
        (Value::Rational(a), Value::Rational(b)) => Ok(Value::Bool(a.as_ref() > b.as_ref())),
        (Value::Rational(a), Value::Integer(b)) => Ok(Value::Bool(a.as_ref() > &Rational::from(b))),
        (Value::Integer(a), Value::Rational(b)) => Ok(Value::Bool(&Rational::from(a) > b.as_ref())),
        (Value::Rational(a), Value::Real(b)) => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a.numer())
                / Float::with_val(DEFAULT_PRECISION, a.denom());
            Ok(Value::Bool(a_f > *b))
        }
        (Value::Real(a), Value::Rational(b)) => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b.numer())
                / Float::with_val(DEFAULT_PRECISION, b.denom());
            Ok(Value::Bool(*a > b_f))
        }
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a > b)),
        _ => {
            // Symbolic args: return unevaluated Greater[args...]
            Ok(Value::Call {
                head: "Greater".to_string(),
                args: args.to_vec(),
            })
        }
    }
}

pub fn builtin_less_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LessEqual requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a <= b)),
        (Value::Real(a), Value::Real(b)) => Ok(Value::Bool(a <= b)),
        (Value::Integer(a), Value::Real(b)) => {
            Ok(Value::Bool(Float::with_val(DEFAULT_PRECISION, a) <= *b))
        }
        (Value::Real(a), Value::Integer(b)) => {
            Ok(Value::Bool(*a <= Float::with_val(DEFAULT_PRECISION, b)))
        }
        (Value::Rational(a), Value::Rational(b)) => Ok(Value::Bool(a.as_ref() <= b.as_ref())),
        (Value::Rational(a), Value::Integer(b)) => {
            Ok(Value::Bool(a.as_ref() <= &Rational::from(b)))
        }
        (Value::Integer(a), Value::Rational(b)) => {
            Ok(Value::Bool(&Rational::from(a) <= b.as_ref()))
        }
        (Value::Rational(a), Value::Real(b)) => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a.numer())
                / Float::with_val(DEFAULT_PRECISION, a.denom());
            Ok(Value::Bool(a_f <= *b))
        }
        (Value::Real(a), Value::Rational(b)) => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b.numer())
                / Float::with_val(DEFAULT_PRECISION, b.denom());
            Ok(Value::Bool(*a <= b_f))
        }
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a <= b)),
        _ => Ok(Value::Call {
            head: "LessEqual".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_greater_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GreaterEqual requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a >= b)),
        (Value::Real(a), Value::Real(b)) => Ok(Value::Bool(a >= b)),
        (Value::Integer(a), Value::Real(b)) => {
            Ok(Value::Bool(Float::with_val(DEFAULT_PRECISION, a) >= *b))
        }
        (Value::Real(a), Value::Integer(b)) => {
            Ok(Value::Bool(*a >= Float::with_val(DEFAULT_PRECISION, b)))
        }
        (Value::Rational(a), Value::Rational(b)) => Ok(Value::Bool(a.as_ref() >= b.as_ref())),
        (Value::Rational(a), Value::Integer(b)) => {
            Ok(Value::Bool(a.as_ref() >= &Rational::from(b)))
        }
        (Value::Integer(a), Value::Rational(b)) => {
            Ok(Value::Bool(&Rational::from(a) >= b.as_ref()))
        }
        (Value::Rational(a), Value::Real(b)) => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a.numer())
                / Float::with_val(DEFAULT_PRECISION, a.denom());
            Ok(Value::Bool(a_f >= *b))
        }
        (Value::Real(a), Value::Rational(b)) => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b.numer())
                / Float::with_val(DEFAULT_PRECISION, b.denom());
            Ok(Value::Bool(*a >= b_f))
        }
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a >= b)),
        _ => Ok(Value::Call {
            head: "GreaterEqual".to_string(),
            args: args.to_vec(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn real(r: f64) -> Value {
        Value::Real(rug::Float::with_val(DEFAULT_PRECISION, r))
    }
    fn rational(n: i64, d: i64) -> Value {
        Value::Rational(Box::new(rug::Rational::from((n, d))))
    }

    #[test]
    fn test_equal_integers() {
        assert_eq!(builtin_equal(&[int(5), int(5)]).unwrap(), Value::Bool(true));
        assert_eq!(
            builtin_equal(&[int(5), int(3)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_equal_strings() {
        assert_eq!(
            builtin_equal(&[Value::Str("hi".into()), Value::Str("hi".into())]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[Value::Str("hi".into()), Value::Str("bye".into())]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_equal_numeric_promotion() {
        assert_eq!(
            builtin_equal(&[int(1), real(1.0)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[real(1.0), int(1)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[rational(1, 2), real(0.5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[int(2), rational(4, 2)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[int(1), real(1.5)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_equal_nested_numeric_promotion() {
        let a = Value::List(vec![int(1), real(2.5)]);
        let b = Value::List(vec![real(1.0), rational(5, 2)]);
        assert_eq!(builtin_equal(&[a, b]).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_equal_symbolic_stays_structural() {
        let x = Value::Symbol("x".into());
        let y = Value::Symbol("y".into());
        assert_eq!(builtin_equal(&[x.clone(), x]).unwrap(), Value::Bool(true));
        assert_eq!(
            builtin_equal(&[Value::Symbol("x".into()), y]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_unequal_numeric_promotion() {
        assert_eq!(
            builtin_unequal(&[int(1), real(1.0)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_unequal(&[int(1), real(1.5)]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_equal_complex_zero_imag() {
        let c = Value::Complex { re: 2.0, im: 0.0 };
        assert_eq!(builtin_equal(&[c, real(2.0)]).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_unequal_integers() {
        assert_eq!(
            builtin_unequal(&[int(5), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_unequal(&[int(5), int(5)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_less_integers() {
        assert_eq!(builtin_less(&[int(3), int(5)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_less(&[int(5), int(3)]).unwrap(), Value::Bool(false));
        assert_eq!(builtin_less(&[int(5), int(5)]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_less_mixed_types() {
        assert_eq!(
            builtin_less(&[int(3), real(5.0)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less(&[real(5.0), int(3)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_less(&[int(1), rational(3, 2)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less(&[real(1.5), rational(1, 2)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_less_strings() {
        assert_eq!(
            builtin_less(&[Value::Str("a".into()), Value::Str("b".into())]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_greater_integers() {
        assert_eq!(
            builtin_greater(&[int(5), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater(&[int(3), int(5)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_greater_mixed() {
        assert_eq!(
            builtin_greater(&[int(5), real(3.0)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater(&[rational(5, 2), int(2)]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_less_equal() {
        assert_eq!(
            builtin_less_equal(&[int(3), int(5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less_equal(&[int(5), int(5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less_equal(&[int(5), int(3)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_greater_equal() {
        assert_eq!(
            builtin_greater_equal(&[int(5), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater_equal(&[int(5), int(5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater_equal(&[int(3), int(5)]).unwrap(),
            Value::Bool(false)
        );
    }
}
