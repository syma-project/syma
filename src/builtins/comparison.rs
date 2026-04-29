use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Rational;

/// Helper: compare two values numerically, return Some(bool) or None for incomparable
fn compare_two<F: Fn(&Float, &Float) -> bool>(a: &Value, b: &Value, op: F) -> Option<bool> {
    match (a, b) {
        (Value::Integer(a), Value::Integer(b)) => Some(op(
            &Float::with_val(DEFAULT_PRECISION, a),
            &Float::with_val(DEFAULT_PRECISION, b),
        )),
        (Value::Real(a), Value::Real(b)) => Some(op(a, b)),
        (Value::Integer(a), Value::Real(b)) => Some(op(&Float::with_val(DEFAULT_PRECISION, a), b)),
        (Value::Real(a), Value::Integer(b)) => Some(op(a, &Float::with_val(DEFAULT_PRECISION, b))),
        (Value::Rational(a), Value::Rational(b)) => Some(op(
            &(Float::with_val(DEFAULT_PRECISION, a.numer())
                / Float::with_val(DEFAULT_PRECISION, a.denom())),
            &(Float::with_val(DEFAULT_PRECISION, b.numer())
                / Float::with_val(DEFAULT_PRECISION, b.denom())),
        )),
        (Value::Rational(a), Value::Integer(b)) => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a.numer())
                / Float::with_val(DEFAULT_PRECISION, a.denom());
            Some(op(&a_f, &Float::with_val(DEFAULT_PRECISION, b)))
        }
        (Value::Integer(a), Value::Rational(b)) => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b.numer())
                / Float::with_val(DEFAULT_PRECISION, b.denom());
            Some(op(&Float::with_val(DEFAULT_PRECISION, a), &b_f))
        }
        (Value::Rational(a), Value::Real(b)) => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a.numer())
                / Float::with_val(DEFAULT_PRECISION, a.denom());
            Some(op(&a_f, b))
        }
        (Value::Real(a), Value::Rational(b)) => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b.numer())
                / Float::with_val(DEFAULT_PRECISION, b.denom());
            Some(op(a, &b_f))
        }
        _ => None,
    }
}

/// Numeric equality: a == b (using float comparison, not structural)
fn numeric_eq(a: &Value, b: &Value) -> Option<bool> {
    compare_two(a, b, |x, y| x == y)
}

/// Equal[a, b, c, ...] — chain comparison: all adjacent pairs equal
pub fn builtin_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Equal requires at least 2 arguments".to_string(),
        ));
    }
    for pair in args.windows(2) {
        // Try numeric equality first; fall back to structural for strings/symbols/etc.
        let eq = match numeric_eq(&pair[0], &pair[1]) {
            Some(true) => true,
            Some(false) => false,
            None => pair[0].struct_eq(&pair[1]),
        };
        if !eq {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

/// Unequal[a, b, c, ...] — pairs are not all equal
pub fn builtin_unequal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Unequal requires at least 2 arguments".to_string(),
        ));
    }
    // Unequal[a, b, c] is True if a != b, a != c, and b != c (all pairwise distinct)
    for i in 0..args.len() {
        for j in (i + 1)..args.len() {
            let eq = match numeric_eq(&args[i], &args[j]) {
                Some(true) => true,
                Some(false) => false,
                None => args[i].struct_eq(&args[j]),
            };
            if eq {
                return Ok(Value::Bool(false));
            }
        }
    }
    Ok(Value::Bool(true))
}

