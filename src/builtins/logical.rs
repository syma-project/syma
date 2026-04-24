use crate::env::Env;
use crate::value::{EvalError, Value};

/// Evaluate a potentially-held value (Value::Pattern) in the environment.
/// If the value is already evaluated (not a Pattern), returns it as-is.
fn eval_held(val: &Value, env: &Env) -> Result<Value, EvalError> {
    match val {
        Value::Pattern(expr) => crate::eval::eval(expr, env),
        _ => Ok(val.clone()),
    }
}

/// And[args] — short-circuit logical conjunction.
///
/// And[] = True. And[x] = x.
/// Returns the first non-truthy value; if all truthy, returns the last value.
pub fn builtin_and(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Bool(true));
    }
    let mut last = Value::Bool(true);
    for arg in args {
        let val = eval_held(arg, env)?;
        if !val.to_bool() {
            return Ok(val);
        }
        last = val;
    }
    Ok(last)
}

/// Or[args] — short-circuit logical disjunction.
///
/// Or[] = False. Or[x] = x.
/// Returns the first truthy value; if all falsy, returns the last value.
pub fn builtin_or(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Bool(false));
    }
    let mut last = Value::Bool(false);
    for arg in args {
        let val = eval_held(arg, env)?;
        if val.to_bool() {
            return Ok(val);
        }
        last = val;
    }
    Ok(last)
}

/// Not[expr] — logical negation.
pub fn builtin_not(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Not requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(!args[0].to_bool()))
}

/// Xor[args] — logical exclusive OR (parity).
///
/// Xor[] = False. Xor[x] = x.
/// Returns True when an odd number of arguments are truthy.
pub fn builtin_xor(args: &[Value]) -> Result<Value, EvalError> {
    let count = args.iter().filter(|v| v.to_bool()).count();
    Ok(Value::Bool(count % 2 == 1))
}

/// Nand[args] — logical NAND.
///
/// Nand[] = False. Nand[x] = Not[x].
/// Returns False if all arguments are truthy, True otherwise.
pub fn builtin_nand(args: &[Value]) -> Result<Value, EvalError> {
    let all_true = args.iter().all(|v| v.to_bool());
    Ok(Value::Bool(!all_true))
}

/// Nor[args] — logical NOR.
///
/// Nor[] = True. Nor[x] = Not[x].
/// Returns True if no argument is truthy, False otherwise.
pub fn builtin_nor(args: &[Value]) -> Result<Value, EvalError> {
    let any_true = args.iter().any(|v| v.to_bool());
    Ok(Value::Bool(!any_true))
}

/// Implies[p, q] — logical implication.
///
/// p → q is equivalent to Not[p] || q.
/// Returns True unless p is truthy and q is falsy.
pub fn builtin_implies(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Implies requires exactly 2 arguments".to_string(),
        ));
    }
    let p = eval_held(&args[0], env)?;
    let q = eval_held(&args[1], env)?;
    Ok(Value::Bool(!p.to_bool() || q.to_bool()))
}

/// Equivalent[args] — all arguments have the same truth value.
///
/// Equivalent[] = True. Equivalent[x] = True.
/// Returns True if all arguments have the same truth value.
pub fn builtin_equivalent(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() <= 1 {
        return Ok(Value::Bool(true));
    }
    let first_bool = args[0].to_bool();
    let all_same = args.iter().all(|v| v.to_bool() == first_bool);
    Ok(Value::Bool(all_same))
}

/// Majority[args] — majority rule.
///
/// Requires an odd number of arguments.
/// Returns True if more than half of the arguments are truthy.
pub fn builtin_majority(args: &[Value]) -> Result<Value, EvalError> {
    #[allow(clippy::manual_is_multiple_of)]
    if args.is_empty() || args.len() % 2 == 0 {
        return Err(EvalError::Error(
            "Majority requires an odd number of arguments".to_string(),
        ));
    }
    let count = args.iter().filter(|v| v.to_bool()).count();
    Ok(Value::Bool(count > args.len() / 2))
}

/// Boole[expr] — convert boolean to integer.
///
/// Returns 1 if expr is True, 0 otherwise.
pub fn builtin_boole(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Boole requires exactly 1 argument".to_string(),
        ));
    }
    if matches!(&args[0], Value::Bool(true)) {
        Ok(Value::Integer(rug::Integer::from(1)))
    } else {
        Ok(Value::Integer(rug::Integer::from(0)))
    }
}

