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
