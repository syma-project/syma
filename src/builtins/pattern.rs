use crate::pattern::{match_pattern, MatchResult};
use crate::value::EvalError;
use crate::value::Value;

pub fn builtin_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MatchQ requires exactly 2 arguments".to_string(),
        ));
    }
    let result = match &args[1] {
        Value::Pattern(pat_expr) => {
            matches!(match_pattern(pat_expr, &args[0]), MatchResult::Match(_))
        }
        _ => args[0].struct_eq(&args[1]),
    };
    Ok(Value::Bool(result))
}

pub fn builtin_head(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Head requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Symbol(args[0].type_name().to_string()))
}

pub fn builtin_type_of(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TypeOf requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Symbol(args[0].type_name().to_string()))
}

pub fn builtin_free_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FreeQ requires exactly 2 arguments".to_string(),
        ));
    }
    fn contains_pattern(value: &Value, pattern: &Value) -> bool {
        let matched = match pattern {
            Value::Pattern(pat_expr) => {
                matches!(match_pattern(pat_expr, value), MatchResult::Match(_))
            }
            _ => value.struct_eq(pattern),
        };
        if matched {
            return true;
        }
        match value {
            Value::List(items) => items.iter().any(|item| contains_pattern(item, pattern)),
            Value::Call { args, .. } => args.iter().any(|arg| contains_pattern(arg, pattern)),
            _ => false,
        }
    }
    Ok(Value::Bool(!contains_pattern(&args[0], &args[1])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn list(vals: Vec<Value>) -> Value {
        Value::List(vals)
    }
    fn string(s: &str) -> Value {
        Value::Str(s.to_string())
    }
    fn boolean(b: bool) -> Value {
        Value::Bool(b)
    }

    #[test]
    fn test_head() {
        assert_eq!(
            builtin_head(&[int(42)]).unwrap(),
            Value::Symbol("Integer".to_string())
        );
        assert_eq!(
            builtin_head(&[list(vec![])]).unwrap(),
            Value::Symbol("List".to_string())
        );
    }

    #[test]
    fn test_type_of() {
        assert_eq!(
            builtin_type_of(&[string("hello")]).unwrap(),
            Value::Symbol("String".to_string())
        );
        assert_eq!(
            builtin_type_of(&[boolean(true)]).unwrap(),
            Value::Symbol("Boolean".to_string())
        );
    }
}
