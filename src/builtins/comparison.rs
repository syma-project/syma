use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;


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
    fn boolean(b: bool) -> Value {
        Value::Bool(b)
    }
    fn string(s: &str) -> Value {
        Value::Str(s.to_string())
    }

    #[test]
    fn test_equal() {
        assert_eq!(builtin_equal(&[int(1), int(1)]).unwrap(), boolean(true));
        assert_eq!(builtin_equal(&[int(1), int(2)]).unwrap(), boolean(false));
    }

    #[test]
    fn test_unequal() {
        assert_eq!(builtin_unequal(&[int(1), int(2)]).unwrap(), boolean(true));
        assert_eq!(builtin_unequal(&[int(1), int(1)]).unwrap(), boolean(false));
    }

    #[test]
    fn test_less() {
        assert_eq!(builtin_less(&[int(1), int(2)]).unwrap(), boolean(true));
        assert_eq!(builtin_less(&[int(2), int(1)]).unwrap(), boolean(false));
    }

    #[test]
    fn test_greater() {
        assert_eq!(builtin_greater(&[int(2), int(1)]).unwrap(), boolean(true));
        assert_eq!(builtin_greater(&[int(1), int(2)]).unwrap(), boolean(false));
    }

    #[test]
    fn test_less_equal() {
        assert_eq!(
            builtin_less_equal(&[int(1), int(1)]).unwrap(),
            boolean(true)
        );
        assert_eq!(
            builtin_less_equal(&[int(1), int(2)]).unwrap(),
            boolean(true)
        );
        assert_eq!(
            builtin_less_equal(&[int(2), int(1)]).unwrap(),
            boolean(false)
        );
    }

    #[test]
    fn test_greater_equal() {
        assert_eq!(
            builtin_greater_equal(&[int(1), int(1)]).unwrap(),
            boolean(true)
        );
        assert_eq!(
            builtin_greater_equal(&[int(2), int(1)]).unwrap(),
            boolean(true)
        );
        assert_eq!(
            builtin_greater_equal(&[int(1), int(2)]).unwrap(),
            boolean(false)
        );
    }

    #[test]
    fn test_less_strings() {
        assert_eq!(
            builtin_less(&[string("a"), string("b")]).unwrap(),
            boolean(true)
        );
    }
}
