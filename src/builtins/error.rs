use crate::value::EvalError;
use crate::value::Value;

/// Throw[val] / Throw[val, tag] — raise a value as a thrown exception
/// (caught by Catch). With a tag only a matching `Catch[..., tag]` handler
/// intercepts it.
pub fn builtin_throw(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => Err(EvalError::Thrown(Box::new(args[0].clone()))),
        2 => Err(EvalError::ThrownTagged {
            value: Box::new(args[0].clone()),
            tag: Box::new(args[1].clone()),
        }),
        _ => Err(EvalError::Error(
            "Throw requires 1 or 2 arguments".to_string(),
        )),
    }
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

    #[test]
    fn test_throw_tagged() {
        let result = builtin_throw(&[int(1), Value::Str("tag".to_string())]);
        assert!(matches!(
            result,
            Err(EvalError::ThrownTagged { ref tag, .. })
                if matches!(tag.as_ref(), Value::Str(s) if s == "tag")
        ));
    }

    #[test]
    fn test_throw_wrong_arity() {
        assert!(builtin_throw(&[]).is_err());
        assert!(builtin_throw(&[int(1), int(2), int(3)]).is_err());
    }

    #[test]
    fn test_error_from_string() {
        let result = builtin_error(&[Value::Str("boom".to_string())]);
        assert!(matches!(result, Err(EvalError::Error(ref m)) if m == "boom"));
    }

    #[test]
    fn test_error_formats_non_string() {
        let result = builtin_error(&[int(7)]);
        assert!(matches!(result, Err(EvalError::Error(ref m)) if m == "7"));
    }
}