/// SameQ[a, b, c, ...] — structural equality (===), all identical
pub fn builtin_same_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "SameQ requires at least 2 arguments".to_string(),
        ));
    }
    for pair in args.windows(2) {
        if !pair[0].struct_eq(&pair[1]) {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

/// Order[a, b] — canonical ordering: -1, 0, or 1
pub fn builtin_order(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Order requires exactly 2 arguments".to_string(),
        ));
    }
    // Use numeric comparison first
    if let Some(less) = compare_two(&args[0], &args[1], |x, y| x < y) {
        if less {
            return Ok(Value::Integer(rug::Integer::from(-1)));
        }
    }
    if let Some(greater) = compare_two(&args[0], &args[1], |x, y| x > y) {
        if greater {
            return Ok(Value::Integer(rug::Integer::from(1)));
        }
    }
    if let Some(equal) = compare_two(&args[0], &args[1], |x, y| x == y) {
        if equal {
            return Ok(Value::Integer(rug::Integer::from(0)));
        }
    }
    // String comparison
    match (&args[0], &args[1]) {
        (Value::Str(a), Value::Str(b)) => {
            if a < b {
                return Ok(Value::Integer(rug::Integer::from(-1)));
            }
            if a > b {
                return Ok(Value::Integer(rug::Integer::from(1)));
            }
            return Ok(Value::Integer(rug::Integer::from(0)));
        }
        _ => {}
    }
    // Structural comparison as final fallback
    if args[0].struct_eq(&args[1]) {
        Ok(Value::Integer(rug::Integer::from(0)))
    } else {
        Ok(Value::Integer(rug::Integer::from(-1)))
    }
}

/// Less[a, b, c, ...] — strict increasing chain: a < b < c
pub fn builtin_less(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Less requires at least 2 arguments".to_string(),
        ));
    }
    for pair in args.windows(2) {
        match compare_two(&pair[0], &pair[1], |x, y| x < y) {
            Some(true) => continue,
            Some(false) => return Ok(Value::Bool(false)),
            None => match (&pair[0], &pair[1]) {
                (Value::Str(a), Value::Str(b)) => {
                    if a >= b {
                        return Ok(Value::Bool(false));
                    }
                }
                _ => {
                    return Ok(Value::Call {
                        head: "Less".to_string(),
                        args: args.to_vec(),
                    });
                }
            },
        }
    }
    Ok(Value::Bool(true))
}

/// Greater[a, b, c, ...] — strict decreasing chain: a > b > c
pub fn builtin_greater(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Greater requires at least 2 arguments".to_string(),
        ));
    }
    for pair in args.windows(2) {
        match compare_two(&pair[0], &pair[1], |x, y| x > y) {
            Some(true) => continue,
            Some(false) => return Ok(Value::Bool(false)),
            None => match (&pair[0], &pair[1]) {
                (Value::Str(a), Value::Str(b)) => {
                    if a <= b {
                        return Ok(Value::Bool(false));
                    }
                }
                _ => {
                    return Ok(Value::Call {
                        head: "Greater".to_string(),
                        args: args.to_vec(),
                    });
                }
            },
        }
    }
    Ok(Value::Bool(true))
}

/// LessEqual[a, b, c, ...] — non-decreasing chain: a <= b <= c
pub fn builtin_less_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "LessEqual requires at least 2 arguments".to_string(),
        ));
    }
    for pair in args.windows(2) {
        match compare_two(&pair[0], &pair[1], |x, y| x <= y) {
            Some(true) => continue,
            Some(false) => return Ok(Value::Bool(false)),
            None => match (&pair[0], &pair[1]) {
                (Value::Str(a), Value::Str(b)) => {
                    if a > b {
                        return Ok(Value::Bool(false));
                    }
                }
                _ => {
                    return Ok(Value::Call {
                        head: "LessEqual".to_string(),
                        args: args.to_vec(),
                    });
                }
            },
        }
    }
    Ok(Value::Bool(true))
}

