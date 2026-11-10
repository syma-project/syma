use crate::builtins::symbolic::simplify_call;
use crate::value::{EvalError, Value};
use rug::Integer;

/// Return true if `v` is a numeric zero.
fn numeric_zero(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n.is_zero(),
        Value::Real(r) => r.is_zero(),
        Value::Rational(r) => r.is_zero(),
        Value::Complex { re, im } => *re == 0.0 && *im == 0.0,
        _ => false,
    }
}

/// NonCommutativeMultiply[a, b, ...] — associative, non-commutative product.
///
/// Numeric factors are combined into a single coefficient placed in front;
/// `1` is dropped and an empty product is `1` (the `OneIdentity` behaviour is
/// applied explicitly because the evaluator does not synthesise it for this
/// head). Order of the remaining factors is preserved.
pub fn builtin_nc_multiply(args: &[Value]) -> Result<Value, EvalError> {
    let mut coeffs: Vec<Value> = Vec::new();
    let mut rest: Vec<Value> = Vec::new();

    for arg in args {
        match arg {
            // 1 is the multiplicative identity.
            Value::Integer(n) if *n == 1 => {}
            Value::Integer(_) | Value::Rational(_) | Value::Real(_) | Value::Complex { .. } => {
                coeffs.push(arg.clone());
            }
            _ => rest.push(arg.clone()),
        }
    }

    if rest.is_empty() {
        return Ok(if coeffs.is_empty() {
            Value::Integer(Integer::from(1))
        } else {
            simplify_call("Times", &coeffs)
        });
    }

    let mut final_args: Vec<Value> = Vec::with_capacity(rest.len() + 1);
    if !coeffs.is_empty() {
        let coeff = simplify_call("Times", &coeffs);
        if numeric_zero(&coeff) {
            return Ok(Value::Integer(Integer::from(0)));
        }
        if coeff != Value::Integer(Integer::from(1)) {
            final_args.push(coeff);
        }
    }
    final_args.extend(rest);

    // OneIdentity: a single remaining factor is returned bare.
    if final_args.len() == 1 {
        return Ok(final_args.pop().expect("non-empty"));
    }

    Ok(Value::Call {
        head: "NonCommutativeMultiply".to_string(),
        args: final_args,
    })
}

fn nc_product(a: &Value, b: &Value) -> Value {
    Value::Call {
        head: "NonCommutativeMultiply".to_string(),
        args: vec![a.clone(), b.clone()],
    }
}

fn expect_two(name: &str, args: &[Value]) -> Result<(), EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(format!(
            "{} requires exactly 2 arguments",
            name
        )));
    }
    Ok(())
}

/// Commutator[x, y] = x**y - y**x. Equal arguments vanish.
pub fn builtin_commutator(args: &[Value]) -> Result<Value, EvalError> {
    expect_two("Commutator", args)?;
    let x = &args[0];
    let y = &args[1];
    if x == y {
        return Ok(Value::Integer(Integer::from(0)));
    }
    let xy = nc_product(x, y);
    let yx = nc_product(y, x);
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

/// Anticommutator[x, y] = x**y + y**x. Equal arguments give 2 x**x.
pub fn builtin_anticommutator(args: &[Value]) -> Result<Value, EvalError> {
    expect_two("Anticommutator", args)?;
    let x = &args[0];
    let y = &args[1];
    let xy = nc_product(x, y);
    if x == y {
        return Ok(Value::Call {
            head: "Times".to_string(),
            args: vec![Value::Integer(Integer::from(2)), xy],
        });
    }
    let yx = nc_product(y, x);
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

    fn ncm(args: Vec<Value>) -> Value {
        Value::Call {
            head: "NonCommutativeMultiply".to_string(),
            args,
        }
    }

    #[test]
    fn test_nc_multiply_preserves_order() {
        let result = builtin_nc_multiply(&[symbol("a"), symbol("b")]).unwrap();
        assert_eq!(result, ncm(vec![symbol("a"), symbol("b")]));
    }

    #[test]
    fn test_nc_multiply_three_args() {
        let result = builtin_nc_multiply(&[symbol("a"), symbol("b"), symbol("c")]).unwrap();
        assert_eq!(result, ncm(vec![symbol("a"), symbol("b"), symbol("c")]));
    }

    #[test]
    fn test_nc_multiply_empty_is_one() {
        assert_eq!(builtin_nc_multiply(&[]).unwrap(), int(1));
    }

    #[test]
    fn test_nc_multiply_single_arg_is_identity() {
        assert_eq!(builtin_nc_multiply(&[symbol("a")]).unwrap(), symbol("a"));
        assert_eq!(builtin_nc_multiply(&[int(1)]).unwrap(), int(1));
    }

    #[test]
    fn test_nc_multiply_drops_ones() {
        let result = builtin_nc_multiply(&[symbol("a"), int(1), symbol("b")]).unwrap();
        assert_eq!(result, ncm(vec![symbol("a"), symbol("b")]));
    }

    #[test]
    fn test_nc_multiply_combines_numbers() {
        assert_eq!(builtin_nc_multiply(&[int(2), int(3)]).unwrap(), int(6));
        let result = builtin_nc_multiply(&[symbol("a"), int(2), symbol("b")]).unwrap();
        assert_eq!(result, ncm(vec![int(2), symbol("a"), symbol("b")]));
    }

    #[test]
    fn test_nc_multiply_zero_annihilates() {
        let result = builtin_nc_multiply(&[int(0), symbol("a")]).unwrap();
        assert_eq!(result, int(0));
    }

    #[test]
    fn test_commutator_definition() {
        let result = builtin_commutator(&[symbol("x"), symbol("y")]).unwrap();
        let xy = ncm(vec![symbol("x"), symbol("y")]);
        let yx = ncm(vec![symbol("y"), symbol("x")]);
        let neg_yx = Value::Call {
            head: "Times".to_string(),
            args: vec![int(-1), yx],
        };
        assert_eq!(
            result,
            Value::Call {
                head: "Plus".to_string(),
                args: vec![xy, neg_yx],
            }
        );
    }

    #[test]
    fn test_commutator_equal_is_zero() {
        assert_eq!(
            builtin_commutator(&[symbol("x"), symbol("x")]).unwrap(),
            int(0)
        );
    }

    #[test]
    fn test_anticommutator_definition() {
        let result = builtin_anticommutator(&[symbol("x"), symbol("y")]).unwrap();
        let xy = ncm(vec![symbol("x"), symbol("y")]);
        let yx = ncm(vec![symbol("y"), symbol("x")]);
        assert_eq!(
            result,
            Value::Call {
                head: "Plus".to_string(),
                args: vec![xy, yx],
            }
        );
    }

    #[test]
    fn test_anticommutator_equal_is_double() {
        let result = builtin_anticommutator(&[symbol("x"), symbol("x")]).unwrap();
        assert_eq!(
            result,
            Value::Call {
                head: "Times".to_string(),
                args: vec![int(2), ncm(vec![symbol("x"), symbol("x")])],
            }
        );
    }

    #[test]
    fn test_commutator_wrong_arg_count() {
        assert!(builtin_commutator(&[symbol("x")]).is_err());
        assert!(builtin_anticommutator(&[symbol("x")]).is_err());
    }
}
