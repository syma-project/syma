use crate::value::EvalError;
use crate::value::Value;

/// Throw[val] — raise a value as a thrown exception (caught by Catch).
pub fn builtin_throw(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Throw requires exactly 1 argument".to_string(),
        ));
    }
    Err(EvalError::Thrown(Box::new(args[0].clone())))
}

/// Error["message"] — raise a general error.
pub fn builtin_error(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Error requires exactly 1 argument".to_string(),
        ));
    }
    let msg = match &args[0] {
        Value::Str(s) => s.clone(),
        other => format!("{}", other),
    };
    Err(EvalError::Error(msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    #[test]
    fn test_throw() {
        let result = builtin_throw(&[int(99)]);
        assert!(
            matches!(result, Err(EvalError::Thrown(ref v)) if matches!(v.as_ref(), Value::Integer(_)))
        );
    }
}
