use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Rational;

pub fn builtin_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Equal requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(args[0].struct_eq(&args[1])))
}

pub fn builtin_unequal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Unequal requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(!args[0].struct_eq(&args[1])))
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
        (Value::Rational(a), Value::Integer(b)) => {
            Ok(Value::Bool(a.as_ref() < &Rational::from(b)))
        }
        (Value::Integer(a), Value::Rational(b)) => {
            Ok(Value::Bool(&Rational::from(a) < b.as_ref()))
        }
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
        _ => Err(EvalError::TypeError {
            expected: "Number or String".to_string(),
            got: args[0].type_name().to_string(),
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
        (Value::Rational(a), Value::Integer(b)) => {
            Ok(Value::Bool(a.as_ref() > &Rational::from(b)))
        }
        (Value::Integer(a), Value::Rational(b)) => {
            Ok(Value::Bool(&Rational::from(a) > b.as_ref()))
        }
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
        _ => Err(EvalError::TypeError {
            expected: "Number or String".to_string(),
            got: args[0].type_name().to_string(),
        }),
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
        _ => Err(EvalError::TypeError {
            expected: "Number or String".to_string(),
            got: args[0].type_name().to_string(),
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
        _ => Err(EvalError::TypeError {
            expected: "Number or String".to_string(),
            got: args[0].type_name().to_string(),
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
        assert_eq!(builtin_equal(&[int(5), int(3)]).unwrap(), Value::Bool(false));
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
    fn test_unequal_integers() {
        assert_eq!(builtin_unequal(&[int(5), int(3)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_unequal(&[int(5), int(5)]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_less_integers() {
        assert_eq!(builtin_less(&[int(3), int(5)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_less(&[int(5), int(3)]).unwrap(), Value::Bool(false));
        assert_eq!(builtin_less(&[int(5), int(5)]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_less_mixed_types() {
        assert_eq!(builtin_less(&[int(3), real(5.0)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_less(&[real(5.0), int(3)]).unwrap(), Value::Bool(false));
        assert_eq!(builtin_less(&[int(1), rational(3, 2)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_less(&[real(1.5), rational(1, 2)]).unwrap(), Value::Bool(false));
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
        assert_eq!(builtin_greater(&[int(5), int(3)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_greater(&[int(3), int(5)]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_greater_mixed() {
        assert_eq!(builtin_greater(&[int(5), real(3.0)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_greater(&[rational(5, 2), int(2)]).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_less_equal() {
        assert_eq!(builtin_less_equal(&[int(3), int(5)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_less_equal(&[int(5), int(5)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_less_equal(&[int(5), int(3)]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_greater_equal() {
        assert_eq!(builtin_greater_equal(&[int(5), int(3)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_greater_equal(&[int(5), int(5)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_greater_equal(&[int(3), int(5)]).unwrap(), Value::Bool(false));
    }
}