/// BooleanQ[expr] — test if expr is a boolean (True or False).
pub fn builtin_boolean_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "BooleanQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(matches!(&args[0], Value::Bool(_))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Env;

    fn test_env() -> Env {
        Env::new()
    }

    fn b(b: bool) -> Value {
        Value::Bool(b)
    }

    // ── And ──

    #[test]
    fn test_and_empty() {
        let env = test_env();
        assert_eq!(builtin_and(&[], &env).unwrap(), b(true));
    }

    #[test]
    fn test_and_single_arg() {
        let env = test_env();
        assert_eq!(builtin_and(&[b(true)], &env).unwrap(), b(true));
        assert_eq!(builtin_and(&[b(false)], &env).unwrap(), b(false));
    }

    #[test]
    fn test_and_two_args() {
        let env = test_env();
        assert_eq!(builtin_and(&[b(true), b(true)], &env).unwrap(), b(true));
        assert_eq!(
            builtin_and(&[b(true), b(false)], &env).unwrap(),
            b(false)
        );
        assert_eq!(
            builtin_and(&[b(false), b(true)], &env).unwrap(),
            b(false)
        );
        assert_eq!(
            builtin_and(&[b(false), b(false)], &env).unwrap(),
            b(false)
        );
    }

    #[test]
    fn test_and_returns_last_value() {
        let env = test_env();
        let forty_two = Value::Integer(rug::Integer::from(42));
        assert_eq!(
            builtin_and(&[b(true), forty_two.clone()], &env).unwrap(),
            forty_two
        );
    }

    // ── Or ──

    #[test]
    fn test_or_empty() {
        let env = test_env();
        assert_eq!(builtin_or(&[], &env).unwrap(), b(false));
    }

    #[test]
    fn test_or_single_arg() {
        let env = test_env();
        assert_eq!(builtin_or(&[b(true)], &env).unwrap(), b(true));
        assert_eq!(builtin_or(&[b(false)], &env).unwrap(), b(false));
    }

    #[test]
    fn test_or_two_args() {
        let env = test_env();
        assert_eq!(builtin_or(&[b(true), b(true)], &env).unwrap(), b(true));
        assert_eq!(builtin_or(&[b(true), b(false)], &env).unwrap(), b(true));
        assert_eq!(builtin_or(&[b(false), b(true)], &env).unwrap(), b(true));
        assert_eq!(
            builtin_or(&[b(false), b(false)], &env).unwrap(),
            b(false)
        );
    }

    #[test]
    fn test_or_returns_first_truthy() {
        let env = test_env();
        let forty_two = Value::Integer(rug::Integer::from(42));
        assert_eq!(
            builtin_or(&[b(false), forty_two.clone()], &env).unwrap(),
            forty_two
        );
    }

    // ── Not ──

    #[test]
    fn test_not() {
        assert_eq!(builtin_not(&[b(true)]).unwrap(), b(false));
        assert_eq!(builtin_not(&[b(false)]).unwrap(), b(true));
    }

    #[test]
    fn test_not_wrong_arity() {
        assert!(builtin_not(&[]).is_err());
        assert!(builtin_not(&[b(true), b(false)]).is_err());
    }

    // ── Xor ──

    #[test]
    fn test_xor_empty() {
        assert_eq!(builtin_xor(&[]).unwrap(), b(false));
    }

    #[test]
    fn test_xor_single() {
        assert_eq!(builtin_xor(&[b(true)]).unwrap(), b(true));
        assert_eq!(builtin_xor(&[b(false)]).unwrap(), b(false));
    }

    #[test]
    fn test_xor_two_args() {
        assert_eq!(builtin_xor(&[b(true), b(true)]).unwrap(), b(false));
        assert_eq!(builtin_xor(&[b(true), b(false)]).unwrap(), b(true));
        assert_eq!(builtin_xor(&[b(false), b(true)]).unwrap(), b(true));
        assert_eq!(builtin_xor(&[b(false), b(false)]).unwrap(), b(false));
    }

    #[test]
    fn test_xor_three_args() {
        // True, True, True → 3 trues (odd) → True
        assert_eq!(builtin_xor(&[b(true), b(true), b(true)]).unwrap(), b(true));
        // True, True, False → 2 trues (even) → False
        assert_eq!(builtin_xor(&[b(true), b(true), b(false)]).unwrap(), b(false));
    }

    // ── Nand ──

    #[test]
    fn test_nand_empty() {
        assert_eq!(builtin_nand(&[]).unwrap(), b(false));
    }

    #[test]
    fn test_nand_two_args() {
        assert_eq!(builtin_nand(&[b(true), b(true)]).unwrap(), b(false));
        assert_eq!(builtin_nand(&[b(true), b(false)]).unwrap(), b(true));
        assert_eq!(builtin_nand(&[b(false), b(true)]).unwrap(), b(true));
        assert_eq!(builtin_nand(&[b(false), b(false)]).unwrap(), b(true));
    }

    // ── Nor ──

    #[test]
    fn test_nor_empty() {
        assert_eq!(builtin_nor(&[]).unwrap(), b(true));
    }

    #[test]
    fn test_nor_two_args() {
        assert_eq!(builtin_nor(&[b(true), b(true)]).unwrap(), b(false));
        assert_eq!(builtin_nor(&[b(true), b(false)]).unwrap(), b(false));
        assert_eq!(builtin_nor(&[b(false), b(true)]).unwrap(), b(false));
        assert_eq!(builtin_nor(&[b(false), b(false)]).unwrap(), b(true));
    }

    // ── Implies ──

    #[test]
    fn test_implies_truth_table() {
        let env = test_env();
        assert_eq!(
            builtin_implies(&[b(true), b(true)], &env).unwrap(),
            b(true)
        );
        assert_eq!(
            builtin_implies(&[b(true), b(false)], &env).unwrap(),
            b(false)
        );
        assert_eq!(
            builtin_implies(&[b(false), b(true)], &env).unwrap(),
            b(true)
        );
        assert_eq!(
            builtin_implies(&[b(false), b(false)], &env).unwrap(),
            b(true)
        );
    }

    #[test]
    fn test_implies_wrong_arity() {
        let env = test_env();
        assert!(builtin_implies(&[], &env).is_err());
        assert!(builtin_implies(&[b(true)], &env).is_err());
        assert!(builtin_implies(&[b(true), b(true), b(true)], &env).is_err());
    }

    // ── Equivalent ──

    #[test]
    fn test_equivalent_empty() {
        assert_eq!(builtin_equivalent(&[]).unwrap(), b(true));
    }

    #[test]
    fn test_equivalent_single() {
        assert_eq!(builtin_equivalent(&[b(true)]).unwrap(), b(true));
        assert_eq!(builtin_equivalent(&[b(false)]).unwrap(), b(true));
    }

    #[test]
    fn test_equivalent_two_args() {
        assert_eq!(builtin_equivalent(&[b(true), b(true)]).unwrap(), b(true));
        assert_eq!(builtin_equivalent(&[b(true), b(false)]).unwrap(), b(false));
        assert_eq!(builtin_equivalent(&[b(false), b(true)]).unwrap(), b(false));
        assert_eq!(builtin_equivalent(&[b(false), b(false)]).unwrap(), b(true));
    }

    #[test]
    fn test_equivalent_three_args() {
        assert_eq!(
            builtin_equivalent(&[b(true), b(true), b(true)]).unwrap(),
            b(true)
        );
        assert_eq!(
            builtin_equivalent(&[b(true), b(false), b(true)]).unwrap(),
            b(false)
        );
    }

    // ── Majority ──

    #[test]
    fn test_majority_basic() {
        // 3 args: need 2+ to be True
        assert_eq!(
            builtin_majority(&[b(true), b(true), b(false)]).unwrap(),
            b(true)
        );
        assert_eq!(
            builtin_majority(&[b(true), b(false), b(false)]).unwrap(),
            b(false)
        );
    }

    #[test]
    fn test_majority_odd_required() {
        assert!(builtin_majority(&[]).is_err());
        assert!(builtin_majority(&[b(true), b(false)]).is_err());
        assert!(builtin_majority(&[b(true), b(false), b(true), b(false)]).is_err());
    }

    // ── Boole ──

    #[test]
    fn test_boole_true() {
        let result = builtin_boole(&[b(true)]).unwrap();
        assert_eq!(result, Value::Integer(rug::Integer::from(1)));
    }

    #[test]
    fn test_boole_false() {
        let result = builtin_boole(&[b(false)]).unwrap();
        assert_eq!(result, Value::Integer(rug::Integer::from(0)));
    }

    #[test]
    fn test_boole_non_bool() {
        let result =
            builtin_boole(&[Value::Integer(rug::Integer::from(42))]).unwrap();
        assert_eq!(result, Value::Integer(rug::Integer::from(0)));
        let result = builtin_boole(&[Value::Str("hello".to_string())]).unwrap();
        assert_eq!(result, Value::Integer(rug::Integer::from(0)));
        let result = builtin_boole(&[Value::Null]).unwrap();
        assert_eq!(result, Value::Integer(rug::Integer::from(0)));
    }

    #[test]
    fn test_boole_wrong_arity() {
        assert!(builtin_boole(&[]).is_err());
        assert!(builtin_boole(&[b(true), b(false)]).is_err());
    }

    // ── BooleanQ ──

    #[test]
    fn test_boolean_q_true() {
        assert_eq!(builtin_boolean_q(&[b(true)]).unwrap(), b(true));
        assert_eq!(builtin_boolean_q(&[b(false)]).unwrap(), b(true));
    }

    #[test]
    fn test_boolean_q_false() {
        assert_eq!(
            builtin_boolean_q(&[Value::Integer(rug::Integer::from(1))]).unwrap(),
            b(false)
        );
        assert_eq!(
            builtin_boolean_q(&[Value::Str("hello".to_string())]).unwrap(),
            b(false)
        );
        assert_eq!(builtin_boolean_q(&[Value::Null]).unwrap(), b(false));
        assert_eq!(
            builtin_boolean_q(&[Value::Symbol("x".to_string())]).unwrap(),
            b(false)
        );
    }

    #[test]
    fn test_boolean_q_wrong_arity() {
        assert!(builtin_boolean_q(&[]).is_err());
        assert!(builtin_boolean_q(&[b(true), b(false)]).is_err());
    }
}
