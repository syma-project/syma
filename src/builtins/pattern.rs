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

/// Check if a value matches a pattern.
/// If pattern is a Value::Pattern, use the pattern engine; otherwise use structural equality.
fn matches_value(value: &Value, pattern: &Value) -> bool {
    match pattern {
        Value::Pattern(pat_expr) => {
            matches!(match_pattern(pat_expr, value), MatchResult::Match(_))
        }
        _ => value.struct_eq(pattern),
    }
}

/// Cases[list, pattern] — select elements matching a pattern.
pub fn builtin_cases(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Cases requires 2 or 3 arguments".to_string(),
        ));
    }
    let items = match &args[0] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: args[0].type_name().to_string(),
            })
        }
    };
    let pattern = &args[1];
    let result: Vec<Value> = items
        .iter()
        .filter(|item| matches_value(item, pattern))
        .cloned()
        .collect();
    Ok(Value::List(result))
}

/// DeleteCases[list, pattern] — remove elements matching a pattern.
pub fn builtin_delete_cases(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "DeleteCases requires 2 or 3 arguments".to_string(),
        ));
    }
    let items = match &args[0] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: args[0].type_name().to_string(),
            })
        }
    };
    let pattern = &args[1];
    let result: Vec<Value> = items
        .iter()
        .filter(|item| !matches_value(item, pattern))
        .cloned()
        .collect();
    Ok(Value::List(result))
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

    fn pat_blank() -> Value {
        Value::Pattern(crate::ast::Expr::Blank {
            type_constraint: None,
        })
    }
    fn pat_int(n: i64) -> Value {
        Value::Pattern(crate::ast::Expr::Integer(n.into()))
    }
    fn pat_string(s: &str) -> Value {
        Value::Pattern(crate::ast::Expr::Str(s.to_string()))
    }

    #[test]
    fn test_cases_basic() {
        let items = list(vec![int(1), int(2), string("x"), int(3)]);
        // Cases with a literal pattern (structural equality via Value::Pattern)
        let result = builtin_cases(&[items.clone(), pat_int(2)]).unwrap();
        assert_eq!(result, list(vec![int(2)]));
    }

    #[test]
    fn test_cases_blank() {
        let items = list(vec![int(1), string("x"), int(3)]);
        // Cases with blank pattern matches everything
        let result = builtin_cases(&[items.clone(), pat_blank()]).unwrap();
        assert_eq!(result, items);
    }

    #[test]
    fn test_cases_no_match() {
        let items = list(vec![int(1), int(2), int(3)]);
        let result = builtin_cases(&[items, pat_string("x")]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_cases_non_list_error() {
        let result = builtin_cases(&[int(42), pat_blank()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_cases_basic() {
        let items = list(vec![int(1), int(2), string("x"), int(3)]);
        // DeleteCases removes matching elements
        let result = builtin_delete_cases(&[items, pat_int(2)]).unwrap();
        assert_eq!(result, list(vec![int(1), string("x"), int(3)]));
    }

    #[test]
    fn test_delete_cases_blank() {
        let items = list(vec![int(1), int(2), int(3)]);
        // DeleteCases with blank removes everything
        let result = builtin_delete_cases(&[items, pat_blank()]).unwrap();
        assert_eq!(result, list(vec![]));
    }

    #[test]
    fn test_delete_cases_no_match() {
        let items = list(vec![int(1), int(2), int(3)]);
        let result = builtin_delete_cases(&[items, pat_string("x")]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }
}
