use crate::value::EvalError;
use crate::value::Value;

pub fn builtin_and(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "And requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(args[0].to_bool() && args[1].to_bool()))
}

pub fn builtin_or(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Or requires exactly 2 arguments".to_string(),
        ));
    }
    Ok(Value::Bool(args[0].to_bool() || args[1].to_bool()))
}

pub fn builtin_not(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Not requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(!args[0].to_bool()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boolean(b: bool) -> Value {
        Value::Bool(b)
    }

    #[test]
    fn test_and() {
        assert_eq!(
            builtin_and(&[boolean(true), boolean(true)]).unwrap(),
            boolean(true)
        );
        assert_eq!(
            builtin_and(&[boolean(true), boolean(false)]).unwrap(),
            boolean(false)
        );
    }

    #[test]
    fn test_or() {
        assert_eq!(
            builtin_or(&[boolean(false), boolean(true)]).unwrap(),
            boolean(true)
        );
        assert_eq!(
            builtin_or(&[boolean(false), boolean(false)]).unwrap(),
            boolean(false)
        );
    }

    #[test]
    fn test_not() {
        assert_eq!(builtin_not(&[boolean(true)]).unwrap(), boolean(false));
        assert_eq!(builtin_not(&[boolean(false)]).unwrap(), boolean(true));
    }
}
