use crate::value::{EvalError, Value};
use rug::Integer;

/// NonCommutativeMultiply[a, b, ...] — associative, non-commutative product.
/// No automatic simplification beyond flattening (via Flat attribute).
pub fn builtin_nc_multiply(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Call {
        head: "NonCommutativeMultiply".to_string(),
        args: args.to_vec(),
    })
}

/// Commutator[x, y] = x**y - y**x
pub fn builtin_commutator(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeError {
            expected: "2 arguments".to_string(),
            got: format!("{} arguments", args.len()),
        });
    }
    let x = &args[0];
    let y = &args[1];
    let xy = Value::Call {
        head: "NonCommutativeMultiply".to_string(),
        args: vec![x.clone(), y.clone()],
    };
    let yx = Value::Call {
        head: "NonCommutativeMultiply".to_string(),
        args: vec![y.clone(), x.clone()],
    };
    // x**y - y**x = Plus[NonCommutativeMultiply[x, y], Times[-1, NonCommutativeMultiply[y, x]]]
    let neg_yx = Value::Call {
        head: "Times".to_string(),
        args: vec![Value::Integer(Integer::from(-1)), yx],
    };
    Ok(Value::Call {
        head: "Plus".to_string(),
        args: vec![xy, neg_yx],
    })
}

/// Anticommutator[x, y] = x**y + y**x
pub fn builtin_anticommutator(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeError {
            expected: "2 arguments".to_string(),
            got: format!("{} arguments", args.len()),
        });
    }
    let x = &args[0];
    let y = &args[1];
    let xy = Value::Call {
        head: "NonCommutativeMultiply".to_string(),
        args: vec![x.clone(), y.clone()],
    };
    let yx = Value::Call {
        head: "NonCommutativeMultiply".to_string(),
        args: vec![y.clone(), x.clone()],
    };
    // x**y + y**x = Plus[NonCommutativeMultiply[x, y], NonCommutativeMultiply[y, x]]
    Ok(Value::Call {
        head: "Plus".to_string(),
        args: vec![xy, yx],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn symbol(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }

    #[test]
    fn test_nc_multiply_preserves_order() {
        let result = builtin_nc_multiply(&[symbol("a"), symbol("b")]).unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "NonCommutativeMultiply".to_string(),
                args: vec![symbol("a"), symbol("b")],
            }
        );
    }

    #[test]
    fn test_nc_multiply_three_args() {
        let result =
            builtin_nc_multiply(&[symbol("a"), symbol("b"), symbol("c")]).unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "NonCommutativeMultiply".to_string(),
                args: vec![symbol("a"), symbol("b"), symbol("c")],
            }
        );
    }

    #[test]
    fn test_commutator_definition() {
        let result = builtin_commutator(&[symbol("x"), symbol("y")]).unwrap();
        // Should be Plus[NonCommutativeMultiply[x, y], Times[-1, NonCommutativeMultiply[y, x]]]
        let xy = Value::Call {
            head: "NonCommutativeMultiply".to_string(),
            args: vec![symbol("x"), symbol("y")],
        };
        let yx = Value::Call {
            head: "NonCommutativeMultiply".to_string(),
            args: vec![symbol("y"), symbol("x")],
        };
        let neg_yx = Value::Call {
            head: "Times".to_string(),
            args: vec![int(-1), yx],
        };
        let expected = Value::Call {
            head: "Plus".to_string(),
            args: vec![xy, neg_yx],
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_anticommutator_definition() {
        let result = builtin_anticommutator(&[symbol("x"), symbol("y")]).unwrap();
        // Should be Plus[NonCommutativeMultiply[x, y], NonCommutativeMultiply[y, x]]
        let xy = Value::Call {
            head: "NonCommutativeMultiply".to_string(),
            args: vec![symbol("x"), symbol("y")],
        };
        let yx = Value::Call {
            head: "NonCommutativeMultiply".to_string(),
            args: vec![symbol("y"), symbol("x")],
        };
        let expected = Value::Call {
            head: "Plus".to_string(),
            args: vec![xy, yx],
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_commutator_wrong_arg_count() {
        let result = builtin_commutator(&[symbol("x")]);
        assert!(result.is_err());
    }
}