/// GreaterEqual[a, b, c, ...] — non-increasing chain: a >= b >= c
pub fn builtin_greater_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "GreaterEqual requires at least 2 arguments".to_string(),
        ));
    }
    for pair in args.windows(2) {
        match compare_two(&pair[0], &pair[1], |x, y| x >= y) {
            Some(true) => continue,
            Some(false) => return Ok(Value::Bool(false)),
            None => match (&pair[0], &pair[1]) {
                (Value::Str(a), Value::Str(b)) => {
                    if a < b {
                        return Ok(Value::Bool(false));
                    }
                }
                _ => {
                    return Ok(Value::Call {
                        head: "GreaterEqual".to_string(),
                        args: args.to_vec(),
                    });
                }
            },
        }
    }
    Ok(Value::Bool(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn real(r: f64) -> Value {
        Value::Real(rug::Float::with_val(DEFAULT_PRECISION, r))
    }
    fn rational(n: i64, d: i64) -> Value {
        Value::Rational(Box::new(rug::Rational::from((n, d))))
    }

    // ── Equal tests ──
    #[test]
    fn test_equal_integers() {
        assert_eq!(builtin_equal(&[int(5), int(5)]).unwrap(), Value::Bool(true));
        assert_eq!(
            builtin_equal(&[int(5), int(3)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_equal_mixed_numeric() {
        assert_eq!(
            builtin_equal(&[int(1), real(1.0)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[int(1), real(1.5)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_equal_strings() {
        assert_eq!(
            builtin_equal(&[Value::Str("hi".into()), Value::Str("hi".into())]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[Value::Str("hi".into()), Value::Str("bye".into())]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_equal_chain() {
        assert_eq!(
            builtin_equal(&[int(1), int(1), int(1)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_equal(&[int(1), int(1), int(2)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_equal(&[int(1), int(2), int(1)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── Unequal tests ──
    #[test]
    fn test_unequal_integers() {
        assert_eq!(
            builtin_unequal(&[int(5), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_unequal(&[int(5), int(5)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_unequal_chain() {
        assert_eq!(
            builtin_unequal(&[int(1), int(2), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_unequal(&[int(1), int(1), int(2)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── SameQ tests ──
    #[test]
    fn test_same_q() {
        assert_eq!(
            builtin_same_q(&[int(5), int(5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_same_q(&[int(5), int(3)]).unwrap(),
            Value::Bool(false)
        );
        // SameQ is structural: 1 !== 1.0
        assert_eq!(
            builtin_same_q(&[int(1), real(1.0)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_same_q(&[int(1), int(1), int(1)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_same_q(&[int(1), int(1), int(2)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── Order tests ──
    #[test]
    fn test_order() {
        assert_eq!(builtin_order(&[int(3), int(5)]).unwrap(), int(-1));
        assert_eq!(builtin_order(&[int(5), int(3)]).unwrap(), int(1));
        assert_eq!(builtin_order(&[int(5), int(5)]).unwrap(), int(0));
    }

    // ── Less tests ──
    #[test]
    fn test_less_integers() {
        assert_eq!(builtin_less(&[int(3), int(5)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_less(&[int(5), int(3)]).unwrap(), Value::Bool(false));
        assert_eq!(builtin_less(&[int(5), int(5)]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_less_mixed_types() {
        assert_eq!(
            builtin_less(&[int(3), real(5.0)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less(&[real(5.0), int(3)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_less(&[int(1), rational(3, 2)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less(&[real(1.5), rational(1, 2)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_less_strings() {
        assert_eq!(
            builtin_less(&[Value::Str("a".into()), Value::Str("b".into())]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_less_chain() {
        assert_eq!(
            builtin_less(&[int(1), int(2), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less(&[int(1), int(3), int(2)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_less(&[int(1), int(2), int(2)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── Greater tests ──
    #[test]
    fn test_greater_integers() {
        assert_eq!(
            builtin_greater(&[int(5), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater(&[int(3), int(5)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_greater_mixed() {
        assert_eq!(
            builtin_greater(&[int(5), real(3.0)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater(&[rational(5, 2), int(2)]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_greater_chain() {
        assert_eq!(
            builtin_greater(&[int(3), int(2), int(1)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater(&[int(3), int(1), int(2)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── LessEqual tests ──
    #[test]
    fn test_less_equal() {
        assert_eq!(
            builtin_less_equal(&[int(3), int(5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less_equal(&[int(5), int(5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less_equal(&[int(5), int(3)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_less_equal_chain() {
        assert_eq!(
            builtin_less_equal(&[int(1), int(2), int(2)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less_equal(&[int(1), int(2), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_less_equal(&[int(2), int(1), int(3)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── GreaterEqual tests ──
    #[test]
    fn test_greater_equal() {
        assert_eq!(
            builtin_greater_equal(&[int(5), int(3)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater_equal(&[int(5), int(5)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_greater_equal(&[int(3), int(5)]).unwrap(),
            Value::Bool(false)
        );
    }
}
